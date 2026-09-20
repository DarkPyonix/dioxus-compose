//! Asset delivery: registering bytes once, drawing them by id, and releasing them.

use dioxus_compose::prelude::*;
use dioxus_compose::protocol::{Mutation, PropertyValue, ProtocolError, decode_batch};
use dioxus_compose::{AssetKind, Host, IconRole, PropertyKind, WidgetKind};

const PNG_HEADER: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

/// The bytes ride behind the records, the way a string does, so the record itself stays
/// the fixed layout everything else on this wire is.
#[test]
fn fr16_register_asset_carries_the_bytes_behind_a_fixed_layout_record() {
    let mut host = Host::new(|| rsx! { Text { text: "" } });
    let batch = host.register_asset(7, AssetKind::Png, PNG_HEADER).unwrap();
    let decoded = decode_batch(batch).unwrap();
    assert_eq!(
        decoded,
        vec![Mutation::RegisterAsset {
            asset_id: 7,
            kind: AssetKind::Png,
            bytes: PNG_HEADER,
        }],
    );
}

/// A release names the id and nothing else: the Host owns the lifetime and the Renderer
/// only needs to know which entry to drop.
#[test]
fn fr16_release_asset_names_only_the_id() {
    let mut host = Host::new(|| rsx! { Text { text: "" } });
    let batch = host.release_asset(7).unwrap();
    assert_eq!(
        decode_batch(batch).unwrap(),
        vec![Mutation::ReleaseAsset { asset_id: 7 }],
    );
}

/// An icon is registered by the meaning it carries, never by a system icon name. A name
/// would move the check that the icon exists to run time and would pin one platform's
/// artwork into the protocol, so what crosses is the role tag.
#[test]
fn fr16_icons_are_registered_by_role_not_by_name() {
    let mut host = Host::new(|| rsx! { Text { text: "" } });
    let batch = host.register_icon(3, IconRole::Search).unwrap();
    assert_eq!(
        decode_batch(batch).unwrap(),
        vec![Mutation::RegisterAsset {
            asset_id: 3,
            kind: AssetKind::VectorIcon,
            bytes: &(IconRole::Search as u16).to_le_bytes(),
        }],
    );
}

/// A kind the schema does not name never becomes a mutation. The Renderer answers the
/// same case with a reported protocol error rather than a guess.
#[test]
fn fr16_an_unknown_asset_kind_does_not_decode() {
    let mut batch = vec![0, 0, 12, 0, 32, 0, 0, 0, 1, 0, 0, 0];
    batch.extend_from_slice(&[
        10, 0, 20, 0, // RegisterAsset, 20 bytes
        7, 0, 0, 0, // asset id
        0xff, 0xff, 0, 0, // an unnamed kind
        32, 0, 0, 0, // offset
        0, 0, 0, 0, // length
    ]);
    assert_eq!(
        decode_batch(&batch),
        Err(ProtocolError::InvalidAssetKind(u16::MAX)),
    );
}

fn image_app() -> Element {
    rsx! {
        Image { asset_id: 7 }
    }
}

/// An `Image` carries the asset id and nothing else. The bytes crossed once, at
/// registration, and the per-frame record stays fixed layout because of it.
#[test]
fn fr16_image_sends_only_the_asset_id() {
    let mut host = Host::new(image_app);
    let batch = host.rebuild().unwrap().to_vec();
    let decoded = decode_batch(&batch).unwrap();
    let node = decoded
        .iter()
        .find_map(|mutation| match mutation {
            Mutation::Create {
                node_id,
                widget: WidgetKind::Image,
            } => Some(*node_id),
            _ => None,
        })
        .expect("no Image was created");
    let props: Vec<_> = decoded
        .iter()
        .filter_map(|mutation| match mutation {
            Mutation::SetProp {
                node_id,
                property,
                value,
            } if *node_id == node => Some((*property, value.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        props,
        vec![(PropertyKind::Asset, PropertyValue::Integer(7))],
    );
}

fn icon_app() -> Element {
    rsx! {
        Icon { asset_id: 4, color: Paint::Role(ColorRole::Primary) }
    }
}

/// An `Icon` adds a tint, and the tint is a role: which pixels that is belongs to the
/// design system, the same way it does for every other colour on this wire.
#[test]
fn fr16_icon_sends_the_asset_id_and_a_role_tint() {
    let mut host = Host::new(icon_app);
    let batch = host.rebuild().unwrap().to_vec();
    let decoded = decode_batch(&batch).unwrap();
    let node = decoded
        .iter()
        .find_map(|mutation| match mutation {
            Mutation::Create {
                node_id,
                widget: WidgetKind::Icon,
            } => Some(*node_id),
            _ => None,
        })
        .expect("no Icon was created");
    let props: Vec<_> = decoded
        .iter()
        .filter_map(|mutation| match mutation {
            Mutation::SetProp {
                node_id,
                property,
                value,
            } if *node_id == node => Some((*property, value.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        props,
        vec![
            (PropertyKind::Asset, PropertyValue::Integer(4)),
            (
                PropertyKind::Color,
                PropertyValue::Integer(Paint::Role(ColorRole::Primary).to_bits() as i64),
            ),
        ],
    );
}

/// The asset tags are assigned once. Renumbering one silently breaks every Renderer
/// already built against it, so the numbers are pinned here.
#[test]
fn fr16_asset_kinds_keep_their_assigned_tags() {
    assert_eq!(
        [
            AssetKind::Png as u16,
            AssetKind::Jpeg as u16,
            AssetKind::Svg as u16,
            AssetKind::VectorIcon as u16,
        ],
        [1, 2, 3, 4],
    );
    assert_eq!(
        [WidgetKind::Image as u16, WidgetKind::Icon as u16],
        [10, 11],
    );
}
