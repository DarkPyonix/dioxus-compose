use dioxus_compose::codegen::{
    generate_event_vector, generate_kotlin, generate_mutation_vector, generate_vector_description,
};
use dioxus_compose::protocol::decode_event;

#[test]
fn checked_in_generated_artifacts_are_current() {
    assert_eq!(
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../dioxus-compose-renderer/native/src/protocol/Protocol.gen.kt"
        )),
        generate_kotlin(),
        "generated Kotlin is stale; run `cargo run -p dioxus-compose --bin codegen`",
    );
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
fn rust_decodes_every_event_vector_record() {
    let bytes = include_bytes!("vectors/events.bin");
    let records = [(0, 16), (16, 30), (46, 28), (74, 16), (90, 35)];
    for (offset, length) in records {
        decode_event(&bytes[offset..offset + length]).unwrap();
    }
}
