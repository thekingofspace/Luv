use mlua::{FromLua, Lua, MetaMethod, Result, UserData, UserDataFields, UserDataMethods, Value};

use super::{Operand, format_number};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UDim {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl UDim {
    pub const TYPE_NAME: &'static str = "UDim";
    pub const ZERO: UDim = UDim { x: 0.0, y: 0.0, z: 0.0 };

    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    pub fn lerp(self, goal: UDim, alpha: f64) -> UDim {
        self.zip(goal, |from, to| from + (to - from) * alpha)
    }

    fn zip(self, other: UDim, operation: impl Fn(f64, f64) -> f64) -> UDim {
        UDim::new(operation(self.x, other.x), operation(self.y, other.y), operation(self.z, other.z))
    }

    fn each(self, operation: impl Fn(f64) -> f64) -> UDim {
        UDim::new(operation(self.x), operation(self.y), operation(self.z))
    }
}

impl FromLua for UDim {
    fn from_lua(value: Value, _: &Lua) -> Result<Self> {
        match value {
            Value::UserData(userdata) if userdata.is::<UDim>() => Ok(*userdata.borrow::<UDim>()?),
            other => Err(mlua::Error::FromLuaConversionError {
                from: other.type_name(),
                to: Self::TYPE_NAME.to_owned(),
                message: None,
            }),
        }
    }
}

impl UserData for UDim {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_meta_field(MetaMethod::Type, Self::TYPE_NAME);
        fields.add_field_method_get("X", |_, this| Ok(this.x));
        fields.add_field_method_get("Y", |_, this| Ok(this.y));
        fields.add_field_method_get("Z", |_, this| Ok(this.z));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Lerp", |_, this, (goal, alpha): (UDim, f64)| Ok(this.lerp(goal, alpha)));

        methods.add_meta_function(MetaMethod::Add, |_, (left, right): (UDim, UDim)| {
            Ok(left.zip(right, |a, b| a + b))
        });
        methods.add_meta_function(MetaMethod::Sub, |_, (left, right): (UDim, UDim)| {
            Ok(left.zip(right, |a, b| a - b))
        });
        methods.add_meta_function(MetaMethod::Mul, |_, (left, right): (Operand<UDim>, Operand<UDim>)| {
            match (left, right) {
                (Operand::Value(a), Operand::Value(b)) => Ok(a.zip(b, |a, b| a * b)),
                (Operand::Value(value), Operand::Number(factor)) | (Operand::Number(factor), Operand::Value(value)) => {
                    Ok(value.each(|component| component * factor))
                }
                (Operand::Number(_), Operand::Number(_)) => Err(super::mixed(Self::TYPE_NAME)),
            }
        });
        methods.add_meta_function(MetaMethod::Div, |_, (left, right): (Operand<UDim>, Operand<UDim>)| {
            match (left, right) {
                (Operand::Value(a), Operand::Value(b)) => Ok(a.zip(b, |a, b| a / b)),
                (Operand::Value(value), Operand::Number(divisor)) => Ok(value.each(|component| component / divisor)),
                (Operand::Number(dividend), Operand::Value(value)) => Ok(value.each(|component| dividend / component)),
                (Operand::Number(_), Operand::Number(_)) => Err(super::mixed(Self::TYPE_NAME)),
            }
        });
        methods.add_meta_method(MetaMethod::Unm, |_, this, ()| Ok(this.each(|component| -component)));
        methods.add_meta_function(MetaMethod::Eq, |_, (left, right): (UDim, UDim)| Ok(left == right));
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!(
                "UDim({}, {}, {})",
                format_number(this.x),
                format_number(this.y),
                format_number(this.z)
            ))
        });
    }
}
