//! The design primitives as seen from rsx.

use dioxus_compose::prelude::*;
use dioxus_compose::protocol::{Mutation, PropertyValue, decode_batch};
use dioxus_compose::{Host, PropertyKind, WidgetKind};

fn titled_text() -> Element {
    rsx! {
        Text { text: "제목", type_role: TypeRole::Title }
    }
}

fn styled_column() -> Element {
    rsx! {
        Column {
            arrangement: Arrangement::SpaceBetween,
            space_role: SpaceRole::Md,
            alignment: Alignment::Center,
            Button { text: "확인", variant: ButtonVariant::Tonal }
        }
    }
}

fn scrolling() -> Element {
    rsx! {
        ScrollColumn {
            Text { text: "long" }
        }
    }
}

fn props_of(app: fn() -> Element) -> Vec<(PropertyKind, PropertyValue<'static>)> {
    let mut host = Host::new(app);
    let batch = host.rebuild().unwrap().to_vec();
    // The batch borrows its strings, and the assertions only need the non-string values.
    decode_batch(&batch)
        .unwrap()
        .into_iter()
        .filter_map(|mutation| match mutation {
            Mutation::SetProp {
                property, value, ..
            } => Some((
                property,
                match value {
                    // Neither strings nor byte blobs are compared by value here.
                    PropertyValue::String(_) | PropertyValue::Bytes(_) => PropertyValue::None,
                    PropertyValue::None => PropertyValue::None,
                    PropertyValue::Bool(value) => PropertyValue::Bool(value),
                    PropertyValue::Integer(value) => PropertyValue::Integer(value),
                    PropertyValue::Float(value) => PropertyValue::Float(value),
                },
            )),
            _ => None,
        })
        .collect()
}

/// A Text that only names a `type_role` pays exactly one SetProp for it, and
/// the eight overrides it did not set cost nothing. A role is sent; zero is not.
#[test]
fn fr13_type_role_costs_one_set_prop() {
    let props = props_of(titled_text);
    let design: Vec<_> = props
        .iter()
        .filter(|(property, _)| *property as u16 >= PropertyKind::TypeRole as u16)
        .collect();
    assert_eq!(
        design,
        [&(
            PropertyKind::TypeRole,
            PropertyValue::Integer(TypeRole::Title as i64)
        )]
    );
}

/// Layout roles and the component variant reach the wire as their tags. The Host sends the
/// role; what it looks like is the design system's decision, not the caller's.
#[test]
fn fr13_layout_roles_and_variant_are_sent_as_tags() {
    let props = props_of(styled_column);
    for expected in [
        (
            PropertyKind::Arrangement,
            PropertyValue::Integer(Arrangement::SpaceBetween as i64),
        ),
        (
            PropertyKind::SpaceRole,
            PropertyValue::Integer(SpaceRole::Md as i64),
        ),
        (
            PropertyKind::Alignment,
            PropertyValue::Integer(Alignment::Center as i64),
        ),
        (
            PropertyKind::Variant,
            PropertyValue::Integer(ButtonVariant::Tonal as i64),
        ),
    ] {
        assert!(props.contains(&expected), "missing {expected:?}");
    }
    // The roles nobody set are absent rather than sent as zero.
    assert!(
        !props
            .iter()
            .any(|(property, _)| *property == PropertyKind::Spacing)
    );
}

/// ScrollColumn is a widget of its own, at wire tag 9.
#[test]
fn fr13_scroll_column_is_its_own_widget() {
    let mut host = Host::new(scrolling);
    let batch = host.rebuild().unwrap().to_vec();
    assert!(
        decode_batch(&batch)
            .unwrap()
            .iter()
            .any(|mutation| matches!(
                mutation,
                Mutation::Create {
                    widget: WidgetKind::ScrollColumn,
                    ..
                }
            ))
    );
}
