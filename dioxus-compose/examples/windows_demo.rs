//! Two windows sharing one signal, and a tab that tears off into a third window
//! (`DragFromPointer`) when dragged out of the tab bar.

use dioxus_compose::prelude::*;

#[derive(Clone, PartialEq)]
struct DetachedTab {
    title: String,
    drag: Option<DragFromPointer>,
}

fn app() -> Element {
    let mut counter = use_signal(|| 0_i32);
    let mut primary_open = use_signal(|| true);
    let mut inspector_open = use_signal(|| true);
    let mut tabs = use_signal(|| vec!["main.rs".to_string(), "notes.md".to_string()]);
    let mut press_anchor = use_signal(|| Option::<(String, u32, f32, f32)>::None);
    let mut detached = use_signal(|| Option::<DetachedTab>::None);

    use_window_requested(move |req: WindowRequest| {
        if let Some(title) = req.key.strip_prefix("tab:") {
            let title = title.to_string();
            tabs.write().retain(|t| t != &title);
            detached.set(Some(DetachedTab { title, drag: None }));
        }
    });

    use_application_reopen(move || {
        if !primary_open() && !inspector_open() && detached().is_none() {
            primary_open.set(true);
        }
    });

    rsx! {
        if primary_open() {
            window {
                title: "Workspace",
                restore_key: "workspace",
                size: (720.0, 480.0),
                min_size: (400.0, 280.0),
                onclosing: move |_| primary_open.set(false),
                Column {
                    fill_max_width: true,
                    padding_role: SpaceRole::Lg,
                    space_role: SpaceRole::Md,
                    Text {
                        text: "Shared count: {counter}",
                        type_role: TypeRole::Headline,
                    }
                    Row {
                        space_role: SpaceRole::Sm,
                        Button {
                            text: "Increment",
                            on_click: move |_| counter += 1,
                        }
                        if !inspector_open() {
                            Button {
                                text: "Open Inspector",
                                variant: ButtonVariant::Tonal,
                                on_click: move |_| inspector_open.set(true),
                            }
                        }
                    }
                    Row {
                        space_role: SpaceRole::Sm,
                        on_window_drag_over: move |over: WindowDragOver| {
                            if over.kind == WindowDragOverKind::Drop
                                && let Some(tab) = detached.write().take()
                            {
                                tabs.write().push(tab.title);
                            }
                        },
                        for tab in tabs() {
                            {
                                let tab_for_pointer = tab.clone();
                                rsx! {
                                    Surface {
                                        key: "{tab}",
                                        tear_off_key: "tab:{tab}",
                                        padding_role: SpaceRole::Sm,
                                        shape_role: ShapeRole::Medium,
                                        pointer_kinds: PointerKinds::DOWN | PointerKinds::MOVE | PointerKinds::UP,
                                        on_pointer: move |ev: PointerEvent| {
                                            match ev.kind() {
                                                PointerKind::Down => {
                                                    press_anchor.set(Some((
                                                        tab_for_pointer.clone(),
                                                        ev.pointer_id(),
                                                        ev.x(),
                                                        ev.y(),
                                                    )));
                                                }
                                                PointerKind::Move if ev.y() < -20.0 => {
                                                    if let Some((title, pointer_id, anchor_x, anchor_y)) =
                                                        press_anchor.write().take()
                                                    {
                                                        tabs.write().retain(|t| t != &title);
                                                        detached.set(Some(DetachedTab {
                                                            title,
                                                            drag: Some(DragFromPointer {
                                                                pointer_id,
                                                                anchor_x,
                                                                anchor_y,
                                                            }),
                                                        }));
                                                    }
                                                }
                                                PointerKind::Up | PointerKind::Cancel => {
                                                    press_anchor.set(None);
                                                }
                                                _ => {}
                                            }
                                        },
                                        Text { text: "{tab}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if inspector_open() {
            window {
                title: "Inspector",
                restore_key: "inspector",
                size: (420.0, 360.0),
                onclosing: move |_| inspector_open.set(false),
                Column {
                    fill_max_width: true,
                    padding_role: SpaceRole::Lg,
                    space_role: SpaceRole::Md,
                    Text {
                        text: "Inspector count: {counter}",
                        type_role: TypeRole::Title,
                    }
                    Button {
                        text: "Add 10",
                        on_click: move |_| counter += 10,
                    }
                }
            }
        }
        if let Some(tab) = detached() {
            window {
                title: "{tab.title}",
                restore_key: "tab:{tab.title}",
                size: (480.0, 320.0),
                drag_from_pointer: tab.drag,
                onclosing: move |_| detached.set(None),
                onwindowdrag: move |drag: Event<WindowDragEvent>| {
                    if drag.data().phase != WindowDragPhase::Move
                        && let Some(cur) = detached.write().as_mut()
                    {
                        cur.drag = None;
                    }
                },
                Column {
                    fill_max_width: true,
                    padding_role: SpaceRole::Lg,
                    Text { text: "Torn-off tab: {tab.title} (count={counter})" }
                }
            }
        }
    }
}

fn main() {
    dioxus_compose::LaunchBuilder::new()
        .windows()
        .on_last_window_closed(LastWindowClosed::Exit)
        .launch(app);
}
