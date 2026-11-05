//! An HTML page inside a Compose widget tree: `HtmlView`.

use dioxus_compose::Host;
use dioxus_compose::prelude::*;
use dioxus_compose::protocol::{HostEvent, Mutation, decode_batch};
use dioxus_compose::schema::{EventPayload, Modifier, WidgetKind};
use dioxus_compose::window::reset_window_size;
use std::cell::Cell;
use std::collections::HashSet;

mod page {
    use dioxus_compose::html::prelude::*;

    /// A bar as wide as the page, so the width the page was laid out at shows up as the
    /// bar's size.
    pub fn app() -> Element {
        rsx! {
            article {
                style: "display: block; width: 100%; height: 40px; background: #ff0000",
            }
        }
    }
}

// Per thread: the tests in this file run in parallel, and each builds its own Host on its
// own thread, so a shared counter would add up renders from the other tests.
thread_local! {
    static SIBLING_RENDERS: Cell<usize> = const { Cell::new(0) };
}

#[component]
fn Sibling() -> Element {
    SIBLING_RENDERS.with(|count| count.set(count.get() + 1));
    rsx! { Text { text: "fixed" } }
}

fn screen() -> Element {
    rsx! {
        Column {
            Sibling {}
            HtmlView { app: page::app, height: 200.0 }
        }
    }
}

/// The renderer node the `HtmlView` asked to have measured.
fn observed_node(mutations: &[Mutation<'_>]) -> u32 {
    mutations
        .iter()
        .find_map(|mutation| match mutation {
            Mutation::SetModifier {
                node_id,
                modifier: Modifier::ObserveSize { .. },
                ..
            } => Some(*node_id),
            _ => None,
        })
        .expect("the HtmlView asked the renderer to measure its box")
}

fn required_widths(mutations: &[Mutation<'_>]) -> Vec<f32> {
    mutations
        .iter()
        .filter_map(|mutation| match mutation {
            Mutation::SetModifier {
                modifier: Modifier::RequiredSize { width, .. },
                ..
            } => Some(*width),
            _ => None,
        })
        .collect()
}

fn measure(host: &mut Host, node_id: u32, width_dp: f32) -> Vec<f32> {
    let event = HostEvent {
        node_id,
        handler_id: 0,
        payload: EventPayload::WindowSizeChanged {
            width_dp,
            height_dp: 200.0,
            class: WindowSizeClass::from_width_dp(width_dp),
            height_class: WindowHeightClass::from_height_dp(200.0),
            safe_area_leading: 0.0,
            safe_area_top: 0.0,
            safe_area_trailing: 0.0,
        },
    };
    let (batch, _) = host.dispatch(event).unwrap();
    required_widths(&decode_batch(batch).unwrap())
}

#[test]
fn fr34_3_htmlview_lays_out_at_parent_width() {
    reset_window_size();
    let mut host = Host::new(screen);
    let first = decode_batch(host.rebuild().unwrap()).unwrap();
    let node = observed_node(&first);

    let widths = measure(&mut host, node, 420.0);
    assert!(
        widths.contains(&420.0),
        "the page was not laid out at the width its box was measured at: {widths:?}",
    );
    reset_window_size();
}

#[test]
fn fr34_3_width_change_relays_out_only_the_island() {
    reset_window_size();
    SIBLING_RENDERS.with(|count| count.set(0));
    let mut host = Host::new(screen);
    let first = decode_batch(host.rebuild().unwrap()).unwrap();
    let node = observed_node(&first);
    assert_eq!(SIBLING_RENDERS.with(Cell::get), 1);

    measure(&mut host, node, 420.0);
    let widths = measure(&mut host, node, 520.0);
    assert!(
        widths.contains(&520.0),
        "the page was not laid out again at the new width: {widths:?}",
    );
    assert_eq!(
        SIBLING_RENDERS.with(Cell::get),
        1,
        "a width change in the island re-rendered a widget beside it",
    );
    reset_window_size();
}

#[test]
fn fr34_3_island_nodes_hang_from_the_box_and_share_no_id_with_widgets() {
    reset_window_size();
    let mut host = Host::new(screen);
    let first = decode_batch(host.rebuild().unwrap()).unwrap();
    let host_node = observed_node(&first);

    let created: Vec<u32> = first
        .iter()
        .filter_map(|mutation| match mutation {
            Mutation::Create { node_id, .. } => Some(*node_id),
            _ => None,
        })
        .collect();
    let unique: HashSet<u32> = created.iter().copied().collect();
    assert_eq!(unique.len(), created.len(), "two nodes were given one id");

    let island_root = first
        .iter()
        .find_map(|mutation| match mutation {
            Mutation::Insert {
                parent_id, node_id, ..
            } if *parent_id == host_node => Some(*node_id),
            _ => None,
        })
        .expect("the page was not inserted into the HtmlView's box");
    assert!(
        first.iter().any(|mutation| matches!(
            mutation,
            Mutation::Create { node_id, widget: WidgetKind::ScrollColumn } if *node_id == island_root
        )),
        "the page's root is not the scroll node the HTML path writes",
    );
    reset_window_size();
}

/// The scroll an island's page sits in.
fn scroll_node(mutations: &[Mutation<'_>]) -> u32 {
    mutations
        .iter()
        .find_map(|mutation| match mutation {
            Mutation::Create {
                node_id,
                widget: WidgetKind::ScrollColumn,
            } => Some(*node_id),
            _ => None,
        })
        .expect("the island's page has no scroll")
}

fn modifiers_of(mutations: &[Mutation<'_>], node: u32) -> Vec<Modifier> {
    mutations
        .iter()
        .filter_map(|mutation| match mutation {
            Mutation::SetModifier {
                node_id, modifier, ..
            } if *node_id == node => Some(modifier.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn fr34_3_island_scroll_gets_a_size_not_a_fill() {
    // The box an island hangs from measures its children without bounds, so a scroll that
    // only fills would be measured with an infinite height and abort the renderer.
    reset_window_size();
    let mut host = Host::new(screen);
    let first = decode_batch(host.rebuild().unwrap()).unwrap();
    let node = observed_node(&first);
    let scroll = scroll_node(&first);
    // Copied out now: the next dispatch borrows the host and the batch cannot outlive that.
    let mut modifiers = modifiers_of(&first, scroll);
    let (batch, _) = host
        .dispatch(HostEvent {
            node_id: node,
            handler_id: 0,
            payload: EventPayload::WindowSizeChanged {
                width_dp: 420.0,
                height_dp: 200.0,
                class: WindowSizeClass::from_width_dp(420.0),
                height_class: WindowHeightClass::from_height_dp(200.0),
                safe_area_leading: 0.0,
                safe_area_top: 0.0,
                safe_area_trailing: 0.0,
            },
        })
        .unwrap();
    modifiers.extend(modifiers_of(&decode_batch(batch).unwrap(), scroll));
    assert!(
        modifiers
            .iter()
            .any(|modifier| matches!(modifier, Modifier::RequiredSize { .. })),
        "the scroll was not given a size: {modifiers:?}",
    );
    assert!(
        !modifiers.iter().any(|modifier| matches!(
            modifier,
            Modifier::FillMaxWidth | Modifier::FillMaxHeight
        )),
        "the scroll only fills, which an unbounded box cannot satisfy: {modifiers:?}",
    );
    reset_window_size();
}
