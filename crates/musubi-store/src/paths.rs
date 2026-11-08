//! XDG-konforme Pfade.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;

#[derive(Debug, Clone)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub state_dir: PathBuf,
    pub runtime_dir: PathBuf,
}

impl Paths {
    /// Ermittelt die Standardpfade anhand der XDG-Variablen.
    pub fn resolve() -> std::io::Result<Self> {
        let dirs = ProjectDirs::from("", "", "musubi")
            .ok_or_else(|| std::io::Error::other("could not determine the home directory"))?;
        let state_dir = dirs
            .state_dir()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| dirs.data_local_dir().join("state"));
        let runtime_dir = dirs
            .runtime_dir()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| dirs.cache_dir().join("run"));
        let paths = Self {
            config_dir: dirs.config_dir().to_path_buf(),
            data_dir: dirs.data_dir().to_path_buf(),
            cache_dir: dirs.cache_dir().to_path_buf(),
            state_dir,
            runtime_dir,
        };
        paths.ensure()?;
        Ok(paths)
    }

    /// Alle Pfade unterhalb eines Wurzelverzeichnisses (für Tests).
    pub fn under(root: &Path) -> std::io::Result<Self> {
        let paths = Self {
            config_dir: root.join("config"),
            data_dir: root.join("data"),
            cache_dir: root.join("cache"),
            state_dir: root.join("state"),
            runtime_dir: root.join("run"),
        };
        paths.ensure()?;
        Ok(paths)
    }

    fn ensure(&self) -> std::io::Result<()> {
        for dir in [&self.config_dir, &self.data_dir, &self.runtime_dir] {
            std::fs::create_dir_all(dir)?;
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        }
        std::fs::create_dir_all(self.images_dir())?;
        std::fs::create_dir_all(&self.state_dir)?;
        Ok(())
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    pub fn credentials_file(&self) -> PathBuf {
        self.config_dir.join("credentials.toml")
    }

    pub fn database_file(&self) -> PathBuf {
        self.data_dir.join("musubi.db")
    }

    pub fn images_dir(&self) -> PathBuf {
        self.cache_dir.join("images")
    }
}
