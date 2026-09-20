use dioxus_compose::codegen::{
    generate_event_vector, generate_kotlin, generate_mutation_vector, generate_vector_description,
};
use dioxus_compose::protocol::{HostEvent, decode_event, encode_event};
use dioxus_compose::{EventPayload, Key};

#[test]
fn fr7_generated_kotlin_matches_schema() {
    let generated = generate_kotlin();
    assert!(generated.contains("enum class Key { Enter }"));
    assert!(generated.contains(
        "data class KeyDown(override val nodeId: Int, override val handlerId: Long, val key: Key, val shiftKey: Boolean, val ctrlKey: Boolean, val altKey: Boolean, val metaKey: Boolean) : HostEvent"
    ));
    assert!(generated.contains("is HostEvent.KeyDown -> 20"));
    for (field, mask) in [
        ("shiftKey", "0x01"),
        ("ctrlKey", "0x02"),
        ("altKey", "0x04"),
        ("metaKey", "0x08"),
    ] {
        assert!(generated.contains(&format!(
            "if (event.{field}) modifiers = modifiers or {mask}"
        )));
    }
    assert_eq!(
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../dioxus-compose-renderer/native/src/protocol/Protocol.gen.kt"
        )),
        generated,
        "generated Kotlin is stale; run `cargo run -p dioxus-compose --bin codegen`",
    );
}

#[test]
fn fr7_generated_vectors_match_schema() {
    assert_eq!(
        include_bytes!("vectors/mutations.bin").as_slice(),
        generate_mutation_vector().unwrap(),
        "mutation vector is stale; run `cargo run -p dioxus-compose --bin codegen`",
    );
    assert_eq!(
        include_bytes!("vectors/events.bin").as_slice(),
        generate_event_vector().unwrap(),
        "event vector is stale; run `cargo run -p dioxus-compose --bin codegen`",
    );
    assert_eq!(
        include_str!("vectors/vectors.json"),
        generate_vector_description(),
        "vector description is stale; run `cargo run -p dioxus-compose --bin codegen`",
    );
}

#[test]
fn fr12_keydown_roundtrips_through_vectors() {
    let bytes = include_bytes!("vectors/events.bin");
    for (offset, length) in [(0, 16), (16, 30), (46, 28), (74, 16), (90, 35), (125, 20)] {
        decode_event(&bytes[offset..offset + length]).unwrap();
    }

    let key_down = &bytes[125..145];
    assert_eq!(&key_down[0..4], &[6, 0, 20, 0]);
    assert_eq!(key_down[18], 0x0f);

    let expected = HostEvent {
        node_id: 9,
        handler_id: 15,
        payload: EventPayload::KeyDown {
            key: Key::Enter,
            shift_key: true,
            ctrl_key: true,
            alt_key: true,
            meta_key: true,
        },
    };
    assert_eq!(decode_event(key_down).unwrap(), expected);

    let mut encoded = Vec::new();
    encode_event(&expected, &mut encoded).unwrap();
    assert_eq!(encoded, key_down);
}
