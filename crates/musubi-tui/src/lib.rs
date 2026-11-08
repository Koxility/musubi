//! Terminal-Oberfläche: startet das Ink-Frontend und bedient es über einen privaten Socket.
//!
//! Das Frontend ist ein eigenes Programm in TypeScript (`tui/`) und erbt das Terminal. Der
//! Rust-Kern bleibt im Elternprozess, sodass Sync, Outbox und mpv unverändert laufen.

pub mod rpc;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use musubi_core::{Core, PlayerCommand};
use tokio::net::UnixListener;
use tokio::process::Command;

const FRONTEND: &str = "musubi-tui";
const FRONTEND_ENV: &str = "MUSUBI_TUI_BIN";
const SOCKET_ENV: &str = "MUSUBI_RPC_SOCKET";

fn find_in_path(name: &str, path: Option<std::ffi::OsString>) -> Option<PathBuf> {
    std::env::split_paths(&path?)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// Sucht das Frontend: Umgebungsvariable, neben dem laufenden Programm, dann im `PATH`.
fn find_frontend(
    override_path: Option<PathBuf>,
    executable: Option<&Path>,
    path: Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    override_path
        .filter(|p| p.is_file())
        .or_else(|| {
            let sibling = executable?.parent()?.join(FRONTEND);
            sibling.is_file().then_some(sibling)
        })
        .or_else(|| find_in_path(FRONTEND, path))
}

/// Startet die TUI und kehrt beim Beenden zurück.
pub async fn run(core: Core) -> Result<()> {
    let executable = std::env::current_exe().ok();
    let frontend = find_frontend(
        std::env::var_os(FRONTEND_ENV).map(PathBuf::from),
        executable.as_deref(),
        std::env::var_os("PATH"),
    )
    .with_context(|| {
        format!("could not find the `{FRONTEND}` program, build it with `bun run build` in tui/")
    })?;

    // Das Verzeichnis gehört nur dem Nutzer, damit niemand sonst den Socket erreicht.
    let dir = tempfile::Builder::new()
        .prefix("musubi-rpc-")
        .tempdir()
        .context("could not create the socket directory")?;
    let socket = dir.path().join("rpc.sock");
    let listener = UnixListener::bind(&socket).context("could not open the local socket")?;
    let mut child = Command::new(&frontend)
        .env(SOCKET_ENV, &socket)
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("could not start {}", frontend.display()))?;

    let server = {
        let core = core.clone();
        async move {
            let (stream, _) = listener.accept().await?;
            rpc::serve(core, stream).await
        }
    };
    let status = tokio::select! {
        status = child.wait() => status?,
        served = server => {
            served.context("connection to the terminal interface failed")?;
            child.wait().await?
        }
    };
    let _ = core.player_command(PlayerCommand::Stop).await;
    if !status.success() {
        bail!("the terminal interface exited with {status}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontend_lookup_prefers_override_then_sibling_then_path() {
        let dir = tempfile::tempdir().unwrap();
        let (bin, other) = (dir.path().join("bin"), dir.path().join("other"));
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        let sibling = bin.join(FRONTEND);
        let in_path = other.join(FRONTEND);
        let custom = dir.path().join("custom");
        for file in [&sibling, &in_path, &custom] {
            std::fs::write(file, "").unwrap();
        }
        let exe = bin.join("musubi");
        let path = Some(other.clone().into_os_string());

        assert_eq!(
            find_frontend(Some(custom.clone()), Some(&exe), path.clone()),
            Some(custom)
        );
        assert_eq!(
            find_frontend(None, Some(&exe), path.clone()),
            Some(sibling.clone())
        );
        std::fs::remove_file(&sibling).unwrap();
        assert_eq!(find_frontend(None, Some(&exe), path), Some(in_path));
        assert_eq!(find_frontend(None, Some(&exe), None), None);
    }
}
