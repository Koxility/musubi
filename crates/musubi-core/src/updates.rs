//! Looks for a newer release on GitHub and installs it the way this copy was installed.
//!
//! A copy that `install.sh` put into a folder the user owns is replaced from the release
//! archive. A copy that pacman owns is rebuilt from the project's PKGBUILD in a terminal,
//! because that needs the user's password. Anything else is pointed at the release page.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::Deserialize;
use tokio::process::Command;

use crate::{Core, CoreError, Result};

const LATEST_RELEASE: &str = "https://api.github.com/repos/Koxility/musubi/releases/latest";
const PKGBUILDS: &str = "https://raw.githubusercontent.com/Koxility/musubi/master/packaging/aur";
const RELEASE_PAGES: &str = "https://github.com/Koxility/musubi/releases/tag";

/// A release version such as `0.5.0`. Ordered, so a newer one compares greater.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    major: u32,
    minor: u32,
    patch: u32,
}

impl Version {
    /// Reads `0.5.0` or the tag form `v0.5.0`. Anything else, pre-releases included, is `None`.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let mut parts = text.strip_prefix('v').unwrap_or(text).split('.');
        let mut next = || parts.next()?.parse::<u32>().ok();
        let version = Self {
            major: next()?,
            minor: next()?,
            patch: next()?,
        };
        parts.next().is_none().then_some(version)
    }

    /// The version of this build.
    pub fn current() -> Self {
        Self::parse(env!("CARGO_PKG_VERSION"))
            .expect("the workspace version is three numbers separated by dots")
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// How the running copy got onto this machine, which decides how it is updated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallKind {
    /// Owned by this pacman package, `musubi` or `musubi-bin`.
    Pacman { package: String },
    /// Put below this prefix by `install.sh`, in a place the user may write to.
    Script { prefix: PathBuf },
    /// Built with cargo, run from a checkout, or otherwise unknown.
    Other,
}

/// A newer version than the running one, and what is needed to install it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Update {
    pub version: Version,
    /// The release's page on GitHub.
    pub page: String,
    pub install: InstallKind,
    archive: Option<Asset>,
    pkgbuild: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Asset {
    url: String,
    checksum_url: String,
}

/// What pressing "Update" led to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateOutcome {
    /// The new version is in place; it runs after a restart.
    Installed,
    /// A terminal opened and builds the package there.
    Started,
    /// Nothing was changed. The user updates by hand, with `command` if there is one.
    Manual {
        command: Option<String>,
        page: String,
    },
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    assets: Vec<ReleaseAsset>,
}

#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}

fn failed(message: impl Into<String>) -> CoreError {
    CoreError::Update(message.into())
}

/// The prefix of an `install.sh` installation, if the binary sits at `<prefix>/bin/musubi`.
fn script_prefix(exe: &Path) -> Option<&Path> {
    let bin = exe.parent()?;
    (exe.file_name()? == "musubi" && bin.file_name()? == "bin")
        .then(|| bin.parent())
        .flatten()
}

/// The `pkgver` line of a PKGBUILD.
fn pkgbuild_version(pkgbuild: &str) -> Option<Version> {
    pkgbuild
        .lines()
        .find_map(|line| line.strip_prefix("pkgver="))
        .and_then(Version::parse)
}

fn archive_asset(release: &Release, version: Version) -> Option<Asset> {
    let name = format!("musubi-{version}-x86_64-linux.tar.gz");
    let url_of = |name: &str| {
        release
            .assets
            .iter()
            .find(|asset| asset.name == name)
            .map(|asset| asset.browser_download_url.clone())
    };
    Some(Asset {
        url: url_of(&name)?,
        checksum_url: url_of(&format!("{name}.sha256"))?,
    })
}

/// Runs in the terminal for a pacman-owned copy. It keeps the window open at the end, so
/// the result can be read whichever terminal it is.
fn pacman_script(package: &str, version: Version, build_dir: &Path) -> String {
    format!(
        r#"#!/bin/sh
# Written by Musubi: updates the {package} package through the project's PKGBUILD.
finish() {{
    status=$?
    echo
    if [ "$status" -eq 0 ]; then
        echo "Musubi {version} is installed. Restart Musubi to use it."
    else
        echo "The update did not finish. Nothing was changed if no package was installed."
    fi
    printf 'Press Enter to close this window. '
    read -r _ || true
}}
trap finish EXIT
set -eu
cd '{dir}'
echo "Updating {package} to {version} with makepkg."
makepkg -si
"#,
        dir = build_dir.display().to_string().replace('\'', r"'\''"),
    )
}

/// The command line that runs `script` in a terminal window, for the first terminal that
/// `installed` knows. `preferred` is the user's `$TERMINAL`.
fn terminal_command(
    preferred: Option<&str>,
    installed: impl Fn(&str) -> bool,
    script: &Path,
) -> Option<(String, Vec<String>)> {
    // How each terminal takes a command to run: the arguments before `sh <script>`.
    const KNOWN: [(&str, &[&str]); 12] = [
        ("xdg-terminal-exec", &[]),
        ("kitty", &[]),
        ("foot", &[]),
        ("alacritty", &["-e"]),
        ("ghostty", &["-e"]),
        ("wezterm", &["start", "--"]),
        ("konsole", &["-e"]),
        ("gnome-terminal", &["--"]),
        ("ptyxis", &["--"]),
        ("kgx", &["-e"]),
        ("xfce4-terminal", &["-x"]),
        ("xterm", &["-e"]),
    ];
    let preferred = preferred
        .map(str::trim)
        .filter(|name| !name.is_empty() && installed(name))
        .map(|name| {
            let lead = KNOWN
                .iter()
                .find(|(known, _)| Path::new(name).file_name().is_some_and(|f| f == *known))
                .map_or(&["-e"][..], |(_, lead)| *lead);
            (name, lead)
        });
    let (program, lead) = preferred.or_else(|| {
        KNOWN
            .iter()
            .find(|(name, _)| installed(name))
            .map(|(name, lead)| (*name, *lead))
    })?;
    let mut args: Vec<String> = lead.iter().map(|arg| (*arg).to_owned()).collect();
    args.push("sh".to_owned());
    args.push(script.display().to_string());
    Some((program.to_owned(), args))
}

/// Whether `program` can be started: a path to a file, or a name found on `PATH`.
fn on_path(program: &str) -> bool {
    if program.contains('/') {
        return Path::new(program).is_file();
    }
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(program).is_file()))
}

/// The pacman package that owns `exe`, if pacman is there and knows the file.
async fn pacman_owner(exe: &Path) -> Option<String> {
    let output = Command::new("pacman")
        .arg("-Qqo")
        .arg(exe)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .await
        .ok()
        .filter(|output| output.status.success())?;
    let package = String::from_utf8(output.stdout).ok()?;
    let package = package.lines().next()?.trim();
    (!package.is_empty()).then(|| package.to_owned())
}

/// Whether the current user may replace files below `prefix/bin`.
async fn writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".musubi-{}", uuid::Uuid::new_v4().simple()));
    match tokio::fs::write(&probe, b"").await {
        Ok(()) => {
            let _ = tokio::fs::remove_file(&probe).await;
            true
        }
        Err(_) => false,
    }
}

async fn detect_install() -> InstallKind {
    let Ok(exe) = std::env::current_exe().and_then(|exe| exe.canonicalize()) else {
        return InstallKind::Other;
    };
    if let Some(package) = pacman_owner(&exe).await {
        return InstallKind::Pacman { package };
    }
    match script_prefix(&exe) {
        // The launcher entry is what tells an install.sh prefix from, say, ~/.cargo.
        Some(prefix)
            if prefix.join("share/applications/musubi.desktop").is_file()
                && writable(&prefix.join("bin")).await =>
        {
            InstallKind::Script {
                prefix: prefix.to_path_buf(),
            }
        }
        _ => InstallKind::Other,
    }
}

/// Runs a helper program and turns a failure into an error that names it.
async fn run(command: &mut Command, what: &str) -> Result<String> {
    let output = command.stdin(Stdio::null()).output().await?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let detail = String::from_utf8_lossy(&output.stderr);
    let detail = detail.lines().last().unwrap_or_default().trim();
    Err(failed(if detail.is_empty() {
        format!("{what} failed")
    } else {
        format!("{what} failed: {detail}")
    }))
}

impl Core {
    fn latest_release_url(&self) -> String {
        match &self.inner.update_endpoint {
            Some(base) => format!("{base}/releases/latest"),
            None => LATEST_RELEASE.to_owned(),
        }
    }

    fn pkgbuild_url(&self, package: &str) -> String {
        let base = match &self.inner.update_endpoint {
            Some(base) => format!("{base}/aur"),
            None => PKGBUILDS.to_owned(),
        };
        format!("{base}/{package}/PKGBUILD")
    }

    async fn install_kind(&self) -> InstallKind {
        match &self.inner.install_kind {
            Some(kind) => kind.clone(),
            None => detect_install().await,
        }
    }

    async fn download_text(&self, url: &str) -> Result<String> {
        let response = self.inner.http.get(url).send().await?.error_for_status()?;
        Ok(response.text().await?)
    }

    /// Asks GitHub for the latest release. `Some` if it is newer than this build and can be
    /// installed here: a pacman-owned copy is only offered a version once the project's
    /// PKGBUILD for its package has reached it.
    pub async fn check_for_update(&self) -> Result<Option<Update>> {
        let release: Release = self
            .inner
            .http
            .get(self.latest_release_url())
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let current = Version::current();
        let Some(latest) = Version::parse(&release.tag_name).filter(|latest| *latest > current)
        else {
            return Ok(None);
        };
        let install = self.install_kind().await;
        let mut update = Update {
            version: latest,
            page: if release.html_url.is_empty() {
                format!("{RELEASE_PAGES}/v{latest}")
            } else {
                release.html_url.clone()
            },
            archive: archive_asset(&release, latest),
            pkgbuild: None,
            install,
        };
        if let InstallKind::Pacman { package } = &update.install {
            let pkgbuild = self.download_text(&self.pkgbuild_url(package)).await?;
            let Some(packaged) = pkgbuild_version(&pkgbuild).filter(|v| *v > current) else {
                return Ok(None);
            };
            if packaged != latest {
                update.version = packaged;
                update.page = format!("{RELEASE_PAGES}/v{packaged}");
            }
            update.pkgbuild = Some(pkgbuild);
        }
        Ok(Some(update))
    }

    /// Installs `update` the way this copy was installed, see [`UpdateOutcome`].
    pub async fn install_update(&self, update: &Update) -> Result<UpdateOutcome> {
        match &update.install {
            InstallKind::Script { prefix } => {
                self.install_archive(update, prefix).await?;
                Ok(UpdateOutcome::Installed)
            }
            InstallKind::Pacman { package } => self.start_makepkg(update, package).await,
            InstallKind::Other => Ok(UpdateOutcome::Manual {
                command: None,
                page: update.page.clone(),
            }),
        }
    }

    /// An empty directory for the files of one update.
    async fn update_dir(&self) -> Result<PathBuf> {
        let dir = self.inner.paths.cache_dir.join("update");
        match tokio::fs::remove_dir_all(&dir).await {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
            _ => {}
        }
        tokio::fs::create_dir_all(&dir).await?;
        Ok(dir)
    }

    /// Downloads the release archive, checks it against its published checksum, and lets
    /// the `install.sh` inside replace the installation below `prefix`.
    async fn install_archive(&self, update: &Update, prefix: &Path) -> Result<()> {
        let asset = update
            .archive
            .as_ref()
            .ok_or_else(|| failed("the release has no archive for this system"))?;
        let dir = self.update_dir().await?;
        let archive = dir.join("musubi.tar.gz");
        let http = &self.inner.http;
        let bytes = http.get(&asset.url).send().await?.error_for_status()?;
        tokio::fs::write(&archive, bytes.bytes().await?).await?;

        let published = self.download_text(&asset.checksum_url).await?;
        let expected = published.split_whitespace().next().unwrap_or_default();
        let computed = run(Command::new("sha256sum").arg(&archive), "sha256sum").await?;
        let computed = computed.split_whitespace().next().unwrap_or_default();
        if expected.is_empty() || !expected.eq_ignore_ascii_case(computed) {
            let _ = tokio::fs::remove_dir_all(&dir).await;
            return Err(failed("the downloaded archive does not match its checksum"));
        }

        run(
            Command::new("tar")
                .arg("-xzf")
                .arg(&archive)
                .arg("-C")
                .arg(&dir),
            "unpacking the archive",
        )
        .await?;
        let installer = dir
            .join(format!("musubi-{}-x86_64-linux", update.version))
            .join("install.sh");
        run(
            Command::new("sh")
                .arg(&installer)
                .arg("--prefix")
                .arg(prefix),
            "install.sh",
        )
        .await?;
        let _ = tokio::fs::remove_dir_all(&dir).await;
        Ok(())
    }

    /// Puts the PKGBUILD that the check read into a build directory and runs `makepkg -si`
    /// on it in a terminal, where sudo can ask for the password.
    async fn start_makepkg(&self, update: &Update, package: &str) -> Result<UpdateOutcome> {
        let pkgbuild = update
            .pkgbuild
            .as_ref()
            .ok_or_else(|| failed("the PKGBUILD of the update is missing"))?;
        let dir = self.update_dir().await?;
        let build_dir = dir.join(package);
        tokio::fs::create_dir_all(&build_dir).await?;
        tokio::fs::write(build_dir.join("PKGBUILD"), pkgbuild).await?;
        let script = dir.join("update.sh");
        tokio::fs::write(&script, pacman_script(package, update.version, &build_dir)).await?;

        let preferred = std::env::var("TERMINAL").ok();
        let manual = || UpdateOutcome::Manual {
            command: Some(format!("cd '{}' && makepkg -si", build_dir.display())),
            page: update.page.clone(),
        };
        let Some((program, args)) = terminal_command(preferred.as_deref(), on_path, &script) else {
            return Ok(manual());
        };
        let started = Command::new(&program)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        Ok(match started {
            Ok(_) => UpdateOutcome::Started,
            Err(error) => {
                tracing::warn!(%program, %error, "could not open a terminal for the update");
                manual()
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_parse_and_order() {
        let v = |text| Version::parse(text).unwrap();
        assert_eq!(v("v0.5.0"), v("0.5.0"));
        assert_eq!(v(" 1.2.3\n").to_string(), "1.2.3");
        assert!(v("0.5.0") > v("0.4.12"));
        assert!(v("0.10.0") > v("0.9.9"));
        assert!(v("1.0.0") > v("0.99.99"));
        for bad in ["", "0.5", "0.5.0.1", "0.5.0-rc1", "v", "a.b.c"] {
            assert_eq!(Version::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn install_prefix_is_read_from_the_binary_path() {
        let prefix = |exe| script_prefix(Path::new(exe));
        assert_eq!(
            prefix("/home/me/.local/bin/musubi"),
            Some(Path::new("/home/me/.local"))
        );
        assert_eq!(
            prefix("/opt/my apps/bin/musubi"),
            Some(Path::new("/opt/my apps"))
        );
        assert_eq!(prefix("/home/me/src/musubi/target/debug/musubi"), None);
        assert_eq!(prefix("/home/me/.local/bin/other"), None);
    }

    #[test]
    fn pkgver_is_read_from_a_pkgbuild() {
        let pkgbuild = "# Maintainer: x\n\npkgname=musubi\npkgver=0.5.0\npkgrel=1\n";
        assert_eq!(pkgbuild_version(pkgbuild), Version::parse("0.5.0"));
        assert_eq!(pkgbuild_version("pkgname=musubi\n"), None);
    }

    #[test]
    fn terminal_is_chosen_with_its_way_to_run_a_command() {
        let script = Path::new("/tmp/u.sh");
        let only = |name: &'static str| move |candidate: &str| candidate == name;
        let run = |args: &[&str]| {
            let mut all: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
            all.extend(["sh".to_owned(), "/tmp/u.sh".to_owned()]);
            all
        };
        assert_eq!(
            terminal_command(None, only("konsole"), script),
            Some(("konsole".to_owned(), run(&["-e"])))
        );
        assert_eq!(
            terminal_command(None, only("gnome-terminal"), script),
            Some(("gnome-terminal".to_owned(), run(&["--"])))
        );
        // $TERMINAL wins over the list, also with a full path and when it is unknown.
        assert_eq!(
            terminal_command(Some("/usr/bin/kitty"), |_| true, script),
            Some(("/usr/bin/kitty".to_owned(), run(&[])))
        );
        assert_eq!(
            terminal_command(Some("st"), |_| true, script),
            Some(("st".to_owned(), run(&["-e"])))
        );
        // A $TERMINAL that is not installed is skipped.
        assert_eq!(
            terminal_command(Some("st"), only("xterm"), script),
            Some(("xterm".to_owned(), run(&["-e"])))
        );
        assert_eq!(terminal_command(None, |_| false, script), None);
    }

    #[test]
    fn pacman_script_quotes_its_directory() {
        let version = Version::parse("0.5.0").unwrap();
        let script = pacman_script("musubi-bin", version, Path::new("/home/o'neil/.cache/x"));
        assert!(script.contains(r"cd '/home/o'\''neil/.cache/x'"));
        assert!(script.contains("makepkg -si"));
    }
}
