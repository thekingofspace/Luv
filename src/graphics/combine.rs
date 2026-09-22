use std::collections::HashSet;

use wgpu::naga::ShaderStage;

use super::shader::Language;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    pub name: String,
    pub code: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub name: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug)]
pub struct Combined {
    pub name: String,
    pub language: Language,
    pub stage: Option<ShaderStage>,
    pub parts: Vec<Part>,
    pub code: String,
    pub segments: Vec<Segment>,
}

fn directive(language: Language, line: &str) -> bool {
    let line = line.trim_start();
    match language {
        Language::Wgsl => {
            line.starts_with("enable ")
                || line.starts_with("requires ")
                || line.starts_with("diagnostic(")
                || line.starts_with("diagnostic (")
        }
        Language::Glsl => line.starts_with("#version") || line.starts_with("#extension"),
        Language::Spirv => false,
    }
}

pub fn combine(
    name: impl Into<String>,
    language: Language,
    stage: Option<ShaderStage>,
    parts: Vec<Part>,
) -> Result<Combined, String> {
    let name = name.into();
    if language == Language::Spirv {
        return Err("SPIR-V shaders cannot be combined, combine their WGSL or GLSL sources instead".to_owned());
    }
    if parts.is_empty() {
        return Err(format!("combo '{name}' needs at least one shader part"));
    }
    let mut seen = HashSet::new();
    let parts: Vec<Part> = parts.into_iter().filter(|part| seen.insert(part.code.clone())).collect();

    let mut directives: Vec<String> = Vec::new();
    let mut bodies = Vec::with_capacity(parts.len());
    for part in &parts {
        let mut body = String::with_capacity(part.code.len() + 1);
        for line in part.code.split_inclusive('\n') {
            if directive(language, line) {
                let text = line.trim().to_owned();
                let duplicate_version = text.starts_with("#version")
                    && directives.iter().any(|existing| existing.starts_with("#version"));
                if !duplicate_version && !directives.contains(&text) {
                    directives.push(text);
                }
                if line.ends_with('\n') {
                    body.push('\n');
                }
            } else {
                body.push_str(line);
            }
        }
        bodies.push(body);
    }
    directives.sort_by_key(|text| !text.starts_with("#version"));

    let mut code = String::new();
    for text in &directives {
        code.push_str(text);
        code.push('\n');
    }
    let mut segments = Vec::with_capacity(parts.len());
    for (part, body) in parts.iter().zip(bodies) {
        let start = code.len();
        code.push_str(&body);
        if !code.ends_with('\n') {
            code.push('\n');
        }
        segments.push(Segment {
            name: part.name.clone(),
            start,
            end: code.len(),
        });
    }
    Ok(Combined {
        name,
        language,
        stage,
        parts,
        code,
        segments,
    })
}

pub fn locate(code: &str, segments: &[Segment], offset: usize) -> Option<(String, usize, usize, String)> {
    let segment = segments
        .iter()
        .find(|segment| segment.start <= offset && offset < segment.end)?;
    let before = code.get(segment.start..offset)?;
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(segment.start, |index| segment.start + index + 1);
    let column = code.get(line_start..offset)?.chars().count() + 1;
    let text = code[line_start..].lines().next().unwrap_or_default().to_owned();
    Some((segment.name.clone(), line, column, text))
}

pub fn describe(name: &str, code: &str, segments: &[Segment], offset: Option<usize>, message: &str) -> String {
    match offset.and_then(|offset| locate(code, segments, offset)) {
        Some((part, line, column, text)) => {
            format!("{part}:{line}:{column}: {message}\n{line:>5} | {text}")
        }
        None => format!("{name}: {message}"),
    }
}
