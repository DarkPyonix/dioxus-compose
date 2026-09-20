//! Fixed-layout little-endian boundary protocol.

use crate::schema::{Modifier, PropertyKind, Selection, WidgetKind};
use core::fmt;

const TAG_ENVELOPE: u16 = 0;
const TAG_CREATE: u16 = 1;
const TAG_SET_PROP: u16 = 2;
const TAG_SET_MODIFIER: u16 = 3;
const TAG_INSERT: u16 = 4;
const TAG_MOVE: u16 = 5;
const TAG_REMOVE: u16 = 6;
const TAG_SET_TEXT: u16 = 7;
const ENVELOPE_LEN: usize = 12;

const VALUE_NONE: u16 = 0;
const VALUE_STRING: u16 = 1;
const VALUE_BOOL: u16 = 2;
const VALUE_I64: u16 = 3;
const VALUE_F32: u16 = 4;

#[derive(Clone, Debug, PartialEq)]
pub enum PropertyValue<'a> {
    None,
    String(&'a str),
    Bool(bool),
    Integer(i64),
    Float(f32),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Mutation<'a> {
    Create {
        node_id: u32,
        widget: WidgetKind,
    },
    SetProp {
        node_id: u32,
        property: PropertyKind,
        value: PropertyValue<'a>,
    },
    SetModifier {
        node_id: u32,
        index: u16,
        modifier: Modifier,
    },
    Insert {
        parent_id: u32,
        node_id: u32,
        index: u32,
    },
    Move {
        parent_id: u32,
        node_id: u32,
        index: u32,
    },
    Remove {
        node_id: u32,
    },
    SetText {
        node_id: u32,
        text: &'a str,
        selection: Option<Selection>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProtocolError {
    Truncated,
    InvalidEnvelope,
    InvalidRecordLength,
    InvalidTag(u16),
    InvalidWidget(u16),
    InvalidProperty(u16),
    InvalidValueKind(u16),
    InvalidModifier(u16),
    InvalidStringRange,
    InvalidUtf8,
    LengthOverflow,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HostEvent<'a> {
    pub node_id: u32,
    pub handler_id: u64,
    pub payload: crate::schema::EventPayload<'a>,
}

const EVENT_CLICK: u16 = 1;
const EVENT_TEXT_CHANGED: u16 = 2;
const EVENT_TEXT_SUBMITTED: u16 = 3;
const EVENT_FOCUS_LOST: u16 = 4;
const EVENT_PROTOCOL_ERROR: u16 = 5;

/// Decode one Renderer-to-Host event record.
pub fn decode_event(bytes: &[u8]) -> Result<HostEvent<'_>, ProtocolError> {
    if bytes.len() < 16 {
        return Err(ProtocolError::Truncated);
    }
    let tag = read_u16(bytes, 0)?;
    let record_len = usize::from(read_u16(bytes, 2)?);
    if record_len < 16 || record_len % 4 != 0 || record_len > bytes.len() {
        return Err(ProtocolError::InvalidRecordLength);
    }
    let node_id = read_u32(bytes, 4)?;
    let handler_id = read_u64(bytes, 8)?;
    let payload = match tag {
        EVENT_CLICK if record_len == 16 => crate::schema::EventPayload::Clicked,
        EVENT_TEXT_CHANGED if record_len == 24 => {
            crate::schema::EventPayload::TextChanged(read_string(bytes, 16)?)
        }
        EVENT_TEXT_SUBMITTED if record_len == 24 => {
            crate::schema::EventPayload::TextSubmitted(read_string(bytes, 16)?)
        }
        EVENT_FOCUS_LOST if record_len == 16 => crate::schema::EventPayload::FocusLost,
        EVENT_PROTOCOL_ERROR if record_len == 28 => crate::schema::EventPayload::ProtocolError {
            code: read_u32(bytes, 16)?,
            message: read_string(bytes, 20)?,
        },
        EVENT_CLICK..=EVENT_PROTOCOL_ERROR => return Err(ProtocolError::InvalidRecordLength),
        other => return Err(ProtocolError::InvalidTag(other)),
    };
    Ok(HostEvent {
        node_id,
        handler_id,
        payload,
    })
}

/// Test/mock helper for constructing a Renderer-to-Host event.
pub fn encode_event(event: &HostEvent<'_>, output: &mut Vec<u8>) -> Result<(), ProtocolError> {
    output.clear();
    let (tag, record_len, text, error_code) = match &event.payload {
        crate::schema::EventPayload::Clicked => (EVENT_CLICK, 16_u16, None, None),
        crate::schema::EventPayload::TextChanged(value) => {
            (EVENT_TEXT_CHANGED, 24, Some(*value), None)
        }
        crate::schema::EventPayload::TextSubmitted(value) => {
            (EVENT_TEXT_SUBMITTED, 24, Some(*value), None)
        }
        crate::schema::EventPayload::FocusLost => (EVENT_FOCUS_LOST, 16, None, None),
        crate::schema::EventPayload::ProtocolError { code, message } => {
            (EVENT_PROTOCOL_ERROR, 28, Some(*message), Some(*code))
        }
    };
    output.extend_from_slice(&tag.to_le_bytes());
    output.extend_from_slice(&record_len.to_le_bytes());
    output.extend_from_slice(&event.node_id.to_le_bytes());
    output.extend_from_slice(&event.handler_id.to_le_bytes());
    if let Some(code) = error_code {
        output.extend_from_slice(&code.to_le_bytes());
    }
    if let Some(text) = text {
        let offset = u32::from(record_len);
        let len = u32::try_from(text.len()).map_err(|_| ProtocolError::LengthOverflow)?;
        output.extend_from_slice(&offset.to_le_bytes());
        output.extend_from_slice(&len.to_le_bytes());
        output.extend_from_slice(text.as_bytes());
    }
    Ok(())
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "protocol error: {self:?}")
    }
}

impl std::error::Error for ProtocolError {}

/// Reusable encoder. Once capacities are warmed, `clear` and `finish` allocate no memory.
pub struct BatchEncoder {
    records: Vec<u8>,
    strings: Vec<u8>,
    string_fixups: Vec<usize>,
    arena: Vec<u8>,
    record_count: u32,
}

impl Default for BatchEncoder {
    fn default() -> Self {
        Self::with_capacity(4096, 1024, 64)
    }
}

impl BatchEncoder {
    pub fn with_capacity(record_bytes: usize, string_bytes: usize, string_count: usize) -> Self {
        let mut encoder = Self {
            records: Vec::with_capacity(record_bytes),
            strings: Vec::with_capacity(string_bytes),
            string_fixups: Vec::with_capacity(string_count),
            arena: Vec::with_capacity(record_bytes + string_bytes),
            record_count: 0,
        };
        encoder.clear();
        encoder
    }

    pub fn clear(&mut self) {
        self.records.clear();
        self.strings.clear();
        self.string_fixups.clear();
        self.arena.clear();
        self.record_count = 0;
        self.begin_record(TAG_ENVELOPE, 8);
        self.put_u32(0);
        self.put_u32(0);
    }

    pub fn encode(&mut self, mutation: &Mutation<'_>) -> Result<(), ProtocolError> {
        match mutation {
            Mutation::Create { node_id, widget } => {
                self.begin_record(TAG_CREATE, 8);
                self.put_u32(*node_id);
                self.put_u16(*widget as u16);
                self.put_u16(0);
            }
            Mutation::SetProp {
                node_id,
                property,
                value,
            } => {
                self.begin_record(TAG_SET_PROP, 20);
                self.put_u32(*node_id);
                self.put_u16(*property as u16);
                match value {
                    PropertyValue::None => {
                        self.put_u16(VALUE_NONE);
                        self.put_u64(0);
                        self.put_u32(0);
                    }
                    PropertyValue::String(value) => {
                        self.put_u16(VALUE_STRING);
                        self.put_string_ref(value)?;
                        self.put_u32(0);
                    }
                    PropertyValue::Bool(value) => {
                        self.put_u16(VALUE_BOOL);
                        self.put_u64(u64::from(*value));
                        self.put_u32(0);
                    }
                    PropertyValue::Integer(value) => {
                        self.put_u16(VALUE_I64);
                        self.put_u64(*value as u64);
                        self.put_u32(0);
                    }
                    PropertyValue::Float(value) => {
                        self.put_u16(VALUE_F32);
                        self.put_u64(u64::from(value.to_bits()));
                        self.put_u32(0);
                    }
                }
            }
            Mutation::SetModifier {
                node_id,
                index,
                modifier,
            } => {
                self.begin_record(TAG_SET_MODIFIER, 24);
                self.put_u32(*node_id);
                self.put_u16(*index);
                let (tag, first, second) = modifier_fields(modifier);
                self.put_u16(tag);
                self.put_u64(first);
                self.put_u64(second);
            }
            Mutation::Insert {
                parent_id,
                node_id,
                index,
            } => {
                self.begin_record(TAG_INSERT, 12);
                self.put_u32(*parent_id);
                self.put_u32(*node_id);
                self.put_u32(*index);
            }
            Mutation::Move {
                parent_id,
                node_id,
                index,
            } => {
                self.begin_record(TAG_MOVE, 12);
                self.put_u32(*parent_id);
                self.put_u32(*node_id);
                self.put_u32(*index);
            }
            Mutation::Remove { node_id } => {
                self.begin_record(TAG_REMOVE, 4);
                self.put_u32(*node_id);
            }
            Mutation::SetText {
                node_id,
                text,
                selection,
            } => {
                self.begin_record(TAG_SET_TEXT, 20);
                self.put_u32(*node_id);
                self.put_string_ref(text)?;
                let (start, end) =
                    selection.map_or((u32::MAX, u32::MAX), |value| (value.start, value.end));
                self.put_u32(start);
                self.put_u32(end);
            }
        }
        self.record_count = self
            .record_count
            .checked_add(1)
            .ok_or(ProtocolError::LengthOverflow)?;
        Ok(())
    }

    pub fn finish(&mut self) -> Result<&[u8], ProtocolError> {
        let records_len =
            u32::try_from(self.records.len()).map_err(|_| ProtocolError::LengthOverflow)?;
        self.records[4..8].copy_from_slice(&records_len.to_le_bytes());
        self.records[8..12].copy_from_slice(&self.record_count.to_le_bytes());
        let total = self
            .records
            .len()
            .checked_add(self.strings.len())
            .ok_or(ProtocolError::LengthOverflow)?;
        self.arena.clear();
        if self.arena.capacity() < total {
            self.arena.reserve(total);
        }
        self.arena.extend_from_slice(&self.records);
        self.arena.extend_from_slice(&self.strings);
        for &position in &self.string_fixups {
            let relative = read_u32(&self.arena, position)?;
            let absolute = records_len
                .checked_add(relative)
                .ok_or(ProtocolError::LengthOverflow)?;
            self.arena[position..position + 4].copy_from_slice(&absolute.to_le_bytes());
        }
        Ok(&self.arena)
    }

    pub fn capacity(&self) -> usize {
        self.records.capacity() + self.strings.capacity() + self.arena.capacity()
    }

    fn begin_record(&mut self, tag: u16, payload_len: u16) {
        self.put_u16(tag);
        self.put_u16(payload_len + 4);
    }

    fn put_string_ref(&mut self, value: &str) -> Result<(), ProtocolError> {
        let offset =
            u32::try_from(self.strings.len()).map_err(|_| ProtocolError::LengthOverflow)?;
        let len = u32::try_from(value.len()).map_err(|_| ProtocolError::LengthOverflow)?;
        self.string_fixups.push(self.records.len());
        self.put_u32(offset);
        self.put_u32(len);
        self.strings.extend_from_slice(value.as_bytes());
        Ok(())
    }

    fn put_u16(&mut self, value: u16) {
        self.records.extend_from_slice(&value.to_le_bytes());
    }

    fn put_u32(&mut self, value: u32) {
        self.records.extend_from_slice(&value.to_le_bytes());
    }

    fn put_u64(&mut self, value: u64) {
        self.records.extend_from_slice(&value.to_le_bytes());
    }
}

pub fn decode_batch(bytes: &[u8]) -> Result<Vec<Mutation<'_>>, ProtocolError> {
    if bytes.len() < ENVELOPE_LEN
        || read_u16(bytes, 0)? != TAG_ENVELOPE
        || read_u16(bytes, 2)? != 12
    {
        return Err(ProtocolError::InvalidEnvelope);
    }
    let records_len =
        usize::try_from(read_u32(bytes, 4)?).map_err(|_| ProtocolError::LengthOverflow)?;
    let count = usize::try_from(read_u32(bytes, 8)?).map_err(|_| ProtocolError::LengthOverflow)?;
    if records_len < ENVELOPE_LEN || records_len > bytes.len() || records_len % 4 != 0 {
        return Err(ProtocolError::InvalidEnvelope);
    }
    let mut output = Vec::with_capacity(count);
    let mut position = ENVELOPE_LEN;
    while position < records_len {
        let tag = read_u16(bytes, position)?;
        let len = usize::from(read_u16(bytes, position + 2)?);
        if len < 4 || len % 4 != 0 || position + len > records_len {
            return Err(ProtocolError::InvalidRecordLength);
        }
        let payload = position + 4;
        let mutation = match tag {
            TAG_CREATE if len == 12 => Mutation::Create {
                node_id: read_u32(bytes, payload)?,
                widget: WidgetKind::try_from(read_u16(bytes, payload + 4)?).map_err(|()| {
                    ProtocolError::InvalidWidget(read_u16(bytes, payload + 4).unwrap_or(0))
                })?,
            },
            TAG_SET_PROP if len == 24 => {
                let property_raw = read_u16(bytes, payload + 4)?;
                let value_kind = read_u16(bytes, payload + 6)?;
                let value = match value_kind {
                    VALUE_NONE => PropertyValue::None,
                    VALUE_STRING => PropertyValue::String(read_string(bytes, payload + 8)?),
                    VALUE_BOOL => PropertyValue::Bool(read_u64(bytes, payload + 8)? != 0),
                    VALUE_I64 => PropertyValue::Integer(read_u64(bytes, payload + 8)? as i64),
                    VALUE_F32 => {
                        PropertyValue::Float(f32::from_bits(read_u64(bytes, payload + 8)? as u32))
                    }
                    other => return Err(ProtocolError::InvalidValueKind(other)),
                };
                Mutation::SetProp {
                    node_id: read_u32(bytes, payload)?,
                    property: PropertyKind::try_from(property_raw)
                        .map_err(|()| ProtocolError::InvalidProperty(property_raw))?,
                    value,
                }
            }
            TAG_SET_MODIFIER if len == 28 => Mutation::SetModifier {
                node_id: read_u32(bytes, payload)?,
                index: read_u16(bytes, payload + 4)?,
                modifier: decode_modifier(
                    read_u16(bytes, payload + 6)?,
                    read_u64(bytes, payload + 8)?,
                    read_u64(bytes, payload + 16)?,
                )?,
            },
            TAG_INSERT if len == 16 => Mutation::Insert {
                parent_id: read_u32(bytes, payload)?,
                node_id: read_u32(bytes, payload + 4)?,
                index: read_u32(bytes, payload + 8)?,
            },
            TAG_MOVE if len == 16 => Mutation::Move {
                parent_id: read_u32(bytes, payload)?,
                node_id: read_u32(bytes, payload + 4)?,
                index: read_u32(bytes, payload + 8)?,
            },
            TAG_REMOVE if len == 8 => Mutation::Remove {
                node_id: read_u32(bytes, payload)?,
            },
            TAG_SET_TEXT if len == 24 => {
                let start = read_u32(bytes, payload + 12)?;
                let end = read_u32(bytes, payload + 16)?;
                Mutation::SetText {
                    node_id: read_u32(bytes, payload)?,
                    text: read_string(bytes, payload + 4)?,
                    selection: (start != u32::MAX || end != u32::MAX)
                        .then_some(Selection { start, end }),
                }
            }
            TAG_CREATE..=TAG_SET_TEXT => {
                return Err(ProtocolError::InvalidRecordLength);
            }
            other => return Err(ProtocolError::InvalidTag(other)),
        };
        output.push(mutation);
        position += len;
    }
    if output.len() != count {
        return Err(ProtocolError::InvalidEnvelope);
    }
    Ok(output)
}

fn modifier_fields(modifier: &Modifier) -> (u16, u64, u64) {
    match modifier {
        Modifier::Empty => (0, 0, 0),
        Modifier::Padding(value) => (1, u64::from(value.to_bits()), 0),
        Modifier::FillMaxWidth => (2, 0, 0),
        Modifier::FillMaxHeight => (3, 0, 0),
        Modifier::Width(value) => (4, u64::from(value.to_bits()), 0),
        Modifier::Height(value) => (5, u64::from(value.to_bits()), 0),
        Modifier::Size { width, height } => {
            (6, u64::from(width.to_bits()), u64::from(height.to_bits()))
        }
        Modifier::Background(argb) => (7, u64::from(*argb), 0),
        Modifier::Clickable { handler_id } => (8, *handler_id, 0),
    }
}

fn decode_modifier(tag: u16, first: u64, second: u64) -> Result<Modifier, ProtocolError> {
    match tag {
        0 => Ok(Modifier::Empty),
        1 => Ok(Modifier::Padding(f32::from_bits(first as u32))),
        2 => Ok(Modifier::FillMaxWidth),
        3 => Ok(Modifier::FillMaxHeight),
        4 => Ok(Modifier::Width(f32::from_bits(first as u32))),
        5 => Ok(Modifier::Height(f32::from_bits(first as u32))),
        6 => Ok(Modifier::Size {
            width: f32::from_bits(first as u32),
            height: f32::from_bits(second as u32),
        }),
        7 => Ok(Modifier::Background(first as u32)),
        8 => Ok(Modifier::Clickable { handler_id: first }),
        other => Err(ProtocolError::InvalidModifier(other)),
    }
}

fn read_string(bytes: &[u8], position: usize) -> Result<&str, ProtocolError> {
    let offset =
        usize::try_from(read_u32(bytes, position)?).map_err(|_| ProtocolError::LengthOverflow)?;
    let len = usize::try_from(read_u32(bytes, position + 4)?)
        .map_err(|_| ProtocolError::LengthOverflow)?;
    let end = offset
        .checked_add(len)
        .ok_or(ProtocolError::InvalidStringRange)?;
    let value = bytes
        .get(offset..end)
        .ok_or(ProtocolError::InvalidStringRange)?;
    std::str::from_utf8(value).map_err(|_| ProtocolError::InvalidUtf8)
}

fn read_u16(bytes: &[u8], position: usize) -> Result<u16, ProtocolError> {
    let value = bytes
        .get(position..position + 2)
        .ok_or(ProtocolError::Truncated)?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], position: usize) -> Result<u32, ProtocolError> {
    let value = bytes
        .get(position..position + 4)
        .ok_or(ProtocolError::Truncated)?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn read_u64(bytes: &[u8], position: usize) -> Result<u64, ProtocolError> {
    let value = bytes
        .get(position..position + 8)
        .ok_or(ProtocolError::Truncated)?;
    Ok(u64::from_le_bytes(
        value.try_into().map_err(|_| ProtocolError::Truncated)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_all_m0_mutations() {
        let mutations = [
            Mutation::Create {
                node_id: 1,
                widget: WidgetKind::Column,
            },
            Mutation::SetProp {
                node_id: 2,
                property: PropertyKind::Text,
                value: PropertyValue::String("안녕"),
            },
            Mutation::SetProp {
                node_id: 3,
                property: PropertyKind::Enabled,
                value: PropertyValue::Bool(true),
            },
            Mutation::SetModifier {
                node_id: 1,
                index: 0,
                modifier: Modifier::Padding(16.0),
            },
            Mutation::Insert {
                parent_id: 1,
                node_id: 2,
                index: 0,
            },
            Mutation::Move {
                parent_id: 1,
                node_id: 2,
                index: 1,
            },
            Mutation::Remove { node_id: 3 },
            Mutation::SetText {
                node_id: 4,
                text: "compose",
                selection: Some(Selection { start: 1, end: 4 }),
            },
        ];
        let mut encoder = BatchEncoder::default();
        for mutation in &mutations {
            encoder.encode(mutation).unwrap();
        }
        let decoded = decode_batch(encoder.finish().unwrap()).unwrap();
        assert_eq!(decoded, mutations);
    }

    #[test]
    fn rejects_unknown_tags_without_panicking() {
        let mut bytes = vec![0, 0, 12, 0, 20, 0, 0, 0, 1, 0, 0, 0];
        bytes.extend_from_slice(&999_u16.to_le_bytes());
        bytes.extend_from_slice(&8_u16.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        assert_eq!(decode_batch(&bytes), Err(ProtocolError::InvalidTag(999)));
    }

    #[test]
    fn round_trips_renderer_events() {
        let events = [
            HostEvent {
                node_id: 7,
                handler_id: 11,
                payload: crate::EventPayload::Clicked,
            },
            HostEvent {
                node_id: 8,
                handler_id: 12,
                payload: crate::EventPayload::TextChanged("한글"),
            },
            HostEvent {
                node_id: 8,
                handler_id: 13,
                payload: crate::EventPayload::TextSubmitted("done"),
            },
            HostEvent {
                node_id: 8,
                handler_id: 14,
                payload: crate::EventPayload::FocusLost,
            },
        ];
        let mut bytes = Vec::new();
        for event in events {
            encode_event(&event, &mut bytes).unwrap();
            assert_eq!(decode_event(&bytes).unwrap(), event);
        }
    }
}
