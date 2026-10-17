//! A Dioxus app over a blitz-dom document: mutations in, display lists out, pointers routed
//! back to handlers.
//!
//! The form controls' behaviour (what a click on a checkbox or a submit button does, and
//! the values a form event carries) is in [`form`], and the writer that applies Dioxus
//! mutations to the document in `writer`.

pub(crate) mod form;
mod writer;

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

use crate::html::NodeId;
use crate::layout::image::{ImageLookup, ImageResolver, UNRESOLVED_BASE, apply_natural_sizes};
use crate::layout::measure::{ParleyMeasurer, TextMeasurer};
use crate::layout::{local, resolve_styles, run_layout};
use crate::paint::build::build_display_list;
use crate::paint::display_list::{DisplayList, DisplayListDiff, SkippedTransform};
use crate::paint::motion::{
    MotionState, Playing, SETTLE_MARGIN, Scan, rewrite_reduced_motion, transitions,
};
use crate::paint::plan::{ColourResolver, Plan, plan_from_images};
use writer::{DomWriter, WriterState, element_name};

/// What a browser's own stylesheet says about `<select>` and blitz-dom's does not: the
/// control is a box of its own, and its options are not drawn in the page, since the
/// renderer's dropdown lists them.
pub(crate) const FORM_CONTROLS_CSS: &str =
    "select { display: inline-block; } select option, select optgroup { display: none; }";

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
    /// The user's `prefers-reduced-motion` setting: `true` for `reduce`. The author
    /// stylesheets are rewritten so that `@media (prefers-reduced-motion: reduce)` matches
    /// exactly when this is set and `(prefers-reduced-motion: no-preference)` when it is
    /// not, since the style engine has no such media feature of its own. Only
    /// [`HtmlConfig::stylesheets`] are rewritten.
    pub reduced_motion: bool,
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
            reduced_motion: false,
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
    /// The time CSS transitions and animations are resolved at, in seconds.
    now: f64,
    motion: MotionState,
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
        doc.add_user_agent_stylesheet(FORM_CONTROLS_CSS);

        let body = {
            let mut mutator = doc.mutate();
            let html = mutator.create_element(element_name("html", None), Vec::new());
            let head = mutator.create_element(element_name("head", None), Vec::new());
            let body = mutator.create_element(element_name("body", None), Vec::new());
            mutator.append_children(html, &[head, body]);
            for css in &config.stylesheets {
                let style = mutator.create_element(element_name("style", None), Vec::new());
                let css = rewrite_reduced_motion(css, config.reduced_motion);
                let text = mutator.create_text_node(&css);
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
            now: 0.0,
            motion: MotionState::default(),
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
    ///
    /// CSS transitions and animations are resolved at the document's clock
    /// ([`HtmlDom::set_time`]). A transition or animation starts at the clock of the layout
    /// that first sees it.
    ///
    /// How a motion reaches the renderer depends on what it changes. `opacity`,
    /// `transform` and `background-color` change nothing about layout, so a transition of
    /// them is a [`TransitionStart`](crate::html::TransitionStart) in the returned list
    /// and an `@keyframes` animation of them is on its entry's
    /// [`animations`](crate::html::NodeEntry::animations), both described for a renderer
    /// to play by itself, with the entry holding the value the motion settles at or
    /// starts from. Anything else that moves (`width`, `margin`, `color`, `all`) is ticked
    /// by the Host: see [`HtmlDom::tick`].
    pub fn layout(&mut self, width: f32, height: f32, scale: f32) -> &DisplayList {
        self.layout_at(width, height, scale, false)
    }

    fn layout_at(&mut self, width: f32, height: f32, scale: f32, ticked: bool) -> &DisplayList {
        self.set_viewport(width, height, scale);
        let now = self.now;
        let resolver = self
            .images
            .as_deref_mut()
            .map(|images| images as &mut dyn ImageResolver);
        let mut lookup = ImageLookup::new(resolver, Some(&self.base_url));
        resolve_styles(&mut self.doc, now);
        let scan = self.motion.scan(&self.doc, now);
        for _ in 0..scan.catch_up {
            resolve_styles(&mut self.doc, now);
        }
        let animating = self.doc.is_animating();
        // A page can declare a transition of `width` and leave it idle for as long as it
        // runs, and loops that a renderer plays beside it. The declaration alone does not
        // make the Host tick: a motion that changes layout has to be running, which is
        // true while the Host is already ticking, while a declared delay may be hiding it,
        // and while the boxes that declare one are seen to move.
        let layout_running = scan.layout_motion
            && animating
            && ((ticked && self.motion.ticking)
                || scan.layout_delayed
                || layout_moves(&mut self.doc, &scan, now));
        let ticking = self
            .motion
            .decide_ticking(&scan, layout_running, now, ticked);
        // A motion a renderer plays is described, not ticked. A transition is described
        // from the values before and after it, so when one is running the document is
        // resolved a second time, past the end of every transition, and the styles read
        // for the display list hold the values the transitions settle at.
        let described = !ticking && !scan.nodes.is_empty();
        if described && animating && scan.needs_settling() {
            resolve_styles(&mut self.doc, now + f64::from(scan.horizon + SETTLE_MARGIN));
        }
        apply_natural_sizes(&mut self.doc, &mut lookup);
        let texts = run_layout(&mut self.doc, &mut *self.measurer);
        let list = if described {
            self.motion.begin_frame();
            let mut playing = Playing {
                scan: &scan,
                state: &mut self.motion,
                prior: self.current.as_ref(),
                now,
                skipped: Vec::new(),
            };
            let mut list = build_display_list(&self.doc, &texts, &mut lookup, Some(&mut playing));
            list.skipped_transforms.append(&mut playing.skipped);
            if animating {
                list.transitions = transitions(&scan, self.current.as_ref(), &list);
            }
            self.motion.end_frame();
            list
        } else {
            build_display_list(&self.doc, &texts, &mut lookup, None)
        };
        self.report_skipped(&list);
        self.previous = self.current.replace(list);
        self.current
            .as_ref()
            .expect("the display list was stored on the line above")
    }

    /// Says on stderr, once for each, which element's which 3D transform the layout left
    /// out. The display list carries them all in
    /// [`skipped_transforms`](DisplayList::skipped_transforms).
    fn report_skipped(&mut self, list: &DisplayList) {
        for SkippedTransform {
            node,
            tag,
            transform,
        } in &list.skipped_transforms
        {
            if self.motion.warned.insert((*node, transform.clone())) {
                eprintln!(
                    "dioxus-compose: <{tag}> (node {node}) has the 3D transform `{transform}`. \
                     3D transforms are not drawn yet, so the element is drawn without it."
                );
            }
        }
    }

    /// Sets the clock CSS transitions and animations are resolved at, in seconds from any
    /// fixed moment. It only moves forward in a running app: a transition that has
    /// started is measured from the clock of the layout that saw it.
    pub fn set_time(&mut self, seconds: f64) {
        self.now = seconds;
    }

    /// The clock set by [`HtmlDom::set_time`].
    pub fn time(&self) -> f64 {
        self.now
    }

    /// Whether the document has a CSS transition or animation running.
    pub fn is_animating(&self) -> bool {
        self.doc.is_animating()
    }

    /// Whether the Host has to call [`HtmlDom::tick`] each frame: a motion that changes
    /// layout (or something a renderer does not play by itself) is running. When this is
    /// `false` the Host has nothing to do per frame, even while a renderer plays an
    /// `opacity`, `transform` or `background-color` motion.
    pub fn needs_frames(&self) -> bool {
        self.motion.ticking
    }

    /// One frame of a motion that changes layout: moves the clock to `seconds`, resolves
    /// the document there, lays it out again, and returns only what changed since the
    /// last layout. Does nothing, and returns an empty diff, when
    /// [`HtmlDom::needs_frames`] is `false`.
    ///
    /// The Host's own frame is the clock, so frame times are whatever the caller's
    /// frame loop reports; this crate never reads a clock of its own.
    pub fn tick(&mut self, seconds: f64, width: f32, height: f32, scale: f32) -> DisplayListDiff {
        if !self.motion.ticking {
            return DisplayListDiff::default();
        }
        self.now = seconds;
        self.layout_at(width, height, scale, true);
        self.diff_from_previous()
    }

    /// Lays the document out and returns only what changed since the previous layout.
    /// The first call reports every entry as changed.
    pub fn layout_diff(&mut self, width: f32, height: f32, scale: f32) -> DisplayListDiff {
        self.layout(width, height, scale);
        self.diff_from_previous()
    }

    fn diff_from_previous(&self) -> DisplayListDiff {
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
            if self.state.listeners(id).contains(&event)
                && let Some(element) = self.state.element_of(id)
            {
                return Some((id, element));
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
    /// handler of the element under it, bubbling the way Dioxus bubbles, and then does what
    /// a browser does after a click: a checkbox or radio button under the point changes
    /// state and sends `input` and `change`, a submit button submits its form, and a
    /// `<label>` clicks the control it is for. Returns the node whose click handler
    /// received it, or `None` when nothing under the point listens for clicks (the checkbox
    /// still changes, the form is still submitted).
    ///
    /// The checkbox or radio button changes before the click handlers run, so they see the
    /// new state, and changes back if one of them prevents the default.
    ///
    /// Handlers run immediately; call [`HtmlDom::render`] to apply what they changed.
    pub fn click(&mut self, x: f32, y: f32) -> Option<NodeId> {
        let hit = self.hit_test(x, y)?;
        self.activate(hit, Some((x, y)))
    }

    /// What a renderer calls when the user commits text to the field `node` (an
    /// `<input>` or `<textarea>`): delivers an `input` event carrying `value` to the
    /// nearest handler at or around the field. Returns the node whose handler received it.
    ///
    /// The renderer owns the field's text while the user edits it. Only committed text
    /// comes here, never text an input method is still composing. The field holds `value`
    /// from now on, for the form it belongs to and for later events, until the app sets
    /// its value again. Nothing is written into the document: the app decides whether its
    /// `value` follows.
    pub fn input(&mut self, node: NodeId, value: &str) -> Option<NodeId> {
        self.state.commit(node, value);
        self.send_form_event(node, "input")
    }

    /// What a renderer calls when the user is done changing a field: delivers `change`
    /// to the nearest handler at or around `node`. Returns the node whose handler
    /// received it.
    ///
    /// For an `<input>` or `<textarea>`, `value` is the committed text (see
    /// [`HtmlDom::input`]). For a `<select>`, it is the value of the option the user chose;
    /// as in a browser, `input` is delivered first, then `change`. For a checkbox or radio
    /// button use [`HtmlDom::check`] instead, which also delivers the click a browser
    /// sends.
    pub fn change(&mut self, node: NodeId, value: &str) -> Option<NodeId> {
        self.state.commit(node, value);
        let is_select = self.doc.get_node(node).and_then(form::tag) == Some("select");
        if is_select {
            let input = self.send_form_event(node, "input");
            return self.send_form_event(node, "change").or(input);
        }
        self.send_form_event(node, "change")
    }

    /// What a renderer calls when the user picks the option at `index` of the `<select>`
    /// `node`, counting every `<option>` in document order: [`HtmlDom::change`] with that
    /// option's value. `None`, and nothing delivered, when there is no such option.
    pub fn select(&mut self, node: NodeId, index: usize) -> Option<NodeId> {
        let value = form::select_options(&self.doc, node)
            .get(index)?
            .value
            .clone();
        self.change(node, &value)
    }

    /// What a renderer calls when the user ticks or clears the checkbox `node`, or picks
    /// the radio button `node` (`checked` true): the click a browser delivers for it, at
    /// its centre, then `input` and `change`. Nothing happens when the control already is
    /// in that state. Returns the node whose click handler received the click.
    pub fn check(&mut self, node: NodeId, checked: bool) -> Option<NodeId> {
        if form::checkedness(&self.doc, node) == checked {
            return None;
        }
        self.activate(node, None)
    }

    /// What a renderer calls when the user presses Enter in a single-line field: submits
    /// the form the field belongs to, as a browser does. When the form has a submit
    /// button, that button is clicked, so its click handler runs first; otherwise the form
    /// is submitted directly. Returns the node whose handler received the click or the
    /// `submit`, or `None` when `node` is in no form.
    ///
    /// `node` may also be the form itself.
    pub fn submit(&mut self, node: NodeId) -> Option<NodeId> {
        let is_form = self.doc.get_node(node).and_then(form::tag) == Some("form");
        let form = if is_form {
            node
        } else {
            form::form_owner(&self.doc, node)?
        };
        match form::default_button(&self.doc, form) {
            Some(button) => self.activate(button, None),
            None => self.submit_form(form, None),
        }
    }

    /// Clicks `hit`, at `point` or else at the centre of its box, and does what the click
    /// activates. See [`HtmlDom::click`].
    fn activate(&mut self, hit: NodeId, point: Option<(f32, f32)>) -> Option<NodeId> {
        let activation = form::activation_target(&self.doc, hit);
        let toggled = activation.and_then(|activation| form::toggle(&mut self.doc, activation));

        let (x, y) = point.unwrap_or_else(|| {
            // The centre of the box, where the box is drawn.
            self.current
                .as_ref()
                .and_then(|list| {
                    let entry = list.get(hit)?;
                    list.project(
                        hit,
                        entry.rect.x + entry.rect.width / 2.0,
                        entry.rect.y + entry.rect.height / 2.0,
                    )
                })
                .unwrap_or((0.0, 0.0))
        });
        let clicked = self.send_click(hit, x, y);
        let prevented = clicked.is_some_and(|(_, prevented)| prevented);

        if let Some(toggled) = toggled {
            if prevented {
                toggled.undo(&mut self.doc);
            } else if toggled.target_changed(&self.doc) {
                let target = toggled.target;
                self.send_form_event(target, "input");
                self.send_form_event(target, "change");
            }
        } else if !prevented {
            match activation {
                Some(form::Activation::Submit { button, form }) => {
                    let submitted = self.submit_form(form, Some(button));
                    if clicked.is_none() {
                        return submitted;
                    }
                }
                Some(form::Activation::Label(control)) => {
                    // The control a label is for is clicked in turn, so a checkbox ticks
                    // when its label's text is clicked.
                    let activated = self.activate(control, None);
                    if clicked.is_none() {
                        return activated;
                    }
                }
                _ => {}
            }
        }
        clicked.map(|(target, _)| target)
    }

    /// Delivers a click at a point to the nearest click handler at or around `hit`.
    /// Returns that handler's node and whether a handler prevented the default.
    fn send_click(&mut self, hit: NodeId, x: f32, y: f32) -> Option<(NodeId, bool)> {
        let (target, element) = self.listener_target(hit, "click")?;
        // The point in the coordinates the target was laid out in: a transformed target
        // sees the pointer where it would be if the box were not transformed.
        let (local_x, local_y) = self
            .current
            .as_ref()
            .and_then(|list| list.local_point(target, x, y))
            .unwrap_or((x, y));
        let origin = self
            .current
            .as_ref()
            .and_then(|list| list.get(target))
            .map_or((0.0, 0.0), |entry| (entry.rect.x, entry.rect.y));
        let data = PointerData {
            client: (x as f64, y as f64),
            element: ((local_x - origin.0) as f64, (local_y - origin.1) as f64),
        };
        Some((target, self.send(element, "click", Box::new(data))))
    }

    /// Delivers `name` (`input` or `change`) to the nearest handler at or around the
    /// field `node`, with the field's value and its form's values.
    fn send_form_event(&mut self, node: NodeId, name: &'static str) -> Option<NodeId> {
        let (target, element) = self.listener_target(node, name)?;
        let committed = self.state.committed_values();
        let value = form::event_value(&self.doc, committed, node);
        let values = form::form_owner(&self.doc, node)
            .map(|owner| form::form_values(&self.doc, committed, owner, None))
            .unwrap_or_default();
        let data = form::FormEventData { value, values };
        self.send(element, name, Box::new(data));
        Some(target)
    }

    /// Delivers `submit` to the handler of `form` or the nearest one around it, with what
    /// the form submits. A form has nowhere to go from here, so the app's handler is the
    /// whole of the submission.
    fn submit_form(&mut self, form: NodeId, submitter: Option<NodeId>) -> Option<NodeId> {
        let (target, element) = self.listener_target(form, "submit")?;
        let values = form::form_values(&self.doc, self.state.committed_values(), form, submitter);
        let data = form::FormEventData {
            value: String::new(),
            values,
        };
        self.send(element, "submit", Box::new(data));
        Some(target)
    }

    /// Runs the handlers for `name` from `element` up, the way Dioxus bubbles. Returns
    /// whether one of them prevented the default.
    fn send(&mut self, element: ElementId, name: &str, data: Box<dyn Any>) -> bool {
        let event = Event::new(Rc::new(PlatformEventData::new(data)), true);
        let probe = event.clone();
        self.vdom
            .runtime()
            .handle_event(name, event.into_any(), element);
        !probe.default_action_enabled()
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

/// Whether any box that declares a motion of layout changes its style in the next moment.
/// Resolves the document a thousandth of a second on, which a motion that is running shows
/// and one that is not does not.
fn layout_moves(doc: &mut BaseDocument, scan: &Scan, now: f64) -> bool {
    let before: Vec<_> = scan
        .layout_nodes
        .iter()
        .map(|&id| doc.get_node(id).map(|node| node.style.clone()))
        .collect();
    resolve_styles(doc, now + 0.001);
    let after: Vec<_> = scan
        .layout_nodes
        .iter()
        .map(|&id| doc.get_node(id).map(|node| node.style.clone()))
        .collect();
    before != after
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

/// Turns the events this document sends into Dioxus event data: clicks, and the `input`,
/// `change` and `submit` events of form controls. Every other conversion is unreachable
/// from here.
struct Converter;

fn not_sent(kind: &str) -> ! {
    panic!(
        "the HTML path does not send {kind} events yet, so it has no {kind} data to \
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
    fn convert_form_data(&self, event: &PlatformEventData) -> dioxus_html::FormData {
        let data = event
            .downcast::<form::FormEventData>()
            .expect("every form event this document sends carries FormEventData")
            .clone();
        dioxus_html::FormData::new(data)
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
