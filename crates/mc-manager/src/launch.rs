use crate::auth::Profile;
use crate::install;
use crate::paths::DataRoot;
use crate::rules;
use crate::version::{artifact_path, VersionFile};
use mc_core::{Error, LaunchPlan, Result};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct LaunchOptions {
    pub memory_mb: u32,
    pub min_memory_mb: u32,
    pub extra_jvm: Vec<String>,
    pub extra_game: Vec<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub demo: bool,
}

impl Default for LaunchOptions {
    fn default() -> Self {
        Self {
            memory_mb: 2048,
            min_memory_mb: 512,
            extra_jvm: Vec::new(),
            extra_game: Vec::new(),
            width: None,
            height: None,
            demo: false,
        }
    }
}

/// Turn a version file plus a profile into a spawnable plan.
pub fn build(
    root: &DataRoot,
    version: &VersionFile,
    instance_id: &str,
    java: PathBuf,
    profile: &Profile,
    opts: &LaunchOptions,
) -> Result<LaunchPlan> {
    let natives_dir = install::natives_dir(root, version);
    std::fs::create_dir_all(&natives_dir)?;

    let game_dir = root.instances().join(instance_id);
    std::fs::create_dir_all(&game_dir)?;

    let classpath = classpath(root, version);
    let classpath_string = join_paths(&classpath);

    let features: HashMap<String, bool> = HashMap::from([
        ("is_demo_user".to_string(), opts.demo),
        (
            "has_custom_resolution".to_string(),
            opts.width.is_some() && opts.height.is_some(),
        ),
        ("has_quick_plays_support".to_string(), false),
        ("is_quick_play_singleplayer".to_string(), false),
        ("is_quick_play_multiplayer".to_string(), false),
        ("is_quick_play_realms".to_string(), false),
    ]);

    let legacy_assets = matches!(version.assets.as_deref(), Some("legacy") | Some("pre-1.6"));
    let assets_root = if legacy_assets {
        root.assets().join("virtual/legacy")
    } else {
        root.assets()
    };
    let assets_index_name = version
        .asset_index
        .as_ref()
        .map(|index| index.id.clone())
        .or_else(|| version.assets.clone())
        .unwrap_or_else(|| "legacy".to_string());

    let mut placeholders: HashMap<String, String> = HashMap::new();
    placeholders.insert("auth_player_name".into(), profile.name.clone());
    placeholders.insert("version_name".into(), version.id.clone());
    placeholders.insert("game_directory".into(), display(&game_dir));
    placeholders.insert("assets_root".into(), display(&assets_root));
    placeholders.insert("assets_index_name".into(), assets_index_name);
    placeholders.insert("auth_uuid".into(), profile.uuid.clone());
    placeholders.insert("auth_access_token".into(), profile.access_token.clone());
    placeholders.insert(
        "auth_session".into(),
        format!("token:{}:{}", profile.access_token, profile.uuid),
    );
    placeholders.insert("user_type".into(), profile.user_type.clone());
    placeholders.insert("version_type".into(), version.kind.clone());
    placeholders.insert("user_properties".into(), "{}".into());
    placeholders.insert("clientid".into(), String::new());
    placeholders.insert("auth_xuid".into(), String::new());
    placeholders.insert("natives_directory".into(), display(&natives_dir));
    placeholders.insert("launcher_name".into(), "QCL".into());
    placeholders.insert(
        "launcher_version".into(),
        env!("CARGO_PKG_VERSION").to_string(),
    );
    placeholders.insert("classpath".into(), classpath_string.clone());
    placeholders.insert(
        "classpath_separator".into(),
        (if cfg!(windows) { ";" } else { ":" }).into(),
    );
    placeholders.insert("library_directory".into(), display(&root.libraries()));
    placeholders.insert(
        "resolution_width".into(),
        opts.width.map(|w| w.to_string()).unwrap_or_default(),
    );
    placeholders.insert(
        "resolution_height".into(),
        opts.height.map(|h| h.to_string()).unwrap_or_default(),
    );

    let mut jvm_args = vec![
        format!("-Xmx{}M", opts.memory_mb),
        format!("-Xms{}M", opts.min_memory_mb),
    ];

    match version.arguments.as_ref().and_then(|a| a.get("jvm")) {
        Some(value) => jvm_args.extend(collect_args(value, &placeholders, &features)),
        None => {
            jvm_args.push(format!(
                "-Djava.library.path={}",
                natives_dir.to_string_lossy()
            ));
            jvm_args.push("-cp".into());
            jvm_args.push(classpath_string.clone());
        }
    }

    // User-supplied flags go right after the memory flags, before `-cp`.
    for (offset, arg) in opts.extra_jvm.iter().enumerate() {
        jvm_args.insert(2 + offset, arg.clone());
    }

    if !jvm_args
        .iter()
        .any(|arg| arg == "-cp" || arg == "-classpath")
    {
        jvm_args.push("-cp".into());
        jvm_args.push(classpath_string.clone());
    }

    if let Some(config) = install::client_log_config(root, version) {
        if config.is_file() {
            if let Some(argument) = version
                .logging
                .as_ref()
                .and_then(|l| l.client.as_ref())
                .map(|c| c.argument.replace("${path}", &display(&config)))
            {
                jvm_args.insert(2, argument);
            }
        }
    }

    let mut game_args = match version.arguments.as_ref().and_then(|a| a.get("game")) {
        Some(value) => collect_args(value, &placeholders, &features),
        None => version
            .minecraft_arguments
            .as_deref()
            .unwrap_or_default()
            .split_whitespace()
            .map(|arg| expand(arg, &placeholders))
            .collect(),
    };
    game_args.extend(opts.extra_game.iter().cloned());

    Ok(LaunchPlan {
        instance_id: instance_id.to_string(),
        java,
        working_dir: game_dir,
        jvm_args,
        main_class: version.main_class.clone(),
        game_args,
        classpath,
        natives_dir: Some(natives_dir),
        env: BTreeMap::new(),
    })
}

pub fn classpath(root: &DataRoot, version: &VersionFile) -> Vec<PathBuf> {
    let mut classpath = Vec::new();
    for lib in &version.libraries {
        if !rules::allowed(lib.rules.as_ref()) {
            continue;
        }
        let Some(downloads) = &lib.downloads else {
            continue;
        };
        let Some(artifact) = &downloads.artifact else {
            continue;
        };
        classpath.push(root.libraries().join(artifact_path(lib, artifact)));
    }
    let client_jar = version.client_jar(root);
    if client_jar.is_file() {
        classpath.push(client_jar);
    }
    classpath
}

fn collect_args(
    value: &serde_json::Value,
    placeholders: &HashMap<String, String>,
    features: &HashMap<String, bool>,
) -> Vec<String> {
    let mut out = Vec::new();
    let Some(entries) = value.as_array() else {
        return out;
    };
    for entry in entries {
        match entry {
            serde_json::Value::String(raw) => out.push(expand(raw, placeholders)),
            serde_json::Value::Object(map) => {
                let allowed = map
                    .get("rules")
                    .map(|rules| rules::allowed_json(rules, features))
                    .unwrap_or(true);
                if !allowed {
                    continue;
                }
                match map.get("value") {
                    Some(serde_json::Value::String(raw)) => out.push(expand(raw, placeholders)),
                    Some(serde_json::Value::Array(values)) => {
                        for value in values {
                            if let Some(raw) = value.as_str() {
                                out.push(expand(raw, placeholders));
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    out
}

fn expand(input: &str, placeholders: &HashMap<String, String>) -> String {
    let mut output = input.to_string();
    for (key, value) in placeholders {
        output = output.replace(&format!("${{{key}}}"), value);
    }
    output
}

fn join_paths(paths: &[PathBuf]) -> String {
    let separator = if cfg!(windows) { ";" } else { ":" };
    paths
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(separator)
}

fn display(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub fn require_client_jar(root: &DataRoot, version: &VersionFile) -> Result<()> {
    let jar = version.client_jar(root);
    if jar.is_file() {
        Ok(())
    } else {
        Err(Error::Other(format!(
            "client jar missing: {} (run `qcl install {}`)",
            jar.display(),
            version.id
        )))
    }
}

/// Apply one L1 launch hook. JVM args must go before `-cp`, otherwise they
/// would be parsed as the main class.
pub fn apply_hook(plan: &mut LaunchPlan, hook: &mc_plugin::LaunchHook) {
    if !hook.jvm_args.is_empty() {
        let position = plan
            .jvm_args
            .iter()
            .position(|arg| arg == "-cp" || arg == "-classpath")
            .unwrap_or(plan.jvm_args.len());
        for (offset, arg) in hook.jvm_args.iter().enumerate() {
            plan.jvm_args.insert(position + offset, arg.clone());
        }
    }

    plan.game_args.extend(hook.game_args.iter().cloned());
    for (key, value) in &hook.env {
        plan.env.insert(key.clone(), value.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::apply_hook;
    use mc_core::LaunchPlan;
    use mc_plugin::LaunchHook;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    #[test]
    fn inserts_jvm_args_before_classpath() {
        let mut plan = LaunchPlan {
            instance_id: "test".into(),
            java: PathBuf::from("java"),
            working_dir: PathBuf::from("."),
            jvm_args: vec!["-Xmx1G".into(), "-cp".into(), "a:b".into()],
            main_class: "Main".into(),
            game_args: vec![],
            classpath: vec![],
            natives_dir: None,
            env: BTreeMap::new(),
        };
        let hook = LaunchHook {
            jvm_args: vec!["-Dfoo=1".into()],
            game_args: vec!["--demo".into()],
            env: BTreeMap::from([("FOO".to_string(), "bar".to_string())]),
        };

        apply_hook(&mut plan, &hook);

        assert_eq!(plan.jvm_args, vec!["-Xmx1G", "-Dfoo=1", "-cp", "a:b"]);
        assert_eq!(plan.game_args, vec!["--demo"]);
        assert_eq!(plan.env.get("FOO").map(String::as_str), Some("bar"));
    }
}
