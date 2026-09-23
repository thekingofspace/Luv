use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{Map, Value};

use crate::api::codec::strip_jsonc;
use crate::project::Project;
use crate::runtime::aliases::{CONFIG_LUAU, LUAURC};
use crate::vfs;

pub const ALIAS_DUMP: &str = "aliases.json";
const ALIASES_KEY: &str = "aliases";
const INDENT: &[u8] = b"    ";

#[derive(Default)]
pub struct AliasReport {
    pub added: Vec<String>,
    pub updated: Vec<String>,
    pub removed: Vec<String>,
    pub note: Option<String>,
}

impl AliasReport {
    pub fn changed(&self) -> bool {
        !self.added.is_empty() || !self.updated.is_empty() || !self.removed.is_empty()
    }

    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        for (label, names) in [("added", &self.added), ("updated", &self.updated), ("removed", &self.removed)] {
            if !names.is_empty() {
                parts.push(format!("{label} {}", names.join(", ")));
            }
        }
        parts.join(", ")
    }
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    let mut text = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(INDENT);
    let mut writer = serde_json::Serializer::with_formatter(&mut text, formatter);
    serde::Serialize::serialize(value, &mut writer)?;
    text.push(b'\n');
    if fs::read(path).is_ok_and(|current| current == text) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, text).with_context(|| format!("failed to write {}", path.display()))
}

fn read_json(path: &Path) -> Result<Option<Value>> {
    let Ok(source) = fs::read_to_string(path) else {
        return Ok(None);
    };
    let value: Value = serde_json::from_str(&strip_jsonc(&source))
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(Some(value))
}

fn alias_map(document: &Value) -> Map<String, Value> {
    document
        .get(ALIASES_KEY)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

fn read_dump(path: &Path) -> Result<BTreeMap<String, String>> {
    let Some(document) = read_json(path)? else {
        return Ok(BTreeMap::new());
    };
    Ok(alias_map(&document)
        .into_iter()
        .filter_map(|(name, value)| value.as_str().map(|value| (name, value.to_owned())))
        .collect())
}

fn alias_target(folder: &str, entry: &str) -> String {
    let trimmed = entry.strip_suffix(".luau").or_else(|| entry.strip_suffix(".lua")).unwrap_or(entry);
    let inside = match trimmed.rsplit_once('/') {
        Some((parent, "init")) => parent.to_owned(),
        None if trimmed == "init" => String::new(),
        _ => trimmed.to_owned(),
    };
    format!("./{}", vfs::join(folder, &inside))
}

fn wanted(project: &Project) -> Result<BTreeMap<String, String>> {
    let mut aliases = BTreeMap::new();
    for container in project.containers()? {
        aliases.insert(container.id(), alias_target(&container.folder, &container.entry()?));
    }
    Ok(aliases)
}

fn find(aliases: &Map<String, Value>, name: &str) -> Option<String> {
    aliases
        .keys()
        .find(|key| key.eq_ignore_ascii_case(name))
        .map(String::to_owned)
}

pub fn sync(project: &Project) -> Result<AliasReport> {
    let mut report = AliasReport::default();
    if !project.manifest.build.aliases {
        return Ok(report);
    }
    let wanted = wanted(project)?;
    let dump = project.output_dir().join(ALIAS_DUMP);
    let previous = read_dump(&dump)?;
    let path = project.root.join(LUAURC);
    let existing = read_json(&path)?;
    if existing.is_none() {
        if project.root.join(CONFIG_LUAU).is_file() {
            if !wanted.is_empty() {
                report.note = Some(format!(
                    "{CONFIG_LUAU} already sets the aliases, so luv left {LUAURC} alone"
                ));
            }
            return Ok(report);
        }
        if wanted.is_empty() {
            return Ok(report);
        }
    }

    let mut document = existing.unwrap_or_else(|| Value::Object(Map::new()));
    let mut aliases = alias_map(&document);
    let mut managed: BTreeMap<String, String> = BTreeMap::new();
    for (name, target) in &wanted {
        let key = find(&aliases, name);
        let current = key.as_ref().and_then(|key| aliases.get(key)).and_then(Value::as_str);
        match current {
            Some(current) if current == target => {
                managed.insert(name.clone(), target.clone());
            }
            Some(current) => {
                if previous.get(name).map(String::as_str) == Some(current) {
                    aliases.insert(key.unwrap_or_else(|| name.clone()), Value::String(target.clone()));
                    report.updated.push(name.clone());
                    managed.insert(name.clone(), target.clone());
                }
            }
            None => {
                aliases.insert(name.clone(), Value::String(target.clone()));
                report.added.push(name.clone());
                managed.insert(name.clone(), target.clone());
            }
        }
    }
    for (name, target) in &previous {
        if wanted.contains_key(name) {
            continue;
        }
        let Some(key) = find(&aliases, name) else {
            continue;
        };
        if aliases.get(&key).and_then(Value::as_str) == Some(target.as_str()) {
            aliases.remove(&key);
            report.removed.push(name.clone());
        }
    }

    if report.changed() {
        match document.as_object_mut() {
            Some(object) => {
                object.insert(ALIASES_KEY.to_owned(), Value::Object(aliases));
            }
            None => document = Value::Object(Map::from_iter([(ALIASES_KEY.to_owned(), Value::Object(aliases))])),
        }
        write_json(&path, &document)?;
    }
    if managed.is_empty() {
        if dump.is_file() {
            fs::remove_file(&dump).with_context(|| format!("failed to remove {}", dump.display()))?;
        }
        return Ok(report);
    }
    let kept: Map<String, Value> = managed
        .into_iter()
        .map(|(name, target)| (name, Value::String(target)))
        .collect();
    write_json(&dump, &Value::Object(Map::from_iter([(ALIASES_KEY.to_owned(), Value::Object(kept))])))?;
    Ok(report)
}
