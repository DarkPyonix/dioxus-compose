//! Multi-window `rsx!` surface (`window` element, hooks, and wire invariants).

use std::sync::atomic::{AtomicUsize, Ordering};

use dioxus_compose::Host;
use dioxus_compose::prelude::*;
use dioxus_compose::protocol::{HostEvent, Mutation, PropertyValue, decode_batch};
use dioxus_compose::schema::{EventPayload, PropertyKind, WidgetKind};

#[test]
fn fr53_single_window_app_unchanged() {
    fn app() -> Element {
        let mut count = use_signal(|| 0_i32);
        rsx! {
            Column {
                spacing: 8.0,
                Text { text: "Count: {count}" }
                Button {
                    text: "Increment",
                    on_click: move |_| count += 1,
                }
            }
        }
    }

    let mut host = Host::new(app);
    let batch = host
        .rebuild()
        .expect("single-window rebuild should succeed");
    let mutations = decode_batch(batch).expect("batch should decode");

    // A single-window app emits SetTheme and SetWindow (with mode=Single, policy=Exit),
    // creates Column, Text, Button, and inserts Text and Button into Column (never into 0).
    let set_windows: Vec<_> = mutations
        .iter()
        .filter_map(|m| match m {
            Mutation::SetWindow(window) => Some((
                window.title,
                window.width,
                window.height,
                window.min_width,
                window.min_height,
                window.resizable,
                window.chrome,
                window.icon,
                window.mode,
                window.last_window_closed,
            )),
            _ => None,
        })
        .collect();
    assert_eq!(
        set_windows,
        vec![(
            "",
            0,
            0,
            0,
            0,
            true,
            Chrome::Modern,
            0,
            WindowMode::Single,
            LastWindowClosed::Exit
        )]
    );

    // No Window widget is created and no Insert into parent 0 is emitted.
    assert!(
        !mutations.iter().any(|m| matches!(
            m,
            Mutation::Create {
                widget: WidgetKind::Window,
                ..
            }
        )),
        "single-window app must not create a Window widget"
    );
    assert!(
        !mutations
            .iter()
            .any(|m| matches!(m, Mutation::Insert { parent_id: 0, .. })),
        "single-window app must not emit Insert into parent_id 0"
    );
}

#[test]
fn fr53_window_element_opens_a_window() {
    static CLOSED: AtomicUsize = AtomicUsize::new(0);
    CLOSED.store(0, Ordering::SeqCst);

    fn app() -> Element {
        let mut count = use_signal(|| 0_i32);
        let mut second_open = use_signal(|| true);

        rsx! {
            window {
                title: "Primary",
                size: (900.0, 620.0),
                min_size: (400.0, 300.0),
                resizable: true,
                chrome: Chrome::Modern,
                title_bar: TitleBar::Simple,
                icon: 5_u32,
                position: (40.0, 60.0),
                restore_key: "win:primary",
                zoom_level: 1_i32,
                onclosing: move |_| {
                    CLOSED.fetch_add(1, Ordering::SeqCst);
                },
                Column {
                    Text { text: "A: {count}" }
                    Button {
                        text: "Bump",
                        on_click: move |_| {
                            let win = current_window();
                            win.focus();
                            count += 1;
                        },
                    }
                }
            }
            if second_open() {
                window {
                    title: "Secondary",
                    size: (640.0, 480.0),
                    restore_key: "win:secondary",
                    onclosing: move |_| {
                        second_open.set(false);
                    },
                    Column {
                        Text { text: "B: {count}" }
                    }
                }
            }
        }
    }

    let mut host = Host::new(app).with_window(Window::new().with_windows());
    let first = host
        .rebuild()
        .expect("multi-window rebuild should succeed")
        .to_vec();
    let mutations = decode_batch(&first).expect("first batch decodes");

    let window_ids: Vec<u32> = mutations
        .iter()
        .filter_map(|m| match m {
            Mutation::Create {
                node_id,
                widget: WidgetKind::Window,
            } => Some(*node_id),
            _ => None,
        })
        .collect();
    assert_eq!(
        window_ids.len(),
        2,
        "two window elements must create two Window nodes"
    );

    let root_inserts: Vec<(u32, u32)> = mutations
        .iter()
        .filter_map(|m| match m {
            Mutation::Insert {
                parent_id: 0,
                node_id,
                index,
            } => Some((*node_id, *index)),
            _ => None,
        })
        .collect();
    assert_eq!(
        root_inserts,
        vec![(window_ids[0], 0), (window_ids[1], 1)],
        "both Window nodes must be inserted directly under root (0)"
    );

    // Clicking the button in the first window increments the shared signal and focuses the first window.
    let (button_id, click_handler) = mutations
        .iter()
        .find_map(|m| match m {
            Mutation::SetProp {
                node_id,
                property: PropertyKind::OnClick,
                value: PropertyValue::Integer(h),
            } => Some((*node_id, *h as u64)),
            _ => None,
        })
        .expect("Button OnClick handler should be registered");

    let (click_batch, _) = host
        .dispatch(HostEvent {
            node_id: button_id,
            handler_id: click_handler,
            payload: EventPayload::Clicked,
        })
        .expect("button click should dispatch");
    let click_mutations = decode_batch(click_batch).expect("click batch decodes");

    let updated_texts: Vec<&str> = click_mutations
        .iter()
        .filter_map(|m| match m {
            Mutation::SetProp {
                property: PropertyKind::Text,
                value: PropertyValue::String(text),
                ..
            } => Some(*text),
            _ => None,
        })
        .collect();
    assert_eq!(
        updated_texts,
        vec!["A: 1", "B: 1"],
        "both windows must update in a single batch when the shared signal changes"
    );
    assert!(
        click_mutations.contains(&Mutation::FocusWindow {
            node_id: window_ids[0]
        }),
        "current_window().focus() inside the first window's handler must emit FocusWindow for the first window"
    );

    // Requesting close on the second window removes only the second window.
    let second_close_handler = mutations
        .iter()
        .find_map(|m| match m {
            Mutation::SetProp {
                node_id,
                property: PropertyKind::OnWindowCloseRequested,
                value: PropertyValue::Integer(h),
            } if *node_id == window_ids[1] => Some(*h as u64),
            _ => None,
        })
        .expect("second window must have OnWindowCloseRequested");

    let (close_batch, _) = host
        .dispatch(HostEvent {
            node_id: window_ids[1],
            handler_id: second_close_handler,
            payload: EventPayload::WindowCloseRequested,
        })
        .expect("close request should dispatch");
    let close_mutations = decode_batch(close_batch).expect("close batch decodes");
    assert!(
        close_mutations.contains(&Mutation::Remove {
            node_id: window_ids[1]
        }),
        "closing the second window must emit Remove for the second window node"
    );
    assert!(
        !close_mutations.contains(&Mutation::Remove {
            node_id: window_ids[0]
        }),
        "the first window must remain open"
    );
}

#[test]
fn fr53_close_request_is_required() {
    fn app_without_close_handler() -> Element {
        rsx! {
            window {
                title: "Missing close handler",
                Text { text: "oops" }
            }
        }
    }

    let mut host = Host::new(app_without_close_handler).with_window(Window::new().with_windows());
    let result = host.rebuild();
    assert!(
        result.is_err(),
        "a window element without onclosing / onclose_request must fail rebuild, got {result:?}"
    );
}

#[test]
fn fr53_window_events_name_their_window() {
    static FIRST_RUNS: AtomicUsize = AtomicUsize::new(0);
    static SECOND_RUNS: AtomicUsize = AtomicUsize::new(0);
    FIRST_RUNS.store(0, Ordering::SeqCst);
    SECOND_RUNS.store(0, Ordering::SeqCst);

    #[component]
    fn FirstPane() -> Element {
        FIRST_RUNS.fetch_add(1, Ordering::SeqCst);
        let size = use_window_size();
        let zoom = use_zoom();
        let win = current_window();
        rsx! {
            Text {
                text: "w1:id={win.id()} width={size.width_dp as i32} zoom={zoom.level()}",
            }
        }
    }

    #[component]
    fn SecondPane() -> Element {
        SECOND_RUNS.fetch_add(1, Ordering::SeqCst);
        let size = use_window_size();
        let zoom = use_zoom();
        let win = current_window();
        rsx! {
            Text {
                text: "w2:id={win.id()} width={size.width_dp as i32} zoom={zoom.level()}",
            }
        }
    }

    fn app() -> Element {
        rsx! {
            window {
                title: "One",
                onclosing: move |_| {},
                FirstPane {}
            }
            window {
                title: "Two",
                onclosing: move |_| {},
                SecondPane {}
            }
        }
    }

    let mut host = Host::new(app).with_window(Window::new().with_windows());
    let first = host.rebuild().expect("initial rebuild succeeds").to_vec();
    let mutations = decode_batch(&first).expect("decodes");
    let window_ids: Vec<u32> = mutations
        .iter()
        .filter_map(|m| match m {
            Mutation::Create {
                node_id,
                widget: WidgetKind::Window,
            } => Some(*node_id),
            _ => None,
        })
        .collect();
    assert_eq!(window_ids.len(), 2);
    let base_first = FIRST_RUNS.load(Ordering::SeqCst);
    let base_second = SECOND_RUNS.load(Ordering::SeqCst);

    // Resize only the second window.
    let (resize_batch, _) = host
        .dispatch(HostEvent {
            node_id: window_ids[1],
            handler_id: 0,
            payload: EventPayload::WindowSizeChanged {
                width_dp: 1280.0,
                height_dp: 800.0,
                class: WindowSizeClass::Expanded,
                height_class: WindowHeightClass::Medium,
            },
        })
        .expect("WindowSizeChanged for window 2 succeeds");
    let resize_mutations = decode_batch(resize_batch).expect("decodes");
    assert_eq!(
        FIRST_RUNS.load(Ordering::SeqCst),
        base_first,
        "FirstPane must not re-render when the second window resizes"
    );
    assert_eq!(
        SECOND_RUNS.load(Ordering::SeqCst),
        base_second + 1,
        "SecondPane must re-render when the second window resizes"
    );
    let expected_w2 = format!("w2:id={} width=1280 zoom=0", window_ids[1]);
    assert!(
        resize_mutations.iter().any(|m| matches!(
            m,
            Mutation::SetProp {
                property: PropertyKind::Text,
                value: PropertyValue::String(t),
                ..
            } if *t == expected_w2
        )),
        "expected updated text {expected_w2:?} in {resize_mutations:?}"
    );

    // Change zoom on the second window.
    let (zoom_batch, _) = host
        .dispatch(HostEvent {
            node_id: window_ids[1],
            handler_id: 0,
            payload: EventPayload::ZoomChanged {
                k: 1.25,
                os: 1.0,
                level: 3,
            },
        })
        .expect("ZoomChanged for window 2 succeeds");
    let zoom_mutations = decode_batch(zoom_batch).expect("decodes");
    assert_eq!(
        FIRST_RUNS.load(Ordering::SeqCst),
        base_first,
        "FirstPane must not re-render when the second window zooms"
    );
    assert_eq!(
        SECOND_RUNS.load(Ordering::SeqCst),
        base_second + 2,
        "SecondPane must re-render when the second window zooms"
    );
    let expected_w2_zoom = format!("w2:id={} width=1280 zoom=3", window_ids[1]);
    assert!(
        zoom_mutations.iter().any(|m| matches!(
            m,
            Mutation::SetProp {
                property: PropertyKind::Text,
                value: PropertyValue::String(t),
                ..
            } if *t == expected_w2_zoom
        )),
        "expected updated text {expected_w2_zoom:?} in {zoom_mutations:?}"
    );
}

#[test]
fn fr53_root_events_reach_their_registrations() {
    fn app() -> Element {
        let mut windows = use_signal(|| vec![("main".to_string(), 0_u32)]);
        let mut reopens = use_signal(|| 0_u32);

        use_window_requested(move |req: WindowRequest| {
            windows.write().push((req.key, req.request));
        });
        use_application_reopen(move || {
            reopens += 1;
        });

        rsx! {
            for (key, req_id) in windows() {
                window {
                    key: "{key}-{req_id}",
                    title: "{key}:{req_id}:{reopens}",
                    restore_key: "{key}",
                    onclosing: move |_| {},
                    Text { text: "{key}" }
                }
            }
        }
    }

    let mut host = Host::new(app).with_window(Window::new().with_windows());
    host.rebuild().expect("initial rebuild succeeds");

    let (batch, _) = host
        .dispatch(HostEvent {
            node_id: 0,
            handler_id: 0,
            payload: EventPayload::WindowRequested {
                request: 7,
                key: "tab:notes",
            },
        })
        .expect("WindowRequested dispatches");
    let mutations = decode_batch(batch).expect("decodes");
    assert!(
        mutations.iter().any(|m| matches!(
            m,
            Mutation::SetProp {
                property: PropertyKind::WindowRestoreKey,
                value: PropertyValue::String("tab:notes"),
                ..
            }
        )),
        "WindowRequested must open a new window with the requested restore_key: {mutations:?}"
    );

    let (reopen_batch, _) = host
        .dispatch(HostEvent {
            node_id: 0,
            handler_id: 0,
            payload: EventPayload::ApplicationReopen,
        })
        .expect("ApplicationReopen dispatches");
    let reopen_mutations = decode_batch(reopen_batch).expect("decodes");
    assert!(
        reopen_mutations.iter().any(|m| matches!(
            m,
            Mutation::SetProp {
                property: PropertyKind::WindowTitle,
                value: PropertyValue::String("main:0:1"),
                ..
            }
        )),
        "ApplicationReopen must wake the reopen handler: {reopen_mutations:?}"
    );
}

#[test]
fn nfr7_misplaced_window_is_a_protocol_error() {
    fn nested_window_app() -> Element {
        rsx! {
            Column {
                window {
                    title: "Inside Column",
                    onclosing: move |_| {},
                    Text { text: "bad" }
                }
            }
        }
    }

    let mut host = Host::new(nested_window_app).with_window(Window::new().with_windows());
    let result = host.rebuild();
    assert!(
        result.is_err(),
        "a window element inside a Column must fail with MisplacedWindow, got {result:?}"
    );
}

#[test]
fn fr53_two_windows_share_a_signal_and_tear_off_a_tab() {
    fn app() -> Element {
        let mut count = use_signal(|| 1_i32);
        let mut anchor = use_signal(|| Option::<(u32, f32, f32)>::None);
        let mut detached = use_signal(|| Option::<(String, Option<DragFromPointer>)>::None);

        use_window_requested(move |req: WindowRequest| {
            detached.set(Some((req.key, None)));
        });

        rsx! {
            window {
                title: "Primary ({count})",
                restore_key: "primary",
                onclosing: move |_| {},
                Column {
                    Button {
                        text: "Bump",
                        on_click: move |_| count += 1,
                    }
                    Surface {
                        tear_off_key: "tab:editor",
                        pointer_kinds: PointerKinds::DOWN | PointerKinds::MOVE,
                        on_pointer: move |ev: PointerEvent| {
                            if ev.kind() == PointerKind::Down {
                                anchor.set(Some((ev.pointer_id(), ev.x(), ev.y())));
                            } else if ev.kind() == PointerKind::Move
                                && ev.y() < -16.0
                                && let Some((pointer_id, anchor_x, anchor_y)) = anchor.write().take()
                            {
                                detached.set(Some((
                                    "tab:editor".to_string(),
                                    Some(DragFromPointer {
                                        pointer_id,
                                        anchor_x,
                                        anchor_y,
                                    }),
                                )));
                            }
                        },
                        Text { text: "editor ({count})" }
                    }
                }
            }
            window {
                title: "Secondary ({count})",
                restore_key: "secondary",
                onclosing: move |_| {},
                Text { text: "mirror={count}" }
            }
            if let Some((key, drag)) = detached() {
                window {
                    title: "Detached {key} ({count})",
                    restore_key: "{key}",
                    drag_from_pointer: drag,
                    onclosing: move |_| detached.set(None),
                    Text { text: "detached={count}" }
                }
            }
        }
    }

    let mut host = Host::new(app).with_window(Window::new().with_windows());
    let first = host.rebuild().expect("initial rebuild succeeds").to_vec();
    let mutations = decode_batch(&first).expect("decodes");
    let button = mutations
        .iter()
        .find_map(|m| match m {
            Mutation::SetProp {
                node_id,
                property: PropertyKind::OnClick,
                value: PropertyValue::Integer(handler_id),
            } => Some((*node_id, *handler_id as u64)),
            _ => None,
        })
        .expect("Button has OnClick handler");
    let tab_surface = mutations
        .iter()
        .find_map(|m| match m {
            Mutation::SetProp {
                node_id,
                property: PropertyKind::OnPointer,
                value: PropertyValue::Integer(handler_id),
            } => Some((*node_id, *handler_id as u64)),
            _ => None,
        })
        .expect("Surface has OnPointer handler");

    // Clicking the button in window 1 updates both windows' titles and texts.
    let (click_batch, _) = host
        .dispatch(HostEvent {
            node_id: button.0,
            handler_id: button.1,
            payload: EventPayload::Clicked,
        })
        .expect("click succeeds");
    let click_mutations = decode_batch(click_batch).expect("decodes");
    assert!(
        click_mutations.iter().any(|m| matches!(
            m,
            Mutation::SetProp {
                property: PropertyKind::WindowTitle,
                value: PropertyValue::String("Primary (2)"),
                ..
            }
        )),
        "primary window title updates from shared signal: {click_mutations:?}"
    );
    assert!(
        click_mutations.iter().any(|m| matches!(
            m,
            Mutation::SetProp {
                property: PropertyKind::WindowTitle,
                value: PropertyValue::String("Secondary (2)"),
                ..
            }
        )),
        "secondary window title updates from shared signal: {click_mutations:?}"
    );

    // Tearing off the tab via pointer drag opens a third window carrying DragFromPointer.
    host.dispatch(HostEvent {
        node_id: tab_surface.0,
        handler_id: tab_surface.1,
        payload: EventPayload::Pointer {
            kind: PointerKind::Down,
            pointer_id: 9,
            pointer_type: PointerType::Mouse,
            x: 32.0,
            y: 14.0,
            buttons: 1,
            button: 1,
            modifiers: 0,
            click_count: 1,
            delta_x: 0.0,
            delta_y: 0.0,
            delta_mode: WheelDeltaMode::Pixel,
            timestamp_nanos: 1,
        },
    })
    .expect("pointer down succeeds");
    let (tear_batch, _) = host
        .dispatch(HostEvent {
            node_id: tab_surface.0,
            handler_id: tab_surface.1,
            payload: EventPayload::Pointer {
                kind: PointerKind::Move,
                pointer_id: 9,
                pointer_type: PointerType::Mouse,
                x: 32.0,
                y: -30.0,
                buttons: 1,
                button: 0,
                modifiers: 0,
                click_count: 0,
                delta_x: 0.0,
                delta_y: 0.0,
                delta_mode: WheelDeltaMode::Pixel,
                timestamp_nanos: 2,
            },
        })
        .expect("pointer drag tear-off succeeds");
    let tear_mutations = decode_batch(tear_batch).expect("decodes");
    let mut expected_drag = [0_u8; 12];
    expected_drag[0..4].copy_from_slice(&9_u32.to_le_bytes());
    expected_drag[4..8].copy_from_slice(&32.0_f32.to_le_bytes());
    expected_drag[8..12].copy_from_slice(&14.0_f32.to_le_bytes());
    assert!(
        tear_mutations.iter().any(|m| matches!(
            m,
            Mutation::SetProp {
                property: PropertyKind::DragFromPointer,
                value: PropertyValue::Bytes(bytes),
                ..
            } if *bytes == expected_drag
        )),
        "torn-off window carries DragFromPointer bytes: {tear_mutations:?}"
    );
}
