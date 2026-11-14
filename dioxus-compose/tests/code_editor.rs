//! A code editor written with `rsx!`: what the component puts on the wire, how its five
//! events reach their handlers, and edits asked of a handle.

use std::cell::RefCell;

use dioxus_compose::code::SYNTAX_SPAN_LEN;
use dioxus_compose::prelude::*;
use dioxus_compose::protocol::{HostEvent, Mutation, PropertyValue, decode_batch, encode_event};
use dioxus_compose::schema::RevealAlign;
use dioxus_compose::schema::{PointerKind, PointerType, WheelDeltaMode};
use dioxus_compose::{
    EventPayload, FontRef, FontRefs, GenericFamily, Host, PropertyKind, WidgetKind,
};

/// One decoded record with its borrowed parts made owned, so the batch can be dropped.
#[derive(Clone, Debug, PartialEq)]
enum Record {
    Create(u32, WidgetKind),
    Prop(u32, PropertyKind, Value),
    EditCode {
        node_id: u32,
        request_id: u32,
        base_version: u32,
        range: CodeRange,
        text: String,
    },
    SetSelection {
        node_id: u32,
        base_version: u32,
        anchor: (u32, u32),
        active: (u32, u32),
    },
    RevealRange {
        node_id: u32,
        base_version: u32,
        range: CodeRange,
        align: RevealAlign,
    },
    SetFolded {
        node_id: u32,
        start_line: u32,
        collapsed: bool,
    },
    QueryPositionRect {
        node_id: u32,
        request: u32,
        position: Position,
    },
    SetScrollOffset {
        node_id: u32,
        offset_px: f64,
    },
    Other,
}

#[derive(Clone, Debug, PartialEq)]
enum Value {
    None,
    Text(String),
    Bool(bool),
    Integer(i64),
    Float(f32),
    Bytes(Vec<u8>),
}

fn records(batch: &[u8]) -> Vec<Record> {
    decode_batch(batch)
        .unwrap()
        .into_iter()
        .map(|mutation| match mutation {
            Mutation::Create { node_id, widget } => Record::Create(node_id, widget),
            Mutation::SetProp {
                node_id,
                property,
                value,
            } => Record::Prop(
                node_id,
                property,
                match value {
                    PropertyValue::None => Value::None,
                    PropertyValue::String(text) => Value::Text(text.to_string()),
                    PropertyValue::Bool(value) => Value::Bool(value),
                    PropertyValue::Integer(value) => Value::Integer(value),
                    PropertyValue::Float(value) => Value::Float(value),
                    PropertyValue::Bytes(bytes) => Value::Bytes(bytes.to_vec()),
                },
            ),
            Mutation::EditCode {
                node_id,
                request_id,
                base_version,
                start_line,
                start_column,
                end_line,
                end_column,
                text,
            } => Record::EditCode {
                node_id,
                request_id,
                base_version,
                range: CodeRange::of(start_line, start_column, end_line, end_column),
                text: text.to_string(),
            },
            Mutation::SetSelection {
                node_id,
                base_version,
                anchor_line,
                anchor_column,
                active_line,
                active_column,
            } => Record::SetSelection {
                node_id,
                base_version,
                anchor: (anchor_line, anchor_column),
                active: (active_line, active_column),
            },
            Mutation::RevealRange {
                node_id,
                base_version,
                start_line,
                start_column,
                end_line,
                end_column,
                align,
            } => Record::RevealRange {
                node_id,
                base_version,
                range: CodeRange::of(start_line, start_column, end_line, end_column),
                align,
            },
            Mutation::SetFolded {
                node_id,
                start_line,
                collapsed,
            } => Record::SetFolded {
                node_id,
                start_line,
                collapsed,
            },
            Mutation::QueryPositionRect {
                node_id,
                request,
                line,
                column,
            } => Record::QueryPositionRect {
                node_id,
                request,
                position: Position::new(line, column),
            },
            Mutation::SetScrollOffset { node_id, offset_px } => {
                Record::SetScrollOffset { node_id, offset_px }
            }
            _ => Record::Other,
        })
        .collect()
}

fn node_of(records: &[Record], widget: WidgetKind) -> u32 {
    records
        .iter()
        .find_map(|record| match record {
            Record::Create(node_id, kind) if *kind == widget => Some(*node_id),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no {widget:?} was created"))
}

fn prop_of(records: &[Record], node: u32, property: PropertyKind) -> Option<Value> {
    records.iter().rev().find_map(|record| match record {
        Record::Prop(node_id, kind, value) if *node_id == node && *kind == property => {
            Some(value.clone())
        }
        _ => None,
    })
}

fn handler_of(records: &[Record], node: u32, property: PropertyKind) -> u64 {
    match prop_of(records, node, property) {
        Some(Value::Integer(handler)) => handler as u64,
        other => panic!("node {node} has no {property:?} handler, found {other:?}"),
    }
}

fn send(host: &mut Host, node: u32, handler: u64, payload: EventPayload<'_>) -> Vec<Record> {
    send_for_result(host, node, handler, payload).0
}

/// What the call returned: its batch, and the handler's synchronous result, which is 1 when
/// the handler consumed the event.
fn send_for_result(
    host: &mut Host,
    node: u32,
    handler: u64,
    payload: EventPayload<'_>,
) -> (Vec<Record>, i64) {
    let mut wire = Vec::new();
    encode_event(
        &HostEvent {
            node_id: node,
            handler_id: handler,
            payload,
        },
        &mut wire,
    )
    .unwrap();
    let (batch, result) = host.dispatch_event(&wire).unwrap();
    (records(batch), result)
}

thread_local! {
    static CHANGES: RefCell<Vec<CodeChange>> = const { RefCell::new(Vec::new()) };
    static REJECTED: RefCell<Vec<EditRejected>> = const { RefCell::new(Vec::new()) };
    static HOVERS: RefCell<Vec<CodeHover>> = const { RefCell::new(Vec::new()) };
    static SAVES: RefCell<Vec<SaveRequest>> = const { RefCell::new(Vec::new()) };
    static ACTIVATED: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
}

fn editor_app() -> Element {
    rsx! {
        CodeEditor {
            text: "fn main() {{}}\n",
            syntax_spans: SyntaxSpans::new([SyntaxSpan::role(
                0,
                CodeRange::of(0, 0, 0, 2),
                ColorRole::Primary,
            )]),
            decorations: Decorations::new([
                Decoration::underline(0, CodeRange::of(0, 3, 0, 7), Severity::Warning),
                Decoration::code_lens(0, 0, "Run", 41),
            ]),
            tab_width: 4_u32,
            on_change: move |change: CodeChange| CHANGES.with_borrow_mut(|seen| seen.push(change)),
            on_edit_rejected: move |rejected: EditRejected| REJECTED.with_borrow_mut(|seen| seen.push(rejected)),
            on_hover: move |hover: CodeHover| HOVERS.with_borrow_mut(|seen| seen.push(hover)),
            on_save: move |save: SaveRequest| SAVES.with_borrow_mut(|seen| seen.push(save)),
            on_decoration_click: move |id: u64| ACTIVATED.with_borrow_mut(|seen| seen.push(id)),
        }
    }
}

/// The editor opens with its document as `Text`, its two lists as blobs, its tab width,
/// and a handler for each of its five events, and nothing it does not need.
#[test]
fn fr38_an_editor_sends_its_document_its_lists_and_its_handlers() {
    let mut host = Host::new(editor_app);
    let batch = records(host.rebuild().unwrap());
    let editor = node_of(&batch, WidgetKind::CodeEditor);
    assert_eq!(
        prop_of(&batch, editor, PropertyKind::Text),
        Some(Value::Text("fn main() {}\n".into()))
    );
    let Some(Value::Bytes(decorations)) = prop_of(&batch, editor, PropertyKind::Decorations) else {
        panic!("the decorations did not travel as bytes");
    };
    assert_eq!(
        Decorations::from_bytes(decorations).decorations(),
        vec![
            Decoration::underline(0, CodeRange::of(0, 3, 0, 7), Severity::Warning),
            Decoration::code_lens(0, 0, "Run", 41),
        ]
    );
    let Some(Value::Bytes(spans)) = prop_of(&batch, editor, PropertyKind::SyntaxSpans) else {
        panic!("the colour runs did not travel as bytes");
    };
    assert_eq!(spans.len(), SYNTAX_SPAN_LEN);
    assert_eq!(
        prop_of(&batch, editor, PropertyKind::TabWidth),
        Some(Value::Integer(4))
    );
    for property in [
        PropertyKind::OnValueChange,
        PropertyKind::OnEditRejected,
        PropertyKind::OnHover,
        PropertyKind::OnSave,
        PropertyKind::OnDecorationClick,
    ] {
        handler_of(&batch, editor, property);
    }
}

/// Every event reaches the handler it belongs to, carrying what the Renderer said, and a
/// hover with no anchor under it arrives as no decoration rather than as id zero.
#[test]
fn fr38_each_code_event_reaches_its_handler() {
    let mut host = Host::new(editor_app);
    let batch = records(host.rebuild().unwrap());
    let editor = node_of(&batch, WidgetKind::CodeEditor);

    send(
        &mut host,
        editor,
        handler_of(&batch, editor, PropertyKind::OnValueChange),
        EventPayload::CodeChanged {
            version: 1,
            start_line: 0,
            start_column: 12,
            end_line: 0,
            end_column: 12,
            text: " // 😀",
        },
    );
    CHANGES.with_borrow(|seen| {
        assert_eq!(
            seen.as_slice(),
            &[CodeChange {
                version: 1,
                range: CodeRange::of(0, 12, 0, 12),
                text: " // 😀".into(),
            }]
        );
    });

    send(
        &mut host,
        editor,
        handler_of(&batch, editor, PropertyKind::OnEditRejected),
        EventPayload::CodeEditRejected {
            request_id: 7,
            base_version: 0,
            current_version: 1,
            start_line: 0,
            start_column: 3,
            end_line: 0,
            end_column: 7,
        },
    );
    REJECTED.with_borrow(|seen| {
        assert_eq!(
            seen.as_slice(),
            &[EditRejected {
                request_id: 7,
                base_version: 0,
                current_version: 1,
                range: CodeRange::of(0, 3, 0, 7),
            }]
        );
    });

    let hover = handler_of(&batch, editor, PropertyKind::OnHover);
    send(
        &mut host,
        editor,
        hover,
        EventPayload::CodeHovered {
            decoration: 42,
            line: 0,
            column: 4,
            phase: HoverPhase::Rest,
        },
    );
    send(
        &mut host,
        editor,
        hover,
        EventPayload::CodeHovered {
            decoration: 0,
            line: 0,
            column: 9,
            phase: HoverPhase::Leave,
        },
    );
    HOVERS.with_borrow(|seen| {
        assert_eq!(
            seen.as_slice(),
            &[
                CodeHover {
                    decoration: Some(42),
                    position: Position::new(0, 4),
                    phase: HoverPhase::Rest,
                },
                CodeHover {
                    decoration: None,
                    position: Position::new(0, 9),
                    phase: HoverPhase::Leave,
                },
            ]
        );
    });

    send(
        &mut host,
        editor,
        handler_of(&batch, editor, PropertyKind::OnSave),
        EventPayload::CodeSaveRequested { version: 1 },
    );
    SAVES.with_borrow(|seen| assert_eq!(seen.as_slice(), &[SaveRequest { version: 1 }]));

    send(
        &mut host,
        editor,
        handler_of(&batch, editor, PropertyKind::OnDecorationClick),
        EventPayload::DecorationActivated { decoration: 41 },
    );
    ACTIVATED.with_borrow(|seen| assert_eq!(seen.as_slice(), &[41]));
}

thread_local! {
    static OPENED: RefCell<String> = RefCell::new(String::from("let a = 1;\n"));
}

fn formatting_app() -> Element {
    let handle = use_code_editor();
    let mut text = use_signal(|| OPENED.with_borrow(Clone::clone));
    rsx! {
        Column {
            CodeEditor {
                text: text(),
                handle,
                // Written back, the way an application keeps its own copy. The text that
                // arrives is the reader's buffer, which the editor treats as nothing new.
                on_change: move |change: CodeChange| {
                    let mut current = text();
                    change.apply_to(&mut current);
                    text.set(current);
                },
            }
            Button {
                text: "Format",
                on_click: move |_| handle.edit(7, 0, CodeRange::of(0, 5, 0, 5), " "),
            }
        }
    }
}

/// An edit asked of a handle goes out as one `EditCode`, addressed to the editor that
/// carries the handle, with its request id and its base version, in the batch the call
/// that asked for it produced. The handle itself never crosses the boundary.
#[test]
fn fr38_an_edit_asked_of_a_handle_goes_out_to_its_editor() {
    let mut host = Host::new(formatting_app);
    let batch = records(host.rebuild().unwrap());
    let editor = node_of(&batch, WidgetKind::CodeEditor);
    let button = node_of(&batch, WidgetKind::Button);
    assert!(
        !batch
            .iter()
            .any(|record| matches!(record, Record::EditCode { .. })),
        "nothing was asked yet"
    );

    let after = send(
        &mut host,
        button,
        handler_of(&batch, button, PropertyKind::OnClick),
        EventPayload::Clicked,
    );
    assert_eq!(
        after
            .iter()
            .filter(|record| matches!(record, Record::EditCode { .. }))
            .collect::<Vec<_>>(),
        vec![&Record::EditCode {
            node_id: editor,
            request_id: 7,
            base_version: 0,
            range: CodeRange::of(0, 5, 0, 5),
            text: " ".into(),
        }]
    );
}

/// The edit comes back as an ordinary change, and an application that writes it back into
/// the signal it passes as `text` sends the buffer it already has. That is the text the
/// Renderer recognises and leaves alone, so the caret and the undo history survive.
#[test]
fn fr38_writing_a_change_back_sends_the_text_the_editor_already_holds() {
    let mut host = Host::new(formatting_app);
    let batch = records(host.rebuild().unwrap());
    let editor = node_of(&batch, WidgetKind::CodeEditor);
    let after = send(
        &mut host,
        editor,
        handler_of(&batch, editor, PropertyKind::OnValueChange),
        EventPayload::CodeChanged {
            version: 1,
            start_line: 0,
            start_column: 5,
            end_line: 0,
            end_column: 5,
            text: " ",
        },
    );
    assert_eq!(
        prop_of(&after, editor, PropertyKind::Text),
        Some(Value::Text("let a  = 1;\n".into()))
    );
}

fn overlays_app() -> Element {
    rsx! {
        CodeEditor {
            text: "let x = 1;\n",
            decorations: Decorations::new([
                Decoration::code_lens(0, 0, "Run | Debug", 7),
                Decoration::ghost_text(0, Position::new(0, 10), " // one", 8),
                Decoration::hover_anchor(0, CodeRange::of(0, 4, 0, 5), 9),
            ]),
        }
    }
}

/// CodeLens zones and ghost text are decorations, so they reach the Renderer through the
/// `decorations` list with their kind, anchor, text and id intact.
#[test]
fn fr38_code_lens_and_ghost_text_travel_in_the_decoration_list() {
    let mut host = Host::new(overlays_app);
    let batch = records(host.rebuild().unwrap());
    let editor = node_of(&batch, WidgetKind::CodeEditor);
    let Some(Value::Bytes(bytes)) = prop_of(&batch, editor, PropertyKind::Decorations) else {
        panic!("the decorations did not travel as bytes");
    };
    let list = Decorations::from_bytes(bytes).decorations();
    assert_eq!(list.len(), 3);
    assert_eq!(list[0], Decoration::code_lens(0, 0, "Run | Debug", 7));
    assert_eq!(
        list[1],
        Decoration::ghost_text(0, Position::new(0, 10), " // one", 8)
    );
    assert_eq!(list[2].id, 9);
}

fn detached_app() -> Element {
    let handle = use_code_editor();
    rsx! {
        Column {
            CodeEditor { text: "x" }
            Button {
                text: "Format",
                on_click: move |_| handle.edit(1, 0, CodeRange::of(0, 0, 0, 0), "y"),
            }
        }
    }
}

/// A handle no editor carries names nothing, so its edit is dropped here instead of being
/// sent to a node the Renderer would have to report as unknown.
#[test]
fn fr38_an_edit_for_a_handle_no_editor_carries_is_dropped() {
    let mut host = Host::new(detached_app);
    let batch = records(host.rebuild().unwrap());
    let button = node_of(&batch, WidgetKind::Button);
    let after = send(
        &mut host,
        button,
        handler_of(&batch, button, PropertyKind::OnClick),
        EventPayload::Clicked,
    );
    assert!(
        !after
            .iter()
            .any(|record| matches!(record, Record::EditCode { .. }))
    );
}

fn navigating_app() -> Element {
    let handle = use_code_editor();
    rsx! {
        Column {
            CodeEditor { text: "let a = 1;\nlet b = 2;\n", handle }
            Button {
                text: "Select",
                on_click: move |_| handle.select(3, Position::new(1, 4), Position::new(1, 5)),
            }
            Button {
                text: "Reveal",
                on_click: move |_| handle.reveal(3, CodeRange::of(1, 4, 1, 5), RevealAlign::Center),
            }
            Button {
                text: "Fold",
                on_click: move |_| handle.fold(0, true),
            }
        }
    }
}

fn button_named(batch: &[Record], host: &mut Host, nth: usize) -> Vec<Record> {
    let buttons: Vec<u32> = batch
        .iter()
        .filter_map(|record| match record {
            Record::Create(node_id, WidgetKind::Button) => Some(*node_id),
            _ => None,
        })
        .collect();
    let button = buttons[nth];
    send(
        host,
        button,
        handler_of(batch, button, PropertyKind::OnClick),
        EventPayload::Clicked,
    )
}

/// A selection, a reveal and a fold asked of a handle each go out as one record addressed to the
/// editor that carries the handle, in the batch of the call that asked, with the version they are
/// written against. The application takes the reader to a match or a problem this way.
#[test]
fn fr38_select_reveal_and_fold_asked_of_a_handle_go_out_to_its_editor() {
    let mut host = Host::new(navigating_app);
    let batch = records(host.rebuild().unwrap());
    let editor = node_of(&batch, WidgetKind::CodeEditor);
    assert!(!batch.iter().any(|r| matches!(
        r,
        Record::SetSelection { .. } | Record::RevealRange { .. } | Record::SetFolded { .. }
    )));

    let after = button_named(&batch, &mut host, 0);
    assert_eq!(
        after
            .iter()
            .filter(|r| matches!(r, Record::SetSelection { .. }))
            .collect::<Vec<_>>(),
        vec![&Record::SetSelection {
            node_id: editor,
            base_version: 3,
            anchor: (1, 4),
            active: (1, 5)
        }]
    );
    let after = button_named(&batch, &mut host, 1);
    assert_eq!(
        after
            .iter()
            .filter(|r| matches!(r, Record::RevealRange { .. }))
            .collect::<Vec<_>>(),
        vec![&Record::RevealRange {
            node_id: editor,
            base_version: 3,
            range: CodeRange::of(1, 4, 1, 5),
            align: RevealAlign::Center
        }]
    );
    let after = button_named(&batch, &mut host, 2);
    assert_eq!(
        after
            .iter()
            .filter(|r| matches!(r, Record::SetFolded { .. }))
            .collect::<Vec<_>>(),
        vec![&Record::SetFolded {
            node_id: editor,
            start_line: 0,
            collapsed: true
        }]
    );
}

fn unattached_app() -> Element {
    // A handle no editor carries.
    let handle = CodeEditorHandle::new();
    rsx! {
        Column {
            Button {
                text: "Select",
                on_click: move |_| {
                    handle.select(0, Position::new(0, 0), Position::new(0, 1));
                    handle.reveal(0, CodeRange::of(0, 0, 0, 1), RevealAlign::Nearest);
                    handle.fold(0, true);
                },
            }
        }
    }
}

/// A handle no editor carries names nothing: its requests are dropped before the boundary rather
/// than sent to a node the Renderer would have to report as unknown.
#[test]
fn fr38_requests_of_a_handle_no_editor_carries_are_dropped() {
    let mut host = Host::new(unattached_app);
    let batch = records(host.rebuild().unwrap());
    let after = button_named(&batch, &mut host, 0);
    assert!(
        !after.iter().any(|r| matches!(
            r,
            Record::SetSelection { .. } | Record::RevealRange { .. } | Record::SetFolded { .. }
        )),
        "{after:?}"
    );
}

/// The fonts the editor in `styled_app` is set in.
fn editor_fonts() -> FontRefs {
    FontRefs::new([
        FontRef::System("JetBrains Mono".to_owned()),
        FontRef::Generic(GenericFamily::Monospace),
    ])
}

thread_local! {
    static METRICS: RefCell<Vec<CodeMetrics>> = const { RefCell::new(Vec::new()) };
    static SCROLLS: RefCell<Vec<CodeScroll>> = const { RefCell::new(Vec::new()) };
}

fn styled_app() -> Element {
    rsx! {
        CodeEditor {
            text: "let a = 1;\n",
            type_role: TypeRole::Mono,
            font_size: 13.0,
            line_height: 20.0,
            font: editor_fonts(),
            on_code_metrics: move |metrics: CodeMetrics| METRICS.with_borrow_mut(|seen| seen.push(metrics)),
            on_code_scrolled: move |scroll: CodeScroll| SCROLLS.with_borrow_mut(|seen| seen.push(scroll)),
        }
    }
}

/// An editor set at the size and row height of a mockup sends its type role, its size in
/// sp, its row height in sp and its own monospace list, as written.
#[test]
fn fr38_an_editor_sends_the_type_size_row_height_and_fonts_it_was_given() {
    let mut host = Host::new(styled_app);
    let batch = records(host.rebuild().unwrap());
    let editor = node_of(&batch, WidgetKind::CodeEditor);
    assert_eq!(
        prop_of(&batch, editor, PropertyKind::TypeRole),
        Some(Value::Integer(i64::from(u16::from(TypeRole::Mono))))
    );
    assert_eq!(
        prop_of(&batch, editor, PropertyKind::FontSize),
        Some(Value::Float(13.0))
    );
    assert_eq!(
        prop_of(&batch, editor, PropertyKind::LineHeight),
        Some(Value::Float(20.0))
    );
    assert_eq!(
        prop_of(&batch, editor, PropertyKind::Font),
        Some(Value::Bytes(editor_fonts().as_bytes().to_vec()))
    );
}

/// An editor that names none of its look, its pointer kinds or its new handlers sends none
/// of them, so the design system draws it as before and the Renderer reports nothing nobody
/// listens for.
#[test]
fn fr38_an_editor_that_names_none_of_its_look_sends_none_of_it() {
    fn plain() -> Element {
        rsx! {
            CodeEditor { text: "x", colors: CodeEditorColors::new() }
        }
    }
    let mut host = Host::new(plain);
    let batch = records(host.rebuild().unwrap());
    let editor = node_of(&batch, WidgetKind::CodeEditor);
    for property in [
        PropertyKind::TypeRole,
        PropertyKind::FontSize,
        PropertyKind::LineHeight,
        PropertyKind::Font,
        PropertyKind::CodeEditorColors,
        PropertyKind::PointerKinds,
        PropertyKind::OnCaretRect,
        PropertyKind::OnKeyPreview,
        PropertyKind::OnCodePointer,
        PropertyKind::OnCodeMetrics,
        PropertyKind::OnCodeScrolled,
    ] {
        assert_eq!(
            prop_of(&batch, editor, property),
            None,
            "{property:?} was sent by an editor that did not name it"
        );
    }
}

/// The row height and the scroll offset arrive as compose-rust's own `CodeMetrics` and
/// `CodeScroll`, in physical pixels, unrounded.
#[test]
fn fr38_row_height_and_scroll_arrive_as_code_metrics_and_code_scroll() {
    let mut host = Host::new(styled_app);
    let batch = records(host.rebuild().unwrap());
    let editor = node_of(&batch, WidgetKind::CodeEditor);
    send(
        &mut host,
        editor,
        handler_of(&batch, editor, PropertyKind::OnCodeMetrics),
        EventPayload::CodeMetrics {
            line_height_px: 40.0,
        },
    );
    send(
        &mut host,
        editor,
        handler_of(&batch, editor, PropertyKind::OnCodeScrolled),
        EventPayload::CodeScrolled { offset_px: 1234.5 },
    );
    METRICS.with_borrow(|seen| {
        assert_eq!(
            seen.as_slice(),
            &[CodeMetrics {
                line_height_px: 40.0
            }]
        );
    });
    SCROLLS.with_borrow(|seen| assert_eq!(seen.as_slice(), &[CodeScroll { offset_px: 1234.5 }]));
}

fn scrolling_app() -> Element {
    let handle = use_code_editor();
    rsx! {
        Column {
            CodeEditor { text: "a\nb\nc\n", handle }
            Button {
                text: "Scroll",
                on_click: move |_| handle.set_scroll_offset(1234.5),
            }
        }
    }
}

/// A scroll offset asked of a handle goes out as one `SetScrollOffset` to the editor that
/// carries the handle, in physical pixels, which is how two editors of a diff are kept at
/// the same place.
#[test]
fn fr38_a_scroll_offset_asked_of_a_handle_goes_out_to_its_editor() {
    let mut host = Host::new(scrolling_app);
    let batch = records(host.rebuild().unwrap());
    let editor = node_of(&batch, WidgetKind::CodeEditor);
    assert!(
        !batch
            .iter()
            .any(|record| matches!(record, Record::SetScrollOffset { .. }))
    );
    let after = button_named(&batch, &mut host, 0);
    assert_eq!(
        after
            .iter()
            .filter(|record| matches!(record, Record::SetScrollOffset { .. }))
            .collect::<Vec<_>>(),
        vec![&Record::SetScrollOffset {
            node_id: editor,
            offset_px: 1234.5,
        }]
    );
}

fn recoloured_app() -> Element {
    let mut coloured = use_signal(|| true);
    rsx! {
        Column {
            CodeEditor {
                text: "x",
                colors: if coloured() {
                    CodeEditorColors::new().container(ColorRole::Surface)
                } else {
                    CodeEditorColors::new()
                },
            }
            Button {
                text: "Plain",
                on_click: move |_| coloured.set(false),
            }
        }
    }
}

/// Colours that become empty are taken away once, with `None`, so the editor goes back to
/// the design system's colours rather than keeping the last ones it was sent.
#[test]
fn fr38_colours_that_become_empty_are_withdrawn() {
    let mut host = Host::new(recoloured_app);
    let batch = records(host.rebuild().unwrap());
    let editor = node_of(&batch, WidgetKind::CodeEditor);
    assert!(matches!(
        prop_of(&batch, editor, PropertyKind::CodeEditorColors),
        Some(Value::Bytes(_))
    ));
    let after = button_named(&batch, &mut host, 0);
    assert_eq!(
        after
            .iter()
            .filter(|record| matches!(record, Record::Prop(_, PropertyKind::CodeEditorColors, _)))
            .collect::<Vec<_>>(),
        vec![&Record::Prop(
            editor,
            PropertyKind::CodeEditorColors,
            Value::None
        )]
    );
}

thread_local! {
    static CARETS: RefCell<Vec<CaretRect>> = const { RefCell::new(Vec::new()) };
}

fn caret_app() -> Element {
    rsx! {
        CodeEditor {
            text: "let a = 1;\n",
            on_caret_rect: move |caret: CaretRect| CARETS.with_borrow_mut(|seen| seen.push(caret)),
        }
    }
}

/// The caret's place and its rectangle in the window's dp reach `on_caret_rect` as
/// compose-rust's `CaretRect`, as the Renderer reported them.
#[test]
fn fr38_on_caret_rect_delivers_the_caret_place_and_rectangle() {
    let mut host = Host::new(caret_app);
    let batch = records(host.rebuild().unwrap());
    let editor = node_of(&batch, WidgetKind::CodeEditor);
    let rect = CodeRect::new(120.5, 64.25, 2.0, 20.0);
    send(
        &mut host,
        editor,
        handler_of(&batch, editor, PropertyKind::OnCaretRect),
        EventPayload::CaretRect {
            line: 2,
            column: 4,
            rect,
        },
    );
    CARETS.with_borrow(|seen| {
        assert_eq!(
            seen.as_slice(),
            &[CaretRect {
                position: Position::new(2, 4),
                rect,
            }]
        );
    });
}

thread_local! {
    static ANSWERS: RefCell<Vec<Option<CodeRect>>> = const { RefCell::new(Vec::new()) };
}

fn hover_card_app() -> Element {
    let handle = use_code_editor();
    let mut card = use_signal(|| None::<CodeRect>);
    rsx! {
        Column {
            CodeEditor { text: "let a = 1;\nlet b = 2;\n", handle }
            Button {
                text: "Where",
                on_click: move |_| {
                    handle.query_position_rect(Position::new(1, 4), move |answer| {
                        ANSWERS.with_borrow_mut(|seen| seen.push(answer));
                        card.set(answer);
                    });
                },
            }
            if let Some(rect) = card() {
                Text { text: format!("card at {},{}", rect.x, rect.bottom()) }
            }
        }
    }
}

/// A position asked of a handle goes out as one `QueryPositionRect` to the editor that
/// carries the handle, numbered by compose-rust. The answer reaches the callback that
/// asked, once, and what the callback changes is in the batch the answer's call returns.
/// A second answer to the same request is nobody's and changes nothing.
#[test]
fn fr38_a_position_asked_of_a_handle_is_answered_through_its_callback() {
    ANSWERS.with_borrow_mut(Vec::clear);
    let mut host = Host::new(hover_card_app);
    let batch = records(host.rebuild().unwrap());
    let editor = node_of(&batch, WidgetKind::CodeEditor);
    let after = button_named(&batch, &mut host, 0);
    let queries: Vec<&Record> = after
        .iter()
        .filter(|record| matches!(record, Record::QueryPositionRect { .. }))
        .collect();
    let [
        Record::QueryPositionRect {
            node_id,
            request,
            position,
        },
    ] = queries.as_slice()
    else {
        panic!("expected one position query, found {after:?}");
    };
    assert_eq!(*node_id, editor);
    assert_eq!(*position, Position::new(1, 4));
    assert_ne!(*request, 0);

    let rect = CodeRect::new(88.75, 64.25, 2.0, 20.0);
    let answered = send(
        &mut host,
        editor,
        0,
        EventPayload::PositionRect {
            request: *request,
            rect: Some(rect),
        },
    );
    ANSWERS.with_borrow(|seen| assert_eq!(seen.as_slice(), &[Some(rect)]));
    assert!(
        answered.iter().any(|record| matches!(
            record,
            Record::Prop(_, PropertyKind::Text, Value::Text(text)) if text == "card at 88.75,84.25"
        )),
        "the callback's change was not in the answer's batch: {answered:?}"
    );

    send(
        &mut host,
        editor,
        0,
        EventPayload::PositionRect {
            request: *request,
            rect: None,
        },
    );
    ANSWERS.with_borrow(|seen| assert_eq!(seen.as_slice(), &[Some(rect)]));
}

fn unattached_query_app() -> Element {
    let handle = CodeEditorHandle::new();
    rsx! {
        Column {
            Button {
                text: "Where",
                on_click: move |_| {
                    handle.query_position_rect(Position::new(0, 0), |_| {});
                    handle.set_scroll_offset(10.0);
                },
            }
        }
    }
}

/// A handle no editor carries asks nothing: neither a position query nor a scroll offset
/// is sent to a node the Renderer would have to report as unknown.
#[test]
fn fr38_a_position_or_scroll_asked_of_a_handle_no_editor_carries_is_dropped() {
    let mut host = Host::new(unattached_query_app);
    let batch = records(host.rebuild().unwrap());
    let after = button_named(&batch, &mut host, 0);
    assert!(
        !after.iter().any(|record| matches!(
            record,
            Record::QueryPositionRect { .. } | Record::SetScrollOffset { .. }
        )),
        "{after:?}"
    );
}

thread_local! {
    static PREVIEWED: RefCell<Vec<Key>> = const { RefCell::new(Vec::new()) };
}

fn suggesting_app() -> Element {
    rsx! {
        CodeEditor {
            text: "let a = 1;\n",
            // A suggestion list takes Down; every other key is the editor's.
            on_key_preview: move |event: KeyEvent| {
                PREVIEWED.with_borrow_mut(|seen| seen.push(event.key()));
                if event.key() == Key::ArrowDown {
                    event.consume();
                }
            },
        }
    }
}

fn key_down(key: Key, text: &'static str) -> EventPayload<'static> {
    EventPayload::KeyDown {
        key,
        shift_key: false,
        ctrl_key: false,
        alt_key: false,
        meta_key: false,
        repeat: false,
        code: 0,
        text,
    }
}

/// A key the editor offers reaches `on_key_preview` as the `KeyEvent` a key handler hears,
/// and the handler consuming it is the call's result, as `on_key_down`'s is: 1 keeps the key
/// from the editor, 0 leaves it the editor's.
#[test]
fn fr38_a_previewed_key_the_handler_consumes_is_returned_as_consumed() {
    PREVIEWED.with_borrow_mut(Vec::clear);
    let mut host = Host::new(suggesting_app);
    let batch = records(host.rebuild().unwrap());
    let editor = node_of(&batch, WidgetKind::CodeEditor);
    let handler = handler_of(&batch, editor, PropertyKind::OnKeyPreview);
    let (_, consumed) = send_for_result(
        &mut host,
        editor,
        handler,
        key_down(Key::ArrowDown, "ArrowDown"),
    );
    assert_eq!(consumed, 1);
    let (_, consumed) = send_for_result(&mut host, editor, handler, key_down(Key::Enter, "Enter"));
    assert_eq!(consumed, 0);
    PREVIEWED.with_borrow(|seen| assert_eq!(seen.as_slice(), &[Key::ArrowDown, Key::Enter]));
}

thread_local! {
    static POINTED: RefCell<Vec<(PointerKind, Option<Position>)>> =
        const { RefCell::new(Vec::new()) };
}

fn definition_app() -> Element {
    rsx! {
        CodeEditor {
            text: "let a = 1;\nlet total = a;\n",
            pointer_kinds: PointerKinds::DOWN | PointerKinds::MOVE,
            // Cmd or Ctrl and a press goes to a definition and keeps the caret where it was.
            on_code_pointer: move |pointer: CodePointer| {
                POINTED.with_borrow_mut(|seen| seen.push((pointer.event.kind(), pointer.position)));
                if pointer.event.kind() == PointerKind::Down && pointer.event.meta_key() {
                    pointer.event.consume();
                }
            },
        }
    }
}

fn code_pointer(
    kind: PointerKind,
    modifiers: u32,
    position: Option<Position>,
) -> EventPayload<'static> {
    EventPayload::CodePointer {
        kind,
        pointer_id: 1,
        pointer_type: PointerType::Mouse,
        x: 96.5,
        y: 30.25,
        buttons: 1,
        button: 1,
        modifiers,
        click_count: 1,
        delta_x: 0.0,
        delta_y: 0.0,
        delta_mode: WheelDeltaMode::Pixel,
        timestamp_nanos: 0,
        position,
    }
}

/// An editor that names pointer kinds sends them with its `OnCodePointer` handler, each
/// pointer event reaches `on_code_pointer` as compose-rust's `CodePointer` with the
/// character under it or none, and consuming the event is the call's result, as a pointer
/// handler's is. A plain pointer payload is refused at the code pointer handler.
#[test]
fn fr38_on_code_pointer_delivers_the_text_position_and_returns_consumption() {
    POINTED.with_borrow_mut(Vec::clear);
    let mut host = Host::new(definition_app);
    let batch = records(host.rebuild().unwrap());
    let editor = node_of(&batch, WidgetKind::CodeEditor);
    assert_eq!(
        prop_of(&batch, editor, PropertyKind::PointerKinds),
        Some(Value::Integer(i64::from(
            (PointerKinds::DOWN | PointerKinds::MOVE).bits()
        )))
    );
    assert_eq!(
        prop_of(&batch, editor, PropertyKind::OnPointer),
        None,
        "the editor's pointer events go to its own handler, not a node's"
    );
    let handler = handler_of(&batch, editor, PropertyKind::OnCodePointer);
    // Meta and a press over `total`.
    let (_, consumed) = send_for_result(
        &mut host,
        editor,
        handler,
        code_pointer(PointerKind::Down, 8, Some(Position::new(1, 6))),
    );
    assert_eq!(consumed, 1);
    let (_, consumed) = send_for_result(
        &mut host,
        editor,
        handler,
        code_pointer(PointerKind::Move, 0, None),
    );
    assert_eq!(consumed, 0);
    POINTED.with_borrow(|seen| {
        assert_eq!(
            seen.as_slice(),
            &[
                (PointerKind::Down, Some(Position::new(1, 6))),
                (PointerKind::Move, None),
            ]
        );
    });

    let mut wire = Vec::new();
    encode_event(
        &HostEvent {
            node_id: editor,
            handler_id: handler,
            payload: EventPayload::Pointer {
                kind: PointerKind::Down,
                pointer_id: 1,
                pointer_type: PointerType::Mouse,
                x: 0.0,
                y: 0.0,
                buttons: 1,
                button: 1,
                modifiers: 0,
                click_count: 1,
                delta_x: 0.0,
                delta_y: 0.0,
                delta_mode: WheelDeltaMode::Pixel,
                timestamp_nanos: 0,
            },
        },
        &mut wire,
    )
    .unwrap();
    assert!(host.dispatch_event(&wire).is_err());
}

fn diff_app() -> Element {
    rsx! {
        CodeEditor {
            text: "let a = 1;\nlet b = 2;\n",
            decorations: Decorations::new([
                Decoration::line_background(0, 1, 1, ColorRole::DiffAdded),
                Decoration::range_background(0, CodeRange::of(1, 4, 1, 5), ColorRole::DiffAdded),
            ]),
        }
    }
}

/// A diff view's line and range backgrounds are decorations, so they reach the Renderer
/// through the `decorations` list with their kind, lines, range and colour role intact.
#[test]
fn fr38_line_and_range_backgrounds_travel_in_the_decoration_list() {
    let mut host = Host::new(diff_app);
    let batch = records(host.rebuild().unwrap());
    let editor = node_of(&batch, WidgetKind::CodeEditor);
    let Some(Value::Bytes(bytes)) = prop_of(&batch, editor, PropertyKind::Decorations) else {
        panic!("the decorations did not travel as bytes");
    };
    assert_eq!(
        Decorations::from_bytes(bytes).decorations(),
        vec![
            Decoration::line_background(0, 1, 1, ColorRole::DiffAdded),
            Decoration::range_background(0, CodeRange::of(1, 4, 1, 5), ColorRole::DiffAdded),
        ]
    );
}

thread_local! {
    static GROUP_RESULTS: RefCell<Vec<EditGroupResult>> = const { RefCell::new(Vec::new()) };
    static ALL_SELECTIONS: RefCell<Vec<SelectionsChange>> = const { RefCell::new(Vec::new()) };
    static PRIMARY: RefCell<Vec<SelectionChange>> = const { RefCell::new(Vec::new()) };
}

fn grouping_app() -> Element {
    let handle = use_code_editor();
    rsx! {
        Column {
            CodeEditor {
                text: "let a = 1;\nlet b = 2;\n",
                handle,
                on_edit_group_result: move |result: EditGroupResult| GROUP_RESULTS.with_borrow_mut(|seen| seen.push(result)),
                on_selections_change: move |change: SelectionsChange| ALL_SELECTIONS.with_borrow_mut(|seen| seen.push(change)),
                on_selection_change: move |change: SelectionChange| PRIMARY.with_borrow_mut(|seen| seen.push(change)),
            }
            Button {
                text: "Rename",
                on_click: move |_| handle.edit_group(
                    8,
                    1,
                    [
                        (CodeRange::of(1, 4, 1, 5), "y"),
                        (CodeRange::of(0, 4, 0, 5), "x"),
                    ],
                ),
            }
            Button {
                text: "Nothing",
                on_click: move |_| handle.edit_group(9, 1, Vec::<(CodeRange, String)>::new()),
            }
            Button {
                text: "Carets",
                on_click: move |_| handle.set_selections(
                    2,
                    [
                        (Position::new(0, 4), Position::new(0, 5)),
                        (Position::new(1, 4), Position::new(1, 5)),
                    ],
                ),
            }
            Button {
                text: "Snippet",
                on_click: move |_| handle.insert_snippet(
                    3,
                    CodeRange::of(1, 0, 1, 0),
                    "for ${1:item} in ${2:list} {\n\t$0\n}",
                ),
            }
        }
    }
}

/// The grouped edits, selections and snippets the batch carried for `node`, in order.
fn editor_commands(batch: &[u8], node: u32) -> Vec<String> {
    decode_batch(batch)
        .unwrap()
        .into_iter()
        .filter_map(|mutation| match mutation {
            Mutation::EditCodeGroup {
                node_id,
                request_id,
                base_version,
                edits,
            } if node_id == node => Some(format!(
                "group {request_id} @{base_version} {:?}",
                dioxus_compose::code::decode_edit_group(edits).expect("the edits decode")
            )),
            Mutation::SetSelections {
                node_id,
                base_version,
                selections,
            } if node_id == node => Some(format!(
                "selections @{base_version} {:?}",
                dioxus_compose::code::decode_selections(selections).expect("the selections decode")
            )),
            Mutation::InsertSnippet {
                node_id,
                base_version,
                start_line,
                start_column,
                end_line,
                end_column,
                snippet,
            } if node_id == node => Some(format!(
                "snippet @{base_version} {start_line}:{start_column}-{end_line}:{end_column} {snippet:?}"
            )),
            _ => None,
        })
        .collect()
}

fn press(host: &mut Host, first: &[Record], nth: usize) -> Vec<u8> {
    let buttons: Vec<u32> = first
        .iter()
        .filter_map(|record| match record {
            Record::Create(node_id, WidgetKind::Button) => Some(*node_id),
            _ => None,
        })
        .collect();
    let button = buttons[nth];
    let mut wire = Vec::new();
    encode_event(
        &HostEvent {
            node_id: button,
            handler_id: handler_of(first, button, PropertyKind::OnClick),
            payload: EventPayload::Clicked,
        },
        &mut wire,
    )
    .unwrap();
    host.dispatch_event(&wire).unwrap().0.to_vec()
}

/// A grouped edit asked of a handle goes out as one `EditCodeGroup` to the editor that
/// carries the handle, every range in the version it names, in the order it was listed.
/// A group with no edits sends nothing at all.
#[test]
fn fr38_a_grouped_edit_asked_of_a_handle_goes_out_to_its_editor() {
    let mut host = Host::new(grouping_app);
    let first = records(host.rebuild().unwrap());
    let editor = node_of(&first, WidgetKind::CodeEditor);
    let batch = press(&mut host, &first, 0);
    assert_eq!(
        editor_commands(&batch, editor),
        vec![format!(
            "group 8 @1 {:?}",
            vec![
                (CodeRange::of(1, 4, 1, 5), "y"),
                (CodeRange::of(0, 4, 0, 5), "x"),
            ]
        )]
    );
    let batch = press(&mut host, &first, 1);
    assert_eq!(editor_commands(&batch, editor), Vec::<String>::new());
}

/// Every selection asked of a handle goes out as one `SetSelections`, the primary first,
/// and a snippet as one `InsertSnippet` with its range and its text as written.
#[test]
fn fr38_selections_and_a_snippet_asked_of_a_handle_go_out_to_its_editor() {
    let mut host = Host::new(grouping_app);
    let first = records(host.rebuild().unwrap());
    let editor = node_of(&first, WidgetKind::CodeEditor);
    let batch = press(&mut host, &first, 2);
    assert_eq!(
        editor_commands(&batch, editor),
        vec![format!(
            "selections @2 {:?}",
            vec![
                (Position::new(0, 4), Position::new(0, 5)),
                (Position::new(1, 4), Position::new(1, 5)),
            ]
        )]
    );
    let batch = press(&mut host, &first, 3);
    assert_eq!(
        editor_commands(&batch, editor),
        vec!["snippet @3 1:0-1:0 \"for ${1:item} in ${2:list} {\\n\\t$0\\n}\"".to_string()]
    );
}

/// What became of a grouped edit reaches `on_edit_group_result` as compose-rust's
/// `EditGroupResult`, through the handler announced for it.
#[test]
fn fr38_on_edit_group_result_delivers_what_became_of_the_group() {
    GROUP_RESULTS.with_borrow_mut(Vec::clear);
    let mut host = Host::new(grouping_app);
    let first = records(host.rebuild().unwrap());
    let editor = node_of(&first, WidgetKind::CodeEditor);
    let handler = handler_of(&first, editor, PropertyKind::OnEditGroupResult);
    send(
        &mut host,
        editor,
        handler,
        EventPayload::CodeEditGroupResult {
            request_id: 8,
            status: EditGroupStatus::Rejected,
            base_version: 1,
            current_version: 3,
            index: 1,
        },
    );
    GROUP_RESULTS.with_borrow(|seen| {
        assert_eq!(
            seen.as_slice(),
            &[EditGroupResult {
                request_id: 8,
                status: EditGroupStatus::Rejected,
                base_version: 1,
                current_version: 3,
                index: 1,
            }]
        )
    });
}

/// Every selection arrives through the editor's selection listener: `on_selections_change`
/// hears all of them, the primary first, and `on_selection_change` goes on hearing the
/// primary alone. The same report sent to any other handler of the editor is refused.
#[test]
fn fr38_every_selection_reaches_on_selections_change_through_the_selection_listener() {
    ALL_SELECTIONS.with_borrow_mut(Vec::clear);
    PRIMARY.with_borrow_mut(Vec::clear);
    let mut host = Host::new(grouping_app);
    let first = records(host.rebuild().unwrap());
    let editor = node_of(&first, WidgetKind::CodeEditor);
    let handler = handler_of(&first, editor, PropertyKind::OnSelectionChange);
    let mut bytes = Vec::new();
    let both = [
        (Position::new(0, 3), Position::new(0, 7)),
        (Position::new(1, 12), Position::new(1, 8)),
    ];
    dioxus_compose::code::encode_selections(&both, &mut bytes);
    send(
        &mut host,
        editor,
        handler,
        EventPayload::SelectionsChanged {
            version: 4,
            selections: &bytes,
        },
    );
    send(
        &mut host,
        editor,
        handler,
        EventPayload::SelectionChanged {
            version: 4,
            anchor_line: 0,
            anchor_column: 3,
            active_line: 0,
            active_column: 7,
        },
    );
    ALL_SELECTIONS.with_borrow(|seen| {
        assert_eq!(
            seen.as_slice(),
            &[SelectionsChange {
                version: 4,
                selections: both.to_vec(),
            }]
        )
    });
    PRIMARY.with_borrow(|seen| {
        assert_eq!(
            seen.as_slice(),
            &[SelectionChange {
                version: 4,
                anchor: Position::new(0, 3),
                active: Position::new(0, 7),
            }]
        )
    });

    let other = handler_of(&first, editor, PropertyKind::OnEditGroupResult);
    let mut wire = Vec::new();
    encode_event(
        &HostEvent {
            node_id: editor,
            handler_id: other,
            payload: EventPayload::SelectionsChanged {
                version: 4,
                selections: &bytes,
            },
        },
        &mut wire,
    )
    .unwrap();
    assert!(host.dispatch_event(&wire).is_err());
}

fn wrapped_app() -> Element {
    let mut shown = use_signal(|| true);
    rsx! {
        Column {
            CodeEditor {
                text: "a\nb\n",
                minimap: shown().then_some(MinimapSide::Right),
                view_zones: if shown() {
                    ViewZones::new([ViewZone::new(0, 1.5).with_text("2 lines removed")])
                } else {
                    ViewZones::default()
                },
                word_wrap: shown().then_some(WordWrap::Bounded(80)),
            }
            Button {
                text: "Plain",
                on_click: move |_| shown.set(false),
            }
        }
    }
}

/// An editor's minimap side, view zones and word wrap arrive as written: the side as its
/// tag, the zones as their records and text, and the wrap as one integer. Each one taken
/// away is withdrawn once, so the editor goes back to drawing none of them.
#[test]
fn fr38_minimap_view_zones_and_word_wrap_arrive_and_are_withdrawn() {
    let mut host = Host::new(wrapped_app);
    let first = records(host.rebuild().unwrap());
    let editor = node_of(&first, WidgetKind::CodeEditor);
    assert_eq!(
        prop_of(&first, editor, PropertyKind::Minimap),
        Some(Value::Integer(i64::from(u16::from(MinimapSide::Right))))
    );
    let Some(Value::Bytes(zones)) = prop_of(&first, editor, PropertyKind::ViewZones) else {
        panic!("the view zones did not travel as bytes: {first:?}");
    };
    assert_eq!(
        ViewZones::from_bytes(zones).zones(),
        vec![ViewZone::new(0, 1.5).with_text("2 lines removed")]
    );
    assert_eq!(
        prop_of(&first, editor, PropertyKind::WordWrap),
        Some(Value::Integer(WordWrap::Bounded(80).to_value()))
    );

    let after = button_named(&first, &mut host, 0);
    for property in [
        PropertyKind::Minimap,
        PropertyKind::ViewZones,
        PropertyKind::WordWrap,
    ] {
        assert_eq!(
            after
                .iter()
                .filter(|record| matches!(record, Record::Prop(_, sent, _) if *sent == property))
                .collect::<Vec<_>>(),
            vec![&Record::Prop(editor, property, Value::None)],
            "{property:?}"
        );
    }
}

fn unattached_advanced_app() -> Element {
    let handle = CodeEditorHandle::new();
    rsx! {
        Column {
            Button {
                text: "Trigger",
                on_click: move |_| {
                    handle.edit_group(1, 0, [(CodeRange::of(0, 0, 0, 0), "foo")]);
                    handle.set_selections(0, [(Position::new(0, 0), Position::new(0, 1))]);
                    handle.insert_snippet(0, CodeRange::of(0, 0, 0, 0), "bar");
                },
            }
        }
    }
}

/// A handle no editor carries drops grouped edits, selections and snippets before the boundary.
#[test]
fn fr38_grouped_edits_selections_and_snippets_for_unattached_handle_are_dropped() {
    let mut host = Host::new(unattached_advanced_app);
    let first = records(host.rebuild().unwrap());
    let after = press(&mut host, &first, 0);
    let mutations = decode_batch(&after).unwrap();
    assert!(
        !mutations.iter().any(|m| matches!(
            m,
            Mutation::EditCodeGroup { .. }
                | Mutation::SetSelections { .. }
                | Mutation::InsertSnippet { .. }
        )),
        "{mutations:?}"
    );
}

/// `escape_snippet_text` escapes special snippet characters so they are inserted literally.
#[test]
fn fr38_escape_snippet_text_escapes_literal_characters() {
    assert_eq!(escape_snippet_text("a$1}\\b"), "a\\$1\\}\\\\b");

    let complex = r"fn test() { let $x = 1; } \ ${choice}";
    let complex_escaped = escape_snippet_text(complex);
    assert_eq!(
        complex_escaped,
        "fn test() { let \\$x = 1; \\} \\\\ \\${choice\\}"
    );
}
