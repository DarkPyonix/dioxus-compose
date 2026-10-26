//! Several shadows on `Column`, `Row`, `Box` and `Card`.
//!
//! The `shadow` attribute takes a list in CSS order, where the first layer is on top. The
//! chain draws a later modifier over an earlier one, so each layer reaches the wire as its
//! own `Shadow` modifier, last layer first, in consecutive chain positions that sit ahead
//! of the shape and the fill.

use dioxus_compose::Host;
use dioxus_compose::prelude::*;
use dioxus_compose::protocol::{HostEvent, Mutation, PropertyValue, decode_batch, encode_event};
use dioxus_compose::renderer::MAX_SHADOWS;
use dioxus_compose::{EventPayload, PropertyKind, WidgetKind};

fn tight() -> ShadowLayer {
    ShadowLayer::new(0.0, 1.0, 2.0, 0.0, Paint::Literal(Color::argb(0x1400_0000)))
}

fn wide() -> ShadowLayer {
    ShadowLayer::new(
        0.0,
        8.0,
        24.0,
        -4.0,
        Paint::Literal(Color::argb(0x2900_0000)),
    )
}

fn shadow_of(layer: ShadowLayer) -> Modifier {
    Modifier::Shadow {
        x: layer.x,
        y: layer.y,
        blur: layer.blur,
        spread: layer.spread,
        paint: layer.paint,
    }
}

fn layered() -> Element {
    rsx! {
        Column {
            corner_radius: 12.0,
            background: Paint::Role(ColorRole::Surface),
            shadow: vec![tight(), wide()],
            Row {
                shadow: vec![tight(), wide()],
                dioxus_compose::Box {
                    shadow: vec![tight(), wide()],
                    Card {
                        shadow: vec![tight(), wide()],
                        Text { text: "layered" }
                    }
                }
            }
        }
    }
}

/// One node's modifier records, as the batch wrote them: chain position and value.
type Chain = Vec<(u16, Modifier)>;

fn modifiers(mutations: &[Mutation<'_>], node: u32) -> Chain {
    mutations
        .iter()
        .filter_map(|mutation| match mutation {
            Mutation::SetModifier {
                node_id,
                index,
                modifier,
            } if *node_id == node => Some((*index, modifier.clone())),
            _ => None,
        })
        .collect()
}

fn nodes_of(mutations: &[Mutation<'_>], widget: WidgetKind) -> Vec<u32> {
    mutations
        .iter()
        .filter_map(|mutation| match mutation {
            Mutation::Create {
                node_id,
                widget: kind,
            } if *kind == widget => Some(*node_id),
            _ => None,
        })
        .collect()
}

fn shadows(chain: &Chain) -> Vec<(u16, Modifier)> {
    chain
        .iter()
        .filter(|(_, modifier)| matches!(modifier, Modifier::Shadow { .. }))
        .cloned()
        .collect()
}

/// Each of the four containers writes both layers, the bottom one first, so the first layer
/// in the list lies over the second as CSS draws it.
#[test]
fn fr51_every_container_writes_its_layers_bottom_first() {
    let mut host = Host::new(layered);
    let mutations = decode_batch(host.rebuild().unwrap()).unwrap();
    for widget in [
        WidgetKind::Column,
        WidgetKind::Row,
        WidgetKind::Box,
        WidgetKind::Card,
    ] {
        let nodes = nodes_of(&mutations, widget);
        assert_eq!(nodes.len(), 1, "one {widget:?} is created");
        let written = shadows(&modifiers(&mutations, nodes[0]));
        assert_eq!(
            written
                .iter()
                .map(|(_, modifier)| modifier.clone())
                .collect::<Vec<_>>(),
            [shadow_of(wide()), shadow_of(tight())],
            "{widget:?} writes its layers last to first"
        );
        assert_eq!(
            written[1].0,
            written[0].0 + 1,
            "{widget:?}: the layers are in consecutive chain positions"
        );
    }
}

/// A shadow is behind the node, so it is ahead of the shape and the fill in the chain, and
/// the shape's clip does not cut it.
#[test]
fn fr51_shadows_sit_ahead_of_the_shape_and_the_fill() {
    let mut host = Host::new(layered);
    let mutations = decode_batch(host.rebuild().unwrap()).unwrap();
    let column = nodes_of(&mutations, WidgetKind::Column)[0];
    let chain = modifiers(&mutations, column);
    let last_shadow = shadows(&chain)
        .iter()
        .map(|(index, _)| *index)
        .max()
        .unwrap();
    for (index, modifier) in &chain {
        if matches!(modifier, Modifier::Shape { .. } | Modifier::Background(_)) {
            assert!(
                last_shadow < *index,
                "{modifier:?} at {index} must come after the shadows, which end at {last_shadow}"
            );
        }
    }
}

fn too_many() -> Element {
    let layers: Vec<ShadowLayer> = (0..MAX_SHADOWS + 2)
        .map(|layer| ShadowLayer::new(layer as f32, 0.0, 1.0, 0.0, Paint::Role(ColorRole::Outline)))
        .collect();
    rsx! {
        Column { shadow: layers, Text { text: "many" } }
    }
}

/// A node carries `MAX_SHADOWS` layers. A longer list keeps the layers on top, the first
/// ones, and drops the rest.
#[test]
fn fr51_a_list_longer_than_the_limit_keeps_the_top_layers() {
    let mut host = Host::new(too_many);
    let mutations = decode_batch(host.rebuild().unwrap()).unwrap();
    let column = nodes_of(&mutations, WidgetKind::Column)[0];
    let written = shadows(&modifiers(&mutations, column));
    assert_eq!(written.len(), MAX_SHADOWS);
    let offsets: Vec<f32> = written
        .iter()
        .map(|(_, modifier)| match modifier {
            Modifier::Shadow { x, .. } => *x,
            other => panic!("{other:?}"),
        })
        .collect();
    let expected: Vec<f32> = (0..MAX_SHADOWS).rev().map(|layer| layer as f32).collect();
    assert_eq!(offsets, expected);
}

fn none() -> Element {
    rsx! {
        Column { corner_radius: 4.0, Text { text: "flat" } }
    }
}

/// A node with no shadows costs no record, neither a shadow nor a clearing one.
#[test]
fn fr51_a_node_without_shadows_writes_nothing_for_them() {
    let mut host = Host::new(none);
    let mutations = decode_batch(host.rebuild().unwrap()).unwrap();
    let column = nodes_of(&mutations, WidgetKind::Column)[0];
    let chain = modifiers(&mutations, column);
    assert!(
        chain
            .iter()
            .all(|(_, modifier)| !matches!(modifier, Modifier::Shadow { .. } | Modifier::Empty)),
        "{chain:?}"
    );
}

fn shrinking() -> Element {
    let mut count = use_signal(|| 3_usize);
    let layers: Vec<ShadowLayer> = (0..count())
        .map(|layer| ShadowLayer::new(layer as f32, 0.0, 1.0, 0.0, Paint::Role(ColorRole::Outline)))
        .collect();
    rsx! {
        Column {
            shadow: layers,
            Button { text: "fewer", on_click: move |_| count.set(1) }
        }
    }
}

/// A list that gets shorter empties the positions it no longer fills, so a layer that was
/// removed is not left drawn.
#[test]
fn fr51_a_shorter_list_clears_the_positions_it_no_longer_fills() {
    let mut host = Host::new(shrinking);
    let first = decode_batch(host.rebuild().unwrap()).unwrap();
    let column = nodes_of(&first, WidgetKind::Column)[0];
    let button = nodes_of(&first, WidgetKind::Button)[0];
    let before = shadows(&modifiers(&first, column));
    assert_eq!(before.len(), 3);
    let base = before[0].0;
    let handler = first
        .iter()
        .find_map(|mutation| match mutation {
            Mutation::SetProp {
                node_id,
                property: PropertyKind::OnClick,
                value: PropertyValue::Integer(handler),
            } if *node_id == button => Some(*handler as u64),
            _ => None,
        })
        .expect("the button has a click handler");
    drop(first);

    let mut wire = Vec::new();
    encode_event(
        &HostEvent {
            node_id: button,
            handler_id: handler,
            payload: EventPayload::Clicked,
        },
        &mut wire,
    )
    .unwrap();
    let mut after: Chain = Vec::new();
    let (batch, _) = host.dispatch_event(&wire).unwrap();
    after.extend(modifiers(&decode_batch(batch).unwrap(), column));
    after.extend(modifiers(
        &decode_batch(host.render_frame(0).unwrap()).unwrap(),
        column,
    ));

    // Position 0 now holds the one layer left, which is the first in the list (x = 0), and
    // the two positions above it are emptied.
    let latest = |index: u16| {
        after
            .iter()
            .rev()
            .find(|(at, _)| *at == index)
            .map(|(_, m)| m.clone())
    };
    assert_eq!(
        latest(base + 2),
        Some(shadow_of(ShadowLayer::new(
            0.0,
            0.0,
            1.0,
            0.0,
            Paint::Role(ColorRole::Outline)
        )))
    );
    assert_eq!(latest(base), Some(Modifier::Empty));
    assert_eq!(latest(base + 1), Some(Modifier::Empty));
}
