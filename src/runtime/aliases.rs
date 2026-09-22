use std::collections::HashMap;
use std::path::{Path, PathBuf};

use mlua::{Lua, Table};

use crate::vfs::{self, Vfs};

pub const LUAURC: &str = ".luaurc";
pub const CONFIG_LUAU: &str = ".config.luau";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolved {
    Game(String),
    Disk(PathBuf),
}

impl Resolved {
    fn join(self, rest: &str) -> Result<Resolved, String> {
        match self {
            Resolved::Game(base) => vfs::normalize(&vfs::join(&base, rest))
                .map(Resolved::Game)
                .ok_or_else(|| format!("{rest} points outside the game's files")),
            Resolved::Disk(base) => Ok(Resolved::Disk(if rest.is_empty() { base } else { base.join(rest) })),
        }
    }
}

pub fn split_alias(path: &str) -> Option<(String, &str)> {
    let aliased = path.strip_prefix('@')?;
    let (alias, rest) = aliased.split_once(['/', '\\']).unwrap_or((aliased, ""));
    Some((alias.to_ascii_lowercase(), rest))
}

pub fn resolve(vfs: &dyn Vfs, directory: &str, alias: &str, rest: &str) -> Result<Resolved, String> {
    resolve_from(vfs, directory, alias, &mut Vec::new())?.join(rest)
}

fn resolve_from(vfs: &dyn Vfs, directory: &str, alias: &str, seen: &mut Vec<String>) -> Result<Resolved, String> {
    if alias == "self" {
        return Ok(Resolved::Game(directory.to_owned()));
    }
    if seen.iter().any(|visited| visited == alias) {
        return Err(format!("@{alias} is part of an alias cycle"));
    }
    seen.push(alias.to_owned());

    let mut current = Some(directory.to_owned());
    while let Some(dir) = current {
        if let Some(value) = aliases_at(vfs, &dir)?.and_then(|mut aliases| aliases.remove(alias)) {
            if let Some((chained, rest)) = split_alias(&value) {
                return resolve_from(vfs, &dir, &chained, seen)?.join(rest);
            }
            if Path::new(&value).is_absolute() {
                return Ok(Resolved::Disk(PathBuf::from(value)));
            }
            return Resolved::Game(dir).join(&value);
        }
        current = dir.rsplit_once('/').map(|(parent, _)| parent.to_owned()).or_else(|| {
            (!dir.is_empty()).then(String::new)
        });
    }
    Err(format!("@{alias} is not a valid alias"))
}

pub fn aliases_at(vfs: &dyn Vfs, directory: &str) -> Result<Option<HashMap<String, String>>, String> {
    let luaurc = vfs::join(directory, LUAURC);
    if vfs.is_file(&luaurc) {
        let source = vfs.read(&luaurc).map_err(|error| format!("cannot read {luaurc}: {error}"))?;
        return parse_luaurc(&String::from_utf8_lossy(&source))
            .map(Some)
            .map_err(|error| format!("{luaurc}: {error}"));
    }
    let config = vfs::join(directory, CONFIG_LUAU);
    if vfs.is_file(&config) {
        let source = vfs.read(&config).map_err(|error| format!("cannot read {config}: {error}"))?;
        return parse_config_luau(&source)
            .map(Some)
            .map_err(|error| format!("{config}: {error}"));
    }
    Ok(None)
}

pub fn parse_config_luau(source: &[u8]) -> Result<HashMap<String, String>, String> {
    let lua = Lua::new();
    lua.sandbox(true).map_err(|error| error.to_string())?;
    let config: Table = lua
        .load(source)
        .set_name("=config")
        .eval()
        .map_err(|error| error.to_string())?;
    let mut aliases = HashMap::new();
    let Some(luau) = config.get::<Option<Table>>("luau").map_err(|error| error.to_string())? else {
        return Ok(aliases);
    };
    let Some(table) = luau.get::<Option<Table>>("aliases").map_err(|error| error.to_string())? else {
        return Ok(aliases);
    };
    for pair in table.pairs::<String, String>() {
        let (alias, value) = pair.map_err(|error| error.to_string())?;
        aliases.insert(alias.to_ascii_lowercase(), value);
    }
    Ok(aliases)
}

#[derive(Debug, PartialEq)]
enum Token {
    Open(char),
    Close(char),
    Colon,
    Comma,
    Text(String),
    Word(String),
}

fn tokenize(source: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = source.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            ch if ch.is_whitespace() => {}
            '{' | '[' => tokens.push(Token::Open(ch)),
            '}' | ']' => tokens.push(Token::Close(ch)),
            ':' => tokens.push(Token::Colon),
            ',' => tokens.push(Token::Comma),
            '/' if chars.peek() == Some(&'/') => {
                for next in chars.by_ref() {
                    if next == '\n' {
                        break;
                    }
                }
            }
            '-' if chars.peek() == Some(&'-') => {
                for next in chars.by_ref() {
                    if next == '\n' {
                        break;
                    }
                }
            }
            '"' | '\'' => {
                let mut text = String::new();
                loop {
                    match chars.next() {
                        Some(end) if end == ch => break,
                        Some('\\') => {
                            if let Some(escaped) = chars.next() {
                                text.push('\\');
                                text.push(escaped);
                            }
                        }
                        Some('\n') | None => return Err("unfinished string".to_owned()),
                        Some(other) => text.push(other),
                    }
                }
                tokens.push(Token::Text(text));
            }
            ch if ch.is_ascii_alphanumeric() => {
                let mut word = ch.to_string();
                while let Some(next) = chars.peek().copied().filter(|next| next.is_ascii_alphanumeric()) {
                    word.push(next);
                    chars.next();
                }
                tokens.push(Token::Word(word));
            }
            other => return Err(format!("unexpected character '{other}'")),
        }
    }
    Ok(tokens)
}

pub fn canonical_luaurc(source: &str) -> Result<String, String> {
    let mut aliases: Vec<(String, String)> = parse_luaurc(source)?.into_iter().collect();
    aliases.sort();
    let entries: Vec<String> = aliases
        .into_iter()
        .map(|(alias, value)| {
            let value = match split_alias(&value) {
                Some((chained, "")) => format!("@{chained}"),
                Some((chained, rest)) => format!("@{chained}/{rest}"),
                None => value,
            };
            format!("\"{alias}\":\"{value}\"")
        })
        .collect();
    Ok(format!("{{\"aliases\":{{{}}}}}", entries.join(",")))
}

pub fn parse_luaurc(source: &str) -> Result<HashMap<String, String>, String> {
    let tokens = tokenize(source)?;
    let mut position = 0;
    let mut aliases = HashMap::new();
    parse_object(&tokens, &mut position, &mut Vec::new(), &mut aliases)?;
    if position != tokens.len() {
        return Err("expected end of file".to_owned());
    }
    Ok(aliases)
}

fn parse_object(
    tokens: &[Token],
    position: &mut usize,
    keys: &mut Vec<String>,
    aliases: &mut HashMap<String, String>,
) -> Result<(), String> {
    if tokens.get(*position) != Some(&Token::Open('{')) {
        return Err("expected '{'".to_owned());
    }
    *position += 1;
    loop {
        match tokens.get(*position) {
            Some(Token::Close('}')) => {
                *position += 1;
                return Ok(());
            }
            Some(Token::Text(key)) => {
                *position += 1;
                if tokens.get(*position) != Some(&Token::Colon) {
                    return Err(format!("expected ':' after \"{key}\""));
                }
                *position += 1;
                keys.push(key.clone());
                parse_value(tokens, position, keys, aliases)?;
                keys.pop();
                match tokens.get(*position) {
                    Some(Token::Comma) => *position += 1,
                    Some(Token::Close('}')) => {}
                    _ => return Err("expected ',' or '}'".to_owned()),
                }
            }
            _ => return Err("expected a field name or '}'".to_owned()),
        }
    }
}

fn parse_value(
    tokens: &[Token],
    position: &mut usize,
    keys: &mut Vec<String>,
    aliases: &mut HashMap<String, String>,
) -> Result<(), String> {
    match tokens.get(*position) {
        Some(Token::Open('{')) => parse_object(tokens, position, keys, aliases),
        Some(Token::Open('[')) => {
            *position += 1;
            loop {
                match tokens.get(*position) {
                    Some(Token::Close(']')) => {
                        *position += 1;
                        return Ok(());
                    }
                    Some(Token::Text(_)) => {
                        *position += 1;
                        match tokens.get(*position) {
                            Some(Token::Comma) => *position += 1,
                            Some(Token::Close(']')) => {}
                            _ => return Err("expected ',' or ']'".to_owned()),
                        }
                    }
                    _ => return Err("expected an array element or ']'".to_owned()),
                }
            }
        }
        Some(Token::Text(value)) => {
            if let [section, alias] = keys.as_slice()
                && section == "aliases"
            {
                aliases.insert(alias.to_ascii_lowercase(), value.clone());
            }
            *position += 1;
            Ok(())
        }
        Some(Token::Word(word)) if word == "true" || word == "false" => {
            *position += 1;
            Ok(())
        }
        _ => Err("expected a field value".to_owned()),
    }
}
