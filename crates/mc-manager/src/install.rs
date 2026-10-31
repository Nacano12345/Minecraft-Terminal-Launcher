use crate::download::{self, DownloadItem};
use crate::paths::DataRoot;
use crate::rules;
use crate::version::{artifact_path, VersionFile};
use mc_core::{Error, Result};
use mc_plugin::MirrorTable;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct InstallOptions {
    /// Also download the asset index and every asset object.
    pub assets: bool,
    pub concurrency: usize,
}

impl Default for InstallOptions {
    fn default() -> Self {
        Self {
            assets: true,
            concurrency: 16,
        }
    }
}

#[derive(Debug, Default)]
pub struct InstallReport {
    pub files: usize,
    pub assets: usize,
}

/// Idempotent: only missing or corrupt files are downloaded.
pub async fn install(
    client: &reqwest::Client,
    root: &DataRoot,
    version: &VersionFile,
    mirrors: &MirrorTable,
    opts: &InstallOptions,
    progress: Option<&download::ProgressSink>,
) -> Result<InstallReport> {
    root.ensure()?;
    std::fs::create_dir_all(root.version_dir(&version.id))?;

    let mut items = Vec::new();

    if let Some(artifact) = &version.downloads.client {
        items.push(
            DownloadItem::new(artifact.url.clone(), version.client_jar(root))
                .with_sha1(artifact.sha1.clone())
                .with_size(artifact.size),
        );
    }

    for lib in &version.libraries {
        if !rules::allowed(lib.rules.as_ref()) {
            continue;
        }
        let Some(downloads) = &lib.downloads else {
            continue;
        };

        if let Some(artifact) = &downloads.artifact {
            let rel = artifact_path(lib, artifact);
            items.push(
                DownloadItem::new(artifact.url.clone(), root.libraries().join(rel))
                    .with_sha1(artifact.sha1.clone())
                    .with_size(artifact.size),
            );
        }

        if let (Some(natives), Some(classifiers)) = (&lib.natives, &downloads.classifiers) {
            if let Some(classifier) = natives.get(rules::os_name()) {
                let classifier = classifier.replace("${arch}", rules::os_arch_bits());
                if let Some(artifact) = classifiers.get(&classifier) {
                    let rel = artifact_path(lib, artifact);
                    items.push(
                        DownloadItem::new(artifact.url.clone(), root.libraries().join(rel))
                            .with_sha1(artifact.sha1.clone())
                            .with_size(artifact.size),
                    );
                }
            }
        }
    }

    let files =
        download::fetch_all(client, items, opts.concurrency, mirrors, "files", progress).await?;
    extract_natives(root, version)?;

    let assets = if opts.assets {
        install_assets(client, root, version, mirrors, opts.concurrency, progress).await?
    } else {
        0
    };

    install_logging(client, root, version, mirrors, progress).await?;

    Ok(InstallReport { files, assets })
}

pub fn natives_dir(root: &DataRoot, version: &VersionFile) -> PathBuf {
    root.version_dir(&version.id).join("natives")
}

/// Older versions ship natives as classifiers that must be unpacked next to
/// the game. Modern LWJGL3 versions load them from the classpath jars, so this
/// is a no-op for them.
fn extract_natives(root: &DataRoot, version: &VersionFile) -> Result<()> {
    let dest = natives_dir(root, version);
    std::fs::create_dir_all(&dest)?;

    for lib in &version.libraries {
        if !rules::allowed(lib.rules.as_ref()) {
            continue;
        }
        let (Some(natives), Some(downloads)) = (&lib.natives, &lib.downloads) else {
            continue;
        };
        let Some(classifier) = natives.get(rules::os_name()) else {
            continue;
        };
        let classifier = classifier.replace("${arch}", rules::os_arch_bits());
        let Some(artifact) = downloads
            .classifiers
            .as_ref()
            .and_then(|classifiers| classifiers.get(&classifier))
        else {
            continue;
        };
        let jar = root.libraries().join(artifact_path(lib, artifact));
        if !jar.is_file() {
            continue;
        }
        let excludes = lib
            .extract
            .as_ref()
            .map(|e| e.exclude.clone())
            .unwrap_or_default();
        if let Err(err) = extract_jar(&jar, &dest, &excludes) {
            tracing::warn!(%err, jar = %jar.display(), "failed to extract natives");
        }
    }
    Ok(())
}

fn extract_jar(jar: &Path, dest: &Path, excludes: &[String]) -> Result<()> {
    let file = std::fs::File::open(jar)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| Error::Other(format!("zip {}: {e}", jar.display())))?;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| Error::Other(format!("zip entry: {e}")))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().replace('\\', "/");
        if name.contains("..") {
            continue;
        }
        if excludes
            .iter()
            .any(|ex| name.starts_with(ex.trim_start_matches('/')))
        {
            continue;
        }
        let out = dest.join(&name);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut target = std::fs::File::create(&out)?;
        std::io::copy(&mut entry, &mut target)?;
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct AssetIndex {
    #[serde(default)]
    objects: HashMap<String, AssetObject>,
}

#[derive(Debug, Deserialize)]
struct AssetObject {
    hash: String,
    #[serde(default)]
    size: u64,
}

async fn install_assets(
    client: &reqwest::Client,
    root: &DataRoot,
    version: &VersionFile,
    mirrors: &MirrorTable,
    concurrency: usize,
    progress: Option<&download::ProgressSink>,
) -> Result<usize> {
    let Some(asset_index) = &version.asset_index else {
        return Ok(0);
    };

    let index_path = root
        .assets_indexes()
        .join(format!("{}.json", asset_index.id));
    let cached = std::fs::read_to_string(&index_path).ok();
    let text = match cached {
        Some(text) if serde_json::from_str::<AssetIndex>(&text).is_ok() => text,
        _ => {
            let text = download::fetch_text(client, &asset_index.url).await?;
            std::fs::create_dir_all(root.assets_indexes())?;
            std::fs::write(&index_path, &text)?;
            text
        }
    };

    let index: AssetIndex = serde_json::from_str(&text)?;
    let items = index
        .objects
        .values()
        .filter(|object| object.hash.len() >= 2)
        .map(|object| {
            let prefix = &object.hash[..2];
            DownloadItem::new(
                format!(
                    "https://resources.download.minecraft.net/{prefix}/{}",
                    object.hash
                ),
                root.assets_objects().join(prefix).join(&object.hash),
            )
            .with_sha1(Some(object.hash.clone()))
            .with_size(Some(object.size))
        })
        .collect::<Vec<_>>();

    let written =
        download::fetch_all(client, items, concurrency, mirrors, "assets", progress).await?;

    if matches!(version.assets.as_deref(), Some("legacy") | Some("pre-1.6")) {
        build_legacy_virtual(root, &index)?;
    }

    Ok(written)
}

/// `legacy` / `pre-1.6` versions expect a flat `assets/virtual/legacy` tree.
fn build_legacy_virtual(root: &DataRoot, index: &AssetIndex) -> Result<()> {
    let virtual_root = root.assets().join("virtual/legacy");
    std::fs::create_dir_all(&virtual_root)?;
    for (name, object) in &index.objects {
        if object.hash.len() < 2 {
            continue;
        }
        let source = root
            .assets_objects()
            .join(&object.hash[..2])
            .join(&object.hash);
        if !source.is_file() {
            continue;
        }
        let target = virtual_root.join(name);
        if target.exists() {
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if std::fs::hard_link(&source, &target).is_err() {
            std::fs::copy(&source, &target)?;
        }
    }
    Ok(())
}

/// Mojang's console appender emits XML (`LegacyXMLLayout`). We rewrite it to a
/// plain pattern so the wrapper can parse one event per line.
const PLAIN_CONSOLE_APPENDER: &str =
    r#"<PatternLayout pattern="[%d{HH:mm:ss}] [%t/%level]: %msg{nolookups}%n" />"#;

fn log_config_name(version: &VersionFile) -> String {
    version
        .logging
        .as_ref()
        .and_then(|logging| logging.client.as_ref())
        .and_then(|client| client.file.path.clone())
        .unwrap_or_else(|| format!("{}.xml", version.id))
}

pub fn original_log_config(root: &DataRoot, version: &VersionFile) -> Option<PathBuf> {
    version.logging.as_ref()?.client.as_ref()?;
    Some(
        root.assets()
            .join("log_configs")
            .join(log_config_name(version)),
    )
}

pub fn patched_log_config(root: &DataRoot, version: &VersionFile) -> PathBuf {
    root.assets()
        .join("log_configs")
        .join(format!("{}-qcl.xml", version.id))
}

/// The config actually handed to the JVM: patched when available.
pub fn client_log_config(root: &DataRoot, version: &VersionFile) -> Option<PathBuf> {
    let original = original_log_config(root, version)?;
    let patched = patched_log_config(root, version);
    if patched.is_file() {
        Some(patched)
    } else {
        Some(original)
    }
}

fn plain_console_config(text: &str) -> Option<String> {
    let mut patched = text.to_string();
    for variant in [
        "<LegacyXMLLayout />",
        "<LegacyXMLLayout/>",
        "<XMLLayout />",
        "<XMLLayout/>",
    ] {
        patched = patched.replace(variant, PLAIN_CONSOLE_APPENDER);
    }
    (patched != text).then_some(patched)
}

async fn install_logging(
    client: &reqwest::Client,
    root: &DataRoot,
    version: &VersionFile,
    mirrors: &MirrorTable,
    progress: Option<&download::ProgressSink>,
) -> Result<()> {
    let Some(logging) = version.logging.as_ref().and_then(|l| l.client.as_ref()) else {
        return Ok(());
    };
    let dest = original_log_config(root, version).unwrap_or_else(|| {
        root.assets()
            .join("log_configs")
            .join(format!("{}.xml", version.id))
    });
    let item = DownloadItem::new(logging.file.url.clone(), dest.clone())
        .with_sha1(logging.file.sha1.clone())
        .with_size(logging.file.size);
    download::fetch_all(client, vec![item], 1, mirrors, "logging", progress).await?;

    if let Ok(text) = std::fs::read_to_string(&dest) {
        if let Some(patched) = plain_console_config(&text) {
            std::fs::create_dir_all(root.assets().join("log_configs"))?;
            std::fs::write(patched_log_config(root, version), patched)?;
        }
    }
    Ok(())
}
