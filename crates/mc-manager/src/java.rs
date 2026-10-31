use crate::paths::DataRoot;
use std::path::{Path, PathBuf};

/// Locate a usable JVM.
///
/// Order: explicit path → `QCL_JAVA` → `JAVA_HOME` → `PATH` → downloaded
/// runtimes under `<root>/java/*`.
pub fn find(explicit: Option<&Path>, root: &DataRoot) -> Option<PathBuf> {
    if let Some(path) = explicit {
        if path.is_file() {
            return Some(path.to_path_buf());
        }
    }
    if let Some(path) = std::env::var_os("QCL_JAVA") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Some(home) = std::env::var_os("JAVA_HOME") {
        let candidate = bin(Path::new(&home));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    if let Some(path) = search_path() {
        return Some(path);
    }
    if let Ok(entries) = std::fs::read_dir(root.java_dir()) {
        for entry in entries.flatten() {
            let candidate = bin(&entry.path());
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

pub fn bin(home: &Path) -> PathBuf {
    home.join("bin")
        .join(if cfg!(windows) { "java.exe" } else { "java" })
}

fn search_path() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    // PATH entries are already binary directories; do not append `bin`.
    std::env::split_paths(&path)
        .map(|dir| dir.join(if cfg!(windows) { "java.exe" } else { "java" }))
        .find(|candidate| candidate.is_file())
}

/// First line of `java -version`, for diagnostics.
pub async fn version_of(java: &Path) -> Option<String> {
    let output = tokio::process::Command::new(java)
        .arg("-version")
        .output()
        .await
        .ok()?;
    let mut text = String::from_utf8_lossy(&output.stderr).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    text.lines().next().map(|line| line.trim().to_string())
}

/// Parse the major version from a `java -version` line
/// (`1.8.0_392` -> 8, `17.0.1` -> 17).
pub fn major_from_line(line: &str) -> Option<u32> {
    let quoted = line.split('"').nth(1).unwrap_or(line);
    let mut parts = quoted.split('.');
    let first: u32 = parts.next()?.trim().parse().ok()?;
    if first == 1 {
        parts.next()?.parse().ok()
    } else {
        Some(first)
    }
}

pub async fn major_of(java: &Path) -> Option<u32> {
    major_from_line(&version_of(java).await?)
}
