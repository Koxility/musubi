//! Einstiegspunkt: Kommandozeile, Logging und Start der Oberfläche.

use std::io::{BufRead, Write};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use musubi_core::domain::MediaKind;
use musubi_core::{
    AniListLoginFlow, Core, CoreOptions, Paths, PlayerCommand, SyncDirection, oauth,
};
use secrecy::SecretString;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "musubi",
    version,
    about = "Jellyfin streaming and AniList management"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Clone, Copy, ValueEnum)]
enum ServiceArg {
    Jellyfin,
    Anilist,
}

#[derive(Subcommand)]
enum Command {
    /// Grafische Oberfläche starten
    Gui,
    /// Bei einem Dienst anmelden
    Login {
        service: ServiceArg,
        /// AniList: Token von Hand einfügen statt lokalem Redirect
        #[arg(long)]
        pin: bool,
        /// Jellyfin: API-Key statt Passwort verwenden
        #[arg(long)]
        api_key: bool,
    },
    /// Von einem Dienst abmelden
    Logout { service: ServiceArg },
    /// Jellyfin und AniList abgleichen
    Sync {
        /// Nur anzeigen, nichts ändern
        #[arg(long)]
        dry_run: bool,
    },
}

fn init_logging(paths: &Paths) -> tracing_appender::non_blocking::WorkerGuard {
    let appender = tracing_appender::rolling::daily(&paths.state_dir, "musubi.log");
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let filter = EnvFilter::try_from_env("MUSUBI_LOG")
        .unwrap_or_else(|_| EnvFilter::new("warn,musubi=info,musubi_core=info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .with_ansi(false)
        .init();
    guard
}

fn prompt(label: &str) -> Result<String> {
    print!("{label}: ");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim().to_owned())
}

fn prompt_secret(label: &str) -> Result<SecretString> {
    Ok(SecretString::from(rpassword::prompt_password(format!(
        "{label}: "
    ))?))
}

async fn login_jellyfin(core: &Core, api_key: bool) -> Result<()> {
    let default = core.config().jellyfin.url.unwrap_or_default();
    let mut url = prompt(&format!("Server URL [{default}]"))?;
    if url.is_empty() {
        url = default;
    }
    let user = prompt("Username")?;
    let name = if api_key {
        core.login_jellyfin_api_key(&url, prompt_secret("API key")?, &user)
            .await?
    } else {
        core.login_jellyfin(&url, &user, &prompt_secret("Password")?)
            .await?
    };
    println!("Logged in to Jellyfin as {name}.");
    Ok(())
}

async fn login_anilist(core: &Core, pin: bool) -> Result<()> {
    if core.config().anilist.client_id.is_none() {
        let port = core.config().anilist.loopback_port;
        println!("Create an app at https://anilist.co/settings/developer");
        println!("Redirect URL: {}", oauth::loopback_redirect(port));
        println!("For the manual flow use: {}", oauth::PIN_REDIRECT);
        let id = prompt("Client ID")?;
        if id.is_empty() {
            bail!("a client ID is required");
        }
        core.update_config(|c| c.anilist.client_id = Some(id))?;
    }
    let use_pin = pin || core.config().anilist.login_flow == AniListLoginFlow::Pin;
    let viewer = if use_pin {
        println!(
            "Open this page and authorize Musubi:\n{}",
            core.anilist_authorize_url()?
        );
        let input = prompt_secret("Token or redirect URL")?;
        core.login_anilist_pin(secrecy::ExposeSecret::expose_secret(&input))
            .await?
    } else {
        println!("Waiting for the browser…");
        core.login_anilist_loopback().await?
    };
    println!("Logged in to AniList as {}.", viewer.name);
    Ok(())
}

async fn run_sync(core: &Core, dry_run: bool) -> Result<()> {
    core.refresh_list(MediaKind::Anime).await?;
    let rows = core.sync_diff().await?;
    if rows.is_empty() {
        println!("Jellyfin and AniList agree.");
        return Ok(());
    }
    for row in &rows {
        let direction = match row.direction {
            SyncDirection::Push => "-> AniList",
            SyncDirection::Pull => "-> Jellyfin",
            SyncDirection::Skip => "skip",
        };
        println!(
            "{:<45} Jellyfin {:>4}  AniList {:>4}  {direction}",
            row.media_title, row.jellyfin_progress, row.anilist_progress
        );
    }
    if dry_run {
        println!("Dry run, nothing changed.");
        return Ok(());
    }
    let report = core.apply_sync(&rows).await?;
    // Das Binary endet gleich: Auf die Outbox warten, damit Änderungen wirklich gesendet sind.
    for _ in 0..600 {
        if core.pending_writes().await?.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    println!(
        "{} pushed, {} episodes marked, {} conflicts, {} failed, {} writes still queued",
        report.pushed,
        report.pulled,
        report.conflicts,
        report.failed,
        core.pending_writes().await?.len()
    );
    Ok(())
}

fn core_options() -> CoreOptions {
    let mut options = CoreOptions::default();
    // Nur in Debug-Builds: Release-Builds senden das Token ausschließlich an AniList.
    #[cfg(debug_assertions)]
    {
        options.anilist_endpoint = std::env::var("MUSUBI_DEV_ANILIST_ENDPOINT").ok();
    }
    #[cfg(not(debug_assertions))]
    let _ = &mut options;
    options
}

fn run_gui(runtime: &tokio::runtime::Runtime, core: Core) -> Result<()> {
    let _enter = runtime.enter();
    musubi_gui::run(core.clone())?;
    runtime.block_on(async {
        let _ = core.player_command(PlayerCommand::Stop).await;
    });
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let paths = Paths::resolve().context("could not prepare the data directories")?;
    let _log_guard = init_logging(&paths);

    let runtime = tokio::runtime::Runtime::new()?;
    let core = runtime.block_on(Core::open(paths, core_options()))?;

    match cli.command {
        None | Some(Command::Gui) => run_gui(&runtime, core)?,
        Some(Command::Login {
            service,
            pin,
            api_key,
        }) => runtime.block_on(async {
            match service {
                ServiceArg::Jellyfin => login_jellyfin(&core, api_key).await,
                ServiceArg::Anilist => login_anilist(&core, pin).await,
            }
        })?,
        Some(Command::Logout { service }) => runtime.block_on(async {
            match service {
                ServiceArg::Jellyfin => core.logout_jellyfin().await,
                ServiceArg::Anilist => core.logout_anilist().await,
            }
        })?,
        Some(Command::Sync { dry_run }) => runtime.block_on(run_sync(&core, dry_run))?,
    }
    runtime.shutdown_timeout(std::time::Duration::from_secs(1));
    Ok(())
}
