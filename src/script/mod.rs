mod parallel;

pub use parallel::{Cluster, ENTER, EXIT, HOOK, ParallelError, Units, split};

use std::fmt;

use crate::runtime::compiler;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptError(String);

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ScriptError {}

impl From<mlua::Error> for ScriptError {
    fn from(error: mlua::Error) -> Self {
        match error {
            mlua::Error::SyntaxError { message, .. } => ScriptError(message),
            other => ScriptError(other.to_string()),
        }
    }
}

pub fn units(source: &[u8]) -> Result<Vec<Vec<u8>>, ScriptError> {
    let Ok(text) = std::str::from_utf8(source) else {
        return Ok(vec![source.to_vec()]);
    };
    if !parallel::has_markers(text) {
        return Ok(vec![source.to_vec()]);
    }
    let units = split(text).map_err(|error| match compiler().compile(text) {
        Err(syntax) => ScriptError::from(syntax),
        Ok(_) => ScriptError(error.to_string()),
    })?;
    let mut sources = Vec::with_capacity(units.clusters.len() + 1);
    sources.push(units.main.into_bytes());
    sources.extend(units.clusters.into_iter().map(|cluster| cluster.source.into_bytes()));
    Ok(sources)
}

pub fn compile(source: &[u8]) -> Result<Vec<u8>, ScriptError> {
    let bytecode = units(source)?
        .iter()
        .map(|unit| compiler().compile(unit).map_err(ScriptError::from))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(encode_bundle(&bytecode))
}

pub fn encode_bundle(units: &[Vec<u8>]) -> Vec<u8> {
    let mut bundle = Vec::new();
    bundle.extend((units.len() as u32).to_le_bytes());
    for unit in units {
        bundle.extend((unit.len() as u32).to_le_bytes());
        bundle.extend(unit);
    }
    bundle
}

pub fn bundle_unit(bundle: &[u8], index: usize) -> Option<&[u8]> {
    let (count, mut rest) = bundle.split_first_chunk::<4>()?;
    if index >= u32::from_le_bytes(*count) as usize {
        return None;
    }
    for current in 0.. {
        let (len, tail) = rest.split_first_chunk::<4>()?;
        let (unit, tail) = tail.split_at_checked(u32::from_le_bytes(*len) as usize)?;
        if current == index {
            return Some(unit);
        }
        rest = tail;
    }
    None
}
