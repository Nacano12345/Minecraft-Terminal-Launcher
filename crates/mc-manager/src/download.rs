use mc_core::{DownloadProgress, Error, Result};
use mc_plugin::MirrorTable;
use sha1::{Digest, Sha1};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone)]
pub struct DownloadItem {
    pub url: String,
    pub dest: PathBuf,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

impl DownloadItem {
    pub fn new(url: impl Into<String>, dest: impl Into<PathBuf>) -> Self {
        Self {
            url: url.into(),
            dest: dest.into(),
            sha1: None,
            size: None,
        }
    }

    pub fn with_sha1(mut self, sha1: Option<String>) -> Self {
        self.sha1 = sha1;
        self
    }

    pub fn with_size(mut self, size: Option<u64>) -> Self {
        self.size = size;
        self
    }
}

pub fn sha1_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

pub fn sha1_file(path: &Path) -> Result<Option<String>> {
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = std::fs::read(path)?;
    Ok(Some(sha1_bytes(&bytes)))
}

pub async fn fetch_text(client: &reqwest::Client, url: &str) -> Result<String> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| Error::Http(e.to_string()))?;
    let resp = resp
        .error_for_status()
        .map_err(|e| Error::Http(e.to_string()))?;
    resp.text().await.map_err(|e| Error::Http(e.to_string()))
}

/// Where download progress is reported (the TUI subscribes to this).
pub type ProgressSink = tokio::sync::mpsc::UnboundedSender<DownloadProgress>;

/// Download every item, skipping files whose SHA-1 already matches.
///
/// Each item is tried against the primary URL first, then once against a
/// mirror (when a provider matches). Returns the number of files written.
pub async fn fetch_all(
    client: &reqwest::Client,
    items: Vec<DownloadItem>,
    concurrency: usize,
    mirrors: &MirrorTable,
    label: &str,
    progress: Option<&ProgressSink>,
) -> Result<usize> {
    use futures::stream::{self, StreamExt};

    let total = items.len();
    if total == 0 {
        return Ok(0);
    }
    tracing::info!(total, concurrency, label, "downloading batch");

    let mut stream = stream::iter(items)
        .map(|item| async move { ensure_one(client, item, mirrors).await })
        .buffer_unordered(concurrency.max(1));

    let mut done = 0u64;
    let mut written = 0usize;
    while let Some(result) = stream.next().await {
        done += 1;
        if result? {
            written += 1;
        }
        if let Some(sink) = progress {
            let _ = sink.send(DownloadProgress {
                label: label.to_string(),
                done,
                total: total as u64,
            });
        }
    }

    tracing::info!(total, written, label, "download batch complete");
    Ok(written)
}

async fn ensure_one(
    client: &reqwest::Client,
    item: DownloadItem,
    mirrors: &MirrorTable,
) -> Result<bool> {
    // Fast path: an exact size match means we already have the file. This
    // avoids re-hashing hundreds of megabytes of assets on every launch.
    if let Some(size) = item.size {
        if std::fs::metadata(&item.dest)
            .map(|meta| meta.len() == size)
            .unwrap_or(false)
        {
            return Ok(false);
        }
    }

    if let Some(expected) = &item.sha1 {
        if let Some(actual) = sha1_file(&item.dest)? {
            if actual.eq_ignore_ascii_case(expected) {
                return Ok(false);
            }
        }
    } else if item.dest.is_file() {
        return Ok(false);
    }

    let mut candidates = vec![item.url.clone()];
    if let Some(mirrored) = mirrors.rewrite(&item.url) {
        if mirrored != item.url {
            candidates.push(mirrored);
        }
    }

    let mut last_error = None;
    for url in candidates {
        for attempt in 0..3u32 {
            match download_to(client, &url, &item.dest).await {
                Ok(bytes) => {
                    if let Some(expected) = &item.sha1 {
                        let actual = sha1_bytes(&bytes);
                        if !actual.eq_ignore_ascii_case(expected) {
                            last_error = Some(Error::HashMismatch {
                                path: item.dest.clone(),
                                expected: expected.clone(),
                                actual,
                            });
                            // Corrupt payload: no point retrying the same source.
                            break;
                        }
                    }
                    return Ok(true);
                }
                Err(err) => {
                    tracing::debug!(%url, attempt, %err, "download attempt failed");
                    last_error = Some(err);
                    if attempt < 2 {
                        tokio::time::sleep(std::time::Duration::from_millis(
                            250 * u64::from(attempt + 1),
                        ))
                        .await;
                    }
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| Error::Other(format!("download failed: {}", item.url))))
}

async fn download_to(client: &reqwest::Client, url: &str, dest: &Path) -> Result<Vec<u8>> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| Error::Http(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(Error::Http(format!("{url} -> {}", resp.status())));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| Error::Http(e.to_string()))?
        .to_vec();

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let tmp = dest.with_extension(format!(
        "{}.part",
        dest.extension().and_then(|e| e.to_str()).unwrap_or("bin")
    ));
    {
        let mut file = tokio::fs::File::create(&tmp).await?;
        file.write_all(&bytes).await?;
        file.flush().await?;
    }
    tokio::fs::rename(&tmp, dest).await?;
    Ok(bytes)
}
