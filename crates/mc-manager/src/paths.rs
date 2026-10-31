use mc_core::Result;
use std::path::{Path, PathBuf};

/// Layout of the QCL data directory.
///
/// ```text
/// <root>/
///   versions/<id>/<id>.json + <id>.jar + natives/
///   libraries/...
///   assets/{indexes,objects,virtual,log_configs}
///   instances/<id>/...
///   java/<vendor>/bin/java
///   plugins/<pack>/plugin.toml
///   cache/version_manifest_v2.json
/// ```
#[derive(Debug, Clone)]
pub struct DataRoot {
    pub root: PathBuf,
}

impl DataRoot {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn default_root() -> PathBuf {
        if let Some(p) = std::env::var_os("QCL_HOME") {
            return PathBuf::from(p);
        }
        #[cfg(windows)]
        {
            if let Some(appdata) = std::env::var_os("APPDATA") {
                return PathBuf::from(appdata).join("QCL");
            }
        }
        #[cfg(not(windows))]
        {
            if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
                return PathBuf::from(xdg).join("qcl");
            }
            if let Some(home) = std::env::var_os("HOME") {
                return PathBuf::from(home).join(".local/share/qcl");
            }
        }
        PathBuf::from(".qcl")
    }

    pub fn versions(&self) -> PathBuf {
        self.root.join("versions")
    }

    pub fn version_dir(&self, id: &str) -> PathBuf {
        self.versions().join(id)
    }

    pub fn libraries(&self) -> PathBuf {
        self.root.join("libraries")
    }

    pub fn assets(&self) -> PathBuf {
        self.root.join("assets")
    }

    pub fn assets_objects(&self) -> PathBuf {
        self.assets().join("objects")
    }

    pub fn assets_indexes(&self) -> PathBuf {
        self.assets().join("indexes")
    }

    pub fn instances(&self) -> PathBuf {
        self.root.join("instances")
    }

    pub fn java_dir(&self) -> PathBuf {
        self.root.join("java")
    }

    pub fn plugins(&self) -> PathBuf {
        self.root.join("plugins")
    }

    pub fn cache(&self) -> PathBuf {
        self.root.join("cache")
    }

    pub fn ensure(&self) -> Result<()> {
        for dir in [
            self.versions(),
            self.libraries(),
            self.assets_objects(),
            self.assets_indexes(),
            self.instances(),
            self.java_dir(),
            self.plugins(),
            self.cache(),
        ] {
            std::fs::create_dir_all(&dir)?;
        }
        Ok(())
    }

    pub fn manifest_cache(&self) -> PathBuf {
        self.cache().join("version_manifest_v2.json")
    }
}

impl Default for DataRoot {
    fn default() -> Self {
        Self::new(Self::default_root())
    }
}

pub fn exists(path: &Path) -> bool {
    path.is_file()
}
