//! FR-11: compile-time schema extensions use the same typed RSX and fixed protocol as M0 widgets.

use dioxus_compose::prelude::*;
use dioxus_compose::protocol::{Mutation, PropertyValue, ProtocolError, decode_batch};
use dioxus_compose::schema::{PROPERTY_SCHEMA, SCHEMA_HASH, WIDGET_SCHEMA};
use dioxus_compose::{Host, PropertyKind, WidgetKind};

fn progress_app() -> Element {
    rsx! { LinearProgressIndicator { progress: 0.625 } }
}

fn zero_progress_app() -> Element {
    rsx! { LinearProgressIndicator { progress: 0.0 } }
}

#[test]
fn fr11_typed_extension_uses_fixed_layout_mutations() {
    let mut host = Host::new(progress_app);
    let batch = host.rebuild().unwrap().to_vec();
    let mutations = decode_batch(&batch).unwrap();

    assert!(mutations.iter().any(|mutation| matches!(
        mutation,
        Mutation::Create {
            widget: WidgetKind::LinearProgressIndicator,
            ..
        }
    )));
    assert!(mutations.iter().any(|mutation| matches!(
        mutation,
        Mutation::SetProp {
            property: PropertyKind::Progress,
            value: PropertyValue::Float(value),
            ..
        } if *value == 0.625
    )));

    let records_length = u32::from_le_bytes(batch[4..8].try_into().unwrap()) as usize;
    let mut offset = 12;
    let mut lengths = Vec::new();
    while offset < records_length {
        let length = u16::from_le_bytes(batch[offset + 2..offset + 4].try_into().unwrap());
        lengths.push(length);
        offset += usize::from(length);
    }
    assert_eq!(lengths, [12, 12, 24]);

    let mut zero_host = Host::new(zero_progress_app);
    let zero_batch = zero_host.rebuild().unwrap().to_vec();
    assert!(decode_batch(&zero_batch).unwrap().iter().any(|mutation| {
        matches!(
            mutation,
            Mutation::SetProp {
                property: PropertyKind::Progress,
                value: PropertyValue::Float(value),
                ..
            } if *value == 0.0
        )
    }));
}

#[test]
fn fr11_extension_is_in_the_closed_schema() {
    assert_eq!(WidgetKind::LinearProgressIndicator as u16, 10);
    assert_eq!(PropertyKind::Progress as u16, 27);
    assert_eq!(
        WIDGET_SCHEMA.last().unwrap().name,
        "LinearProgressIndicator"
    );
    assert_eq!(PROPERTY_SCHEMA.last().unwrap().name, "Progress");
    assert_ne!(SCHEMA_HASH, 10_919_967_072_721_375_032);

    let mut invalid_widget_batch = vec![0, 0, 12, 0, 24, 0, 0, 0, 1, 0, 0, 0];
    invalid_widget_batch.extend_from_slice(&[1, 0, 12, 0, 1, 0, 0, 0, 0xff, 0xff, 0, 0]);
    assert_eq!(
        decode_batch(&invalid_widget_batch),
        Err(ProtocolError::InvalidWidget(u16::MAX))
    );

    let mut invalid_property_batch = vec![0, 0, 12, 0, 36, 0, 0, 0, 1, 0, 0, 0];
    invalid_property_batch.extend_from_slice(&[
        2, 0, 24, 0, 1, 0, 0, 0, 0xff, 0xff, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ]);
    assert_eq!(
        decode_batch(&invalid_property_batch),
        Err(ProtocolError::InvalidProperty(u16::MAX))
    );
}
