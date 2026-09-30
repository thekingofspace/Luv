pub(crate) mod parallel;
mod promise;
mod switch;
mod task;

use mlua::{Lua, MultiValue, Result, Value};

pub use promise::Promise;
pub use task::Task;

pub(crate) fn joined(head: Value, rest: MultiValue) -> MultiValue {
    let mut values = MultiValue::with_capacity(rest.len() + 1);
    values.push_back(head);
    for value in rest {
        values.push_back(value);
    }
    values
}

pub fn install(lua: &Lua) -> Result<()> {
    parallel::install(lua)?;
    promise::install(lua)?;
    switch::install(lua)?;
    task::install(lua)
}
