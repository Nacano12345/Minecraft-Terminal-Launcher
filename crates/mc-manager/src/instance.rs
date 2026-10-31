use crate::paths::DataRoot;
use mc_core::{Instance, Result};
use std::path::PathBuf;

pub fn dir(root: &DataRoot, id: &str) -> PathBuf {
    root.instances().join(id)
}

pub fn path(root: &DataRoot, id: &str) -> PathBuf {
    dir(root, id).join("instance.json")
}

pub fn load(root: &DataRoot, id: &str) -> Result<Option<Instance>> {
    let path = path(root, id);
    if !path.is_file() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path)?;
    Ok(Some(serde_json::from_str(&text)?))
}

pub fn save(root: &DataRoot, instance: &Instance) -> Result<PathBuf> {
    let dir = dir(root, &instance.id);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("instance.json");
    std::fs::write(&path, serde_json::to_string_pretty(instance)?)?;
    Ok(path)
}

pub fn list(root: &DataRoot) -> Result<Vec<Instance>> {
    let mut instances = Vec::new();
    let dir = root.instances();
    if !dir.is_dir() {
        return Ok(instances);
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path().join("instance.json");
        if !path.is_file() {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(instance) = serde_json::from_str::<Instance>(&text) {
                instances.push(instance);
            }
        }
    }
    instances.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(instances)
}
