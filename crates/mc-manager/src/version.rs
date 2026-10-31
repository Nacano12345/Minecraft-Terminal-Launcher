use crate::download::fetch_text;
use crate::paths::DataRoot;
use mc_core::{Error, Result, VersionSummary};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
pub struct VersionFile {
    pub id: String,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(rename = "mainClass")]
    pub main_class: String,
    #[serde(default)]
    pub assets: Option<String>,
    #[serde(rename = "assetIndex", default)]
    pub asset_index: Option<AssetIndexRef>,
    #[serde(default)]
    pub downloads: Downloads,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(default)]
    pub arguments: Option<serde_json::Value>,
    #[serde(rename = "minecraftArguments", default)]
    pub minecraft_arguments: Option<String>,
    #[serde(rename = "javaVersion", default)]
    pub java_version: Option<JavaVersion>,
    #[serde(default)]
    pub logging: Option<Logging>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Downloads {
    #[serde(default)]
    pub client: Option<Artifact>,
    #[serde(default)]
    pub server: Option<Artifact>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Artifact {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetIndexRef {
    pub id: String,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Library {
    pub name: String,
    #[serde(default)]
    pub downloads: Option<LibDownloads>,
    #[serde(default)]
    pub natives: Option<HashMap<String, String>>,
    #[serde(default)]
    pub rules: Option<Vec<Rule>>,
    #[serde(default)]
    pub extract: Option<Extract>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct LibDownloads {
    #[serde(default)]
    pub artifact: Option<Artifact>,
    #[serde(default)]
    pub classifiers: Option<HashMap<String, Artifact>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Rule {
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub os: Option<OsRule>,
    #[serde(default)]
    pub features: Option<HashMap<String, bool>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct OsRule {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Extract {
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct JavaVersion {
    #[serde(rename = "majorVersion", default)]
    pub major_version: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Logging {
    #[serde(default)]
    pub client: Option<LoggingClient>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingClient {
    pub argument: String,
    pub file: Artifact,
    #[serde(rename = "type", default)]
    pub kind: String,
}

impl VersionFile {
    pub fn client_jar(&self, root: &DataRoot) -> std::path::PathBuf {
        root.version_dir(&self.id).join(format!("{}.jar", self.id))
    }

    pub fn required_java(&self) -> u32 {
        self.java_version
            .as_ref()
            .map(|j| j.major_version)
            .filter(|v| *v > 0)
            .unwrap_or(8)
    }
}

/// Fetch a version JSON, preferring the local cache.
pub async fn fetch(
    client: &reqwest::Client,
    summary: &VersionSummary,
    root: &DataRoot,
) -> Result<VersionFile> {
    let dir = root.version_dir(&summary.id);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.json", summary.id));

    if let Ok(text) = std::fs::read_to_string(&path) {
        if let Ok(version) = serde_json::from_str::<VersionFile>(&text) {
            return Ok(version);
        }
    }

    let text = fetch_text(client, &summary.url).await?;
    std::fs::write(&path, &text)?;
    Ok(serde_json::from_str(&text)?)
}

/// Resolve a Maven coordinate (`group:artifact:version[:classifier]`) to a
/// repository relative path.
pub fn maven_path(name: &str) -> String {
    let parts: Vec<&str> = name.split(':').collect();
    let (group, artifact, version, classifier) = match parts.as_slice() {
        [g, a, v] => (*g, *a, *v, None),
        [g, a, v, c, ..] => (*g, *a, *v, Some(*c)),
        _ => return name.to_string(),
    };
    let file = match classifier {
        Some(c) => format!("{artifact}-{version}-{c}.jar"),
        None => format!("{artifact}-{version}.jar"),
    };
    format!("{}/{artifact}/{version}/{file}", group.replace('.', "/"))
}

pub fn artifact_path(lib: &Library, artifact: &Artifact) -> String {
    artifact
        .path
        .clone()
        .unwrap_or_else(|| maven_path(&lib.name))
}

pub fn missing(reason: impl Into<String>) -> Error {
    Error::Other(reason.into())
}
