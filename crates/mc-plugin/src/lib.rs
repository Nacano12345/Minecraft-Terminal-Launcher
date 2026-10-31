//! Plugin system: manifests, discovery, enable/disable state and L1 hooks.
//!
//! A plugin is a directory containing `plugin.toml` (or a standalone `.toml`)
//! under `<root>/plugins`. L1 plugins are declarative:
//!
//! * `[[providers]]` — download mirrors
//! * `[launch]` — extra JVM args, game args and environment variables
//!
//! Enable/disable state lives in `<root>/plugins/state.json`, not in the
//! plugin's own manifest. No code is executed in this layer; L2 (WASM) will
//! build on the same registry.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use url::Url;

pub const API_VERSION: u32 = 1;

fn default_api() -> u32 {
    API_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderPack {
    pub name: String,
    pub version: String,
    #[serde(default = "default_api")]
    pub api: u32,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub homepage: String,
    #[serde(default)]
    pub providers: Vec<Provider>,
    #[serde(default)]
    pub launch: Option<LaunchHook>,
    #[serde(default)]
    pub tui: Option<TuiConfig>,
}

/// Declarative TUI contributions (L1).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TuiConfig {
    /// Template shown after the status chip. Supports `{user}`, `{memory}`,
    /// `{selected}`, `{running}`, `{version}`, `{plugins}`.
    pub status: Option<String>,
    pub actions: Vec<TuiActionSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiActionSpec {
    pub label: String,
    /// Single alphanumeric key for a global shortcut.
    #[serde(default)]
    pub key: Option<String>,
    /// `message` | `url` | `command` | `launch` | `install` | `refresh` |
    /// `open-data-dir`
    pub action: String,
    #[serde(default)]
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    /// Only `mirror` is understood today; unknown kinds are ignored.
    pub kind: String,
    #[serde(default)]
    pub priority: i32,
    #[serde(default, rename = "match")]
    pub match_rule: MatchRule,
    pub replace: Replace,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MatchRule {
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub host_suffix: Option<String>,
    #[serde(default)]
    pub path_prefix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Replace {
    pub with: String,
    #[serde(default)]
    pub path_prefix: String,
}

/// Declarative launch hooks (L1).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LaunchHook {
    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

impl LaunchHook {
    pub fn is_empty(&self) -> bool {
        self.jvm_args.is_empty() && self.game_args.is_empty() && self.env.is_empty()
    }
}

/// One discovered plugin, valid or not.
#[derive(Debug, Clone, Serialize)]
pub struct PluginEntry {
    pub name: String,
    pub version: String,
    pub api: u32,
    pub description: String,
    pub author: String,
    pub homepage: String,
    pub path: PathBuf,
    pub enabled: bool,
    pub providers: Vec<Provider>,
    pub launch: Option<LaunchHook>,
    pub tui: Option<TuiConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl PluginEntry {
    pub fn mirror_count(&self) -> usize {
        self.providers
            .iter()
            .filter(|provider| provider.kind == "mirror")
            .count()
    }

    pub fn has_launch_hook(&self) -> bool {
        self.launch
            .as_ref()
            .map(|hook| !hook.is_empty())
            .unwrap_or(false)
    }

    pub fn tui_status(&self) -> Option<&str> {
        self.tui.as_ref()?.status.as_deref()
    }

    pub fn tui_actions(&self) -> &[TuiActionSpec] {
        self.tui
            .as_ref()
            .map(|tui| tui.actions.as_slice())
            .unwrap_or(&[])
    }

    pub fn capabilities(&self) -> String {
        let mut parts = Vec::new();
        let mirrors = self.mirror_count();
        if mirrors > 0 {
            parts.push(format!(
                "{mirrors} mirror{}",
                if mirrors == 1 { "" } else { "s" }
            ));
        }
        if self.has_launch_hook() {
            parts.push("launch".to_string());
        }
        if self.tui.is_some() {
            parts.push("tui".to_string());
        }
        if parts.is_empty() {
            "no hooks".to_string()
        } else {
            parts.join(" · ")
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PluginRegistry {
    pub entries: Vec<PluginEntry>,
    state_path: PathBuf,
    disabled: BTreeSet<String>,
}

impl PluginRegistry {
    /// Scan `<root>/plugins`, apply the persisted enable/disable state.
    pub fn load(root: &Path) -> Self {
        let dir = root.join("plugins");
        let state_path = dir.join("state.json");
        let state = load_state(&state_path);
        let disabled: BTreeSet<String> = state.disabled.into_iter().collect();

        let mut entries: Vec<PluginEntry> = Vec::new();
        let mut manifests = Vec::new();
        if dir.is_dir() {
            if let Ok(read) = std::fs::read_dir(&dir) {
                for item in read.flatten() {
                    let path = item.path();
                    if path.is_dir() {
                        let manifest = path.join("plugin.toml");
                        if manifest.is_file() {
                            manifests.push(manifest);
                        }
                    } else if path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
                        manifests.push(path);
                    }
                }
            }
        }
        manifests.sort();

        for manifest in manifests {
            let fallback = manifest
                .parent()
                .and_then(|parent| parent.file_name())
                .and_then(|name| name.to_str())
                .or_else(|| manifest.file_stem().and_then(|name| name.to_str()))
                .unwrap_or("plugin")
                .to_string();

            match load_pack(&manifest) {
                Ok(pack) => {
                    let mut error = None;
                    if pack.name.trim().is_empty() {
                        error = Some("plugin name is empty".to_string());
                    } else if pack.api > API_VERSION {
                        error = Some(format!(
                            "requires plugin api {} (supported: {API_VERSION})",
                            pack.api
                        ));
                    } else if entries.iter().any(|entry| entry.name == pack.name) {
                        error = Some("duplicate plugin name".to_string());
                    }

                    let enabled = error.is_none() && !disabled.contains(&pack.name);
                    entries.push(PluginEntry {
                        name: if pack.name.trim().is_empty() {
                            fallback
                        } else {
                            pack.name
                        },
                        version: pack.version,
                        api: pack.api,
                        description: pack.description,
                        author: pack.author,
                        homepage: pack.homepage,
                        path: manifest,
                        enabled,
                        providers: pack.providers,
                        launch: pack.launch,
                        tui: pack.tui,
                        error,
                    });
                }
                Err(err) => entries.push(PluginEntry {
                    name: fallback,
                    version: "-".to_string(),
                    api: 0,
                    description: String::new(),
                    author: String::new(),
                    homepage: String::new(),
                    path: manifest,
                    enabled: false,
                    providers: Vec::new(),
                    launch: None,
                    tui: None,
                    error: Some(err.to_string()),
                }),
            }
        }

        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Self {
            entries,
            state_path,
            disabled,
        }
    }

    pub fn mirror_table(&self) -> MirrorTable {
        let providers = self
            .entries
            .iter()
            .filter(|entry| entry.enabled && entry.error.is_none())
            .flat_map(|entry| entry.providers.iter().cloned())
            .collect();
        MirrorTable::from_providers(providers)
    }

    pub fn launch_hooks(&self) -> impl Iterator<Item = &LaunchHook> {
        self.entries
            .iter()
            .filter(|entry| entry.enabled && entry.error.is_none())
            .filter_map(|entry| entry.launch.as_ref())
    }

    pub fn set_enabled(&mut self, name: &str, enabled: bool) -> Result<()> {
        let entry = self
            .entries
            .iter_mut()
            .find(|entry| entry.name == name)
            .with_context(|| format!("plugin not found: {name}"))?;

        if entry.error.is_some() {
            anyhow::bail!(
                "plugin '{name}' is invalid: {}",
                entry.error.as_deref().unwrap_or("unknown error")
            );
        }

        entry.enabled = enabled;
        if enabled {
            self.disabled.remove(name);
        } else {
            self.disabled.insert(name.to_string());
        }
        self.save_state()
    }

    fn save_state(&self) -> Result<()> {
        let state = PluginState {
            disabled: self.disabled.iter().cloned().collect(),
        };
        if let Some(parent) = self.state_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.state_path, serde_json::to_string_pretty(&state)?)
            .with_context(|| format!("write {}", self.state_path.display()))?;
        Ok(())
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PluginState {
    #[serde(default)]
    disabled: Vec<String>,
}

fn load_state(path: &Path) -> PluginState {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

#[derive(Debug, Default, Clone)]
pub struct MirrorTable {
    providers: Vec<Provider>,
}

impl MirrorTable {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn from_providers(mut providers: Vec<Provider>) -> Self {
        providers.retain(|provider| provider.kind == "mirror");
        providers.sort_by_key(|provider| -provider.priority);
        Self { providers }
    }

    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }

    pub fn providers(&self) -> &[Provider] {
        &self.providers
    }

    /// Returns a rewritten URL when a provider matches.
    pub fn rewrite(&self, raw: &str) -> Option<String> {
        let url = Url::parse(raw).ok()?;
        let host = url.host_str()?;
        let path = url.path();

        for provider in &self.providers {
            let rule = &provider.match_rule;
            let host_ok = match (&rule.host, &rule.host_suffix) {
                (Some(expected), _) => expected.eq_ignore_ascii_case(host),
                (None, Some(suffix)) => host.ends_with(suffix.as_str()),
                (None, None) => false,
            };
            if !host_ok {
                continue;
            }
            if let Some(prefix) = &rule.path_prefix {
                if !path.starts_with(prefix.as_str()) {
                    continue;
                }
            }
            let base = provider.replace.with.trim_end_matches('/');
            let prefix = provider.replace.path_prefix.trim_end_matches('/');
            let query = url
                .query()
                .map(|query| format!("?{query}"))
                .unwrap_or_default();
            return Some(format!("{base}{prefix}{path}{query}"));
        }
        None
    }
}

pub fn load_pack(path: &Path) -> Result<ProviderPack> {
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let pack: ProviderPack =
        toml::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
    Ok(pack)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("qcl-plugin-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("plugins")).unwrap();
        dir
    }

    fn write_plugin(root: &Path, name: &str, body: &str) {
        let dir = root.join("plugins").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("plugin.toml"), body).unwrap();
    }

    #[test]
    fn rewrites_asset_hosts() {
        let table = MirrorTable::from_providers(vec![Provider {
            kind: "mirror".into(),
            priority: 10,
            match_rule: MatchRule {
                host: Some("resources.download.minecraft.net".into()),
                ..Default::default()
            },
            replace: Replace {
                with: "https://bmclapi2.bangbang93.com".into(),
                path_prefix: "/assets".into(),
            },
        }]);
        assert_eq!(
            table
                .rewrite("https://resources.download.minecraft.net/ab/abcdef")
                .as_deref(),
            Some("https://bmclapi2.bangbang93.com/assets/ab/abcdef")
        );
        assert!(table.rewrite("https://example.com/x").is_none());
    }

    #[test]
    fn parses_tui_contributions() {
        let pack: ProviderPack = toml::from_str(
            r#"
name = "demo"
version = "1.0"
[tui]
status = "★ {user} · {memory}MB"
[[tui.actions]]
label = "Hello"
key = "h"
action = "message"
value = "hi"
[[tui.actions]]
label = "Refresh"
action = "refresh"
"#,
        )
        .unwrap();
        let tui = pack.tui.unwrap();
        assert_eq!(tui.status.as_deref(), Some("★ {user} · {memory}MB"));
        assert_eq!(tui.actions.len(), 2);
        assert_eq!(tui.actions[0].key.as_deref(), Some("h"));
        assert_eq!(tui.actions[1].action, "refresh");
    }

    #[test]
    fn parses_launch_hook() {
        let pack: ProviderPack = toml::from_str(
            r#"
name = "tweaks"
version = "1.0"
api = 1
description = "extra flags"
[launch]
jvm_args = ["-Dfoo=bar"]
game_args = ["--demo"]
[launch.env]
FOO = "bar"
"#,
        )
        .unwrap();
        let hook = pack.launch.unwrap();
        assert_eq!(hook.jvm_args, vec!["-Dfoo=bar"]);
        assert_eq!(hook.game_args, vec!["--demo"]);
        assert_eq!(hook.env.get("FOO").map(String::as_str), Some("bar"));
    }

    #[test]
    fn registry_respects_state_and_api() {
        let root = temp_root("registry");
        write_plugin(
            &root,
            "mirror-a",
            r#"
name = "mirror-a"
version = "1.0"
api = 1
[[providers]]
kind = "mirror"
priority = 10
[providers.match]
host = "example.com"
[providers.replace]
with = "https://mirror.example"
"#,
        );
        write_plugin(
            &root,
            "tweaks",
            r#"
name = "tweaks"
version = "2.0"
api = 1
[launch]
jvm_args = ["-Dqcl.tweaks=1"]
"#,
        );
        write_plugin(
            &root,
            "too-new",
            r#"
name = "too-new"
version = "3.0"
api = 99
"#,
        );

        let mut registry = PluginRegistry::load(&root);
        assert_eq!(registry.entries.len(), 3);
        assert_eq!(registry.mirror_table().providers().len(), 1);
        assert_eq!(registry.launch_hooks().count(), 1);
        assert!(registry
            .entries
            .iter()
            .find(|entry| entry.name == "too-new")
            .unwrap()
            .error
            .is_some());

        registry.set_enabled("mirror-a", false).unwrap();
        assert!(registry.mirror_table().is_empty());

        let reloaded = PluginRegistry::load(&root);
        let mirror = reloaded
            .entries
            .iter()
            .find(|entry| entry.name == "mirror-a")
            .unwrap();
        assert!(!mirror.enabled);

        let _ = std::fs::remove_dir_all(&root);
    }
}
