use dioxus_compose::Host;
use dioxus_compose::prelude::*;
use dioxus_compose::protocol::{Mutation, decode_batch};

fn app() -> Element {
    rsx! {
        Column {
            padding: 24.0,
            background: Paint::Role(ColorRole::Surface),
            corner_radius: 16.0,
            elevation: 2.0,
            Text { text: "styled" }
        }
    }
}

#[test]
fn modifiers_written_in_rsx_reach_the_wire() {
    let mut host = Host::new(app);
    let mutations = decode_batch(host.rebuild().unwrap()).unwrap();
    let mods: Vec<_> = mutations
        .iter()
        .filter_map(|m| match m {
            Mutation::SetModifier { modifier, .. } => Some(format!("{modifier:?}")),
            _ => None,
        })
        .collect();
    println!("{mods:#?}");
    assert!(
        mods.iter().any(|m| m.starts_with("Padding")),
        "padding missing: {mods:?}"
    );
    assert!(
        mods.iter().any(|m| m.starts_with("Background")),
        "background missing"
    );
    assert!(mods.iter().any(|m| m.starts_with("Shape")), "shape missing");
    assert!(
        mods.iter().any(|m| m.starts_with("Elevation")),
        "elevation missing"
    );
}

/// Every widget takes the Modifier attributes, and filling the available width is one of
/// them. It used to be declared only on the layout containers, so a Card could not be made
/// to span its parent and every grouped design collapsed to the width of its own text.
fn filling_containers() -> Element {
    rsx! {
        Card {
            fill_max_width: true,
            padding_role: SpaceRole::Md,
            Surface {
                fill_max_width: true,
                fill_max_height: true,
                Text { text: "grouped" }
            }
        }
    }
}

#[test]
fn fr13_fill_max_is_available_on_every_widget() {
    let mut host = Host::new(filling_containers);
    let mutations = decode_batch(host.rebuild().unwrap()).unwrap();
    let mods: Vec<_> = mutations
        .iter()
        .filter_map(|m| match m {
            Mutation::SetModifier { modifier, .. } => Some(format!("{modifier:?}")),
            _ => None,
        })
        .collect();
    assert_eq!(
        mods.iter()
            .filter(|m| m.starts_with("FillMaxWidth"))
            .count(),
        2,
        "both the Card and the Surface should fill their width: {mods:?}"
    );
    assert!(
        mods.iter().any(|m| m.starts_with("FillMaxHeight")),
        "the Surface should fill its height: {mods:?}"
    );
}

/// A windowed list has to be able to take the space its parent column leaves it, or every
/// screen with a list in it is laid out around something that sized itself to its own
/// contents. The two Lazy widgets were the only ones declaring no Modifier props at all.
fn weighted_list() -> Element {
    rsx! {
        Column {
            fill_max_height: true,
            LazyColumn {
                weight: 1.0,
                fill_max_width: true,
                item_count: 3,
                item: move |index: usize| rsx! {
                    Text { text: "{index}" }
                },
            }
        }
    }
}

#[test]
fn fr13_a_windowed_list_takes_the_modifiers_every_widget_takes() {
    let mut host = Host::new(weighted_list);
    let mutations = decode_batch(host.rebuild().unwrap()).unwrap();
    let mods: Vec<_> = mutations
        .iter()
        .filter_map(|m| match m {
            Mutation::SetModifier { modifier, .. } => Some(format!("{modifier:?}")),
            _ => None,
        })
        .collect();
    assert!(
        mods.iter().any(|m| m.starts_with("Weight")),
        "the list should ask its column for a weight: {mods:?}"
    );
    assert!(
        mods.iter().any(|m| m.starts_with("FillMaxWidth")),
        "the list should fill its width: {mods:?}"
    );
}

fn rounded_fill() -> Element {
    rsx! {
        Column {
            background: Paint::Role(ColorRole::SurfaceVariant),
            shape_role: ShapeRole::Large,
            padding_role: SpaceRole::Md,
            Text { text: "bubble" }
        }
    }
}

/// A container that names a shape role keeps it.
///
/// `shape_role` and `corner_radius` fill the same modifier slot, and so do `padding_role`
/// and `padding`. Whichever of a pair is left unset arrives as an empty attribute, and
/// that empty attribute used to clear the slot its partner had just written, one mutation
/// later in the same frame. Every rounded container in every sample was drawn square.
#[test]
fn fr13_an_unset_attribute_does_not_clear_its_partner_in_the_same_slot() {
    let mut host = Host::new(rounded_fill);
    let mutations = decode_batch(host.rebuild().unwrap()).unwrap();
    let mut slots: Vec<(u16, String)> = Vec::new();
    for mutation in &mutations {
        if let Mutation::SetModifier {
            index, modifier, ..
        } = mutation
        {
            slots.push((*index, format!("{modifier:?}")));
        }
    }
    let shape_slot = slots
        .iter()
        .find(|(_, modifier)| modifier.starts_with("ShapeRole"))
        .map(|(index, _)| *index)
        .expect("the shape role never reached the wire");
    let last = slots
        .iter()
        .rfind(|(index, _)| *index == shape_slot)
        .expect("the slot has no writes");
    assert!(
        last.1.starts_with("ShapeRole"),
        "the shape slot ended the frame holding {}: {slots:?}",
        last.1
    );
    let padding_slot = slots
        .iter()
        .find(|(_, modifier)| modifier.starts_with("PaddingRole"))
        .map(|(index, _)| *index)
        .expect("the padding role never reached the wire");
    let last = slots
        .iter()
        .rfind(|(index, _)| *index == padding_slot)
        .expect("the slot has no writes");
    assert!(
        last.1.starts_with("PaddingRole"),
        "the padding slot ended the frame holding {}: {slots:?}",
        last.1
    );
}

/// Every widget, each one given a size and a fill.
///
/// This is here because half of them could not be. `Canvas` documented that its size came
/// from the Modifier chain and then declared no Modifier props at all, so a chart could
/// only ever be as large as the box someone wrapped it in. A `Slider` and a `Divider`
/// could not fill the row they were in, which is the only width either of them is ever
/// wanted at. An `Image` had no size, so a photograph was whatever size the Renderer
/// guessed. The attributes existed on the elements the whole time; the components that
/// wrap them had hand written prop lists, and a prop list is the sort of thing that gets
/// written once and then not extended.
///
/// It is written as one screen holding all of them, because the failure is a compile
/// error rather than a wrong value: a widget that does not take the attribute cannot be
/// written down here at all.
fn every_widget_sized() -> Element {
    rsx! {
        Column {
            fill_max_width: true,
            fill_max_height: true,
            Canvas {
                fill_max_width: true,
                height: 40.0,
                commands: DrawList::builder()
                    .circle(Paint::Role(ColorRole::Primary), 10.0, 10.0, 8.0, 0.0)
                    .build(),
            }
            Image { fill_max_width: true, height: 60.0, asset_id: 1 }
            Icon { width: 24.0, height: 24.0, asset_id: 2 }
            Checkbox { fill_max_width: true, checked: true }
            RadioButton { fill_max_width: true, selected: true }
            Switch { fill_max_width: true, checked: false }
            Slider { fill_max_width: true, value: 0.5 }
            ProgressIndicator { fill_max_width: true, value: 0.25 }
            Divider { fill_max_width: true }
            DatePicker { fill_max_width: true, value: 20_000 }
            TimePicker { fill_max_width: true, value: 600 }
            Dropdown {
                fill_max_width: true,
                selected_index: 0,
                Text { text: "one" }
            }
            Navigation {
                fill_max_width: true,
                NavigationItem { fill_max_width: true, text: "Home" }
                Text { text: "page" }
            }
        }
    }
}

#[test]
fn fr13_every_widget_takes_the_modifier_chain() {
    let mut host = Host::new(every_widget_sized);
    let mutations = decode_batch(host.rebuild().unwrap()).unwrap();
    let filled: Vec<u32> = mutations
        .iter()
        .filter_map(|mutation| match mutation {
            Mutation::SetModifier {
                node_id,
                modifier: dioxus_compose::Modifier::FillMaxWidth,
                ..
            } => Some(*node_id),
            _ => None,
        })
        .collect();
    // The column, the twelve children that asked to fill their width, and the one
    // destination inside the navigation.
    assert_eq!(
        filled.len(),
        14,
        "{} widgets filled their width rather than 14: {filled:?}",
        filled.len()
    );

    let sized: Vec<u32> = mutations
        .iter()
        .filter_map(|mutation| match mutation {
            Mutation::SetModifier {
                node_id,
                modifier: dioxus_compose::Modifier::Height(_),
                ..
            } => Some(*node_id),
            _ => None,
        })
        .collect();
    assert_eq!(
        sized.len(),
        3,
        "the canvas, the image and the icon should each carry a height: {sized:?}"
    );
}

#[test]
fn fr15_compose_rust_006_modifiers_reach_the_wire() {
    fn app() -> Element {
        rsx! {
            dioxus_compose::Box {
                match_parent_size: true,
                window_drag: true,
                press_feedback: (0.95, 0.1),
                Button {
                    text: "click",
                    window_drag: false,
                    press_feedback: (1.0, 0.0),
                }
            }
        }
    }

    let mut host = Host::new(app);
    let mutations = decode_batch(host.rebuild().unwrap()).unwrap();
    let mods: Vec<(u16, dioxus_compose::Modifier)> = mutations
        .iter()
        .filter_map(|m| match m {
            Mutation::SetModifier {
                index, modifier, ..
            } => Some((*index, modifier.clone())),
            _ => None,
        })
        .collect();
    assert!(
        mods.iter()
            .any(|(idx, m)| *idx == 33 && matches!(m, dioxus_compose::Modifier::MatchParentSize)),
        "match parent size missing in slot 33: {mods:?}"
    );
    assert!(
        mods.iter()
            .any(|(idx, m)| *idx == 34 && matches!(m, dioxus_compose::Modifier::WindowDrag(true))),
        "window drag true missing in slot 34: {mods:?}"
    );
    assert!(
        mods.iter()
            .any(|(idx, m)| *idx == 34 && matches!(m, dioxus_compose::Modifier::WindowDrag(false))),
        "window drag false missing in slot 34: {mods:?}"
    );
    assert!(
        mods.iter().any(|(idx, m)| *idx == 35 && matches!(m, dioxus_compose::Modifier::PressFeedback { scale, tint_alpha } if (*scale - 0.95).abs() < 1e-4 && (*tint_alpha - 0.1).abs() < 1e-4)),
        "press feedback missing in slot 35: {mods:?}"
    );

    fn raw_app() -> Element {
        rsx! {
            composebox {
                match_parent_size: true,
                window_drag: true,
                press_feedback: (0.95_f32, 0.1_f32),
            }
            button {
                window_drag: false,
                press_feedback: Modifier::PressFeedback { scale: 0.90, tint_alpha: 0.20 },
            }
        }
    }

    let mut host = Host::new(raw_app);
    let mutations = decode_batch(host.rebuild().unwrap()).unwrap();
    let mods: Vec<(u16, dioxus_compose::Modifier)> = mutations
        .iter()
        .filter_map(|m| match m {
            Mutation::SetModifier {
                index, modifier, ..
            } => Some((*index, modifier.clone())),
            _ => None,
        })
        .collect();
    assert!(
        mods.iter()
            .any(|(idx, m)| *idx == 33 && matches!(m, dioxus_compose::Modifier::MatchParentSize)),
        "raw match parent size missing in slot 33: {mods:?}"
    );
    assert!(
        mods.iter()
            .any(|(idx, m)| *idx == 34 && matches!(m, dioxus_compose::Modifier::WindowDrag(true))),
        "raw window drag true missing in slot 34: {mods:?}"
    );
    assert!(
        mods.iter()
            .any(|(idx, m)| *idx == 34 && matches!(m, dioxus_compose::Modifier::WindowDrag(false))),
        "raw window drag false missing in slot 34: {mods:?}"
    );
    assert!(
        mods.iter().any(|(idx, m)| *idx == 35 && matches!(m, dioxus_compose::Modifier::PressFeedback { scale, tint_alpha } if (*scale - 0.95).abs() < 1e-4 && (*tint_alpha - 0.1).abs() < 1e-4)),
        "raw press feedback missing in slot 35: {mods:?}"
    );
    assert!(
        mods.iter().any(|(idx, m)| *idx == 35 && matches!(m, dioxus_compose::Modifier::PressFeedback { scale, tint_alpha } if (*scale - 0.90).abs() < 1e-4 && (*tint_alpha - 0.20).abs() < 1e-4)),
        "raw press feedback enum missing in slot 35: {mods:?}"
    );
}

/// The modifiers each node was given, by slot, in the order they arrived.
fn slot_writes(batch: &[u8]) -> Vec<(u32, u16, dioxus_compose::Modifier)> {
    decode_batch(batch)
        .unwrap()
        .into_iter()
        .filter_map(|mutation| match mutation {
            Mutation::SetModifier {
                node_id,
                index,
                modifier,
            } => Some((node_id, index, modifier)),
            _ => None,
        })
        .collect()
}

const STRIPES: Paint = Paint::Asset(9);
const DOTS: Paint = Paint::Asset(10);

thread_local! {
    static LAYERS: std::cell::Cell<usize> = const { std::cell::Cell::new(2) };
}

fn layered() -> Element {
    let mut count = use_signal(|| LAYERS.with(std::cell::Cell::get));
    let layers: Vec<BackgroundLayer> = [
        BackgroundLayer::new(DOTS, 0.0, 0.0, 8.0, 8.0).repeat(BackgroundRepeat::Repeat),
        BackgroundLayer::new(STRIPES, 12.5, -4.0, 48.0, 24.0)
            .repeat(BackgroundRepeat::RepeatX)
            .step(56.0, 30.0),
        BackgroundLayer::new(DOTS, 1.0, 1.0, 2.0, 2.0),
        BackgroundLayer::new(STRIPES, 2.0, 2.0, 4.0, 4.0),
        BackgroundLayer::new(DOTS, 3.0, 3.0, 6.0, 6.0),
    ]
    .into_iter()
    .take(count())
    .collect();
    rsx! {
        Column {
            background: Paint::Role(ColorRole::Surface),
            border_width: 1.0,
            border_color: Paint::Role(ColorRole::Outline),
            background_brush: layers,
            Button { text: "fewer", on_click: move |_| count.set(1) }
        }
    }
}

/// Background image layers are `BackgroundBrush` modifiers, one slot each, between the
/// background colour and the border, so a layer is drawn over the colour and under the
/// outline as CSS draws `background-image`. The list is written in CSS order, the first
/// on top, so the chain gets it last.
#[test]
fn fr15_background_brush_layers_sit_over_the_background_and_under_the_border() {
    LAYERS.with(|count| count.set(2));
    let mut host = Host::new(layered);
    let writes = slot_writes(host.rebuild().unwrap());
    let slot_of = |wanted: fn(&dioxus_compose::Modifier) -> bool| {
        writes
            .iter()
            .filter(|(_, _, modifier)| wanted(modifier))
            .map(|(_, index, _)| *index)
            .collect::<Vec<_>>()
    };
    let background = slot_of(|m| matches!(m, dioxus_compose::Modifier::Background(_)));
    let border = slot_of(|m| matches!(m, dioxus_compose::Modifier::Border { .. }));
    let layers: Vec<(u16, dioxus_compose::Modifier)> = writes
        .iter()
        .filter(|(_, _, m)| matches!(m, dioxus_compose::Modifier::BackgroundBrush { .. }))
        .map(|(_, index, m)| (*index, m.clone()))
        .collect();
    assert_eq!(layers.len(), 2, "{writes:?}");
    assert!(
        background[0] < layers[0].0 && layers[1].0 < border[0],
        "{writes:?}"
    );
    assert_eq!(layers[1].0, layers[0].0 + 1);
    assert_eq!(
        layers[0].1,
        BackgroundLayer::new(STRIPES, 12.5, -4.0, 48.0, 24.0)
            .repeat(BackgroundRepeat::RepeatX)
            .step(56.0, 30.0)
            .modifier(),
        "the last layer in CSS order is the bottom of the chain"
    );
    assert_eq!(
        layers[1].1,
        dioxus_compose::Modifier::BackgroundBrush {
            x: 0.0,
            y: 0.0,
            width: 8.0,
            height: 8.0,
            paint: DOTS,
            repeat: BackgroundRepeat::Repeat,
            tile_width: 8.0,
            tile_height: 8.0,
        }
    );
}

/// A node carries four layers, the four on top of a longer list, and a list that gets
/// shorter clears the slots it no longer fills, once.
#[test]
fn fr15_background_brush_keeps_four_layers_and_clears_what_a_shorter_list_leaves() {
    LAYERS.with(|count| count.set(5));
    let mut host = Host::new(layered);
    let first = host.rebuild().unwrap().to_vec();
    let writes = slot_writes(&first);
    let layers: Vec<&(u32, u16, dioxus_compose::Modifier)> = writes
        .iter()
        .filter(|(_, _, m)| matches!(m, dioxus_compose::Modifier::BackgroundBrush { .. }))
        .collect();
    assert_eq!(layers.len(), 4, "{writes:?}");
    assert!(
        !layers.iter().any(|(_, _, m)| matches!(
            m,
            dioxus_compose::Modifier::BackgroundBrush { x, .. } if *x == 3.0
        )),
        "the fifth layer is the bottom one and is the one left out: {layers:?}"
    );
    let column = layers[0].0;
    let slots: Vec<u16> = layers.iter().map(|(_, index, _)| *index).collect();

    let decoded = decode_batch(&first).unwrap();
    let button = decoded
        .iter()
        .find_map(|m| match m {
            Mutation::Create {
                node_id,
                widget: dioxus_compose::WidgetKind::Button,
            } => Some(*node_id),
            _ => None,
        })
        .unwrap();
    let handler = decoded
        .iter()
        .find_map(|m| match m {
            Mutation::SetProp {
                node_id,
                property: dioxus_compose::PropertyKind::OnClick,
                value: dioxus_compose::protocol::PropertyValue::Integer(handler),
            } if *node_id == button => Some(*handler as u64),
            _ => None,
        })
        .unwrap();
    let (batch, _) = host
        .dispatch(dioxus_compose::protocol::HostEvent {
            node_id: button,
            handler_id: handler,
            payload: dioxus_compose::EventPayload::Clicked,
        })
        .unwrap();
    let after: Vec<(u32, u16, dioxus_compose::Modifier)> = slot_writes(batch)
        .into_iter()
        .filter(|(node, index, _)| *node == column && slots.contains(index))
        .collect();
    assert_eq!(
        after,
        vec![
            (
                column,
                slots[0],
                BackgroundLayer::new(DOTS, 0.0, 0.0, 8.0, 8.0)
                    .repeat(BackgroundRepeat::Repeat)
                    .modifier()
            ),
            (column, slots[1], dioxus_compose::Modifier::Empty),
            (column, slots[2], dioxus_compose::Modifier::Empty),
            (column, slots[3], dioxus_compose::Modifier::Empty),
        ]
    );
}

fn raw_layers() -> Element {
    rsx! {
        composebox {
            background_brush: dioxus_compose::Modifier::BackgroundBrush {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
                paint: STRIPES,
                repeat: BackgroundRepeat::NoRepeat,
                tile_width: 10.0,
                tile_height: 10.0,
            },
        }
        card {
            background_brush: vec![
                BackgroundLayer::new(DOTS, 0.0, 0.0, 4.0, 4.0),
                BackgroundLayer::new(STRIPES, 0.0, 0.0, 4.0, 4.0).step(0.0, 4.0),
                BackgroundLayer::new(STRIPES, f32::NAN, 0.0, 4.0, 4.0),
            ],
        }
        surface {}
    }
}

/// The raw elements take one `Modifier::BackgroundBrush` or a list of `BackgroundLayer`s
/// in CSS order. A layer the Renderer would refuse, with a step that is not a size or a
/// number that is not finite, is left out rather than sent, and a node that names none
/// sends none.
#[test]
fn fr15_raw_elements_take_one_background_brush_or_a_list_and_drop_what_cannot_draw() {
    let mut host = Host::new(raw_layers);
    let writes = slot_writes(host.rebuild().unwrap());
    let layers: Vec<dioxus_compose::Modifier> = writes
        .iter()
        .filter(|(_, _, m)| matches!(m, dioxus_compose::Modifier::BackgroundBrush { .. }))
        .map(|(_, _, m)| m.clone())
        .collect();
    assert_eq!(
        layers,
        vec![
            BackgroundLayer::new(STRIPES, 0.0, 0.0, 10.0, 10.0).modifier(),
            BackgroundLayer::new(DOTS, 0.0, 0.0, 4.0, 4.0).modifier(),
        ]
    );
}

/// The checked-in protocol vector holds one `BackgroundBrush` modifier, and the record a
/// `BackgroundLayer` makes is that one: the rectangle, the paint, the repeat and the step.
#[test]
fn fr15_a_background_layer_is_the_record_in_the_protocol_vector() {
    let mutations = decode_batch(include_bytes!("vectors/background_brush.bin")).unwrap();
    let [
        Mutation::SetModifier {
            node_id: 4,
            index: 2,
            modifier,
        },
    ] = mutations.as_slice()
    else {
        panic!("the vector holds one modifier on node 4: {mutations:?}");
    };
    assert_eq!(
        *modifier,
        BackgroundLayer::new(STRIPES, 12.5, -4.0, 48.0, 24.0)
            .repeat(BackgroundRepeat::RepeatX)
            .step(56.0, 30.0)
            .modifier()
    );
}
