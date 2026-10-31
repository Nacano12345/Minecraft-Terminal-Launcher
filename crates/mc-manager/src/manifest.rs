use mc_core::{Error, Result, VersionSummary};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub latest: Latest,
    pub versions: Vec<VersionSummary>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Latest {
    pub release: String,
    pub snapshot: String,
}

impl Manifest {
    pub fn find(&self, id: &str) -> Option<&VersionSummary> {
        self.versions.iter().find(|v| v.id == id)
    }

    pub fn latest_release(&self) -> &str {
        &self.latest.release
    }
}

/// Fetch the version manifest, falling back to the on-disk cache when offline.
pub async fn fetch(client: &reqwest::Client, url: &str, cache: &Path) -> Result<Manifest> {
    let request = client.get(url).send();
    let response = match tokio::time::timeout(std::time::Duration::from_secs(10), request).await {
        Ok(result) => result,
        Err(_) => {
            tracing::warn!("manifest request timed out, using cache");
            return from_cache(cache, url);
        }
    };

    match response {
        Ok(response) => {
            if response.status().is_success() {
                let text = response
                    .text()
                    .await
                    .map_err(|e| Error::Http(e.to_string()))?;
                if let Some(parent) = cache.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                if let Err(err) = std::fs::write(cache, &text) {
                    tracing::warn!(%err, "could not write manifest cache");
                }
                return Ok(serde_json::from_str(&text)?);
            }
            tracing::warn!(status = %response.status(), "manifest request failed, using cache");
        }
        Err(err) => {
            tracing::warn!(%err, "manifest request failed, using cache");
        }
    }

    from_cache(cache, url)
}

fn from_cache(cache: &Path, url: &str) -> Result<Manifest> {
    tracing::warn!(path = %cache.display(), "using cached version manifest");
    let text = std::fs::read_to_string(cache)
        .map_err(|_| Error::Http(format!("failed to fetch manifest from {url}")))?;
    Ok(serde_json::from_str(&text)?)
}
