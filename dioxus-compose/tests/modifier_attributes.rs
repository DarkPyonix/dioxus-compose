use dioxus_compose::prelude::*;
use dioxus_compose::protocol::{Mutation, decode_batch};
use dioxus_compose::Host;

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
    let mods: Vec<_> = mutations.iter().filter_map(|m| match m {
        Mutation::SetModifier { modifier, .. } => Some(format!("{modifier:?}")),
        _ => None,
    }).collect();
    println!("{mods:#?}");
    assert!(mods.iter().any(|m| m.starts_with("Padding")), "padding missing: {mods:?}");
    assert!(mods.iter().any(|m| m.starts_with("Background")), "background missing");
    assert!(mods.iter().any(|m| m.starts_with("Shape")), "shape missing");
    assert!(mods.iter().any(|m| m.starts_with("Elevation")), "elevation missing");
}
