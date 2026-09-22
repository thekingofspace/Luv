use mlua::{FromLua, Lua, MetaMethod, Result, UserData, UserDataFields, UserDataMethods, Value};

use super::{Operand, format_number, mixed};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl Color {
    pub const TYPE_NAME: &'static str = "Color";
    pub const WHITE: Color = Color::new(1.0, 1.0, 1.0, 1.0);
    pub const BLACK: Color = Color::new(0.0, 0.0, 0.0, 1.0);
    pub const TRANSPARENT: Color = Color::new(0.0, 0.0, 0.0, 0.0);

    pub const fn new(r: f64, g: f64, b: f64, a: f64) -> Self {
        Self { r, g, b, a }
    }

    pub fn from_rgb(r: f64, g: f64, b: f64, a: f64) -> Self {
        Self::new(r / 255.0, g / 255.0, b / 255.0, a / 255.0)
    }

    pub fn from_hex(hex: &str) -> Option<Self> {
        let digits = hex.trim().trim_start_matches('#');
        let channel = |text: &str| u8::from_str_radix(text, 16).ok().map(f64::from);
        let short = |index: usize| channel(&digits[index..=index].repeat(2));
        let long = |index: usize| channel(&digits[index * 2..index * 2 + 2]);
        if !digits.is_ascii() {
            return None;
        }
        let (r, g, b, a) = match digits.len() {
            3 => (short(0)?, short(1)?, short(2)?, 255.0),
            4 => (short(0)?, short(1)?, short(2)?, short(3)?),
            6 => (long(0)?, long(1)?, long(2)?, 255.0),
            8 => (long(0)?, long(1)?, long(2)?, long(3)?),
            _ => return None,
        };
        Some(Self::from_rgb(r, g, b, a))
    }

    pub fn from_hsv(hue: f64, saturation: f64, value: f64, alpha: f64) -> Self {
        let hue = hue.rem_euclid(1.0) * 6.0;
        let sector = hue.floor();
        let fraction = hue - sector;
        let p = value * (1.0 - saturation);
        let q = value * (1.0 - saturation * fraction);
        let t = value * (1.0 - saturation * (1.0 - fraction));
        let (r, g, b) = match sector as u8 {
            0 => (value, t, p),
            1 => (q, value, p),
            2 => (p, value, t),
            3 => (p, q, value),
            4 => (t, p, value),
            _ => (value, p, q),
        };
        Self::new(r, g, b, alpha)
    }

    pub fn to_hsv(self) -> (f64, f64, f64) {
        let max = self.r.max(self.g).max(self.b);
        let min = self.r.min(self.g).min(self.b);
        let delta = max - min;
        let hue = if delta == 0.0 {
            0.0
        } else if max == self.r {
            ((self.g - self.b) / delta).rem_euclid(6.0) / 6.0
        } else if max == self.g {
            ((self.b - self.r) / delta + 2.0) / 6.0
        } else {
            ((self.r - self.g) / delta + 4.0) / 6.0
        };
        let saturation = if max == 0.0 { 0.0 } else { delta / max };
        (hue, saturation, max)
    }

    pub fn to_rgb(self) -> (f64, f64, f64, f64) {
        let byte = |channel: f64| (channel.clamp(0.0, 1.0) * 255.0).round();
        (byte(self.r), byte(self.g), byte(self.b), byte(self.a))
    }

    pub fn to_hex(self, include_alpha: bool) -> String {
        let (r, g, b, a) = self.to_rgb();
        let mut hex = format!("#{:02x}{:02x}{:02x}", r as u8, g as u8, b as u8);
        if include_alpha {
            hex.push_str(&format!("{:02x}", a as u8));
        }
        hex
    }

    pub fn lerp(self, goal: Color, alpha: f64) -> Color {
        self.zip(goal, |from, to| from + (to - from) * alpha)
    }

    fn zip(self, other: Color, operation: impl Fn(f64, f64) -> f64) -> Color {
        Color::new(
            operation(self.r, other.r),
            operation(self.g, other.g),
            operation(self.b, other.b),
            operation(self.a, other.a),
        )
    }

    fn tint(self, operation: impl Fn(f64) -> f64) -> Color {
        Color::new(operation(self.r), operation(self.g), operation(self.b), self.a)
    }
}

impl FromLua for Color {
    fn from_lua(value: Value, _: &Lua) -> Result<Self> {
        match value {
            Value::UserData(userdata) if userdata.is::<Color>() => Ok(*userdata.borrow::<Color>()?),
            other => Err(mlua::Error::FromLuaConversionError {
                from: other.type_name(),
                to: Self::TYPE_NAME.to_owned(),
                message: None,
            }),
        }
    }
}

impl UserData for Color {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_meta_field(MetaMethod::Type, Self::TYPE_NAME);
        fields.add_field_method_get("R", |_, this| Ok(this.r));
        fields.add_field_method_get("G", |_, this| Ok(this.g));
        fields.add_field_method_get("B", |_, this| Ok(this.b));
        fields.add_field_method_get("A", |_, this| Ok(this.a));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Lerp", |_, this, (goal, alpha): (Color, f64)| Ok(this.lerp(goal, alpha)));
        methods.add_method("ToHex", |_, this, include_alpha: Option<bool>| {
            Ok(this.to_hex(include_alpha.unwrap_or(false)))
        });
        methods.add_method("ToRGB", |_, this, ()| Ok(this.to_rgb()));
        methods.add_method("ToHSV", |_, this, ()| Ok(this.to_hsv()));

        methods.add_meta_function(MetaMethod::Add, |_, (left, right): (Color, Color)| {
            Ok(left.zip(right, |a, b| a + b))
        });
        methods.add_meta_function(MetaMethod::Sub, |_, (left, right): (Color, Color)| {
            Ok(left.zip(right, |a, b| a - b))
        });
        methods.add_meta_function(MetaMethod::Mul, |_, (left, right): (Operand<Color>, Operand<Color>)| {
            match (left, right) {
                (Operand::Value(a), Operand::Value(b)) => Ok(a.zip(b, |a, b| a * b)),
                (Operand::Value(color), Operand::Number(factor)) | (Operand::Number(factor), Operand::Value(color)) => {
                    Ok(color.tint(|channel| channel * factor))
                }
                (Operand::Number(_), Operand::Number(_)) => Err(mixed(Self::TYPE_NAME)),
            }
        });
        methods.add_meta_function(MetaMethod::Div, |_, (left, right): (Operand<Color>, Operand<Color>)| {
            match (left, right) {
                (Operand::Value(a), Operand::Value(b)) => Ok(a.zip(b, |a, b| a / b)),
                (Operand::Value(color), Operand::Number(divisor)) => Ok(color.tint(|channel| channel / divisor)),
                (Operand::Number(dividend), Operand::Value(color)) => Ok(color.tint(|channel| dividend / channel)),
                (Operand::Number(_), Operand::Number(_)) => Err(mixed(Self::TYPE_NAME)),
            }
        });
        methods.add_meta_function(MetaMethod::Eq, |_, (left, right): (Color, Color)| Ok(left == right));
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!(
                "Color({}, {}, {}, {})",
                format_number(this.r),
                format_number(this.g),
                format_number(this.b),
                format_number(this.a)
            ))
        });
    }
}
