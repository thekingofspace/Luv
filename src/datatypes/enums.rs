use mlua::{AnyUserData, FromLua, Lua, MetaMethod, Result, Table, UserData, UserDataFields, UserDataMethods, Value};

pub const GLOBAL: &str = "enum";
const REGISTRY: &str = "luv.enums";

pub const WINDOW_TYPE: &str = "WindowType";
pub const SHAPE_TYPE: &str = "ShapeType";
pub const TEXT_X_ALIGNMENT: &str = "TextXAlignment";
pub const TEXT_Y_ALIGNMENT: &str = "TextYAlignment";
pub const RESAMPLE_MODE: &str = "ResampleMode";
pub const BLEND_MODE: &str = "BlendMode";
pub const HASH_ALGORITHM: &str = "HashAlgorithm";
pub const CIPHER_ALGORITHM: &str = "CipherAlgorithm";
pub const KEY_ALGORITHM: &str = "KeyAlgorithm";
pub const KEY_CODE: &str = "KeyCode";
pub const MOUSE_BUTTON: &str = "MouseButton";
pub const MOUSE_ICON: &str = "MouseIcon";
pub const MOUSE_LOCK_MODE: &str = "MouseLockMode";
pub const CONTROLLER_BUTTON: &str = "ControllerButton";
pub const CONTROLLER_AXIS: &str = "ControllerAxis";
pub const CONTROLLER_STICK: &str = "ControllerStick";
pub const AUDIO_FORMAT: &str = "AudioFormat";
pub const ROLL_OFF_MODE: &str = "RollOffMode";

pub const KEY_CODES: &[&str] = &[
    "Unknown", "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T",
    "U", "V", "W", "X", "Y", "Z", "Zero", "One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine",
    "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12", "F13", "F14", "F15", "F16", "F17",
    "F18", "F19", "F20", "F21", "F22", "F23", "F24", "Space", "Return", "Escape", "Tab", "Backspace", "Delete",
    "Insert", "Home", "End", "PageUp", "PageDown", "Up", "Down", "Left", "Right", "LeftShift", "RightShift",
    "LeftControl", "RightControl", "LeftAlt", "RightAlt", "LeftSuper", "RightSuper", "CapsLock", "NumLock",
    "ScrollLock", "PrintScreen", "Pause", "Menu", "Backquote", "Minus", "Equals", "LeftBracket", "RightBracket",
    "Backslash", "Semicolon", "Quote", "Comma", "Period", "Slash", "KeypadZero", "KeypadOne", "KeypadTwo",
    "KeypadThree", "KeypadFour", "KeypadFive", "KeypadSix", "KeypadSeven", "KeypadEight", "KeypadNine",
    "KeypadPeriod", "KeypadDivide", "KeypadMultiply", "KeypadMinus", "KeypadPlus", "KeypadEnter", "KeypadEquals",
];

const ENUMS: &[(&str, &[&str])] = &[
    (AUDIO_FORMAT, &["Float32", "Int16"]),
    (BLEND_MODE, &["Alpha", "Additive", "Multiply", "Opaque"]),
    (CIPHER_ALGORITHM, &["AES128GCM", "AES256GCM", "ChaCha20Poly1305"]),
    (
        HASH_ALGORITHM,
        &["MD5", "SHA1", "SHA224", "SHA256", "SHA384", "SHA512", "SHA3_256", "SHA3_384", "SHA3_512", "BLAKE3"],
    ),
    (
        CONTROLLER_AXIS,
        &["LeftStickX", "LeftStickY", "RightStickX", "RightStickY", "LeftTrigger", "RightTrigger"],
    ),
    (
        CONTROLLER_BUTTON,
        &[
            "A", "B", "X", "Y", "LeftBumper", "RightBumper", "LeftTrigger", "RightTrigger", "Select", "Start", "Home",
            "LeftStick", "RightStick", "DPadUp", "DPadDown", "DPadLeft", "DPadRight",
        ],
    ),
    (CONTROLLER_STICK, &["Left", "Right"]),
    (KEY_ALGORITHM, &["Ed25519", "EcdsaP256", "EcdsaP384", "X25519"]),
    (KEY_CODE, KEY_CODES),
    (MOUSE_BUTTON, &["Left", "Right", "Middle", "Back", "Forward"]),
    (
        MOUSE_ICON,
        &[
            "Default", "Pointer", "Text", "Crosshair", "Wait", "Progress", "Move", "NotAllowed", "Grab", "Grabbing",
            "Help", "ResizeHorizontal", "ResizeVertical", "ResizeDiagonalDown", "ResizeDiagonalUp", "ZoomIn",
            "ZoomOut", "Cell", "Copy", "ContextMenu",
        ],
    ),
    (MOUSE_LOCK_MODE, &["None", "Confined", "Locked"]),
    (RESAMPLE_MODE, &["Smooth", "Pixelated"]),
    (ROLL_OFF_MODE, &["Inverse", "Linear", "LinearSquare", "InverseTapered"]),
    (
        SHAPE_TYPE,
        &["Rectangle", "Circle", "Triangle", "RightTriangle", "Diamond", "Pentagon", "Hexagon", "Octagon"],
    ),
    (TEXT_X_ALIGNMENT, &["Left", "Center", "Right"]),
    (TEXT_Y_ALIGNMENT, &["Top", "Center", "Bottom"]),
    (
        WINDOW_TYPE,
        &["Windowed", "Borderless", "Maximized", "FullScreen", "ExclusiveFullScreen"],
    ),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnumItem {
    pub enum_type: &'static str,
    pub name: &'static str,
    pub value: u32,
}

impl EnumItem {
    pub const TYPE_NAME: &'static str = "EnumItem";

    pub fn find(enum_type: &str, name: &str) -> Option<EnumItem> {
        entries(enum_type).and_then(|(enum_type, names)| {
            let index = names.iter().position(|entry| *entry == name)?;
            Some(EnumItem {
                enum_type,
                name: names[index],
                value: index as u32,
            })
        })
    }

    pub fn canonical(self, lua: &Lua) -> Result<Value> {
        let enums: Table = lua.named_registry_value(REGISTRY)?;
        let items: Table = enums.get(self.enum_type)?;
        items.get(self.name)
    }

    pub fn of(self, enum_type: &str) -> Result<EnumItem> {
        if self.enum_type != enum_type {
            return Err(mlua::Error::runtime(format!(
                "expected an {GLOBAL}.{enum_type} item, got {GLOBAL}.{}.{}",
                self.enum_type, self.name
            )));
        }
        Ok(self)
    }

    pub fn named(enum_type: &str, value: u32) -> Option<EnumItem> {
        entries(enum_type).and_then(|(enum_type, names)| {
            let name = names.get(value as usize)?;
            Some(EnumItem {
                enum_type,
                name,
                value,
            })
        })
    }
}

fn entries(enum_type: &str) -> Option<(&'static str, &'static [&'static str])> {
    ENUMS
        .iter()
        .find(|(name, _)| *name == enum_type)
        .map(|(name, names)| (*name, *names))
}

pub fn items(enum_type: &str) -> Vec<EnumItem> {
    ENUMS
        .iter()
        .filter(|(name, _)| *name == enum_type)
        .flat_map(|(enum_type, names)| {
            names.iter().enumerate().map(|(index, name)| EnumItem {
                enum_type,
                name,
                value: index as u32,
            })
        })
        .collect()
}

impl FromLua for EnumItem {
    fn from_lua(value: Value, _: &Lua) -> Result<Self> {
        match value {
            Value::UserData(userdata) if userdata.is::<EnumItem>() => Ok(*userdata.borrow::<EnumItem>()?),
            other => Err(mlua::Error::FromLuaConversionError {
                from: other.type_name(),
                to: Self::TYPE_NAME.to_owned(),
                message: None,
            }),
        }
    }
}

impl UserData for EnumItem {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_meta_field(MetaMethod::Type, Self::TYPE_NAME);
        fields.add_field_method_get("Name", |_, this| Ok(this.name));
        fields.add_field_method_get("Value", |_, this| Ok(this.value));
        fields.add_field_method_get("EnumType", |_, this| Ok(this.enum_type));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_function(MetaMethod::Eq, |_, (left, right): (EnumItem, EnumItem)| Ok(left == right));
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!("{GLOBAL}.{}.{}", this.enum_type, this.name))
        });
    }
}

pub fn install(lua: &Lua) -> Result<()> {
    let root = lua.create_table()?;
    for (enum_type, _) in ENUMS {
        let table = lua.create_table()?;
        let created = items(enum_type)
            .into_iter()
            .map(|item| lua.create_userdata(item))
            .collect::<Result<Vec<AnyUserData>>>()?;
        for (item, userdata) in items(enum_type).into_iter().zip(&created) {
            table.set(item.name, userdata.clone())?;
        }
        table.set(
            "GetEnumItems",
            lua.create_function(move |lua, _: Value| lua.create_sequence_from(created.clone()))?,
        )?;
        table.set_readonly(true);
        root.set(*enum_type, table)?;
    }
    root.set_readonly(true);
    lua.set_named_registry_value(REGISTRY, root.clone())?;
    lua.globals().set(GLOBAL, root)
}
