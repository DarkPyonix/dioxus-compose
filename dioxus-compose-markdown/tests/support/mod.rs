//! What the tests see: the tree a Renderer would hold after applying every batch, written
//! out as text.
//!
//! The dump keeps what a reader of the screen would notice (which widget, which words,
//! which runs, which roles) and leaves out what only layout cares about, such as spacing
//! and padding, and anything that is a number the Host made up, such as node and handler
//! ids. Two trees that dump the same are the same document on screen.

#![allow(dead_code)]

use dioxus_compose::protocol::{Mutation, PropertyValue, decode_batch};
use dioxus_compose::schema::{Modifier, PropertyKind, WidgetKind};
use dioxus_compose::spans::TextSpans;
use dioxus_compose::{Host, Paint, TextAlign, TypeRole};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

#[derive(Clone, Debug)]
pub struct Node {
    pub widget: WidgetKind,
    pub text: Option<String>,
    pub spans: Vec<u8>,
    pub props: BTreeMap<u16, Prop>,
    pub modifiers: BTreeMap<u16, Modifier>,
    pub children: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Prop {
    Str(String),
    Int(i64),
    Float(f32),
    Bool(bool),
    Bytes(Vec<u8>),
}

#[derive(Default)]
pub struct Tree {
    pub nodes: BTreeMap<u32, Node>,
    parents: HashMap<u32, u32>,
}

impl Tree {
    pub fn apply(&mut self, mutations: &[Mutation<'_>]) {
        for mutation in mutations {
            self.apply_one(mutation);
        }
    }

    fn apply_one(&mut self, mutation: &Mutation<'_>) {
        match mutation {
            Mutation::Create { node_id, widget } => {
                self.nodes.insert(
                    *node_id,
                    Node {
                        widget: *widget,
                        text: None,
                        spans: Vec::new(),
                        props: BTreeMap::new(),
                        modifiers: BTreeMap::new(),
                        children: Vec::new(),
                    },
                );
            }
            Mutation::SetProp {
                node_id,
                property,
                value,
            } => {
                let Some(node) = self.nodes.get_mut(node_id) else {
                    return;
                };
                let tag = *property as u16;
                if *property == PropertyKind::Text {
                    node.text = match value {
                        PropertyValue::String(text) => Some((*text).to_owned()),
                        _ => None,
                    };
                    return;
                }
                if *property == PropertyKind::Spans {
                    node.spans = match value {
                        PropertyValue::Bytes(bytes) => bytes.to_vec(),
                        _ => Vec::new(),
                    };
                    return;
                }
                let prop = match value {
                    PropertyValue::None => None,
                    PropertyValue::String(text) => Some(Prop::Str((*text).to_owned())),
                    PropertyValue::Integer(value) => Some(Prop::Int(*value)),
                    PropertyValue::Float(value) => Some(Prop::Float(*value)),
                    PropertyValue::Bool(value) => Some(Prop::Bool(*value)),
                    PropertyValue::Bytes(bytes) => Some(Prop::Bytes(bytes.to_vec())),
                };
                match prop {
                    Some(prop) => {
                        node.props.insert(tag, prop);
                    }
                    None => {
                        node.props.remove(&tag);
                    }
                }
            }
            Mutation::SetModifier {
                node_id,
                index,
                modifier,
            } => {
                if let Some(node) = self.nodes.get_mut(node_id) {
                    if *modifier == Modifier::Empty {
                        node.modifiers.remove(index);
                    } else {
                        node.modifiers.insert(*index, modifier.clone());
                    }
                }
            }
            Mutation::Insert {
                parent_id,
                node_id,
                index,
            }
            | Mutation::Move {
                parent_id,
                node_id,
                index,
            } => {
                self.detach(*node_id);
                if let Some(parent) = self.nodes.get_mut(parent_id) {
                    let at = (*index as usize).min(parent.children.len());
                    parent.children.insert(at, *node_id);
                    self.parents.insert(*node_id, *parent_id);
                }
            }
            Mutation::Remove { node_id } => {
                self.detach(*node_id);
                self.forget(*node_id);
            }
            Mutation::SetText { node_id, text, .. } => {
                if let Some(node) = self.nodes.get_mut(node_id) {
                    node.text = Some((*text).to_owned());
                }
            }
            Mutation::AppendText { node_id, text } => {
                if let Some(node) = self.nodes.get_mut(node_id) {
                    node.text.get_or_insert_with(String::new).push_str(text);
                }
            }
            _ => {}
        }
    }

    fn detach(&mut self, node_id: u32) {
        if let Some(parent) = self.parents.remove(&node_id) {
            if let Some(parent) = self.nodes.get_mut(&parent) {
                parent.children.retain(|child| *child != node_id);
            }
        }
    }

    fn forget(&mut self, node_id: u32) {
        if let Some(node) = self.nodes.remove(&node_id) {
            for child in node.children {
                self.parents.remove(&child);
                self.forget(child);
            }
        }
    }

    pub fn roots(&self) -> Vec<u32> {
        self.nodes
            .keys()
            .filter(|id| !self.parents.contains_key(id))
            .copied()
            .collect()
    }

    pub fn parent_of(&self, node_id: u32) -> Option<u32> {
        self.parents.get(&node_id).copied()
    }

    /// Every node under `node_id`, itself included, in document order.
    pub fn subtree(&self, node_id: u32) -> Vec<u32> {
        let mut out = vec![node_id];
        if let Some(node) = self.nodes.get(&node_id) {
            for child in &node.children {
                out.extend(self.subtree(*child));
            }
        }
        out
    }

    /// The nodes of every root, in document order.
    pub fn all(&self) -> Vec<u32> {
        self.roots()
            .into_iter()
            .flat_map(|root| self.subtree(root))
            .collect()
    }

    pub fn find(&self, mut matches: impl FnMut(&Node) -> bool) -> Vec<u32> {
        self.all()
            .into_iter()
            .filter(|id| matches(&self.nodes[id]))
            .collect()
    }

    /// The node whose text is exactly `text`.
    pub fn text_node(&self, text: &str) -> Option<u32> {
        self.find(|node| node.text.as_deref() == Some(text))
            .into_iter()
            .next()
    }

    pub fn dump(&self) -> String {
        let mut out = String::new();
        for root in self.roots() {
            self.dump_node(root, 0, &mut out);
        }
        out
    }

    fn dump_node(&self, node_id: u32, depth: usize, out: &mut String) {
        let Some(node) = self.nodes.get(&node_id) else {
            return;
        };
        let indent = "  ".repeat(depth);
        let _ = write!(out, "{indent}{:?}", node.widget);
        let int = |kind: PropertyKind| match node.props.get(&(kind as u16)) {
            Some(Prop::Int(value)) => Some(*value),
            _ => None,
        };
        let flag = |kind: PropertyKind| node.props.get(&(kind as u16)) == Some(&Prop::Bool(true));
        if let Some(role) = int(PropertyKind::TypeRole) {
            let _ = write!(out, " role={}", type_role_name(role));
        }
        if let Some(bits) = int(PropertyKind::Color) {
            let _ = write!(out, " color={}", paint_name(bits as u64));
        }
        if let Some(align) = int(PropertyKind::TextAlign) {
            let name = u16::try_from(align)
                .ok()
                .and_then(|tag| TextAlign::try_from(tag).ok())
                .map_or_else(|| format!("?{align}"), |align| format!("{align:?}"));
            let _ = write!(out, " align={name}");
        }
        if flag(PropertyKind::Checked) {
            out.push_str(" checked");
        }
        if flag(PropertyKind::Vertical) {
            out.push_str(" vertical");
        }
        if node.props.get(&(PropertyKind::Enabled as u16)) == Some(&Prop::Bool(false)) {
            out.push_str(" disabled");
        }
        if let Some(asset) = int(PropertyKind::Asset) {
            let _ = write!(out, " asset={asset}");
        }
        if let Some(text) = &node.text {
            let _ = write!(out, " {:?}", text);
        }
        out.push('\n');
        for span in TextSpans::from_bytes(node.spans.clone()).spans() {
            let _ = write!(
                out,
                "{indent}  ~ {}..{}",
                span.start,
                span.start + span.length
            );
            if let Some(role) = span.type_role {
                let _ = write!(out, " role={role:?}");
            }
            if let Some(color) = span.color {
                let _ = write!(out, " color={}", paint_name(color.to_bits()));
            }
            for (set, name) in [
                (span.bold, "bold"),
                (span.italic, "italic"),
                (span.underline, "underline"),
                (span.strikethrough, "strikethrough"),
                (span.on_click.is_some(), "link"),
            ] {
                if set {
                    let _ = write!(out, " {name}");
                }
            }
            out.push('\n');
        }
        for child in &node.children {
            self.dump_node(*child, depth + 1, out);
        }
    }
}

fn type_role_name(value: i64) -> String {
    u16::try_from(value)
        .ok()
        .and_then(|tag| TypeRole::try_from(tag).ok())
        .map_or_else(|| format!("?{value}"), |role| format!("{role:?}"))
}

pub fn paint_name(bits: u64) -> String {
    match Paint::from_bits(bits) {
        Some(Paint::Role(role)) => format!("{role:?}"),
        Some(Paint::Literal(color)) => format!("LITERAL({:08x})", color.0),
        Some(Paint::Asset(id)) => format!("asset{id}"),
        None => format!("?{bits:x}"),
    }
}

/// Every place a colour can travel in a batch, and whether any of them is a literal.
pub fn literal_paints(mutations: &[Mutation<'_>]) -> Vec<String> {
    let mut found = Vec::new();
    for mutation in mutations {
        match mutation {
            Mutation::SetProp {
                property,
                value: PropertyValue::Integer(bits),
                node_id,
            } if *property == PropertyKind::Color => {
                if let Some(Paint::Literal(_)) = Paint::from_bits(*bits as u64) {
                    found.push(format!("node {node_id} color"));
                }
            }
            Mutation::SetProp {
                property,
                value: PropertyValue::Bytes(bytes),
                node_id,
            } if *property == PropertyKind::Spans => {
                for span in TextSpans::from_bytes(bytes.to_vec()).spans() {
                    if let Some(Paint::Literal(_)) = span.color {
                        found.push(format!("node {node_id} span {}", span.start));
                    }
                }
            }
            Mutation::SetModifier {
                node_id, modifier, ..
            } => match modifier {
                Modifier::Background(Paint::Literal(_))
                | Modifier::Border {
                    paint: Paint::Literal(_),
                    ..
                } => found.push(format!("node {node_id} modifier {modifier:?}")),
                _ => {}
            },
            _ => {}
        }
    }
    found
}

/// A Host and the tree it has built so far.
pub struct Screen {
    pub host: Host,
    pub tree: Tree,
}

impl Screen {
    pub fn new(app: fn() -> dioxus_compose::Element) -> Self {
        let mut host = Host::new(app);
        let mut tree = Tree::default();
        let batch = host.rebuild().expect("the first frame failed to encode");
        let mutations = decode_batch(batch).expect("the first frame did not decode");
        assert!(
            literal_paints(&mutations).is_empty(),
            "a literal colour travelled: {:?}",
            literal_paints(&mutations)
        );
        tree.apply(&mutations);
        Self { host, tree }
    }

    /// Takes one frame, applies it, and lets the caller look at what it carried.
    pub fn frame_with<R>(&mut self, look: impl FnOnce(&Tree, &[Mutation<'_>]) -> R) -> R {
        let batch = self.host.render_frame(0).expect("a frame failed to encode");
        let mutations = decode_batch(batch).expect("a frame did not decode");
        assert!(
            literal_paints(&mutations).is_empty(),
            "a literal colour travelled: {:?}",
            literal_paints(&mutations)
        );
        let before = Tree {
            nodes: self.tree.nodes.clone(),
            parents: self.tree.parents.clone(),
        };
        self.tree.apply(&mutations);
        look(&before, &mutations)
    }

    pub fn frame(&mut self) -> usize {
        self.frame_with(|_, mutations| mutations.len())
    }

    /// Frames until one carries something, or until `limit` passes. For work that lands
    /// from another thread.
    pub fn frame_until(
        &mut self,
        limit: std::time::Duration,
        mut done: impl FnMut(&Tree) -> bool,
    ) -> bool {
        let started = std::time::Instant::now();
        loop {
            self.frame();
            if done(&self.tree) {
                return true;
            }
            if started.elapsed() > limit {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    /// Presses a node's click handler, or one of its link runs.
    pub fn click(&mut self, node_id: u32, handler_id: u64) -> usize {
        use dioxus_compose::protocol::HostEvent;
        use dioxus_compose::schema::EventPayload;
        let (batch, _) = self
            .host
            .dispatch(HostEvent {
                node_id,
                handler_id,
                payload: EventPayload::Clicked,
            })
            .expect("the click failed");
        let mutations = decode_batch(batch).expect("the click's frame did not decode");
        self.tree.apply(&mutations);
        mutations.len()
    }

    /// The handler a Button carries.
    pub fn button_handler(&self, node_id: u32) -> u64 {
        match self.tree.nodes[&node_id]
            .props
            .get(&(PropertyKind::OnClick as u16))
        {
            Some(Prop::Int(handler)) => *handler as u64,
            other => panic!("node {node_id} has no click handler: {other:?}"),
        }
    }

    /// The Button whose label is `label`.
    pub fn button(&self, label: &str) -> u32 {
        self.tree
            .find(|node| node.widget == WidgetKind::Button && node.text.as_deref() == Some(label))
            .into_iter()
            .next()
            .unwrap_or_else(|| panic!("no button says {label:?}:\n{}", self.tree.dump()))
    }
}

/// The widget kinds a document may be made of, and nothing else.
pub const ALLOWED_WIDGETS: &[&str] = &[
    "Column",
    "Row",
    "Box",
    "Text",
    "Divider",
    "Checkbox",
    "Button",
    "ScrollRow",
    "SelectionContainer",
    "Image",
];

/// Reads a fixture from `tests/fixtures`.
pub fn fixture(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// The reference documents the streaming and colour tests run over.
pub fn corpus() -> Vec<String> {
    ["reference.md", "streaming.md"]
        .into_iter()
        .map(fixture)
        .collect()
}

/// Screens whose input the test sets before building them. `Host::new` takes a plain
/// function, so what the function draws is read from this thread's slots.
pub mod apps {
    use dioxus_compose::prelude::*;
    use dioxus_compose_markdown::{CopyRequest, Markdown, MarkdownOptions, MarkdownStream};
    use std::cell::RefCell;
    use std::collections::BTreeSet;

    thread_local! {
        pub static SOURCE: RefCell<String> = const { RefCell::new(String::new()) };
        pub static OPTIONS: RefCell<Option<MarkdownOptions>> = const { RefCell::new(None) };
        pub static STREAM: RefCell<Option<SyncSignal<MarkdownStream>>> = const { RefCell::new(None) };
        pub static LINKS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
        pub static COPIES: RefCell<Vec<CopyRequest>> = const { RefCell::new(Vec::new()) };
        pub static LINKED: RefCell<bool> = const { RefCell::new(true) };
    }

    pub fn options() -> MarkdownOptions {
        OPTIONS.with(|options| options.borrow().clone().unwrap_or_default())
    }

    /// Options with colouring off, for comparisons that must not depend on when the
    /// highlighter thread answers.
    pub fn quiet() -> MarkdownOptions {
        MarkdownOptions {
            highlight: false,
            ..MarkdownOptions::default()
        }
    }

    pub fn reset(source: &str, options: MarkdownOptions) {
        SOURCE.with(|slot| *slot.borrow_mut() = source.to_owned());
        OPTIONS.with(|slot| *slot.borrow_mut() = Some(options));
        STREAM.with(|slot| *slot.borrow_mut() = None);
        LINKS.with(|slot| slot.borrow_mut().clear());
        COPIES.with(|slot| slot.borrow_mut().clear());
        LINKED.with(|slot| *slot.borrow_mut() = true);
    }

    pub fn without_link_handler() {
        LINKED.with(|slot| *slot.borrow_mut() = false);
    }

    pub fn links() -> Vec<String> {
        LINKS.with(|slot| slot.borrow().clone())
    }

    pub fn copies() -> Vec<CopyRequest> {
        COPIES.with(|slot| slot.borrow().clone())
    }

    fn linked() -> bool {
        LINKED.with(|slot| *slot.borrow())
    }

    /// `Markdown { source }`, the finished-text way in.
    pub fn one_shot() -> Element {
        let source = SOURCE.with(|slot| slot.borrow().clone());
        let options = options();
        let expanded = use_signal(BTreeSet::new);
        if linked() {
            rsx! {
                Column {
                    Markdown {
                        source,
                        options,
                        expanded,
                        on_link: move |url: String| LINKS.with(|slot| slot.borrow_mut().push(url)),
                        on_copy: move |copy: CopyRequest| COPIES.with(|slot| slot.borrow_mut().push(copy)),
                    }
                }
            }
        } else {
            rsx! {
                Column {
                    Markdown {
                        source,
                        options,
                        expanded,
                        on_copy: move |copy: CopyRequest| COPIES.with(|slot| slot.borrow_mut().push(copy)),
                    }
                }
            }
        }
    }

    /// `Markdown { stream }`, the arriving-in-pieces way in.
    pub fn streamed() -> Element {
        let options = options();
        let stream = use_signal_sync({
            let options = options.clone();
            move || MarkdownStream::new(options)
        });
        STREAM.with(|slot| *slot.borrow_mut() = Some(stream));
        let expanded = use_signal(BTreeSet::new);
        rsx! {
            Column {
                Markdown {
                    stream,
                    options,
                    expanded,
                    on_link: move |url: String| LINKS.with(|slot| slot.borrow_mut().push(url)),
                    on_copy: move |copy: CopyRequest| COPIES.with(|slot| slot.borrow_mut().push(copy)),
                }
            }
        }
    }

    /// The stream the streamed screen is drawing.
    pub fn stream() -> SyncSignal<MarkdownStream> {
        STREAM.with(|slot| slot.borrow().expect("the streamed screen has not been built"))
    }

    pub fn push(delta: &str) {
        stream().write().push_delta(delta);
    }

    pub fn finish() {
        stream().write().finish();
    }
}

/// The tree `Markdown { source }` draws for `source`.
pub fn one_shot_dump(source: &str, options: dioxus_compose_markdown::MarkdownOptions) -> String {
    apps::reset(source, options);
    Screen::new(apps::one_shot).tree.dump()
}
