use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::plugins::NATIVE_DIR;
use crate::project::{Project, TYPES_FILE, TYPES_TEMPLATE};

const MERGED: [&str; 2] = ["WindowAPIs", "Imports"];
const SUFFIX: &str = ".d.luau";
const DIRECTIVE: &str = "--!";

#[derive(Default)]
pub struct TypeReport {
    pub sources: Vec<String>,
    pub changed: bool,
}

impl TypeReport {
    pub fn summary(&self) -> String {
        match self.sources.len() {
            0 => "no plugin types".to_owned(),
            1 => format!("1 plugin type file, {}", self.sources[0]),
            count => format!("{count} plugin type files"),
        }
    }
}

fn walk(directory: &Path, found: &mut Vec<PathBuf>) -> io::Result<()> {
    let mut entries: Vec<PathBuf> = fs::read_dir(directory)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<_>>()?;
    entries.sort();
    for path in entries {
        if path.is_dir() {
            walk(&path, found)?;
        } else if path.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.ends_with(SUFFIX)) {
            found.push(path);
        }
    }
    Ok(())
}

fn label(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn gather(root: &Path, roots: &[PathBuf]) -> Result<Vec<(String, String)>> {
    let mut found = Vec::new();
    for base in roots {
        let native = base.join(NATIVE_DIR);
        if !native.is_dir() {
            continue;
        }
        let mut files = Vec::new();
        walk(&native, &mut files).with_context(|| format!("failed to read {}", native.display()))?;
        for path in files {
            let text = fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
            found.push((label(root, &path), text));
        }
    }
    Ok(found)
}

fn opener(name: &str) -> String {
    format!("export type {name} = {{")
}

fn split(text: &str) -> (String, Vec<(&'static str, Vec<String>)>) {
    let mut body = String::new();
    let mut merges = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        if line.trim_start().starts_with(DIRECTIVE) {
            continue;
        }
        let Some(name) = MERGED.iter().find(|name| line.trim() == opener(name)) else {
            body.push_str(line);
            body.push('\n');
            continue;
        };
        let mut fields = Vec::new();
        for line in lines.by_ref() {
            if line.trim() == "}" {
                break;
            }
            fields.push(line.to_owned());
        }
        merges.push((*name, fields));
    }
    (body, merges)
}

pub fn build(root: &Path, roots: &[PathBuf]) -> Result<(String, Vec<String>)> {
    let mut extras: BTreeMap<&str, Vec<(String, Vec<String>)>> = BTreeMap::new();
    let mut bodies: Vec<(String, String)> = Vec::new();
    let mut sources = Vec::new();
    for (label, text) in gather(root, roots)? {
        let (body, merges) = split(&text);
        for (name, fields) in merges {
            extras.entry(name).or_default().push((label.clone(), fields));
        }
        if !body.trim().is_empty() {
            bodies.push((label.clone(), body));
        }
        sources.push(label);
    }
    let mut out = String::with_capacity(TYPES_TEMPLATE.len());
    let mut lines = TYPES_TEMPLATE.lines();
    while let Some(line) = lines.next() {
        out.push_str(line);
        out.push('\n');
        let Some(name) = MERGED.iter().find(|name| line.trim() == opener(name)) else {
            continue;
        };
        for line in lines.by_ref() {
            if line.trim() != "}" {
                out.push_str(line);
                out.push('\n');
                continue;
            }
            for (label, fields) in extras.get(name).into_iter().flatten() {
                out.push_str(&format!("\t-- from {label}\n"));
                for field in fields {
                    out.push_str(field);
                    out.push('\n');
                }
            }
            out.push_str("}\n");
            break;
        }
    }
    for (label, body) in bodies {
        out.push_str(&format!("\n-- luv plugin types from {label}\n"));
        out.push_str(body.trim());
        out.push_str(&format!("\n-- end of {label}\n"));
    }
    Ok((out, sources))
}

pub fn roots(project: &Project) -> Vec<PathBuf> {
    let mut roots = vec![project.root.clone()];
    for folder in project.container_folders() {
        roots.push(project.root.join(folder));
    }
    roots
}

pub fn generate(project: &Project) -> Result<(String, Vec<String>)> {
    build(&project.root, &roots(project))
}

pub fn sync(project: &Project) -> Result<TypeReport> {
    let (text, sources) = generate(project)?;
    let path = project.root.join(TYPES_FILE);
    if fs::read(&path).is_ok_and(|current| current == text.as_bytes()) {
        return Ok(TypeReport { sources, changed: false });
    }
    fs::write(&path, &text).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(TypeReport { sources, changed: true })
}
