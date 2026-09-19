//! Closed Rust-side schema used as the source for future Kotlin code generation.

/// Canonical schema text. Variant order is wire-significant and must only be appended to.
pub const SCHEMA_DESCRIPTOR: &str = concat!(
    "dioxus-compose/v1;",
    "widgets=Column,Row,Box,Text,TextField,Button,Spacer;",
    "properties=text,placeholder,enabled,multiline,on_click,on_value_change,on_submit,on_focus_lost;",
    "modifiers=Padding,FillMaxWidth,FillMaxHeight,Width,Height,Size,Background,Clickable;",
    "events=Click,TextChanged,TextSubmitted,FocusLost,ProtocolError;",
    "commands=Create,SetProp,SetModifier,Insert,Move,Remove,SetText"
);

const fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut index = 0;
    while index < bytes.len() {
        hash ^= bytes[index] as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        index += 1;
    }
    hash
}

/// Stable handshake hash of [`SCHEMA_DESCRIPTOR`].
pub const SCHEMA_HASH: u64 = fnv1a64(SCHEMA_DESCRIPTOR.as_bytes());
pub const PROTOCOL_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum WidgetKind {
    Column = 1,
    Row = 2,
    Box = 3,
    Text = 4,
    TextField = 5,
    Button = 6,
    Spacer = 7,
}

impl TryFrom<u16> for WidgetKind {
    type Error = ();

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Column),
            2 => Ok(Self::Row),
            3 => Ok(Self::Box),
            4 => Ok(Self::Text),
            5 => Ok(Self::TextField),
            6 => Ok(Self::Button),
            7 => Ok(Self::Spacer),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum LoopMode {
    #[default]
    Renderer = 0,
    Platform = 1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Selection {
    pub start: u32,
    pub end: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Modifier {
    Padding(f32),
    FillMaxWidth,
    FillMaxHeight,
    Width(f32),
    Height(f32),
    Size { width: f32, height: f32 },
    Background(u32),
    Clickable { handler_id: u64 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ColumnProps {
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RowProps {
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BoxProps {
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextProps<'a> {
    pub text: &'a str,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextFieldProps<'a> {
    pub placeholder: &'a str,
    pub enabled: bool,
    pub multiline: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ButtonProps<'a> {
    pub text: &'a str,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpacerProps {
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum PropertyKind {
    Text = 1,
    Placeholder = 2,
    Enabled = 3,
    Multiline = 4,
    OnClick = 5,
    OnValueChange = 6,
    OnSubmit = 7,
    OnFocusLost = 8,
}

impl TryFrom<u16> for PropertyKind {
    type Error = ();

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Text),
            2 => Ok(Self::Placeholder),
            3 => Ok(Self::Enabled),
            4 => Ok(Self::Multiline),
            5 => Ok(Self::OnClick),
            6 => Ok(Self::OnValueChange),
            7 => Ok(Self::OnSubmit),
            8 => Ok(Self::OnFocusLost),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum EventPayload<'a> {
    Click,
    TextChanged(&'a str),
    TextSubmitted(&'a str),
    FocusLost,
    ProtocolError { code: u32, message: &'a str },
}
