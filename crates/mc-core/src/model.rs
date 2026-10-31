use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// A fully resolved, ready-to-spawn launch description.
///
/// The manager builds it, the wrapper executes it. It is intentionally pure
/// data so it can be printed with `--dry-run`, logged, or handed to a plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchPlan {
    pub instance_id: String,
    pub java: PathBuf,
    pub working_dir: PathBuf,
    /// Complete JVM arguments, *including* `-cp <classpath>` when required.
    pub jvm_args: Vec<String>,
    pub main_class: String,
    pub game_args: Vec<String>,
    /// Kept for diagnostics / plugins; already reflected in `jvm_args`.
    pub classpath: Vec<PathBuf>,
    pub natives_dir: Option<PathBuf>,
    pub env: BTreeMap<String, String>,
}

impl LaunchPlan {
    pub fn classpath_string(&self) -> String {
        let sep = if cfg!(windows) { ";" } else { ":" };
        self.classpath
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(sep)
    }

    /// `[java, jvm_args..., main_class, game_args...]`
    pub fn command_parts(&self) -> Vec<String> {
        let mut parts = Vec::with_capacity(2 + self.jvm_args.len() + self.game_args.len());
        parts.push(self.java.to_string_lossy().into_owned());
        parts.extend(self.jvm_args.iter().cloned());
        parts.push(self.main_class.clone());
        parts.extend(self.game_args.iter().cloned());
        parts
    }

    /// Human readable single line, useful for logs and `--dry-run`.
    pub fn command_line(&self) -> String {
        self.command_parts()
            .iter()
            .map(|p| shell_quote(p))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn shell_quote(s: &str) -> String {
    if s.is_empty()
        || s.chars()
            .any(|c| c.is_whitespace() || "\"'$`\\".contains(c))
    {
        format!("'{}'", s.replace('\'', "'\\''"))
    } else {
        s.to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogSource {
    Game,
    Loader,
    Jvm,
    Wrapper,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogLine {
    /// Unix milliseconds.
    pub ts: i64,
    pub level: LogLevel,
    pub source: LogSource,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub label: String,
    pub done: u64,
    pub total: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessState {
    Starting,
    Running,
    Stopping,
    Exited,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashInfo {
    pub exit_code: Option<i32>,
    pub reason: String,
    pub hs_err: Option<PathBuf>,
    pub crash_report: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionSummary {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    pub time: String,
    #[serde(rename = "releaseTime")]
    pub release_time: String,
}

/// One isolated game installation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub version_id: String,
    pub memory_mb: u32,
    pub min_memory_mb: u32,
    #[serde(default)]
    pub java: Option<PathBuf>,
    #[serde(default)]
    pub jvm_args: Vec<String>,
    #[serde(default)]
    pub game_args: Vec<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
}

impl Instance {
    pub fn new(id: impl Into<String>, version_id: impl Into<String>) -> Self {
        let id = id.into();
        Self {
            name: id.clone(),
            id,
            version_id: version_id.into(),
            memory_mb: 2048,
            min_memory_mb: 512,
            java: None,
            jvm_args: Vec::new(),
            game_args: Vec::new(),
            width: None,
            height: None,
        }
    }
}
