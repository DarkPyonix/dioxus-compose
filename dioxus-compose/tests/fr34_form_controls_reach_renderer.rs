//! A `<select>`'s value and a checked radio button reach the renderer's `Dropdown` and
//! `RadioButton`, in the shape the settings sample writes them: a select whose value is
//! bound to the app's state, inside a flex row and styled as a control, and radio buttons
//! inside labels whose `checked` follows a signal.

use dioxus_compose::Host;
use dioxus_compose::html::HtmlConfig;
use dioxus_compose::html::prelude::*;
use dioxus_compose::prelude::WritableExt;
use dioxus_compose::protocol::{HostEvent, Mutation, PropertyValue, decode_batch, encode_event};
use dioxus_compose::schema::{EventPayload, PropertyKind, WidgetKind};
use dioxus_hooks::use_signal;
use std::collections::HashMap;

const STYLE: &str = "
    body { margin: 0; font-size: 14px; line-height: 20px; }
    .field { display: flex; align-items: center; gap: 12px; }
    .control { flex: 1; height: 36px; padding: 7px 11px; border: 1px solid #d0d7de; }
    .options { display: flex; flex-direction: column; gap: 8px; }
    .check { display: flex; align-items: center; gap: 8px; }
    .check input { width: 16px; height: 16px; margin: 0; }
";

fn config() -> HtmlConfig {
    HtmlConfig {
        stylesheets: vec![STYLE.to_string()],
        ..HtmlConfig::default()
    }
}

const DENSITIES: [&str; 2] = ["Comfortable", "Compact"];

fn settings() -> Element {
    let mut theme = use_signal(|| "system".to_string());
    let mut density = use_signal(|| DENSITIES[0]);
    rsx! {
        div { class: "field",
            label { r#for: "theme", "Theme" }
            select {
                id: "theme",
                class: "control",
                value: "{theme}",
                onchange: move |event| theme.set(event.value()),
                option { value: "light", "Light" }
                option { value: "dark", "Dark" }
                option { value: "system", "Match the system" }
            }
        }
        div { class: "options",
            for (index, choice) in DENSITIES.into_iter().enumerate() {
                label { key: "{choice}", class: "check",
                    input {
                        id: "density-{index}",
                        r#type: "radio",
                        name: "density",
                        value: "{choice}",
                        checked: density() == choice,
                        onclick: move |_| density.set(choice),
                    }
                    "{choice}"
                }
            }
        }
    }
}

/// What the renderer holds after applying every batch so far: each control's widget, its
/// last `SelectedIndex` or `Checked`, its value change handler, and a dropdown's options.
#[derive(Default)]
struct Controls {
    widgets: HashMap<u32, WidgetKind>,
    selected: HashMap<u32, i64>,
    checked: HashMap<u32, bool>,
    handlers: HashMap<u32, u64>,
    options: HashMap<u32, Vec<u32>>,
}

impl Controls {
    fn apply(&mut self, bytes: &[u8]) {
        for record in decode_batch(bytes).expect("the batch decodes") {
            match record {
                Mutation::Create { node_id, widget } => {
                    self.widgets.insert(node_id, widget);
                }
                Mutation::Remove { node_id } => {
                    self.widgets.remove(&node_id);
                }
                Mutation::Insert {
                    parent_id, node_id, ..
                } if self.widgets.get(&parent_id) == Some(&WidgetKind::Dropdown) => {
                    self.options.entry(parent_id).or_default().push(node_id);
                }
                Mutation::SetProp {
                    node_id,
                    property,
                    value,
                } => match (property, value) {
                    (PropertyKind::SelectedIndex, PropertyValue::Integer(index)) => {
                        self.selected.insert(node_id, index);
                    }
                    (PropertyKind::SelectedIndex, PropertyValue::None) => {
                        self.selected.remove(&node_id);
                    }
                    (PropertyKind::Checked, PropertyValue::Bool(on)) => {
                        self.checked.insert(node_id, on);
                    }
                    (PropertyKind::OnValueChange, PropertyValue::Integer(handler)) => {
                        self.handlers.insert(node_id, handler as u64);
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }

    fn of(&self, widget: WidgetKind) -> Vec<u32> {
        let mut found: Vec<u32> = self
            .widgets
            .iter()
            .filter(|(_, kind)| **kind == widget)
            .map(|(node, _)| *node)
            .collect();
        found.sort_unstable();
        found
    }

    fn radios(&self) -> Vec<Option<bool>> {
        self.of(WidgetKind::RadioButton)
            .iter()
            .map(|radio| self.checked.get(radio).copied())
            .collect()
    }
}

fn change(host: &mut Host, controls: &mut Controls, node_id: u32, value: f64) {
    let handler_id = controls.handlers[&node_id];
    let mut wire = Vec::new();
    encode_event(
        &HostEvent {
            node_id,
            handler_id,
            payload: EventPayload::ValueChanged(value),
        },
        &mut wire,
    )
    .unwrap();
    let (batch, _) = host.dispatch_event(&wire).unwrap();
    let batch = batch.to_vec();
    controls.apply(&batch);
}

fn start() -> (Host, Controls) {
    let mut host = Host::html(settings, config);
    let mut controls = Controls::default();
    let first = host.rebuild().unwrap().to_vec();
    controls.apply(&first);
    (host, controls)
}

/// The value the app bound picks the option the dropdown shows: "system" is the third
/// option, so the renderer is told index 2 and has the three options to show it from.
/// Picking another option in the renderer comes back as the new index on the wire.
#[test]
fn fr34_a_selects_value_reaches_its_dropdown() {
    let (mut host, mut controls) = start();
    let dropdowns = controls.of(WidgetKind::Dropdown);
    assert_eq!(dropdowns.len(), 1, "one select, one Dropdown");
    let dropdown = dropdowns[0];
    assert_eq!(controls.options[&dropdown].len(), 3);
    assert_eq!(
        controls.selected.get(&dropdown),
        Some(&2),
        "value \"system\" is the third option"
    );

    change(&mut host, &mut controls, dropdown, 0.0);
    assert_eq!(
        controls.selected.get(&dropdown),
        Some(&0),
        "Light was picked"
    );
    change(&mut host, &mut controls, dropdown, 2.0);
    assert_eq!(controls.selected.get(&dropdown), Some(&2), "and back");
}

/// The radio button the app checked is sent checked and its sibling unchecked, and when
/// the app moves the choice the two swap on the wire.
#[test]
fn fr34_a_checked_radio_reaches_its_control() {
    let (mut host, mut controls) = start();
    assert_eq!(
        controls.radios(),
        vec![Some(true), Some(false)],
        "Comfortable is checked, Compact is not"
    );

    let compact = controls.of(WidgetKind::RadioButton)[1];
    change(&mut host, &mut controls, compact, 1.0);
    assert_eq!(controls.radios(), vec![Some(false), Some(true)]);

    let comfortable = controls.of(WidgetKind::RadioButton)[0];
    change(&mut host, &mut controls, comfortable, 1.0);
    assert_eq!(controls.radios(), vec![Some(true), Some(false)]);
}
