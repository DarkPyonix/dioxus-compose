//! Closed Rust-side schema and metadata used for Kotlin code generation.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EnumVariantSchema {
    pub name: &'static str,
    pub tag: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldType {
    Float,
    U32,
    U64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FieldSchema {
    pub name: &'static str,
    pub ty: FieldType,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VariantSchema {
    pub name: &'static str,
    pub tag: u16,
    pub fields: &'static [FieldSchema],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventPayloadType {
    None,
    Text,
    ProtocolError,
    KeyDown,
    Range,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EventSchema {
    pub name: &'static str,
    pub tag: u16,
    pub payload: EventPayloadType,
}

/// Canonical schema text. Variant order is wire-significant and must only be appended to.
pub const SCHEMA_DESCRIPTOR: &str = concat!(
    "dioxus-compose/v1;",
    "widgets=Column,Row,Box,Text,TextField,Button,Spacer,LazyColumn;",
    "properties=text,placeholder,enabled,multiline,on_click,on_value_change,on_submit,on_focus_lost,on_key_down,item_count,item_key,on_range_requested;",
    "modifiers=Empty,Padding,FillMaxWidth,FillMaxHeight,Width,Height,Size,Background,Clickable;",
    "keys=Enter;",
    "events=Clicked,TextChanged,TextSubmitted,FocusLost,ProtocolError,KeyDown,RangeRequested;",
    "commands=Create,SetProp,SetModifier,Insert,Move,Remove,SetText,AppendText"
);

const fn hash_bytes(mut hash: u64, bytes: &[u8]) -> u64 {
    let mut index = 0;
    while index < bytes.len() {
        hash ^= bytes[index] as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        index += 1;
    }
    hash
}

const fn hash_enum_schema(mut hash: u64, schema: &[EnumVariantSchema]) -> u64 {
    let mut index = 0;
    while index < schema.len() {
        hash = hash_bytes(hash, schema[index].name.as_bytes());
        hash = hash_bytes(hash, &schema[index].tag.to_le_bytes());
        index += 1;
    }
    hash
}

const fn schema_hash() -> u64 {
    let mut hash = hash_bytes(0xcbf2_9ce4_8422_2325_u64, SCHEMA_DESCRIPTOR.as_bytes());
    hash = hash_enum_schema(hash, WIDGET_SCHEMA);
    hash = hash_enum_schema(hash, PROPERTY_SCHEMA);
    hash = hash_enum_schema(hash, KEY_SCHEMA);
    let mut index = 0;
    while index < MODIFIER_SCHEMA.len() {
        let modifier = MODIFIER_SCHEMA[index];
        hash = hash_bytes(hash, modifier.name.as_bytes());
        hash = hash_bytes(hash, &modifier.tag.to_le_bytes());
        let mut field_index = 0;
        while field_index < modifier.fields.len() {
            let field = modifier.fields[field_index];
            hash = hash_bytes(hash, field.name.as_bytes());
            hash = hash_bytes(
                hash,
                &[match field.ty {
                    FieldType::Float => 1,
                    FieldType::U32 => 2,
                    FieldType::U64 => 3,
                }],
            );
            field_index += 1;
        }
        index += 1;
    }
    index = 0;
    while index < EVENT_SCHEMA.len() {
        let event = EVENT_SCHEMA[index];
        hash = hash_bytes(hash, event.name.as_bytes());
        hash = hash_bytes(hash, &event.tag.to_le_bytes());
        hash = hash_bytes(
            hash,
            &[match event.payload {
                EventPayloadType::None => 0,
                EventPayloadType::Text => 1,
                EventPayloadType::ProtocolError => 2,
                EventPayloadType::KeyDown => 3,
                EventPayloadType::Range => 4,
            }],
        );
        index += 1;
    }
    hash
}

/// Stable handshake hash of the complete ordered schema metadata.
pub const SCHEMA_HASH: u64 = schema_hash();
pub const PROTOCOL_VERSION: u16 = 1;

macro_rules! define_wire_enum {
    ($schema:ident, $name:ident { $($variant:ident = $tag:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        #[repr(u16)]
        pub enum $name {
            $($variant = $tag),+
        }

        pub const $schema: &[EnumVariantSchema] = &[
            $(EnumVariantSchema { name: stringify!($variant), tag: $tag }),+
        ];

        impl TryFrom<u16> for $name {
            type Error = ();

            fn try_from(value: u16) -> Result<Self, Self::Error> {
                match value {
                    $($tag => Ok(Self::$variant),)+
                    _ => Err(()),
                }
            }
        }
    };
}

define_wire_enum!(WIDGET_SCHEMA, WidgetKind {
    Column = 1,
    Row = 2,
    Box = 3,
    Text = 4,
    TextField = 5,
    Button = 6,
    Spacer = 7,
    LazyColumn = 8,
});

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

// Compose key identities supported by the M0 schema.
define_wire_enum!(KEY_SCHEMA, Key {
    Enter = 1,
});

#[derive(Clone, Debug, PartialEq)]
pub enum Modifier {
    Empty,
    Padding(f32),
    FillMaxWidth,
    FillMaxHeight,
    Width(f32),
    Height(f32),
    Size { width: f32, height: f32 },
    Background(u32),
    Clickable { handler_id: u64 },
}

const NO_FIELDS: &[FieldSchema] = &[];
const VALUE_FLOAT_FIELD: &[FieldSchema] = &[FieldSchema {
    name: "value",
    ty: FieldType::Float,
}];
const SIZE_FIELDS: &[FieldSchema] = &[
    FieldSchema {
        name: "width",
        ty: FieldType::Float,
    },
    FieldSchema {
        name: "height",
        ty: FieldType::Float,
    },
];
const ARGB_FIELD: &[FieldSchema] = &[FieldSchema {
    name: "argb",
    ty: FieldType::U32,
}];
const HANDLER_ID_FIELD: &[FieldSchema] = &[FieldSchema {
    name: "handlerId",
    ty: FieldType::U64,
}];

pub const MODIFIER_SCHEMA: &[VariantSchema] = &[
    VariantSchema {
        name: "Empty",
        tag: 0,
        fields: NO_FIELDS,
    },
    VariantSchema {
        name: "Padding",
        tag: 1,
        fields: VALUE_FLOAT_FIELD,
    },
    VariantSchema {
        name: "FillMaxWidth",
        tag: 2,
        fields: NO_FIELDS,
    },
    VariantSchema {
        name: "FillMaxHeight",
        tag: 3,
        fields: NO_FIELDS,
    },
    VariantSchema {
        name: "Width",
        tag: 4,
        fields: VALUE_FLOAT_FIELD,
    },
    VariantSchema {
        name: "Height",
        tag: 5,
        fields: VALUE_FLOAT_FIELD,
    },
    VariantSchema {
        name: "Size",
        tag: 6,
        fields: SIZE_FIELDS,
    },
    VariantSchema {
        name: "Background",
        tag: 7,
        fields: ARGB_FIELD,
    },
    VariantSchema {
        name: "Clickable",
        tag: 8,
        fields: HANDLER_ID_FIELD,
    },
];

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

define_wire_enum!(PROPERTY_SCHEMA, PropertyKind {
    Text = 1,
    Placeholder = 2,
    Enabled = 3,
    Multiline = 4,
    OnClick = 5,
    OnValueChange = 6,
    OnSubmit = 7,
    OnFocusLost = 8,
    OnKeyDown = 9,
    ItemCount = 10,
    ItemKey = 11,
    OnRangeRequested = 12,
});

#[derive(Clone, Debug, PartialEq)]
pub enum EventPayload<'a> {
    Clicked,
    TextChanged(&'a str),
    TextSubmitted(&'a str),
    FocusLost,
    ProtocolError {
        code: u32,
        message: &'a str,
    },
    KeyDown {
        key: Key,
        shift_key: bool,
        ctrl_key: bool,
        alt_key: bool,
        meta_key: bool,
    },
    /// FR-8: the Renderer asks the Host to materialise the visible item range.
    RangeRequested {
        start: u32,
        count: u32,
    },
}

pub const EVENT_SCHEMA: &[EventSchema] = &[
    EventSchema {
        name: "Clicked",
        tag: 1,
        payload: EventPayloadType::None,
    },
    EventSchema {
        name: "TextChanged",
        tag: 2,
        payload: EventPayloadType::Text,
    },
    EventSchema {
        name: "TextSubmitted",
        tag: 3,
        payload: EventPayloadType::Text,
    },
    EventSchema {
        name: "FocusLost",
        tag: 4,
        payload: EventPayloadType::None,
    },
    EventSchema {
        name: "ProtocolError",
        tag: 5,
        payload: EventPayloadType::ProtocolError,
    },
    EventSchema {
        name: "KeyDown",
        tag: 6,
        payload: EventPayloadType::KeyDown,
    },
    EventSchema {
        name: "RangeRequested",
        tag: 7,
        payload: EventPayloadType::Range,
    },
];
