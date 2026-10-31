use crate::version::Rule;
use std::collections::HashMap;

pub fn os_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    }
}

pub fn os_arch() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "arm64",
        other => other,
    }
}

/// `32` or `64`, used to expand `${arch}` in natives classifiers.
pub fn os_arch_bits() -> &'static str {
    if cfg!(target_pointer_width = "64") {
        "64"
    } else {
        "32"
    }
}

pub fn allowed(rules: Option<&Vec<Rule>>) -> bool {
    allowed_with_features(rules, &HashMap::new())
}

/// Minecraft's rule semantics: no rules means allow; otherwise start denied
/// and let each matching rule override the decision in order.
pub fn allowed_with_features(rules: Option<&Vec<Rule>>, features: &HashMap<String, bool>) -> bool {
    let Some(rules) = rules else {
        return true;
    };
    if rules.is_empty() {
        return true;
    }
    let mut allow = false;
    for rule in rules {
        if rule_matches(rule, features) {
            allow = rule.action == "allow";
        }
    }
    allow
}

fn rule_matches(rule: &Rule, features: &HashMap<String, bool>) -> bool {
    if let Some(os) = &rule.os {
        if let Some(name) = &os.name {
            if name != os_name() {
                return false;
            }
        }
        if let Some(arch) = &os.arch {
            if arch != os_arch() {
                return false;
            }
        }
        // `os.version` is a regex used by a handful of legacy versions; we
        // accept it rather than silently dropping the library.
    }
    if let Some(required) = &rule.features {
        for (key, value) in required {
            if features.get(key).copied().unwrap_or(false) != *value {
                return false;
            }
        }
    }
    true
}

/// Rules attached to an argument entry in `arguments.{game,jvm}`.
pub fn allowed_json(rules: &serde_json::Value, features: &HashMap<String, bool>) -> bool {
    match serde_json::from_value::<Vec<Rule>>(rules.clone()) {
        Ok(rules) => allowed_with_features(Some(&rules), features),
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(action: &str, os: Option<&str>) -> Rule {
        Rule {
            action: action.into(),
            os: os.map(|name| crate::version::OsRule {
                name: Some(name.into()),
                arch: None,
                version: None,
            }),
            features: None,
        }
    }

    #[test]
    fn no_rules_allows() {
        assert!(allowed(None));
        assert!(allowed(Some(&vec![])));
    }

    #[test]
    fn os_rules() {
        let rules = vec![rule("allow", Some(os_name()))];
        assert!(allowed(Some(&rules)));
        let rules = vec![rule("allow", Some("never-os"))];
        assert!(!allowed(Some(&rules)));
    }

    #[test]
    fn deny_then_allow() {
        let rules = vec![rule("deny", None), rule("allow", Some(os_name()))];
        assert!(allowed(Some(&rules)));
    }
}
