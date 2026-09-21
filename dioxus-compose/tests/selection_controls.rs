//! The selection controls and the indicators: what they put on the wire, and what comes
//! back when the user moves one.

use dioxus_compose::EventPayload;
use dioxus_compose::protocol::{HostEvent, decode_event, encode_event};

/// The single value-change event. Tag 16 because the pointer gestures hold 8 to 15, and
/// one f64 because an epoch count, a slider position and a toggle all fit in it exactly.
#[test]
fn fr15_value_changed_is_tag_16_carrying_one_f64() {
    let event = HostEvent {
        node_id: 4,
        handler_id: 9,
        payload: EventPayload::ValueChanged(0.25),
    };
    let mut encoded = Vec::new();
    encode_event(&event, &mut encoded).unwrap();

    assert_eq!(u16::from_le_bytes([encoded[0], encoded[1]]), 16);
    assert_eq!(u16::from_le_bytes([encoded[2], encoded[3]]), 24);
    assert_eq!(encoded.len(), 24);
    assert_eq!(f64::from_le_bytes(encoded[16..24].try_into().unwrap()), 0.25);
    assert_eq!(decode_event(&encoded).unwrap(), event);
}

/// A fractional position survives the round trip. An integer payload would have rounded
/// it away, and a slider is the reason the event carries a float at all.
#[test]
fn fr15_a_fractional_value_survives_the_round_trip() {
    let event = HostEvent {
        node_id: 1,
        handler_id: 2,
        payload: EventPayload::ValueChanged(-19_723.5),
    };
    let mut encoded = Vec::new();
    encode_event(&event, &mut encoded).unwrap();
    assert_eq!(decode_event(&encoded).unwrap(), event);
}

/// The gesture tags are reserved, not implemented. A record claiming one of them is a
/// reported protocol error rather than a value change decoded off the wrong offset.
#[test]
fn fr15_the_reserved_gesture_tags_are_not_decoded_as_a_value_change() {
    let mut encoded = Vec::new();
    encode_event(
        &HostEvent {
            node_id: 1,
            handler_id: 2,
            payload: EventPayload::ValueChanged(1.0),
        },
        &mut encoded,
    )
    .unwrap();
    for tag in 8_u16..=15 {
        encoded[0..2].copy_from_slice(&tag.to_le_bytes());
        assert!(
            decode_event(&encoded).is_err(),
            "tag {tag} is reserved and must not decode"
        );
    }
}
