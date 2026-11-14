//! The window safe area hook, driven by the size events the Renderer sends.

use dioxus_compose::Host;
use dioxus_compose::prelude::*;
use dioxus_compose::protocol::{HostEvent, Mutation, PropertyValue, decode_batch};
use dioxus_compose::schema::{EventPayload, PropertyKind, WidgetKind};
use dioxus_compose::window::reset_window_size;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The window state is per thread but the tests share counters, so they run one at a time.
static SERIAL: Mutex<()> = Mutex::new(());

fn size_event(node_id: u32, width_dp: f32, safe: WindowSafeArea) -> HostEvent<'static> {
    HostEvent {
        node_id,
        handler_id: 0,
        payload: EventPayload::WindowSizeChanged {
            width_dp,
            height_dp: 800.0,
            class: WindowSizeClass::from_width_dp(width_dp),
            height_class: WindowHeightClass::from_height_dp(800.0),
            safe_area_top: safe.top,
            safe_area_leading: safe.leading,
            safe_area_trailing: safe.trailing,
        },
    }
}

fn texts(batch: &[u8]) -> Vec<String> {
    decode_batch(batch)
        .unwrap()
        .iter()
        .filter_map(|mutation| match mutation {
            Mutation::SetProp {
                property: PropertyKind::Text,
                value: PropertyValue::String(value),
                ..
            } => Some((*value).to_owned()),
            _ => None,
        })
        .collect()
}

const MAC: WindowSafeArea = WindowSafeArea {
    top: 28.0,
    leading: 78.0,
    trailing: 0.0,
};

static READER_RENDERS: AtomicUsize = AtomicUsize::new(0);
static SIBLING_RENDERS: AtomicUsize = AtomicUsize::new(0);

#[component]
fn Reader() -> Element {
    READER_RENDERS.fetch_add(1, Ordering::SeqCst);
    let safe = use_window_safe_area();
    rsx! { Text { text: "{safe.top} {safe.leading} {safe.trailing}" } }
}

#[component]
fn Sibling() -> Element {
    SIBLING_RENDERS.fetch_add(1, Ordering::SeqCst);
    rsx! { Text { text: "fixed" } }
}

fn app() -> Element {
    rsx! {
        Column {
            Reader {}
            Sibling {}
        }
    }
}

#[test]
fn fr19_8_safe_area_hook_returns_the_renderer_insets() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    reset_window_size();
    let mut host = Host::new(app);
    let first = host.rebuild().unwrap().to_vec();
    assert!(
        texts(&first).contains(&"0 0 0".to_owned()),
        "before the renderer reports, every inset is zero: {:?}",
        texts(&first)
    );

    let (batch, _) = host.dispatch(size_event(0, 400.0, MAC)).unwrap();
    assert_eq!(texts(batch), vec!["28 78 0".to_owned()]);
    reset_window_size();
}

#[test]
fn fr19_8_safe_area_change_redraws_only_its_readers() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    reset_window_size();
    READER_RENDERS.store(0, Ordering::SeqCst);
    SIBLING_RENDERS.store(0, Ordering::SeqCst);
    let mut host = Host::new(app);
    host.rebuild().unwrap();
    assert_eq!(READER_RENDERS.load(Ordering::SeqCst), 1);
    assert_eq!(SIBLING_RENDERS.load(Ordering::SeqCst), 1);

    // Same size class, only the safe area moves (entering full screen, say).
    let (batch, _) = host.dispatch(size_event(0, 400.0, MAC)).unwrap();
    assert_eq!(texts(batch), vec!["28 78 0".to_owned()]);
    assert_eq!(READER_RENDERS.load(Ordering::SeqCst), 2);
    assert_eq!(SIBLING_RENDERS.load(Ordering::SeqCst), 1);

    let (batch, _) = host
        .dispatch(size_event(0, 400.0, WindowSafeArea::default()))
        .unwrap();
    assert_eq!(texts(batch), vec!["0 0 0".to_owned()]);
    assert_eq!(READER_RENDERS.load(Ordering::SeqCst), 3);
    assert_eq!(SIBLING_RENDERS.load(Ordering::SeqCst), 1);

    // Two windows: a change in one wakes nothing in the other.
    static FIRST: AtomicUsize = AtomicUsize::new(0);
    static SECOND: AtomicUsize = AtomicUsize::new(0);

    #[component]
    fn FirstPane() -> Element {
        FIRST.fetch_add(1, Ordering::SeqCst);
        let safe = use_window_safe_area();
        rsx! { Text { text: "one {safe.leading}" } }
    }

    #[component]
    fn SecondPane() -> Element {
        SECOND.fetch_add(1, Ordering::SeqCst);
        let safe = use_window_safe_area();
        rsx! { Text { text: "two {safe.leading}" } }
    }

    fn two_windows() -> Element {
        rsx! {
            window { title: "One", onclosing: move |_| {}, FirstPane {} }
            window { title: "Two", onclosing: move |_| {}, SecondPane {} }
        }
    }

    reset_window_size();
    let mut host = Host::new(two_windows).with_window(Window::new().with_windows());
    let first = host.rebuild().unwrap().to_vec();
    let ids: Vec<u32> = decode_batch(&first)
        .unwrap()
        .iter()
        .filter_map(|m| match m {
            Mutation::Create {
                node_id,
                widget: WidgetKind::Window,
            } => Some(*node_id),
            _ => None,
        })
        .collect();
    assert_eq!(ids.len(), 2);
    let (base_first, base_second) = (FIRST.load(Ordering::SeqCst), SECOND.load(Ordering::SeqCst));

    let (batch, _) = host.dispatch(size_event(ids[1], 400.0, MAC)).unwrap();
    assert_eq!(texts(batch), vec!["two 78".to_owned()]);
    assert_eq!(FIRST.load(Ordering::SeqCst), base_first);
    assert_eq!(SECOND.load(Ordering::SeqCst), base_second + 1);
    reset_window_size();
}
