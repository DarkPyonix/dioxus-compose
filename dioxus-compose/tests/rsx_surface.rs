//! The public `rsx!` widget surface, end to end.
//!
//! One screen uses every widget `dioxus_compose::prelude` exports, with its main
//! properties and its event handlers, and builds through the public `Host` the way an
//! application's own tests do. The assertions are on the decoded batch: each widget is
//! created as the kind the schema names, its properties arrive with the values written in
//! rsx, and each handler written in rsx is the one an event reaches.
//!
//! The coverage checks walk the schema tables rather than a list kept here, so a widget,
//! property or modifier added to the schema without a way to write it in rsx turns this
//! red. A new widget then needs two edits in this file: a use in `surface()` and a row in
//! `expectations()`.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use dioxus_compose::Host;
use dioxus_compose::prelude::*;
use dioxus_compose::protocol::{HostEvent, Mutation, PropertyValue, decode_batch};
use dioxus_compose::schema::{
    EventPayload, MODIFIER_SCHEMA, Modifier, PROPERTY_SCHEMA, PointerKind, PointerType,
    PropertyKind, WIDGET_SCHEMA, WheelDeltaMode, WidgetKind,
};
use dioxus_compose::spans::{TextSpan, TextSpans};
use dioxus_compose::{FontRef, FontRefs, GenericFamily, OverflowWrap, WordBreak};

/// The font list of the CSS text in `surface()`.
fn css_font() -> FontRefs {
    FontRefs::new([
        FontRef::System("Menlo".to_owned()),
        FontRef::Generic(GenericFamily::Monospace),
    ])
}

/// Its runs: one names a font of its own.
fn css_spans() -> TextSpans {
    TextSpans::new_with_fonts([(
        TextSpan::new(0, 3),
        FontRefs::new([FontRef::Generic(GenericFamily::Serif)]),
    )])
}

thread_local! {
    /// What the handlers written in `surface()` were called with, in order.
    static FIRED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn fired(label: impl Into<String>) {
    FIRED.with(|log| log.borrow_mut().push(label.into()));
}

fn take_fired() -> Vec<String> {
    FIRED.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

const SPANS_SOURCE: &str = "every **widget** in `rsx`";

/// The two lists a code editor carries, built the same way in surface() and expectations().
fn code_lists() -> (SyntaxSpans, Decorations) {
    (
        SyntaxSpans::new([SyntaxSpan::role(
            0,
            CodeRange::of(0, 0, 0, 1),
            ColorRole::Primary,
        )]),
        Decorations::new([Decoration::underline(
            0,
            CodeRange::of(0, 4, 0, 5),
            Severity::Warning,
        )]),
    )
}

fn spans() -> (String, TextSpans) {
    TextSpans::from_markdown(SPANS_SOURCE)
}

fn drawing() -> DrawList {
    DrawList::builder()
        .line(Paint::Role(ColorRole::Primary), 0.0, 0.0, 10.0, 10.0, 1.0)
        .rect(Paint::Role(ColorRole::Outline), 2.0, 2.0, 6.0, 6.0, 0.0)
        .build()
}

fn paint_bits(paint: Paint) -> i64 {
    paint.to_bits() as i64
}

const PRIMARY: Paint = Paint::Role(ColorRole::Primary);
const ERROR: Paint = Paint::Role(ColorRole::Error);
const SECONDARY: Paint = Paint::Role(ColorRole::Secondary);
const TERTIARY: Paint = Paint::Role(ColorRole::Tertiary);
const ON_SURFACE: Paint = Paint::Role(ColorRole::OnSurface);
const OUTLINE: Paint = Paint::Role(ColorRole::Outline);
const SURFACE: Paint = Paint::Role(ColorRole::Surface);
const SCRIM: Paint = Paint::Role(ColorRole::OnBackground);

/// A 3D matrix that is not the identity, so a value that arrives is plainly this one.
const MATRIX_3D: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.002, 0.0, 0.0, 1.0, 0.0, 4.0, 8.0, 0.0, 1.0,
];

/// What the first Column says to assistive technology.
fn column_semantics() -> Semantics {
    Semantics::new(SemanticsRole::Group).with_state(SemanticsState::expanded(true))
}

/// The colours the code editor in `surface()` names for its own parts.
fn editor_colors() -> CodeEditorColors {
    CodeEditorColors::new()
        .container(ColorRole::Surface)
        .gutter(ColorRole::SurfaceVariant)
}

/// Every element the prelude exports, once, with its main properties and handlers.
fn surface() -> Element {
    let (text, spans) = spans();
    rsx! {
        window {
            title: "Inbox",
            size: (800.0, 600.0),
            min_size: (400.0, 300.0),
            resizable: true,
            chrome: Chrome::Modern,
            title_bar: TitleBar::Simple,
            icon: 9_u32,
            position: (120.0, 80.0),
            restore_key: "main",
            zoom_level: 2_i32,
            drag_from_pointer: DragFromPointer {
                pointer_id: 1,
                anchor_x: 24.0,
                anchor_y: 12.0,
            },
            theme: Theme::unified(DesignSystem::Material3),
            onclosing: move |_| fired("window.onclosing"),
            onfocuschange: move |focused: Event<bool>| {
                fired(format!("window.onfocuschange({})", *focused.data()))
            },
            onmoved: move |frame: Event<WindowFrame>| {
                let frame = frame.data();
                fired(format!(
                    "window.onmoved({},{},{},{})",
                    frame.x, frame.y, frame.width, frame.height
                ))
            },
            onwindowdrag: move |drag: Event<WindowDragEvent>| {
                fired(format!("window.onwindowdrag({:?})", drag.data().phase))
            },
            onunavailable: move |_| fired("window.onunavailable"),
            onclosed: move |_| fired("window.onclosed"),
            report_size: true,
            Scaffold {
                background: Paint::Role(ColorRole::Background),
                material: MaterialRole::Chrome,
                top_bar: rsx! {
                    TopAppBar { title: "Inbox", fill_max_width: true }
                },
                bottom_bar: rsx! {
                    Navigation {
                        selected_index: 1,
                        compact: CompactNavigation::Sidebar,
                        head: rsx! { Text { text: "head" } },
                        foot: rsx! { Text { text: "foot" } },
                        NavigationItem {
                            text: "Inbox",
                            icon: IconRole::Inbox,
                            color: SECONDARY,
                            enabled: true,
                            section: "Mail".to_string(),
                            on_click: move |_| fired("NavigationItem.on_click"),
                        }
                        NavigationItem { text: "Sent", icon: IconRole::Send }
                    }
                },
                floating_action: rsx! {
                    Button {
                        text: "Send",
                        icon: IconRole::Send,
                        enabled: true,
                        variant: ButtonVariant::Tonal,
                        button_kind: ButtonKind::ActionKey,
                        color: ERROR,
                        max_lines: 2,
                        overflow: TextOverflow::Clip,
                        on_click: move |_| fired("Button.on_click"),
                    }
                },
                Column {
                    observe_size: 42,
                    tear_off_key: "tab:inbox",
                    corner_curve: 0.6,
                    semantics: column_semantics(),
                    accessible_name: "Mail",
                    on_window_drag_over: move |over: WindowDragOver| {
                        fired(format!("Column.on_window_drag_over({:?},{})", over.kind, over.window))
                    },
                    motion: MotionRole::Quick,
                    material: MaterialRole::Thin,
                    weight: 1.0,
                    fill_max_width: true,
                    fill_max_height: true,
                    padding: 12.0,
                    background: Paint::Role(ColorRole::Surface),
                    corner_radius: 8.0,
                    border_width: 1.0,
                    border_color: OUTLINE,
                    elevation: 3.0,
                    arrangement: Arrangement::SpaceBetween,
                    spacing: 8.0,
                    space_role: SpaceRole::Md,
                    alignment: Alignment::Center,
                    cursor: Cursor::Pointer,
                    user_select: UserSelect::None,
                    pointer_kinds: PointerKinds::MOVE | PointerKinds::WHEEL,
                    on_pointer: move |event: PointerEvent| {
                        fired(format!("Column.on_pointer({:?})", event.kind()))
                    },
                    Row {
                        width: 320.0,
                        height: 48.0,
                        padding_role: SpaceRole::Sm,
                        shape_role: ShapeRole::Large,
                        arrangement: Arrangement::End,
                        spacing: 4.0,
                        space_role: SpaceRole::Xs,
                        alignment: Alignment::CenterStart,
                        Spacer { width: 16.0, height: 16.0 }
                        Separator {}
                        Divider { vertical: true }
                    }
                    dioxus_compose::Box {
                        alignment: Alignment::BottomEnd,
                        match_parent_size: true,
                        window_drag: true,
                        press_feedback: (0.95, 0.1),
                        Text {
                            window_drag: false,
                            text: text.clone(),
                            spans: spans.clone(),
                            type_role: TypeRole::Title,
                            font_size: 18.0,
                            font_weight: 600,
                            line_height: 24.0,
                            letter_spacing: 0.5,
                            color: PRIMARY,
                            text_align: TextAlign::Center,
                            max_lines: 2,
                            overflow: TextOverflow::Ellipsis,
                        }
                        // Text laid out by CSS: no rung of the ladder, and everything it says
                        // about itself instead.
                        Text {
                            text: "css\ttext",
                            spans: css_spans(),
                            type_role: TypeRole::None,
                            font: css_font(),
                            word_break: WordBreak::KeepAll,
                            overflow_wrap: OverflowWrap::BreakWord,
                            tab_size: 2,
                            absolute_size: true,
                            soft_wrap: false,
                        }
                    }
                    TextField {
                        placeholder: "Search",
                        enabled: true,
                        multiline: true,
                        type_role: TypeRole::Mono,
                        font: css_font(),
                        on_value_change: move |value: String| fired(format!("TextField.on_value_change({value})")),
                        on_submit: move |value: String| fired(format!("TextField.on_submit({value})")),
                        on_focus_lost: move |_| fired("TextField.on_focus_lost"),
                        on_key_down: move |event: KeyEvent| fired(format!("TextField.on_key_down({:?})", event.key())),
                        bare: true,
                        on_focus_gained: move |_| fired("TextField.on_focus_gained"),
                        on_key_up: move |event: KeyEvent| fired(format!(
                            "TextField.on_key_up({:?},{},{},{},{})",
                            event.key(),
                            event.shift_key(),
                            event.repeat(),
                            event.code(),
                            event.text()
                        )),
                    }
                    ScrollColumn {
                        fill_max_width: true,
                        on_scroll_changed: move |change: ScrollChange| fired(format!(
                            "ScrollColumn.on_scroll_changed({},{},{},{},{},{},{},{},{},{})",
                            change.item,
                            change.settled,
                            change.can_scroll_backward,
                            change.can_scroll_forward,
                            change.x_px,
                            change.y_px,
                            change.max_x_px,
                            change.max_y_px,
                            change.viewport_width_px,
                            change.viewport_height_px
                        )),
                        Image { asset_id: 7, width: 64.0, height: 64.0 }
                        Icon { asset_id: 8, color: TERTIARY }
                    }
                    SplitPane {
                        value: 320.0,
                        min: 200.0,
                        max: 400.0,
                        collapsible: true,
                        selected_index: 1,
                        label: "Sessions",
                        on_change: move |value: f32| fired(format!("SplitPane.on_change({value})")),
                        on_dismiss: move |_| fired("SplitPane.on_dismiss"),
                        Column {}
                        Column {}
                    }
                    AbsoluteBox {
                        required_size: (320.0, 180.0),
                        background: Paint::Role(ColorRole::Surface),
                        border: ([1.0, 2.0, 1.0, 2.0], [OUTLINE; 4]),
                        corners: [8.0, 0.0, 8.0, 0.0],
                        shadow: (0.0, 2.0, 6.0, 0.0, OUTLINE),
                        clip: true,
                        alpha: 0.5,
                        transform: (0.0, 1.0, -1.0, 0.0, 4.0, 0.0),
                        island_zoom: 1.5,
                        AbsoluteBox {
                            offset: (12.0, 24.0),
                            required_size: (100.0, 40.0),
                            transform_3d: (MATRIX_3D, true),
                            Text { text: "placed" }
                        }
                    }
                    CodeEditor {
                        text: "x = 1",
                        syntax_spans: code_lists().0,
                        decorations: code_lists().1,
                        folds: compose_rust::code::Folds::new([compose_rust::code::Fold::new(0, 0, 1)]),
                        gutter_decorations: compose_rust::code::GutterDecorations::new([compose_rust::code::GutterDecoration::dot(0, 0, 1)]),
                        tab_width: 4_u32,
                        line_numbers: false,
                        gutter_min_width: 40.0,
                        gutter_padding: 6.0,
                        text_inset: 8.0,
                        colors: editor_colors(),
                        on_code_metrics: move |height: f64| fired(format!("CodeEditor.on_code_metrics({height})")),
                        on_code_scrolled: move |offset: f64| fired(format!("CodeEditor.on_code_scrolled({offset})")),
                        on_change: move |_| fired("CodeEditor.on_change"),
                        on_edit_rejected: move |_| fired("CodeEditor.on_edit_rejected"),
                        on_hover: move |_| fired("CodeEditor.on_hover"),
                        on_save: move |_| fired("CodeEditor.on_save"),
                        on_selection_change: move |_| fired("CodeEditor.on_selection_change"),
                        on_visible_range_change: move |_| fired("CodeEditor.on_visible_range_change"),
                        on_editor_focus_change: move |_| fired("CodeEditor.on_editor_focus_change"),
                        on_fold_change: move |_| fired("CodeEditor.on_fold_change"),
                        on_composition_change: move |_| fired("CodeEditor.on_composition_change"),
                        on_decoration_click: move |id: u64| fired(format!("CodeEditor.on_decoration_click({id})")),
                    }
                    SelectionContainer {
                        Badge {
                            value: 3,
                            color: ColorRole::Primary,
                            Text { text: "Inbox" }
                        }
                    }
                    ScrollRow {
                        fill_max_width: true,
                        Chip {
                            text: "Unread",
                            icon: IconRole::History,
                            selected: true,
                            enabled: true,
                            on_click: move |_| fired("Chip.on_click"),
                        }
                        FloatingAction {
                            text: "New",
                            icon: IconRole::Add,
                            on_click: move |_| fired("FloatingAction.on_click"),
                        }
                    }
                    Card {
                        elevation: 2.0,
                        Checkbox {
                            checked: true,
                            enabled: true,
                            container_color: PRIMARY,
                            check_color: SURFACE,
                            size: 18.0,
                            on_change: move |value: bool| fired(format!("Checkbox.on_change({value})")),
                        }
                        RadioButton {
                            selected: true,
                            enabled: true,
                            on_change: move |value: bool| fired(format!("RadioButton.on_change({value})")),
                        }
                        Switch {
                            checked: true,
                            enabled: true,
                            track_color: SECONDARY,
                            thumb_color: SURFACE,
                            size: (52.0, 32.0),
                            on_change: move |value: bool| fired(format!("Switch.on_change({value})")),
                        }
                    }
                    Surface {
                        background: Paint::Role(ColorRole::SurfaceVariant),
                        on_click: move |()| fired("Surface.on_click"),
                        Slider {
                            value: 0.25,
                            min: -1.0,
                            max: 2.0,
                            steps: 4,
                            enabled: true,
                            color: PRIMARY,
                            track_color: OUTLINE,
                            thumb_color: TERTIARY,
                            active_track_color: SECONDARY,
                            on_change: move |value: f32| fired(format!("Slider.on_change({value})")),
                        }
                        ProgressIndicator { value: 0.75, determinate: true, circular: true }
                        LinearProgressIndicator { progress: 0.5 }
                    }
                    Tabs {
                        selected_index: 1,
                        color: ON_SURFACE,
                        Text { text: "first tab" }
                        Text { text: "second tab" }
                    }
                    Tooltip {
                        text: "More options",
                        Text { text: "hover me" }
                    }
                    Canvas { width: 10.0, height: 10.0, commands: drawing() }
                    DatePicker {
                        value: 19723,
                        min: 19000,
                        max: 20000,
                        enabled: true,
                        on_change: move |value: i64| fired(format!("DatePicker.on_change({value})")),
                    }
                    TimePicker {
                        value: 615,
                        min: 480,
                        max: 1080,
                        enabled: true,
                        on_change: move |value: u32| fired(format!("TimePicker.on_change({value})")),
                    }
                    Dropdown {
                        selected_index: 1,
                        enabled: true,
                        bare: true,
                        on_change: move |value: usize| fired(format!("Dropdown.on_change({value})")),
                        Text { text: "one" }
                        Text { text: "two" }
                        Text { text: "three" }
                    }
                    LazyColumn {
                        item_count: 3,
                        key_of: move |index: usize| format!("message-{index}"),
                        item: move |index: usize| rsx! { Text { text: "message {index}" } },
                    }
                    LazyRow {
                        item_count: 4,
                        item: move |index: usize| rsx! { Text { text: "chip {index}" } },
                    }
                    LazyGrid {
                        item_count: 6,
                        columns: 3,
                        min_column_width: 120.0,
                        item: move |index: usize| rsx! { Text { text: "tile {index}" } },
                    }
                    FileDropTarget {
                        alignment: Alignment::Center,
                        on_files_entered: move |_| fired("FileDropTarget.on_files_entered"),
                        on_files_dropped: move |drop: FileDrop| {
                            fired(format!("FileDropTarget.on_files_dropped({:?})", drop.paths()))
                        },
                        Text { text: "drop files here" }
                    }
                    Dialog {
                        open: true,
                        scrim: SCRIM,
                        on_dismiss: move |_| fired("Dialog.on_dismiss"),
                        Text { text: "dialog body" }
                    }
                    Menu {
                        expanded: true,
                        scrim: SCRIM,
                        on_dismiss: move |_| fired("Menu.on_dismiss"),
                        anchor: rsx! { Text { text: "menu anchor" } },
                        Text { text: "menu entry" }
                    }
                    Sheet {
                        open: true,
                        scrim: SCRIM,
                        on_dismiss: move |_| fired("Sheet.on_dismiss"),
                        Text { text: "sheet body" }
                    }
                    FlowRow {
                        horizontal_spacing: 6.0,
                        vertical_spacing: 10.0,
                        horizontal_alignment: HorizontalAlignment::Center,
                        vertical_alignment: VerticalAlignment::Bottom,
                        max_items_in_each_row: 3,
                        Text { text: "tag one" }
                        Text { text: "tag two" }
                    }
                    Popover {
                        open: true,
                        placement: PopoverPlacement::Top,
                        align: PopoverAlign::End,
                        offset: 6.0,
                        scrim: true,
                        on_dismiss: move |_| fired("Popover.on_dismiss"),
                        anchor: rsx! { Text { text: "popover anchor" } },
                        Text { text: "popover body" }
                    }
                }
            }
        }
    }
}

/// A property value as it arrived, owned so it outlives the batch it came in.
#[derive(Clone, Debug, PartialEq)]
enum Value {
    None,
    String(String),
    Bool(bool),
    Integer(i64),
    Float(f32),
    Bytes(Vec<u8>),
}

impl From<&PropertyValue<'_>> for Value {
    fn from(value: &PropertyValue<'_>) -> Self {
        match value {
            PropertyValue::None => Self::None,
            PropertyValue::String(text) => Self::String((*text).to_owned()),
            PropertyValue::Bool(flag) => Self::Bool(*flag),
            PropertyValue::Integer(number) => Self::Integer(*number),
            PropertyValue::Float(number) => Self::Float(*number),
            PropertyValue::Bytes(bytes) => Self::Bytes(bytes.to_vec()),
        }
    }
}

#[derive(Debug)]
struct Node {
    kind: WidgetKind,
    /// The latest value of each property, keyed by property.
    props: BTreeMap<u16, (PropertyKind, Value)>,
}

/// Everything the Host has told the Renderer so far, by node.
#[derive(Default)]
struct Wire {
    nodes: BTreeMap<u32, Node>,
    /// The name of every modifier variant that reached the wire.
    modifiers: BTreeSet<String>,
}

impl Wire {
    fn apply(&mut self, batch: &[u8]) {
        let mutations = decode_batch(batch).expect("the Host produced a batch it cannot decode");
        for mutation in &mutations {
            match mutation {
                Mutation::Create { node_id, widget } => {
                    self.nodes.insert(
                        *node_id,
                        Node {
                            kind: *widget,
                            props: BTreeMap::new(),
                        },
                    );
                }
                Mutation::SetProp {
                    node_id,
                    property,
                    value,
                } => {
                    let node = self.nodes.get_mut(node_id).unwrap_or_else(|| {
                        panic!("a property {property:?} was set on node {node_id}, which was never created")
                    });
                    node.props
                        .insert(*property as u16, (*property, Value::from(value)));
                }
                Mutation::SetModifier { modifier, .. } => {
                    let debug = format!("{modifier:?}");
                    let name = debug
                        .split(|c: char| !c.is_alphanumeric())
                        .next()
                        .unwrap_or_default()
                        .to_owned();
                    self.modifiers.insert(name);
                }
                _ => {}
            }
        }
    }

    fn of_kind(&self, kind: WidgetKind) -> impl Iterator<Item = (u32, &Node)> {
        self.nodes
            .iter()
            .filter(move |(_, node)| node.kind == kind)
            .map(|(id, node)| (*id, node))
    }

    /// The node of `kind` that carries a handler under `property`, and the handler's id.
    fn handler(&self, kind: WidgetKind, property: PropertyKind) -> (u32, u64) {
        self.of_kind(kind)
            .find_map(|(id, node)| match node.props.get(&(property as u16)) {
                Some((_, Value::Integer(handler))) if *handler > 0 => Some((id, *handler as u64)),
                _ => None,
            })
            .unwrap_or_else(|| {
                panic!(
                    "no {kind:?} node carries a handler under {property:?}, so an event the \
                     Renderer reports for it has nowhere to go; the {kind:?} nodes carry {:?}",
                    self.of_kind(kind)
                        .map(|(_, node)| node.props.values().map(|(p, _)| *p).collect::<Vec<_>>())
                        .collect::<Vec<_>>()
                )
            })
    }
}

/// Builds the surface and asks each windowed list for its first items, so the item
/// wrappers and their keys are on the wire as well.
fn build() -> (Host, Wire) {
    let mut host = Host::new(surface);
    let mut wire = Wire::default();
    wire.apply(host.rebuild().expect("the first frame should build"));
    for kind in [
        WidgetKind::LazyColumn,
        WidgetKind::LazyRow,
        WidgetKind::LazyGrid,
    ] {
        let (node_id, handler_id) = wire.handler(kind, PropertyKind::OnRangeRequested);
        let batch = host
            .dispatch(HostEvent {
                node_id,
                handler_id,
                payload: EventPayload::RangeRequested { start: 0, count: 2 },
            })
            .unwrap_or_else(|error| panic!("asking {kind:?} for its first items failed: {error:?}"))
            .0
            .to_vec();
        wire.apply(&batch);
    }
    (host, wire)
}

enum Expect {
    Is(Value),
    /// A handler id: what crosses for a handler is a number the Host chose, and only
    /// that it is there means anything.
    Handler,
}

use Expect::Handler;

fn is(value: Value) -> Expect {
    Expect::Is(value)
}

fn int(value: i64) -> Expect {
    is(Value::Integer(value))
}

fn float(value: f32) -> Expect {
    is(Value::Float(value))
}

fn flag(value: bool) -> Expect {
    is(Value::Bool(value))
}

fn text(value: &str) -> Expect {
    is(Value::String(value.to_owned()))
}

fn role(value: impl Into<u16>) -> Expect {
    int(i64::from(value.into()))
}

fn pair_bytes(a: f32, b: f32) -> Expect {
    let mut out = Vec::with_capacity(8);
    out.extend_from_slice(&a.to_le_bytes());
    out.extend_from_slice(&b.to_le_bytes());
    is(Value::Bytes(out))
}

fn drag_bytes(pointer_id: u32, anchor_x: f32, anchor_y: f32) -> Expect {
    let mut out = Vec::with_capacity(12);
    out.extend_from_slice(&pointer_id.to_le_bytes());
    out.extend_from_slice(&anchor_x.to_le_bytes());
    out.extend_from_slice(&anchor_y.to_le_bytes());
    is(Value::Bytes(out))
}

/// What each widget in `surface()` should carry on the wire. A kind can appear more than
/// once when the surface uses it in more than one way.
fn expectations() -> Vec<(WidgetKind, Vec<(PropertyKind, Expect)>)> {
    use PropertyKind as P;
    use WidgetKind as W;
    let (spans_text, spans) = spans();
    vec![
        (
            W::Window,
            vec![
                (P::WindowTitle, text("Inbox")),
                (P::WindowSize, pair_bytes(800.0, 600.0)),
                (P::WindowMinSize, pair_bytes(400.0, 300.0)),
                (P::WindowResizable, flag(true)),
                (P::WindowChrome, role(Chrome::Modern)),
                (P::WindowTitleBar, role(TitleBar::Simple)),
                (P::WindowIcon, int(9)),
                (P::WindowPosition, pair_bytes(120.0, 80.0)),
                (P::WindowRestoreKey, text("main")),
                (P::WindowZoomLevel, int(2)),
                (P::DragFromPointer, drag_bytes(1, 24.0, 12.0)),
                (P::OnWindowCloseRequested, Handler),
                (P::OnWindowFocusChanged, Handler),
                (P::OnWindowMoved, Handler),
                (P::OnWindowDrag, Handler),
                (P::OnWindowUnavailable, Handler),
                (P::OnWindowClosed, Handler),
                (P::ReportSize, flag(true)),
            ],
        ),
        (
            W::Column,
            vec![
                (P::Arrangement, role(Arrangement::SpaceBetween)),
                (P::Spacing, float(8.0)),
                (P::SpaceRole, role(SpaceRole::Md)),
                (P::Alignment, role(Alignment::Center)),
                (P::Cursor, role(Cursor::Pointer)),
                (P::UserSelect, role(UserSelect::None)),
                (P::PointerKinds, int(1 | 64)),
                (P::OnPointer, Handler),
                (P::TearOffKey, text("tab:inbox")),
                (P::OnWindowDragOver, Handler),
                (P::AccessibleName, text("Mail")),
            ],
        ),
        (
            W::Row,
            vec![
                (P::Arrangement, role(Arrangement::End)),
                (P::Spacing, float(4.0)),
                (P::SpaceRole, role(SpaceRole::Xs)),
                (P::Alignment, role(Alignment::CenterStart)),
            ],
        ),
        (W::Box, vec![(P::Alignment, role(Alignment::BottomEnd))]),
        // The item wrappers a windowed list materialises, under the key the screen gave.
        (W::Box, vec![(P::ItemKey, text("message-0"))]),
        (W::Box, vec![(P::ItemKey, text("message-1"))]),
        (
            W::Text,
            vec![
                (P::Text, text(&spans_text)),
                (P::Spans, is(Value::Bytes(spans.as_bytes().to_vec()))),
                (P::TypeRole, role(TypeRole::Title)),
                (P::FontSize, float(18.0)),
                (P::FontWeight, int(600)),
                (P::LineHeight, float(24.0)),
                (P::LetterSpacing, float(0.5)),
                (P::Color, int(paint_bits(PRIMARY))),
                (P::TextAlign, role(TextAlign::Center)),
                (P::MaxLines, int(2)),
                (P::Overflow, role(TextOverflow::Ellipsis)),
            ],
        ),
        (
            W::Text,
            vec![
                (P::Text, text("css\ttext")),
                (P::TypeRole, int(0)),
                (P::Font, is(Value::Bytes(css_font().as_bytes().to_vec()))),
                (P::Spans, is(Value::Bytes(css_spans().as_bytes().to_vec()))),
                (
                    P::SpanFonts,
                    is(Value::Bytes(css_spans().font_table().to_vec())),
                ),
                (P::WordBreak, role(WordBreak::KeepAll)),
                (P::OverflowWrap, role(OverflowWrap::BreakWord)),
                (P::TabSize, int(2)),
                (P::AbsoluteSize, int(1)),
                (P::SoftWrap, flag(false)),
            ],
        ),
        (W::Text, vec![(P::Text, text("message 1"))]),
        (W::Text, vec![(P::Text, text("chip 0"))]),
        (W::Text, vec![(P::Text, text("tile 1"))]),
        (
            W::TextField,
            vec![
                (P::Placeholder, text("Search")),
                (P::Enabled, flag(true)),
                (P::Multiline, flag(true)),
                (P::TypeRole, role(TypeRole::Mono)),
                (P::Font, is(Value::Bytes(css_font().as_bytes().to_vec()))),
                (P::OnValueChange, Handler),
                (P::OnSubmit, Handler),
                (P::OnFocusLost, Handler),
                (P::OnKeyDown, Handler),
                (P::Bare, flag(true)),
                (P::OnFocusGained, Handler),
                (P::OnKeyUp, Handler),
            ],
        ),
        (
            W::Button,
            vec![
                (P::Text, text("Send")),
                (P::Icon, role(IconRole::Send)),
                (P::Enabled, flag(true)),
                (P::Variant, role(ButtonVariant::Tonal)),
                (P::ButtonKind, role(ButtonKind::ActionKey)),
                (P::Color, int(paint_bits(ERROR))),
                (P::MaxLines, int(2)),
                (P::Overflow, role(TextOverflow::Clip)),
                (P::OnClick, Handler),
            ],
        ),
        (W::Spacer, vec![]),
        (
            W::LazyColumn,
            vec![(P::ItemCount, int(3)), (P::OnRangeRequested, Handler)],
        ),
        (W::ScrollColumn, vec![(P::OnScrollChanged, Handler)]),
        (W::AbsoluteBox, vec![(P::IslandZoom, float(1.5))]),
        (W::AbsoluteBox, vec![]),
        (W::ScrollRow, vec![]),
        (
            W::CodeEditor,
            vec![
                (P::Text, text("x = 1")),
                (
                    P::SyntaxSpans,
                    is(Value::Bytes(code_lists().0.as_bytes().to_vec())),
                ),
                (
                    P::Decorations,
                    is(Value::Bytes(code_lists().1.as_bytes().to_vec())),
                ),
                (P::TabWidth, int(4)),
                (P::LineNumbers, flag(false)),
                (P::GutterMinWidthDp, float(40.0)),
                (P::GutterPaddingDp, float(6.0)),
                (P::TextInsetDp, float(8.0)),
                (
                    P::CodeEditorColors,
                    is(Value::Bytes(editor_colors().to_bytes().to_vec())),
                ),
                (P::OnCodeMetrics, Handler),
                (P::OnCodeScrolled, Handler),
                (P::OnValueChange, Handler),
                (P::OnEditRejected, Handler),
                (P::OnHover, Handler),
                (P::OnSave, Handler),
                (P::OnDecorationClick, Handler),
            ],
        ),
        (
            W::SplitPane,
            vec![
                (P::Value, float(320.0)),
                (P::Min, float(200.0)),
                (P::Max, float(400.0)),
                (P::Collapsible, flag(true)),
                (P::SelectedIndex, int(1)),
                (P::Text, text("Sessions")),
                (P::OnValueChange, Handler),
                (P::OnDismiss, Handler),
            ],
        ),
        (W::SelectionContainer, vec![]),
        (
            W::Badge,
            vec![
                (P::Count, int(3)),
                (P::Color, int(paint_bits(Paint::Role(ColorRole::Primary)))),
            ],
        ),
        (
            W::Chip,
            vec![
                (P::Text, text("Unread")),
                (P::Icon, role(IconRole::History)),
                (P::Checked, flag(true)),
                (P::Enabled, flag(true)),
                (P::OnClick, Handler),
            ],
        ),
        (
            W::FloatingAction,
            vec![
                (P::Text, text("New")),
                (P::Icon, role(IconRole::Add)),
                (P::OnClick, Handler),
            ],
        ),
        (W::Image, vec![(P::Asset, int(7))]),
        (
            W::Icon,
            vec![(P::Asset, int(8)), (P::Color, int(paint_bits(TERTIARY)))],
        ),
        (
            W::Checkbox,
            vec![
                (P::Checked, flag(true)),
                (P::Enabled, flag(true)),
                (P::OnValueChange, Handler),
                (P::ContainerColor, int(paint_bits(PRIMARY))),
                (P::CheckColor, int(paint_bits(SURFACE))),
                (P::ControlSize, pair_bytes(18.0, 18.0)),
            ],
        ),
        (
            W::RadioButton,
            vec![
                (P::Checked, flag(true)),
                (P::Enabled, flag(true)),
                (P::OnValueChange, Handler),
            ],
        ),
        (
            W::Switch,
            vec![
                (P::Checked, flag(true)),
                (P::Enabled, flag(true)),
                (P::OnValueChange, Handler),
                (P::TrackColor, int(paint_bits(SECONDARY))),
                (P::ThumbColor, int(paint_bits(SURFACE))),
                (P::ControlSize, pair_bytes(52.0, 32.0)),
            ],
        ),
        (
            W::Slider,
            vec![
                (P::Value, float(0.25)),
                (P::Min, float(-1.0)),
                (P::Max, float(2.0)),
                (P::Steps, int(4)),
                (P::Enabled, flag(true)),
                (P::Color, int(paint_bits(PRIMARY))),
                (P::OnValueChange, Handler),
                (P::TrackColor, int(paint_bits(OUTLINE))),
                (P::ThumbColor, int(paint_bits(TERTIARY))),
                (P::ActiveTrackColor, int(paint_bits(SECONDARY))),
            ],
        ),
        (
            W::ProgressIndicator,
            vec![
                (P::Value, float(0.75)),
                (P::Determinate, flag(true)),
                (P::Circular, flag(true)),
            ],
        ),
        (W::Divider, vec![(P::Vertical, flag(true))]),
        (W::Card, vec![]),
        (W::Surface, vec![]),
        (
            W::Dialog,
            vec![
                (P::Open, flag(true)),
                (P::OnDismiss, Handler),
                (P::Scrim, int(paint_bits(SCRIM))),
            ],
        ),
        (
            W::Menu,
            vec![
                (P::Open, flag(true)),
                (P::OnDismiss, Handler),
                (P::Scrim, int(paint_bits(SCRIM))),
            ],
        ),
        (
            W::Tabs,
            vec![
                (P::SelectedIndex, int(1)),
                (P::Color, int(paint_bits(ON_SURFACE))),
            ],
        ),
        (W::TopAppBar, vec![(P::Text, text("Inbox"))]),
        (
            W::LazyRow,
            vec![(P::ItemCount, int(4)), (P::OnRangeRequested, Handler)],
        ),
        (W::Tooltip, vec![(P::Text, text("More options"))]),
        (
            W::Canvas,
            vec![(P::Commands, is(Value::Bytes(drawing().as_bytes().to_vec())))],
        ),
        (
            W::DatePicker,
            vec![
                (P::Value, int(19723)),
                (P::Min, int(19000)),
                (P::Max, int(20000)),
                (P::Enabled, flag(true)),
                (P::OnValueChange, Handler),
            ],
        ),
        (
            W::TimePicker,
            vec![
                (P::Value, int(615)),
                (P::Min, int(480)),
                (P::Max, int(1080)),
                (P::Enabled, flag(true)),
                (P::OnValueChange, Handler),
            ],
        ),
        (
            W::Dropdown,
            vec![
                (P::SelectedIndex, int(1)),
                (P::Enabled, flag(true)),
                (P::OnValueChange, Handler),
                (P::Bare, flag(true)),
            ],
        ),
        (
            W::Navigation,
            vec![
                (P::SelectedIndex, int(1)),
                (P::CompactNavigation, role(CompactNavigation::Sidebar)),
            ],
        ),
        (
            W::NavigationItem,
            vec![
                (P::Text, text("Inbox")),
                (P::Icon, role(IconRole::Inbox)),
                (P::Color, int(paint_bits(SECONDARY))),
                (P::Enabled, flag(true)),
                (P::Section, text("Mail")),
                (P::OnClick, Handler),
            ],
        ),
        (W::NavigationItem, vec![(P::Text, text("Sent"))]),
        (
            W::Sheet,
            vec![
                (P::Open, flag(true)),
                (P::OnDismiss, Handler),
                (P::Scrim, int(paint_bits(SCRIM))),
            ],
        ),
        (W::Scaffold, vec![]),
        // One slot per part of the frame the screen filled, and the two a navigation's
        // head and foot arrive in, which use the same numbers.
        (W::ScaffoldSlot, vec![(P::Slot, int(1))]),
        (W::ScaffoldSlot, vec![(P::Slot, int(2))]),
        (W::ScaffoldSlot, vec![(P::Slot, int(3))]),
        (W::ScaffoldSlot, vec![(P::Slot, int(4))]),
        (
            W::LazyGrid,
            vec![
                (P::ItemCount, int(6)),
                (P::Columns, int(3)),
                (P::MinColumnWidth, float(120.0)),
                (P::OnRangeRequested, Handler),
            ],
        ),
        (
            W::FileDropTarget,
            vec![
                (P::Alignment, role(Alignment::Center)),
                (P::OnFilesEntered, Handler),
                (P::OnFilesDropped, Handler),
            ],
        ),
        (W::LinearProgressIndicator, vec![(P::Progress, float(0.5))]),
        (
            W::FlowRow,
            vec![
                (P::HorizontalSpacing, float(6.0)),
                (P::VerticalSpacing, float(10.0)),
                (P::HorizontalAlignment, role(HorizontalAlignment::Center)),
                (P::VerticalAlignment, role(VerticalAlignment::Bottom)),
                (P::MaxItemsInEachRow, int(3)),
            ],
        ),
        (
            W::Popover,
            vec![
                (P::Open, flag(true)),
                (P::Placement, role(PopoverPlacement::Top)),
                (P::Align, role(PopoverAlign::End)),
                (P::OffsetDp, float(6.0)),
                (P::Scrim, flag(true)),
                (P::OnDismiss, Handler),
            ],
        ),
    ]
}

fn satisfies(node: &Node, expected: &[(PropertyKind, Expect)]) -> bool {
    expected.iter().all(
        |(property, expect)| match (node.props.get(&(*property as u16)), expect) {
            (Some((_, value)), Expect::Is(wanted)) => value == wanted,
            (Some((_, Value::Integer(handler))), Expect::Handler) => *handler > 0,
            _ => false,
        },
    )
}

#[test]
fn fr15_every_widget_in_the_schema_has_an_rsx_element() {
    let (_host, wire) = build();
    let created: BTreeSet<u16> = wire.nodes.values().map(|node| node.kind as u16).collect();
    let missing: Vec<String> = WIDGET_SCHEMA
        .iter()
        .filter(|widget| !created.contains(&widget.tag))
        .map(|widget| format!("{} (tag {})", widget.name, widget.tag))
        .collect();
    assert!(
        missing.is_empty(),
        "these widgets are in the schema, but nothing written with the rsx! elements \
         dioxus_compose::prelude exports created one: {missing:?}. Either the widget has no \
         rsx! element or component an application can write, or it has one and \
         surface() in tests/rsx_surface.rs does not use it yet; in the second case add it \
         there and add its properties to expectations()."
    );
}

#[test]
fn fr15_every_rsx_element_sends_its_properties() {
    let (_host, wire) = build();
    let mut failures = Vec::new();
    for (kind, expected) in expectations() {
        if !wire
            .of_kind(kind)
            .any(|(_, node)| satisfies(node, &expected))
        {
            let wanted: Vec<String> = expected
                .iter()
                .map(|(property, expect)| match expect {
                    Expect::Is(value) => format!("{property:?} = {value:?}"),
                    Expect::Handler => format!("{property:?} = <handler id>"),
                })
                .collect();
            let seen: Vec<Vec<String>> = wire
                .of_kind(kind)
                .map(|(_, node)| {
                    node.props
                        .values()
                        .map(|(property, value)| format!("{property:?} = {value:?}"))
                        .collect()
                })
                .collect();
            failures.push(format!(
                "no {kind:?} node carried all of {wanted:?}; the {kind:?} nodes carried {seen:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "properties written in rsx did not arrive on the wire as written:\n{}",
        failures.join("\n")
    );
}

#[test]
fn fr15_every_property_in_the_schema_is_reachable_from_rsx() {
    let (_host, wire) = build();
    let sent: BTreeSet<u16> = wire
        .nodes
        .values()
        .flat_map(|node| node.props.keys().copied())
        .collect();
    let missing: Vec<String> = PROPERTY_SCHEMA
        .iter()
        .filter(|property| !sent.contains(&property.tag))
        .map(|property| format!("{} (tag {})", property.name, property.tag))
        .collect();
    assert!(
        missing.is_empty(),
        "these properties are in the schema, but nothing written with the public rsx! \
         elements sent one: {missing:?}. Either no element exposes the property, or \
         surface() in tests/rsx_surface.rs does not set it yet."
    );
}

/// Modifiers the protocol carries that no rsx attribute writes today. They are named here
/// so the check below still catches a new modifier that arrives without an attribute,
/// and so a reader can see the gap rather than find it.
///
/// - `Size`: there is no `size` attribute; `width` and `height` are written separately.
/// - `PaddingEach`: there is no attribute for four different paddings.
const MODIFIERS_WITHOUT_AN_RSX_ATTRIBUTE: &[&str] = &["Size", "PaddingEach"];

#[test]
fn fr15_every_modifier_in_the_schema_is_reachable_from_rsx() {
    let (_host, wire) = build();
    for name in MODIFIERS_WITHOUT_AN_RSX_ATTRIBUTE {
        assert!(
            MODIFIER_SCHEMA
                .iter()
                .any(|modifier| modifier.name == *name),
            "{name} is listed as a modifier rsx cannot write, but the schema no longer has \
             it; take it off the list"
        );
    }
    let missing: Vec<&str> = MODIFIER_SCHEMA
        .iter()
        .map(|modifier| modifier.name)
        .filter(|name| !MODIFIERS_WITHOUT_AN_RSX_ATTRIBUTE.contains(name))
        .filter(|name| !wire.modifiers.contains(*name))
        .collect();
    assert!(
        missing.is_empty(),
        "these modifiers are in the schema, but no Modifier attribute written in rsx sent \
         one: {missing:?}. The modifiers that did arrive were {:?}.",
        wire.modifiers
    );
}

#[test]
fn fr15_every_event_handler_written_in_rsx_is_called() {
    use PropertyKind as P;
    use WidgetKind as W;
    let (mut host, wire) = build();
    take_fired();
    let events: &[(W, P, EventPayload<'static>, &str)] = &[
        (
            W::Button,
            P::OnClick,
            EventPayload::Clicked,
            "Button.on_click",
        ),
        (
            W::NavigationItem,
            P::OnClick,
            EventPayload::Clicked,
            "NavigationItem.on_click",
        ),
        (W::Chip, P::OnClick, EventPayload::Clicked, "Chip.on_click"),
        (
            W::FloatingAction,
            P::OnClick,
            EventPayload::Clicked,
            "FloatingAction.on_click",
        ),
        (
            W::Column,
            P::OnPointer,
            EventPayload::Pointer {
                kind: PointerKind::Move,
                pointer_id: 1,
                pointer_type: PointerType::Mouse,
                x: 3.0,
                y: 4.0,
                buttons: 0,
                button: 0,
                modifiers: 0,
                click_count: 0,
                delta_x: 0.0,
                delta_y: 0.0,
                delta_mode: WheelDeltaMode::Pixel,
                timestamp_nanos: 0,
            },
            "Column.on_pointer(Move)",
        ),
        (
            W::TextField,
            P::OnValueChange,
            EventPayload::TextChanged("한글"),
            "TextField.on_value_change(한글)",
        ),
        (
            W::TextField,
            P::OnSubmit,
            EventPayload::TextSubmitted("done"),
            "TextField.on_submit(done)",
        ),
        (
            W::TextField,
            P::OnFocusLost,
            EventPayload::FocusLost,
            "TextField.on_focus_lost",
        ),
        (
            W::TextField,
            P::OnKeyDown,
            EventPayload::KeyDown {
                key: Key::Enter,
                shift_key: false,
                ctrl_key: false,
                alt_key: false,
                meta_key: false,
                repeat: false,
                code: 0x28,
                text: "Enter",
            },
            "TextField.on_key_down(Enter)",
        ),
        (
            W::TextField,
            P::OnFocusGained,
            EventPayload::FocusGained,
            "TextField.on_focus_gained",
        ),
        (
            W::TextField,
            P::OnKeyUp,
            EventPayload::KeyUp {
                key: Key::Enter,
                shift_key: true,
                ctrl_key: false,
                alt_key: false,
                meta_key: false,
                repeat: false,
                code: 0x28,
                text: "Enter",
            },
            "TextField.on_key_up(Enter,true,false,40,Enter)",
        ),
        (
            W::ScrollColumn,
            P::OnScrollChanged,
            EventPayload::ScrollChanged {
                item: 0,
                settled: true,
                can_scroll_backward: true,
                can_scroll_forward: false,
                x_px: 0.0,
                y_px: 700.5,
                max_x_px: 0.0,
                max_y_px: 700.5,
                viewport_width_px: 320.0,
                viewport_height_px: 300.0,
            },
            "ScrollColumn.on_scroll_changed(0,true,true,false,0,700.5,0,700.5,320,300)",
        ),
        (
            W::CodeEditor,
            P::OnCodeMetrics,
            EventPayload::CodeMetrics {
                line_height_px: 18.5,
            },
            "CodeEditor.on_code_metrics(18.5)",
        ),
        (
            W::CodeEditor,
            P::OnCodeScrolled,
            EventPayload::CodeScrolled { offset_px: 240.0 },
            "CodeEditor.on_code_scrolled(240)",
        ),
        (
            W::Popover,
            P::OnDismiss,
            EventPayload::Clicked,
            "Popover.on_dismiss",
        ),
        (
            W::Checkbox,
            P::OnValueChange,
            EventPayload::ValueChanged(0.0),
            "Checkbox.on_change(false)",
        ),
        (
            W::RadioButton,
            P::OnValueChange,
            EventPayload::ValueChanged(0.0),
            "RadioButton.on_change(false)",
        ),
        (
            W::Switch,
            P::OnValueChange,
            EventPayload::ValueChanged(1.0),
            "Switch.on_change(true)",
        ),
        (
            W::Slider,
            P::OnValueChange,
            EventPayload::ValueChanged(0.5),
            "Slider.on_change(0.5)",
        ),
        (
            W::DatePicker,
            P::OnValueChange,
            EventPayload::ValueChanged(19800.0),
            "DatePicker.on_change(19800)",
        ),
        (
            W::TimePicker,
            P::OnValueChange,
            EventPayload::ValueChanged(700.0),
            "TimePicker.on_change(700)",
        ),
        (
            W::Dropdown,
            P::OnValueChange,
            EventPayload::ValueChanged(2.0),
            "Dropdown.on_change(2)",
        ),
        (
            W::SplitPane,
            P::OnValueChange,
            EventPayload::ValueChanged(280.0),
            "SplitPane.on_change(280)",
        ),
        (
            W::SplitPane,
            P::OnDismiss,
            EventPayload::Clicked,
            "SplitPane.on_dismiss",
        ),
        (
            W::Dialog,
            P::OnDismiss,
            EventPayload::Clicked,
            "Dialog.on_dismiss",
        ),
        (
            W::Menu,
            P::OnDismiss,
            EventPayload::Clicked,
            "Menu.on_dismiss",
        ),
        (
            W::Sheet,
            P::OnDismiss,
            EventPayload::Clicked,
            "Sheet.on_dismiss",
        ),
        (
            W::FileDropTarget,
            P::OnFilesEntered,
            EventPayload::FilesEntered,
            "FileDropTarget.on_files_entered",
        ),
        (
            W::FileDropTarget,
            P::OnFilesDropped,
            EventPayload::FilesDropped("/tmp/a.txt\0/tmp/b.txt"),
            "FileDropTarget.on_files_dropped([\"/tmp/a.txt\", \"/tmp/b.txt\"])",
        ),
        (
            W::Window,
            P::OnWindowCloseRequested,
            EventPayload::WindowCloseRequested,
            "window.onclosing",
        ),
        (
            W::Window,
            P::OnWindowFocusChanged,
            EventPayload::WindowFocusChanged { focused: true },
            "window.onfocuschange(true)",
        ),
        (
            W::Window,
            P::OnWindowMoved,
            EventPayload::WindowMoved {
                x: 10.0,
                y: 20.0,
                width: 800.0,
                height: 600.0,
            },
            "window.onmoved(10,20,800,600)",
        ),
        (
            W::Window,
            P::OnWindowDrag,
            EventPayload::WindowDrag {
                phase: 2,
                screen_x: 100.0,
                screen_y: 200.0,
                over_window: 0,
            },
            "window.onwindowdrag(End)",
        ),
        (
            W::Window,
            P::OnWindowUnavailable,
            EventPayload::WindowUnavailable,
            "window.onunavailable",
        ),
        (
            W::Window,
            P::OnWindowClosed,
            EventPayload::WindowClosed,
            "window.onclosed",
        ),
        (
            W::Column,
            P::OnWindowDragOver,
            EventPayload::WindowDragOver {
                kind: 4,
                window: 7,
                x: 12.0,
                y: 16.0,
            },
            "Column.on_window_drag_over(Drop,7)",
        ),
    ];
    let mut failures = Vec::new();
    for (kind, property, payload, label) in events.iter().cloned() {
        let (node_id, handler_id) = wire.handler(kind, property);
        if let Err(error) = host.dispatch(HostEvent {
            node_id,
            handler_id,
            payload,
        }) {
            failures.push(format!(
                "{kind:?} {property:?}: the Host refused the event: {error:?}"
            ));
            continue;
        }
        let calls = take_fired();
        if calls != [label] {
            failures.push(format!(
                "{kind:?} {property:?}: expected the handler written in rsx to be called \
                 once as {label:?}, but the calls were {calls:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The batch the first frame of `app` writes.
fn first_frame(app: fn() -> Element) -> (Host, Vec<u8>) {
    let mut host = Host::new(app);
    let batch = host
        .rebuild()
        .expect("the first frame should build")
        .to_vec();
    (host, batch)
}

fn decoded(batch: &[u8]) -> Vec<Mutation<'_>> {
    decode_batch(batch).expect("the Host produced a batch it cannot decode")
}

/// The handler id each `property` was announced with, by node.
fn announced(mutations: &[Mutation<'_>], property: PropertyKind) -> Vec<(u32, u64)> {
    mutations
        .iter()
        .filter_map(|mutation| match mutation {
            Mutation::SetProp {
                node_id,
                property: sent,
                value: PropertyValue::Integer(handler),
            } if *sent == property && *handler > 0 => Some((*node_id, *handler as u64)),
            _ => None,
        })
        .collect()
}

fn key_logger() -> Element {
    rsx! {
        TextField {
            on_key_down: move |event: KeyEvent| fired(format!(
                "{:?},{},{},{},{},{},{},{}",
                event.key(),
                event.shift_key(),
                event.ctrl_key(),
                event.alt_key(),
                event.meta_key(),
                event.repeat(),
                event.code(),
                event.text()
            )),
            on_key_up: move |event: KeyEvent| fired(format!(
                "{:?},{},{},{},{},{},{},{}",
                event.key(),
                event.shift_key(),
                event.ctrl_key(),
                event.alt_key(),
                event.meta_key(),
                event.repeat(),
                event.code(),
                event.text()
            )),
        }
    }
}

/// A key going up reaches `on_key_up` as the same `KeyEvent` a key going down brings to
/// `on_key_down`: the key, every modifier, the repeat flag, the physical code and the text.
#[test]
fn fr15_on_key_up_delivers_the_key_event_on_key_down_does() {
    let (mut host, batch) = first_frame(key_logger);
    let mutations = decoded(&batch);
    take_fired();
    let down = announced(&mutations, PropertyKind::OnKeyDown);
    let up = announced(&mutations, PropertyKind::OnKeyUp);
    assert_eq!((down.len(), up.len()), (1, 1), "{mutations:?}");
    let fields = |down: bool| {
        let (key, shift_key, ctrl_key, alt_key, meta_key) =
            (Key::ArrowDown, false, true, true, false);
        let (repeat, code, text) = (false, 0x51, "ArrowDown");
        if down {
            EventPayload::KeyDown {
                key,
                shift_key,
                ctrl_key,
                alt_key,
                meta_key,
                repeat,
                code,
                text,
            }
        } else {
            EventPayload::KeyUp {
                key,
                shift_key,
                ctrl_key,
                alt_key,
                meta_key,
                repeat,
                code,
                text,
            }
        }
    };
    for ((node_id, handler_id), payload) in [(down[0], fields(true)), (up[0], fields(false))] {
        host.dispatch(HostEvent {
            node_id,
            handler_id,
            payload,
        })
        .expect("the key event is delivered");
    }
    let calls = take_fired();
    assert_eq!(
        calls, ["ArrowDown,false,true,true,false,false,81,ArrowDown"; 2],
        "the key going down and the key going up should read the same"
    );
}

fn nothing_asked() -> Element {
    rsx! {
        Column {
            TextField { placeholder: "plain" }
            Dropdown { Text { text: "one" } }
            Switch { checked: true }
            Checkbox { checked: true }
            Slider { value: 0.5 }
            ScrollColumn { Text { text: "a" } }
            LazyColumn { item_count: 1, item: move |_| rsx! { Text { text: "b" } } }
            CodeEditor { text: "x" }
            Navigation { NavigationItem { text: "Inbox" } }
            Dialog { Text { text: "c" } }
            FlowRow { Text { text: "d" } }
            Popover { anchor: rsx! { Text { text: "e" } }, Text { text: "f" } }
        }
    }
}

/// A widget that sets none of the new properties and handlers sends none of them: absent
/// is each one's default, and a handler nobody wrote is never announced, so the Renderer
/// is never asked to report what no screen listens for.
#[test]
fn fr15_what_a_screen_leaves_out_is_never_sent() {
    use PropertyKind as P;
    let (_host, batch) = first_frame(nothing_asked);
    let mutations = decoded(&batch);
    let left_out = [
        P::OnFocusGained,
        P::OnKeyUp,
        P::AccessibleName,
        P::TrackColor,
        P::ThumbColor,
        P::ActiveTrackColor,
        P::ContainerColor,
        P::CheckColor,
        P::ControlSize,
        P::Scrim,
        P::Bare,
        P::HorizontalSpacing,
        P::VerticalSpacing,
        P::HorizontalAlignment,
        P::VerticalAlignment,
        P::MaxItemsInEachRow,
        P::Placement,
        P::Align,
        P::OffsetDp,
        P::LineNumbers,
        P::GutterMinWidthDp,
        P::GutterPaddingDp,
        P::TextInsetDp,
        P::CodeEditorColors,
        P::OnCodeMetrics,
        P::OnCodeScrolled,
        P::CompactNavigation,
        P::OnScrollChanged,
    ];
    let sent: Vec<String> = mutations
        .iter()
        .filter_map(|mutation| match mutation {
            Mutation::SetProp { property, .. } if left_out.contains(property) => {
                Some(format!("{mutation:?}"))
            }
            Mutation::SetModifier {
                modifier:
                    Modifier::Semantics { .. }
                    | Modifier::CornerCurve { .. }
                    | Modifier::Transform3D { .. },
                ..
            } => Some(format!("{mutation:?}")),
            _ => None,
        })
        .collect();
    assert!(sent.is_empty(), "nothing asked for these: {sent:?}");
}

fn tinted_popover() -> Element {
    rsx! {
        Popover {
            open: true,
            scrim_color: SCRIM,
            anchor: rsx! { Text { text: "anchor" } },
            Text { text: "body" }
        }
    }
}

/// A popover's scrim is `true` for the design system's dimming, as `surface()` shows, and
/// the paint itself when the screen names a colour.
#[test]
fn fr15_a_popover_scrim_colour_travels_as_a_paint() {
    let (_host, batch) = first_frame(tinted_popover);
    let mutations = decoded(&batch);
    let scrims: Vec<&PropertyValue<'_>> = mutations
        .iter()
        .filter_map(|mutation| match mutation {
            Mutation::SetProp {
                property: PropertyKind::Scrim,
                value,
                ..
            } => Some(value),
            _ => None,
        })
        .collect();
    assert_eq!(
        scrims,
        [&PropertyValue::Integer(paint_bits(SCRIM))],
        "{mutations:?}"
    );
}

fn described() -> Element {
    rsx! {
        Column {
            semantics: Semantics::new(SemanticsRole::Button).with_state(SemanticsState {
                checked: Some(true),
                selected: Some(false),
                hidden: true,
                click_action: true,
                ..SemanticsState::default()
            }),
            corner_radius: 12.0,
            corner_curve: 1.5,
            AbsoluteBox {
                transform_origin: (0.0, 1.0),
                transform_3d: (MATRIX_3D, false),
                Text { text: "x" }
            }
        }
    }
}

/// The three modifiers arrive with the values written: the role and every state field of
/// `Semantics` packed as the protocol defines them, a corner curve outside 0..=1 as the
/// nearer end, and a 3D matrix with the origin it turns about.
#[test]
fn fr15_semantics_corner_curve_and_transform_3d_arrive_as_written() {
    use dioxus_compose::schema::{
        SEMANTICS_CHECKED_SHIFT, SEMANTICS_CLICK_ACTION_BIT, SEMANTICS_HIDDEN_BIT,
        SEMANTICS_SELECTED_SHIFT,
    };
    let (_host, batch) = first_frame(described);
    let mutations = decoded(&batch);
    let modifiers: Vec<&Modifier> = mutations
        .iter()
        .filter_map(|mutation| match mutation {
            Mutation::SetModifier { modifier, .. } => Some(modifier),
            _ => None,
        })
        .collect();
    let state = (2 << SEMANTICS_CHECKED_SHIFT)
        | (1 << SEMANTICS_SELECTED_SHIFT)
        | SEMANTICS_HIDDEN_BIT
        | SEMANTICS_CLICK_ACTION_BIT;
    for wanted in [
        Modifier::Semantics {
            role: SemanticsRole::Button,
            state,
        },
        Modifier::CornerCurve { smoothing: 1.0 },
        Modifier::Transform3D {
            m: MATRIX_3D,
            origin_x: 0.0,
            origin_y: 1.0,
            backface_hidden: false,
        },
    ] {
        assert!(
            modifiers.contains(&&wanted),
            "{wanted:?} did not arrive; the modifiers were {modifiers:?}"
        );
    }
}

fn withdrawn() -> Element {
    let mut listening = use_signal(|| true);
    let on_key_up = listening()
        .then(|| dioxus_core::ListenerCallback::new(move |_: Event<KeyEvent>| fired("key up")));
    rsx! {
        Column {
            textfield { onkeyup: on_key_up }
            Button { text: "stop", on_click: move |_| listening.set(false) }
        }
    }
}

/// A handler that is taken away is withdrawn from the Renderer, so it stops reporting to a
/// listener that is gone, rather than reporting to a handler id nothing answers.
#[test]
fn fr15_a_handler_taken_away_is_withdrawn() {
    let (mut host, first) = first_frame(withdrawn);
    let mutations = decoded(&first);
    let up = announced(&mutations, PropertyKind::OnKeyUp);
    assert_eq!(up.len(), 1, "{mutations:?}");
    let (node_id, handler_id) = announced(&mutations, PropertyKind::OnClick)[0];
    let batch = host
        .dispatch(HostEvent {
            node_id,
            handler_id,
            payload: EventPayload::Clicked,
        })
        .expect("the press is dispatched")
        .0
        .to_vec();
    let after = decoded(&batch);
    assert!(
        after.contains(&Mutation::SetProp {
            node_id: up[0].0,
            property: PropertyKind::OnKeyUp,
            value: PropertyValue::None,
        }),
        "the key-up handler was not withdrawn: {after:?}"
    );
}
