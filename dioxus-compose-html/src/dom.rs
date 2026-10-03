//! A Dioxus app over a blitz-dom document: mutations in, display lists out, pointers routed
//! back to handlers.

use std::any::Any;
use std::rc::Rc;
use std::sync::{Arc, Once};

use blitz_dom::{BaseDocument, DocumentConfig, NodeData};
use blitz_html::HtmlProvider;
use blitz_traits::net::{DummyNetProvider, Url};
use blitz_traits::shell::{ColorScheme, Viewport};
use dioxus_core::{Element, ElementId, Event, VirtualDom};
use dioxus_html::geometry::{ClientPoint, ElementPoint, PagePoint, ScreenPoint};
use dioxus_html::input_data::{MouseButton, MouseButtonSet};
use dioxus_html::point_interaction::{
    InteractionElementOffset, InteractionLocation, ModifiersInteraction, PointerInteraction,
};
use dioxus_html::{HasMouseData, HtmlEventConverter, Modifiers, MouseData, PlatformEventData};

use crate::NodeId;
use crate::build::build_display_list;
use crate::display_list::{DisplayList, DisplayListDiff};
use crate::image::{ImageLookup, ImageResolver, UNRESOLVED_BASE, apply_natural_sizes};
use crate::layout::{local, resolve_styles, run_layout};
use crate::measure::{ParleyMeasurer, TextMeasurer};
use crate::plan::{ColourResolver, Plan, plan_from_images};
use crate::writer::{DomWriter, WriterState, element_name};

/// How an [`HtmlDom`] is set up.
pub struct HtmlConfig {
    /// Author stylesheets, applied in order after the user-agent sheet, as if each were a
    /// `<style>` element in `<head>`.
    pub stylesheets: Vec<String>,
    /// Replaces blitz-dom's default user-agent stylesheet (an HTML default sheet much like
    /// a browser's, with `body { margin: 8px }`) when set.
    pub user_agent_stylesheet: Option<String>,
    /// Sizes text during layout. [`ParleyMeasurer`] when `None`.
    pub measurer: Option<Box<dyn TextMeasurer>>,
    /// Answers `prefers-color-scheme` media queries.
    pub color_scheme: ColorScheme,
    /// The URL relative `src` attributes and CSS `url()` values resolve against. When
    /// `None`, or when it is not a URL relative ones can be resolved against, they are
    /// recorded as written.
    pub base_url: Option<String>,
    /// What the application knows about the images the document names: their natural
    /// sizes during layout, and the assets that draw them when [`HtmlDom::plan`] builds a
    /// plan. Without one, an `<img>` has no natural size and no image draws anything.
    pub images: Option<Box<dyn ImageResolver>>,
}

impl Default for HtmlConfig {
    fn default() -> Self {
        Self {
            stylesheets: Vec::new(),
            user_agent_stylesheet: None,
            measurer: None,
            color_scheme: ColorScheme::Light,
            base_url: None,
            images: None,
        }
    }
}

/// A Dioxus app whose screen is HTML elements and CSS, laid out inside the Host.
///
/// The app's root renders into `<body>`. Nothing is executed from the document: a
/// `<script>` element is an inert element that the user-agent stylesheet hides. Nothing is
/// fetched either: the document's network provider drops every request blitz-dom makes for
/// an image, a stylesheet or a font.
pub struct HtmlDom {
    vdom: VirtualDom,
    doc: BaseDocument,
    state: WriterState,
    measurer: Box<dyn TextMeasurer>,
    images: Option<Box<dyn ImageResolver>>,
    /// The base URL the document was given: the application's, or [`UNRESOLVED_BASE`].
    base_url: String,
    color_scheme: ColorScheme,
    current: Option<DisplayList>,
    previous: Option<DisplayList>,
}

impl HtmlDom {
    /// Builds the app's first render into a fresh document.
    pub fn new(app: fn() -> Element) -> Self {
        Self::with_config(app, HtmlConfig::default())
    }

    pub fn with_config(app: fn() -> Element, config: HtmlConfig) -> Self {
        register_event_converter();
        // A base that relative URLs cannot be resolved against (`data:`, `about:blank`)
        // would make blitz-dom panic on the first relative `<img src>`.
        let base_url = config
            .base_url
            .filter(|base| Url::parse(base).is_ok_and(|url| !url.cannot_be_a_base()))
            .unwrap_or_else(|| UNRESOLVED_BASE.to_string());
        let mut doc = BaseDocument::new(DocumentConfig {
            ua_stylesheets: config.user_agent_stylesheet.map(|sheet| vec![sheet]),
            // `dangerous_inner_html` is parsed as HTML; scripts in it stay inert.
            html_parser_provider: Some(Arc::new(HtmlProvider)),
            viewport: Some(Viewport::new(0, 0, 1.0, config.color_scheme)),
            base_url: Some(base_url.clone()),
            // blitz-dom asks its provider for every `<img src>`, background image, linked
            // stylesheet and web font. This one drops each request: loading is the
            // application's business, through its image resolver.
            net_provider: Some(Arc::new(DummyNetProvider)),
            ..DocumentConfig::default()
        });

        let body = {
            let mut mutator = doc.mutate();
            let html = mutator.create_element(element_name("html", None), Vec::new());
            let head = mutator.create_element(element_name("head", None), Vec::new());
            let body = mutator.create_element(element_name("body", None), Vec::new());
            mutator.append_children(html, &[head, body]);
            for css in &config.stylesheets {
                let style = mutator.create_element(element_name("style", None), Vec::new());
                let text = mutator.create_text_node(css);
                mutator.append_children(style, &[text]);
                mutator.append_children(head, &[style]);
            }
            let document = doc_root_id();
            mutator.append_children(document, &[html]);
            body
        };

        let mut dom = Self {
            vdom: VirtualDom::new(app),
            doc,
            state: WriterState::new(body),
            measurer: config
                .measurer
                .unwrap_or_else(|| Box::new(ParleyMeasurer::new())),
            images: config.images,
            base_url,
            color_scheme: config.color_scheme,
            current: None,
            previous: None,
        };
        let mut writer = DomWriter::new(&mut dom.doc, &mut dom.state);
        dom.vdom.rebuild(&mut writer);
        drop(writer);
        dom
    }

    /// Runs pending work (handlers that wrote signals, scopes marked dirty) and applies
    /// the resulting mutations to the document.
    pub fn render(&mut self) {
        let mut writer = DomWriter::new(&mut self.doc, &mut self.state);
        self.vdom.render_immediate(&mut writer);
    }

    /// Lays the document out in a viewport of `width` by `height` CSS pixels at `scale`
    /// device pixels per CSS pixel, and returns what to draw.
    pub fn layout(&mut self, width: f32, height: f32, scale: f32) -> &DisplayList {
        self.set_viewport(width, height, scale);
        let resolver = self
            .images
            .as_deref_mut()
            .map(|images| images as &mut dyn ImageResolver);
        let mut lookup = ImageLookup::new(resolver, Some(&self.base_url));
        resolve_styles(&mut self.doc);
        apply_natural_sizes(&mut self.doc, &mut lookup);
        let texts = run_layout(&mut self.doc, &mut *self.measurer);
        let list = build_display_list(&self.doc, &texts, &mut lookup);
        self.previous = self.current.replace(list);
        self.current
            .as_ref()
            .expect("the display list was stored on the line above")
    }

    /// Lays the document out and returns only what changed since the previous layout.
    /// The first call reports every entry as changed.
    pub fn layout_diff(&mut self, width: f32, height: f32, scale: f32) -> DisplayListDiff {
        self.layout(width, height, scale);
        let current = self
            .current
            .as_ref()
            .expect("layout stores the display list it returns");
        match &self.previous {
            Some(previous) => current.diff_from(previous),
            None => current.diff_from(&DisplayList::default()),
        }
    }

    /// The display list of the last [`HtmlDom::layout`].
    pub fn display_list(&self) -> Option<&DisplayList> {
        self.current.as_ref()
    }

    /// The plan for the last [`HtmlDom::layout`], with colours expressed through
    /// `colours` and images drawn with the assets the configured
    /// [`ImageResolver`] names. `None` before the first layout.
    pub fn plan(&mut self, colours: &mut dyn ColourResolver) -> Option<Plan> {
        let list = self.current.as_ref()?;
        let images = self
            .images
            .as_deref_mut()
            .map(|images| images as &mut dyn ImageResolver);
        Some(plan_from_images(list, colours, images))
    }

    /// Replaces the image resolver; the next layout and plan use it.
    pub fn set_image_resolver(&mut self, images: Option<Box<dyn ImageResolver>>) {
        self.images = images;
    }

    /// The topmost node at a point of the last layout, in document coordinates.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<NodeId> {
        self.current.as_ref()?.hit_test(x, y)
    }

    /// The node an `event` at `node` is delivered to: `node` itself or its nearest
    /// ancestor with a listener for that event, with the Dioxus element that owns the
    /// listener.
    pub fn listener_target(&self, node: NodeId, event: &str) -> Option<(NodeId, ElementId)> {
        let mut current = Some(node);
        while let Some(id) = current {
            if self.state.listeners(id).contains(&event) {
                if let Some(element) = self.state.element_of(id) {
                    return Some((id, element));
                }
            }
            current = self
                .doc
                .get_node(id)
                .and_then(|node| node.parent.or(node.layout_parent.get()));
        }
        None
    }

    /// The event names `node` has listeners for.
    pub fn listeners(&self, node: NodeId) -> &[&'static str] {
        self.state.listeners(node)
    }

    /// Delivers a primary-button click at a point of the last layout to the Dioxus
    /// handler of the element under it, bubbling the way Dioxus bubbles. Returns the node
    /// whose handler received it, or `None` when nothing under the point listens.
    ///
    /// Handlers run immediately; call [`HtmlDom::render`] to apply what they changed.
    pub fn click(&mut self, x: f32, y: f32) -> Option<NodeId> {
        let hit = self.hit_test(x, y)?;
        let (target, element) = self.listener_target(hit, "click")?;
        let origin = self
            .current
            .as_ref()
            .and_then(|list| list.get(target))
            .map_or((0.0, 0.0), |entry| (entry.rect.x, entry.rect.y));
        let data = PointerData {
            client: (x as f64, y as f64),
            element: ((x - origin.0) as f64, (y - origin.1) as f64),
        };
        let event: Event<dyn Any> =
            Event::new(Rc::new(PlatformEventData::new(Box::new(data))), true).into_any();
        self.vdom.runtime().handle_event("click", event, element);
        Some(target)
    }

    /// The first element whose `id` attribute is `id`.
    pub fn element_by_id(&self, id: &str) -> Option<NodeId> {
        element_by_id(&self.doc, id)
    }

    pub fn document(&self) -> &BaseDocument {
        &self.doc
    }

    pub fn virtual_dom(&self) -> &VirtualDom {
        &self.vdom
    }

    pub fn virtual_dom_mut(&mut self) -> &mut VirtualDom {
        &mut self.vdom
    }

    /// Replaces the text measurer; the next layout uses it.
    pub fn set_measurer(&mut self, measurer: Box<dyn TextMeasurer>) {
        self.measurer = measurer;
    }

    fn set_viewport(&mut self, width: f32, height: f32, scale: f32) {
        let physical = (
            (width * scale).round().max(0.0) as u32,
            (height * scale).round().max(0.0) as u32,
        );
        let current = self.doc.viewport();
        if current.window_size != physical
            || current.hidpi_scale != scale
            || !same_scheme(current.color_scheme, self.color_scheme)
        {
            self.doc.set_viewport(Viewport::new(
                physical.0,
                physical.1,
                scale,
                self.color_scheme,
            ));
        }
    }
}

/// `ColorScheme` does not implement `PartialEq`.
fn same_scheme(a: ColorScheme, b: ColorScheme) -> bool {
    matches!(
        (a, b),
        (ColorScheme::Light, ColorScheme::Light) | (ColorScheme::Dark, ColorScheme::Dark)
    )
}

/// The document node's id: the first node a `BaseDocument` creates.
fn doc_root_id() -> NodeId {
    0
}

/// The first element in the document tree whose `id` attribute is `id`.
///
/// Only elements connected to the document count. Each Dioxus template is built once as a
/// detached prototype and cloned for every use, and the prototype carries the same
/// attributes as its clones, so a search of every node would find the prototype, which is
/// never laid out.
pub fn element_by_id(doc: &BaseDocument, id: &str) -> Option<NodeId> {
    let name = local("id");
    doc.tree()
        .iter()
        .filter(|(_, node)| matches!(node.data, NodeData::Element(_)))
        .filter(|(node_id, _)| is_connected(doc, *node_id))
        .find(|(_, node)| node.attr(name.clone()) == Some(id))
        .map(|(node_id, _)| node_id)
}

/// Whether `node` hangs from the document node through its parents.
fn is_connected(doc: &BaseDocument, mut node: NodeId) -> bool {
    loop {
        if node == doc_root_id() {
            return true;
        }
        match doc.get_node(node).and_then(|n| n.parent) {
            Some(parent) => node = parent,
            None => return false,
        }
    }
}

/// The pointer data a click carries to its handler.
#[derive(Clone)]
struct PointerData {
    client: (f64, f64),
    element: (f64, f64),
}

impl InteractionLocation for PointerData {
    fn client_coordinates(&self) -> ClientPoint {
        ClientPoint::new(self.client.0, self.client.1)
    }

    fn screen_coordinates(&self) -> ScreenPoint {
        ScreenPoint::new(self.client.0, self.client.1)
    }

    fn page_coordinates(&self) -> PagePoint {
        PagePoint::new(self.client.0, self.client.1)
    }
}

impl InteractionElementOffset for PointerData {
    fn element_coordinates(&self) -> ElementPoint {
        ElementPoint::new(self.element.0, self.element.1)
    }
}

impl ModifiersInteraction for PointerData {
    fn modifiers(&self) -> Modifiers {
        Modifiers::empty()
    }
}

impl PointerInteraction for PointerData {
    fn held_buttons(&self) -> MouseButtonSet {
        MouseButtonSet::default()
    }

    fn trigger_button(&self) -> Option<MouseButton> {
        Some(MouseButton::Primary)
    }
}

impl HasMouseData for PointerData {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Dioxus listeners receive a [`PlatformEventData`] and turn it into the event type the
/// handler takes through one process-wide converter. Registered on the first [`HtmlDom`];
/// registering it again would replace it with an identical one, so it is done once.
fn register_event_converter() {
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| dioxus_html::set_event_converter(Box::new(Converter)));
}

/// Turns the events this document sends into Dioxus event data. Clicks are the only kind
/// sent so far, so every other conversion is unreachable from here.
struct Converter;

fn not_sent(kind: &str) -> ! {
    panic!(
        "dioxus-compose-html does not send {kind} events yet, so it has no {kind} data to \
         convert; something else delivered one through the shared event converter"
    )
}

impl HtmlEventConverter for Converter {
    fn convert_mouse_data(&self, event: &PlatformEventData) -> MouseData {
        let pointer = event
            .downcast::<PointerData>()
            .expect("every mouse event this document sends carries PointerData")
            .clone();
        MouseData::new(pointer)
    }
    fn convert_animation_data(&self, _: &PlatformEventData) -> dioxus_html::AnimationData {
        not_sent("animation")
    }
    fn convert_cancel_data(&self, _: &PlatformEventData) -> dioxus_html::CancelData {
        not_sent("cancel")
    }
    fn convert_clipboard_data(&self, _: &PlatformEventData) -> dioxus_html::ClipboardData {
        not_sent("clipboard")
    }
    fn convert_composition_data(&self, _: &PlatformEventData) -> dioxus_html::CompositionData {
        not_sent("composition")
    }
    fn convert_drag_data(&self, _: &PlatformEventData) -> dioxus_html::DragData {
        not_sent("drag")
    }
    fn convert_focus_data(&self, _: &PlatformEventData) -> dioxus_html::FocusData {
        not_sent("focus")
    }
    fn convert_form_data(&self, _: &PlatformEventData) -> dioxus_html::FormData {
        not_sent("form")
    }
    fn convert_image_data(&self, _: &PlatformEventData) -> dioxus_html::ImageData {
        not_sent("image")
    }
    fn convert_keyboard_data(&self, _: &PlatformEventData) -> dioxus_html::KeyboardData {
        not_sent("keyboard")
    }
    fn convert_media_data(&self, _: &PlatformEventData) -> dioxus_html::MediaData {
        not_sent("media")
    }
    fn convert_mounted_data(&self, _: &PlatformEventData) -> dioxus_html::MountedData {
        not_sent("mounted")
    }
    fn convert_pointer_data(&self, _: &PlatformEventData) -> dioxus_html::PointerData {
        not_sent("pointer")
    }
    fn convert_resize_data(&self, _: &PlatformEventData) -> dioxus_html::ResizeData {
        not_sent("resize")
    }
    fn convert_scroll_data(&self, _: &PlatformEventData) -> dioxus_html::ScrollData {
        not_sent("scroll")
    }
    fn convert_selection_data(&self, _: &PlatformEventData) -> dioxus_html::SelectionData {
        not_sent("selection")
    }
    fn convert_toggle_data(&self, _: &PlatformEventData) -> dioxus_html::ToggleData {
        not_sent("toggle")
    }
    fn convert_touch_data(&self, _: &PlatformEventData) -> dioxus_html::TouchData {
        not_sent("touch")
    }
    fn convert_transition_data(&self, _: &PlatformEventData) -> dioxus_html::TransitionData {
        not_sent("transition")
    }
    fn convert_visible_data(&self, _: &PlatformEventData) -> dioxus_html::VisibleData {
        not_sent("visible")
    }
    fn convert_wheel_data(&self, _: &PlatformEventData) -> dioxus_html::WheelData {
        not_sent("wheel")
    }
}
