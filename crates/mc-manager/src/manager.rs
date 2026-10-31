use crate::install::{self, InstallOptions, InstallReport};
use crate::launch::{self, LaunchOptions};
use crate::manifest::{self, Manifest};
use crate::paths::DataRoot;
use crate::version::{self, VersionFile};
use mc_core::{Error, LaunchPlan, Result, VersionSummary};
use mc_plugin::{MirrorTable, PluginRegistry};

pub const DEFAULT_MANIFEST: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Clone)]
pub struct Manager {
    pub root: DataRoot,
    pub client: reqwest::Client,
    pub plugins: PluginRegistry,
    pub mirrors: MirrorTable,
    pub manifest_url: String,
}

impl Manager {
    pub fn new(root: DataRoot, manifest_url: impl Into<String>) -> Result<Self> {
        root.ensure()?;
        let plugins = PluginRegistry::load(&root.root);
        let mirrors = plugins.mirror_table();
        let client = reqwest::Client::builder()
            .user_agent(concat!("QCL/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| Error::Http(e.to_string()))?;
        Ok(Self {
            root,
            client,
            plugins,
            mirrors,
            manifest_url: manifest_url.into(),
        })
    }

    pub async fn manifest(&self) -> Result<Manifest> {
        manifest::fetch(
            &self.client,
            &self.manifest_url,
            &self.root.manifest_cache(),
        )
        .await
    }

    pub async fn version(&self, id: &str) -> Result<VersionFile> {
        let manifest = self.manifest().await?;
        let summary = manifest
            .find(id)
            .ok_or_else(|| Error::VersionNotFound(id.to_string()))?;
        version::fetch(&self.client, summary, &self.root).await
    }

    /// Resolve a version directly from a manifest entry, skipping the manifest
    /// round-trip (the TUI already holds the list).
    pub async fn version_summary(&self, summary: &VersionSummary) -> Result<VersionFile> {
        version::fetch(&self.client, summary, &self.root).await
    }

    pub async fn install(
        &self,
        version: &VersionFile,
        opts: &InstallOptions,
        progress: Option<&crate::download::ProgressSink>,
    ) -> Result<InstallReport> {
        install::install(
            &self.client,
            &self.root,
            version,
            &self.mirrors,
            opts,
            progress,
        )
        .await
    }

    pub fn plan(
        &self,
        version: &VersionFile,
        instance_id: &str,
        java: std::path::PathBuf,
        profile: &crate::auth::Profile,
        opts: &LaunchOptions,
    ) -> Result<LaunchPlan> {
        let mut plan = launch::build(&self.root, version, instance_id, java, profile, opts)?;
        for hook in self.plugins.launch_hooks() {
            launch::apply_hook(&mut plan, hook);
        }
        Ok(plan)
    }

    /// Toggle a plugin and rebuild the mirror table used by downloads.
    pub fn set_plugin_enabled(&mut self, name: &str, enabled: bool) -> Result<()> {
        self.plugins
            .set_enabled(name, enabled)
            .map_err(|err| Error::Other(err.to_string()))?;
        self.mirrors = self.plugins.mirror_table();
        Ok(())
    }
}
