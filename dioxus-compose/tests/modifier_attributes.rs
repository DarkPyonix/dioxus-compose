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
