mod color;
pub mod enums;
mod udim;

pub use color::Color;
pub use enums::EnumItem;
pub use udim::UDim;

use mlua::{FromLua, Lua, Result, Value};

pub const UDIM_GLOBAL: &str = "udim";
pub const COLOR_GLOBAL: &str = "color";

pub enum Operand<T> {
    Value(T),
    Number(f64),
}

impl<T: FromLua> FromLua for Operand<T> {
    fn from_lua(value: Value, lua: &Lua) -> Result<Self> {
        match value {
            Value::Integer(number) => Ok(Operand::Number(number as f64)),
            Value::Number(number) => Ok(Operand::Number(number)),
            other => T::from_lua(other, lua).map(Operand::Value),
        }
    }
}

pub(crate) fn mixed(type_name: &str) -> mlua::Error {
    mlua::Error::runtime(format!("at least one side of this {type_name} operation must be a {type_name}"))
}

pub(crate) fn format_number(number: f64) -> String {
    number.to_string()
}

pub fn install(lua: &Lua) -> Result<()> {
    let udim = lua.create_table()?;
    udim.set(
        "new",
        lua.create_function(|_, (x, y, z): (Option<f64>, Option<f64>, Option<f64>)| {
            Ok(UDim::new(x.unwrap_or(0.0), y.unwrap_or(0.0), z.unwrap_or(0.0)))
        })?,
    )?;
    udim.set("zero", UDim::ZERO)?;
    udim.set_readonly(true);
    lua.globals().set(UDIM_GLOBAL, udim)?;

    let color = lua.create_table()?;
    color.set(
        "new",
        lua.create_function(|_, (r, g, b, a): (Option<f64>, Option<f64>, Option<f64>, Option<f64>)| {
            Ok(Color::new(r.unwrap_or(0.0), g.unwrap_or(0.0), b.unwrap_or(0.0), a.unwrap_or(1.0)))
        })?,
    )?;
    color.set(
        "fromRGB",
        lua.create_function(|_, (r, g, b, a): (f64, f64, f64, Option<f64>)| {
            Ok(Color::from_rgb(r, g, b, a.unwrap_or(255.0)))
        })?,
    )?;
    color.set(
        "fromHex",
        lua.create_function(|_, hex: String| {
            Color::from_hex(&hex).ok_or_else(|| {
                mlua::Error::runtime(format!(
                    "'{hex}' is not a hex color, expected #RGB, #RGBA, #RRGGBB or #RRGGBBAA"
                ))
            })
        })?,
    )?;
    color.set(
        "fromHSV",
        lua.create_function(|_, (h, s, v, a): (f64, f64, f64, Option<f64>)| {
            Ok(Color::from_hsv(h, s, v, a.unwrap_or(1.0)))
        })?,
    )?;
    color.set("white", Color::WHITE)?;
    color.set("black", Color::BLACK)?;
    color.set("transparent", Color::TRANSPARENT)?;
    color.set_readonly(true);
    lua.globals().set(COLOR_GLOBAL, color)?;
    enums::install(lua)
}
