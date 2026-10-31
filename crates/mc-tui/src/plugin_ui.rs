//! TUI-facing plugin contributions.
//!
//! L1 plugins describe status text and actions in `plugin.toml`. This module
//! turns those declarations into runtime objects; a future WASM host can feed
//! the same structures without changing the UI.

use mc_plugin::PluginRegistry;
use std::collections::BTreeSet;

/// Keys the launcher already uses; plugins cannot bind them.
const RESERVED_KEYS: &[char] = &[
    'q', 'j', 'k', 'g', 'i', 'l', 's', 'r', 'f', 'e', '/', '?', ' ',
];

#[derive(Debug, Clone)]
pub struct PluginAction {
    pub plugin: String,
    pub label: String,
    pub key: Option<char>,
    pub action: String,
    pub value: Option<String>,
}

pub struct StatusContext {
    pub user: String,
    pub memory_mb: u32,
    pub selected: String,
    pub running: String,
    pub plugins: usize,
}

/// Collect status templates and actions from enabled, valid plugins.
pub fn collect(registry: &PluginRegistry) -> (Vec<PluginAction>, Vec<String>) {
    let mut actions = Vec::new();
    let mut statuses = Vec::new();
    let mut used_keys = BTreeSet::new();

    for entry in registry
        .entries
        .iter()
        .filter(|entry| entry.enabled && entry.error.is_none())
    {
        if let Some(status) = entry.tui_status() {
            if !status.trim().is_empty() {
                statuses.push(status.to_string());
            }
        }

        for spec in entry.tui_actions() {
            let key = spec
                .key
                .as_deref()
                .and_then(action_key)
                .filter(|key| !used_keys.contains(key));
            if let Some(key) = key {
                used_keys.insert(key);
            }
            actions.push(PluginAction {
                plugin: entry.name.clone(),
                label: spec.label.clone(),
                key,
                action: spec.action.clone(),
                value: spec.value.clone(),
            });
        }
    }

    (actions, statuses)
}

/// Parse a single alphanumeric shortcut; reserved keys are rejected.
pub fn action_key(raw: &str) -> Option<char> {
    let mut chars = raw.chars();
    let ch = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    let ch = ch.to_ascii_lowercase();
    if !ch.is_ascii_alphanumeric() || RESERVED_KEYS.contains(&ch) {
        return None;
    }
    Some(ch)
}

pub fn render_status(template: &str, context: &StatusContext) -> String {
    let version = if context.running == "-" {
        context.selected.clone()
    } else {
        context.running.clone()
    };

    template
        .replace("{user}", &context.user)
        .replace("{memory}", &context.memory_mb.to_string())
        .replace("{selected}", &context.selected)
        .replace("{running}", &context.running)
        .replace("{version}", &version)
        .replace("{plugins}", &context.plugins.to_string())
}

#[cfg(test)]
mod tests {
    use super::{action_key, render_status, StatusContext};

    #[test]
    fn sanitizes_shortcuts() {
        assert_eq!(action_key("h"), Some('h'));
        assert_eq!(action_key("H"), Some('h'));
        assert_eq!(action_key("q"), None);
        assert_eq!(action_key("ab"), None);
        assert_eq!(action_key("/"), None);
    }

    #[test]
    fn renders_templates() {
        let context = StatusContext {
            user: "Steve".to_string(),
            memory_mb: 4096,
            selected: "1.20.1".to_string(),
            running: "-".to_string(),
            plugins: 2,
        };
        assert_eq!(
            render_status("{user}/{memory}/{version}/{plugins}", &context),
            "Steve/4096/1.20.1/2"
        );

        let context = StatusContext {
            running: "26.3".to_string(),
            ..context
        };
        assert_eq!(render_status("{version}", &context), "26.3");
    }
}
