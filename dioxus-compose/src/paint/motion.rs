//! CSS transitions and `@keyframes` animations on the HTML path.
//!
//! blitz-dom advances stylo's animation state when it is handed a time
//! (`BaseDocument::resolve_stylist(now)`), but it keeps that state to itself: nothing says
//! which node is animating, from where, to where. So the document has two ways to play a
//! motion, and which one a document gets depends on what its CSS asks to animate.
//!
//! - **The renderer plays it.** `opacity`, `transform` and `background-color` change
//!   nothing about layout, so a renderer can play them from a description: the property,
//!   its keyframes, the duration, delay, easing, repeat, direction and fill. The Host sends
//!   that description once, with the steady value in the display list, and then does
//!   nothing per frame. A transition is found by comparing the steady value of one layout
//!   with the last (the document is resolved a second time, past the end of every
//!   transition, to read where each one lands). An `@keyframes` animation is read from
//!   the stylesheet and its keyframe values are computed by stylo, in a scratch document.
//! - **The Host ticks it.** Anything else (`width`, `margin`, `color`, `all`) changes
//!   layout or is inherited by what is inside the box, so the Host hands blitz-dom the
//!   frame time each frame while the motion runs, lays the page out again and sends the
//!   entries that changed. blitz-dom has one clock for the whole document, so while such a
//!   motion runs the whole document is ticked, and the motions a renderer would have
//!   played are ticked with it.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use blitz_dom::{BaseDocument, DocumentConfig, LocalName, Namespace, QualName};
use blitz_html::HtmlProvider;
use blitz_traits::net::DummyNetProvider;
use blitz_traits::shell::{ColorScheme, Viewport};
use style::properties::ComputedValues;
use style::values::computed::easing::ComputedTimingFunction;
use style::values::computed::{
    AnimationDirection as StyloDirection, AnimationFillMode as StyloFill, AnimationPlayState,
    TransitionProperty,
};
use style::values::generics::easing::{
    StepPosition as StyloStep, TimingFunction as StyloTiming, TimingKeyword,
};

use crate::html::NodeId;
use crate::layout::image::UNRESOLVED_BASE;
use crate::layout::{convert, local, resolve_styles};
use crate::paint::display_list::{
    AnimatedProperty, AnimatedValue, Animation, DisplayList, Easing, FillMode, Iterations,
    Keyframe, NodeEntry, PlayDirection, Rgba, SkippedTransform, StepPosition, Transform2D,
    TransformFunction, TransitionStart,
};
use crate::paint::transform::functions_of;

/// How far past a transition's end the document is resolved to read where it lands, in
/// seconds. Added to the longest delay plus duration of the transitions found.
pub(crate) const SETTLE_MARGIN: f32 = 0.01;

/// The properties a renderer plays by itself, in the order their animations are listed.
const RENDERER_PROPERTIES: [AnimatedProperty; 3] = [
    AnimatedProperty::Opacity,
    AnimatedProperty::Transform,
    AnimatedProperty::BackgroundColor,
];

fn css_name(property: AnimatedProperty) -> &'static str {
    match property {
        AnimatedProperty::Opacity => "opacity",
        AnimatedProperty::Transform => "transform",
        AnimatedProperty::BackgroundColor => "background-color",
    }
}

fn renderer_property(name: &str) -> Option<AnimatedProperty> {
    RENDERER_PROPERTIES
        .into_iter()
        .find(|property| css_name(*property) == name)
}

/// One transition a node declares for a property a renderer plays.
#[derive(Clone, Debug)]
pub(crate) struct TransitionSpec {
    pub property: AnimatedProperty,
    pub duration: f32,
    pub delay: f32,
    pub easing: Easing,
}

/// One `animation-name` of a node whose keyframes a renderer plays.
#[derive(Clone, Debug)]
pub(crate) struct AnimationSpec {
    pub name: String,
    pub duration: f32,
    pub delay: f32,
    pub easing: Easing,
    pub iterations: Iterations,
    pub direction: PlayDirection,
    pub fill: FillMode,
    pub paused: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct NodeMotion {
    pub transitions: Vec<TransitionSpec>,
    pub animations: Vec<AnimationSpec>,
}

/// What the styles of one resolved document declare.
#[derive(Debug, Default)]
pub(crate) struct Scan {
    /// Per node, the motion a renderer plays. Only nodes that have some.
    pub nodes: HashMap<NodeId, NodeMotion>,
    /// Some node declares a motion the Host has to tick: it changes layout, or it is a
    /// property a renderer does not play.
    pub layout_motion: bool,
    /// The longest those motions run, delay included, in seconds. Meaningless when
    /// `layout_endless`.
    pub layout_span: f32,
    /// One of them repeats forever.
    pub layout_endless: bool,
    /// The nodes that declare one.
    pub layout_nodes: Vec<NodeId>,
    /// One of them waits before it starts, so it cannot be told from one that is not
    /// running by looking at whether anything moves.
    pub layout_delayed: bool,
    /// How far past now the longest transition a renderer plays ends, in seconds; zero when
    /// there is none.
    pub horizon: f32,
    /// How many more times the document has to be resolved at the current time for the
    /// animations that have ended to be over in its styles.
    pub catch_up: u32,
    /// Nodes whose `transform` a renderer plays, which keep even a plain translation as a
    /// transform.
    pub keep_transform: HashSet<NodeId>,
}

impl Scan {
    fn note_layout_motion(&mut self, node: NodeId, delay: f32) {
        self.layout_motion = true;
        self.layout_delayed |= delay > 0.0;
        if self.layout_nodes.last() != Some(&node) {
            self.layout_nodes.push(node);
        }
    }

    /// Whether the second resolve, past the end of the transitions, is worth doing.
    pub fn needs_settling(&self) -> bool {
        self.horizon > 0.0
    }
}

/// A block of an `@keyframes` rule: one keyframe selector and what it declares.
#[derive(Clone, Debug)]
pub(crate) struct KeyframeBlock {
    pub offset: f32,
    pub declarations: String,
}

/// What the Host remembers about motion between layouts.
#[derive(Default)]
pub(crate) struct MotionState {
    /// The text of every `<style>` the keyframes were last read from.
    source: String,
    keyframes: HashMap<String, Vec<KeyframeBlock>>,
    /// Whether each `@keyframes` rule changes only properties a renderer plays.
    renderer_keyframes: HashMap<String, bool>,
    probe: Option<ValueProbe>,
    /// When each running animation was first seen, by node and `@keyframes` name.
    starts: HashMap<(NodeId, String), f64>,
    seen: HashSet<(NodeId, String)>,
    /// Whether the Host is ticking this document's motion.
    pub ticking: bool,
    /// When ticking ends, if it does.
    deadline: Option<f64>,
    /// Transforms already reported as skipped, so each is reported once.
    pub warned: HashSet<(NodeId, String)>,
}

impl MotionState {
    /// Reads the declared motion of every element of a document whose styles have just
    /// been resolved.
    pub fn scan(&mut self, doc: &BaseDocument, now: f64) -> Scan {
        self.refresh_keyframes(doc);
        let mut scan = Scan::default();
        for (id, node) in doc.tree().iter() {
            if !node.is_element() {
                continue;
            }
            let Some(style) = node.primary_styles() else {
                continue;
            };
            let ui = style.get_ui();
            let mut motion = NodeMotion::default();

            for index in 0..ui.transition_property_count() {
                let TransitionProperty::NonCustom(property) = ui.transition_property_at(index)
                else {
                    continue;
                };
                let duration = ui.transition_duration_mod(index).seconds().max(0.0);
                let delay = ui.transition_delay_mod(index).seconds();
                if duration + delay <= 0.0 {
                    continue;
                }
                match renderer_property(property.name()) {
                    Some(property) => {
                        let spec = TransitionSpec {
                            property,
                            duration,
                            delay,
                            easing: easing_of(&ui.transition_timing_function_mod(index)),
                        };
                        // The last declaration of a property is the one that counts.
                        motion
                            .transitions
                            .retain(|other| other.property != property);
                        motion.transitions.push(spec);
                        scan.horizon = scan.horizon.max(duration + delay.max(0.0));
                        if property == AnimatedProperty::Transform {
                            scan.keep_transform.insert(id);
                        }
                    }
                    None => {
                        scan.note_layout_motion(id, delay);
                        scan.layout_span = scan.layout_span.max(duration + delay.max(0.0));
                    }
                }
            }

            for index in 0..ui.animation_name_count() {
                let name = ui.animation_name_at(index);
                let Some(atom) = name.as_atom() else {
                    continue;
                };
                let name = atom.to_string();
                let duration = ui.animation_duration_mod(index).seconds();
                if duration <= 0.0 {
                    continue;
                }
                let delay = ui.animation_delay_mod(index).seconds();
                let count = ui.animation_iteration_count_mod(index).0;
                let Some(&renderer_only) = self.renderer_keyframes.get(&name) else {
                    // Keyframes this crate cannot see are not animated by anyone here.
                    continue;
                };
                if !renderer_only {
                    scan.note_layout_motion(id, delay);
                    if count.is_infinite() {
                        scan.layout_endless = true;
                    } else {
                        scan.layout_span = scan
                            .layout_span
                            .max(delay.max(0.0) + duration * count.max(0.0));
                    }
                    continue;
                }
                if self
                    .keyframes
                    .get(&name)
                    .is_some_and(|blocks| declares(blocks, AnimatedProperty::Transform))
                {
                    scan.keep_transform.insert(id);
                }
                let paused = matches!(
                    ui.animation_play_state_mod(index),
                    AnimationPlayState::Paused
                );
                // blitz-dom moves a finite animation on by one iteration each time it is
                // resolved, so one that ended while nothing was resolved needs resolving
                // once for each of its iterations before its styles say it is over.
                if count.is_finite()
                    && !paused
                    && let Some(&start) = self.starts.get(&(id, name.clone()))
                    && now >= start + f64::from(delay + duration * count)
                {
                    scan.catch_up = scan.catch_up.max(count.ceil() as u32);
                }
                motion.animations.push(AnimationSpec {
                    name,
                    duration,
                    delay,
                    easing: easing_of(&ui.animation_timing_function_mod(index)),
                    iterations: if count.is_infinite() {
                        Iterations::Infinite
                    } else {
                        Iterations::Count(count)
                    },
                    direction: match ui.animation_direction_mod(index) {
                        StyloDirection::Normal => PlayDirection::Normal,
                        StyloDirection::Reverse => PlayDirection::Reverse,
                        StyloDirection::Alternate => PlayDirection::Alternate,
                        StyloDirection::AlternateReverse => PlayDirection::AlternateReverse,
                    },
                    fill: match ui.animation_fill_mode_mod(index) {
                        StyloFill::None => FillMode::None,
                        StyloFill::Forwards => FillMode::Forwards,
                        StyloFill::Backwards => FillMode::Backwards,
                        StyloFill::Both => FillMode::Both,
                    },
                    paused,
                });
            }

            if !motion.transitions.is_empty() || !motion.animations.is_empty() {
                scan.nodes.insert(id, motion);
            }
        }
        scan
    }

    /// Decides whether the Host has to tick the document: some motion that changes layout
    /// is running (`active`). `ticked` says this layout is one of the ticks, not one the
    /// app's own changes caused; an app change restarts the clock on how long ticking
    /// lasts, because it may have started a motion.
    pub fn decide_ticking(&mut self, scan: &Scan, active: bool, now: f64, ticked: bool) -> bool {
        if !(scan.layout_motion && active) {
            self.ticking = false;
            self.deadline = None;
            return false;
        }
        if !(ticked && self.ticking) {
            self.deadline = if scan.layout_endless {
                None
            } else {
                Some(now + f64::from(scan.layout_span))
            };
        }
        self.ticking = self.deadline.is_none_or(|deadline| now < deadline);
        self.ticking
    }

    fn refresh_keyframes(&mut self, doc: &BaseDocument) {
        let style = local("style");
        let mut source = String::new();
        for (_, node) in doc.tree().iter() {
            if node.data.is_element_with_tag_name(&style) {
                source.push_str(&node.text_content());
                source.push('\n');
            }
        }
        if source == self.source {
            return;
        }
        self.keyframes = parse_keyframes(&source);
        self.renderer_keyframes = self
            .keyframes
            .iter()
            .map(|(name, blocks)| (name.clone(), renderer_only(blocks)))
            .collect();
        self.source = source;
    }

    pub fn begin_frame(&mut self) {
        self.seen.clear();
    }

    /// Forgets the animations that are no longer on any node.
    pub fn end_frame(&mut self) {
        let seen = &self.seen;
        self.starts.retain(|key, _| seen.contains(key));
    }
}

/// What the display list asks the motion pass while it places a box.
pub(crate) struct AnimationRequest<'a> {
    pub node: NodeId,
    pub tag: &'a str,
    /// The box's size, which a percentage in a transform is a share of.
    pub width: f32,
    pub height: f32,
    /// The box's font size, which an `em` in a keyframe is a multiple of.
    pub font_px: f32,
    /// The box's `transform-origin`.
    pub origin: (f32, f32),
}

/// The motion pass as the display list builder sees it.
pub(crate) trait Playback {
    /// Whether the node's translation is kept as a transform instead of folded into its
    /// position.
    fn keeps_transform(&self, node: NodeId) -> bool;

    /// The `@keyframes` animations a renderer plays on the node that are running now.
    fn animations(&mut self, request: &AnimationRequest<'_>) -> Vec<Animation>;
}

/// [`Playback`] over a [`Scan`].
pub(crate) struct Playing<'a> {
    pub scan: &'a Scan,
    pub state: &'a mut MotionState,
    /// The display list before this layout, which supplies the value a keyframe animation
    /// starts from or ends at when its `@keyframes` rule does not say.
    pub prior: Option<&'a DisplayList>,
    pub now: f64,
    pub skipped: Vec<SkippedTransform>,
}

impl Playback for Playing<'_> {
    fn keeps_transform(&self, node: NodeId) -> bool {
        self.scan.keep_transform.contains(&node)
    }

    fn animations(&mut self, request: &AnimationRequest<'_>) -> Vec<Animation> {
        let Some(motion) = self.scan.nodes.get(&request.node) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for spec in &motion.animations {
            let key = (request.node, spec.name.clone());
            let start = *self.state.starts.entry(key.clone()).or_insert(self.now);
            self.state.seen.insert(key);
            let Some(blocks) = self.state.keyframes.get(&spec.name).cloned() else {
                continue;
            };
            let probe = self.state.probe.get_or_insert_with(ValueProbe::new);
            let evaluated: Vec<(Vec<String>, Option<BlockValues>)> = blocks
                .iter()
                .map(|block| {
                    (
                        declared_names(&block.declarations),
                        probe.values(
                            &block.declarations,
                            request.font_px,
                            request.width,
                            request.height,
                        ),
                    )
                })
                .collect();

            for property in RENDERER_PROPERTIES {
                let mut keyframes: Vec<Keyframe> = Vec::new();
                let mut flat = true;
                for (block, (names, values)) in blocks.iter().zip(&evaluated) {
                    if !names.iter().any(|name| name == css_name(property)) {
                        continue;
                    }
                    let Some(values) = values else {
                        continue;
                    };
                    let value = match property {
                        AnimatedProperty::Opacity => {
                            AnimatedValue::Opacity(values.opacity.clamp(0.0, 1.0))
                        }
                        AnimatedProperty::BackgroundColor => {
                            AnimatedValue::Colour(values.background)
                        }
                        AnimatedProperty::Transform => match &values.functions {
                            Some(functions) => AnimatedValue::Transform(Transform2D::new(
                                functions.clone(),
                                request.origin,
                            )),
                            None => {
                                self.skipped.push(SkippedTransform {
                                    node: request.node,
                                    tag: request.tag.to_string(),
                                    transform: format!(
                                        "@keyframes {}: {}",
                                        spec.name,
                                        block.declarations.trim()
                                    ),
                                });
                                flat = false;
                                break;
                            }
                        },
                    };
                    match keyframes.last_mut() {
                        Some(last) if last.offset == block.offset => last.value = value,
                        _ => keyframes.push(Keyframe {
                            offset: block.offset,
                            value,
                        }),
                    }
                }
                if !flat || keyframes.is_empty() {
                    continue;
                }
                if keyframes[0].offset > 0.0 {
                    let value = self.implicit(request, property);
                    keyframes.insert(0, Keyframe { offset: 0.0, value });
                }
                if keyframes[keyframes.len() - 1].offset < 1.0 {
                    let value = self.implicit(request, property);
                    keyframes.push(Keyframe { offset: 1.0, value });
                }
                let animation = Animation {
                    property,
                    keyframes,
                    duration: spec.duration,
                    delay: spec.delay,
                    easing: spec.easing,
                    iterations: spec.iterations,
                    direction: spec.direction,
                    fill: spec.fill,
                    paused: spec.paused,
                    name: Some(spec.name.clone()),
                };
                // A finished animation leaves its fill to the cascade, which blitz-dom
                // resolved at the frame time.
                if !spec.paused
                    && animation
                        .total_duration()
                        .is_some_and(|total| self.now >= start + f64::from(total))
                {
                    continue;
                }
                out.push(animation);
            }
        }
        out
    }
}

impl Playing<'_> {
    /// The value of `property` a keyframe animation takes where its rule does not say:
    /// the box's value in the last layout, else the property's initial value.
    fn implicit(
        &self,
        request: &AnimationRequest<'_>,
        property: AnimatedProperty,
    ) -> AnimatedValue {
        let entry = self
            .prior
            .and_then(|list| list.get(request.node).map(|entry| (list, entry)));
        match property {
            AnimatedProperty::Opacity => {
                AnimatedValue::Opacity(entry.map_or(1.0, |(list, entry)| list.own_opacity(entry)))
            }
            AnimatedProperty::Transform => AnimatedValue::Transform(
                entry
                    .and_then(|(_, entry)| entry.transform.clone())
                    .unwrap_or_else(|| Transform2D::identity(request.origin)),
            ),
            AnimatedProperty::BackgroundColor => AnimatedValue::Colour(
                entry
                    .and_then(|(_, entry)| entry.background)
                    .unwrap_or(Rgba::new(0, 0, 0, 0)),
            ),
        }
    }
}

/// The value of `property` an entry has, as an animation sees it.
fn value_of(list: &DisplayList, entry: &NodeEntry, property: AnimatedProperty) -> AnimatedValue {
    match property {
        AnimatedProperty::Opacity => AnimatedValue::Opacity(list.own_opacity(entry)),
        AnimatedProperty::Transform => AnimatedValue::Transform(
            entry
                .transform
                .clone()
                .unwrap_or_else(|| Transform2D::identity((0.0, 0.0))),
        ),
        AnimatedProperty::BackgroundColor => {
            AnimatedValue::Colour(entry.background.unwrap_or(Rgba::new(0, 0, 0, 0)))
        }
    }
}

/// The transitions that began between two layouts: for each node that declares a
/// transition of a property a renderer plays, and whose steady value for it is not what it
/// was, an animation from the old value to the new.
pub(crate) fn transitions(
    scan: &Scan,
    prior: Option<&DisplayList>,
    current: &DisplayList,
) -> Vec<TransitionStart> {
    let Some(prior) = prior else {
        return Vec::new();
    };
    let mut started = Vec::new();
    for entry in &current.entries {
        let Some(motion) = scan.nodes.get(&entry.node) else {
            continue;
        };
        let Some(before) = prior.get(entry.node) else {
            continue;
        };
        for spec in &motion.transitions {
            let mut from = value_of(prior, before, spec.property);
            let mut to = value_of(current, entry, spec.property);
            // A transform that appears or goes away is a transition to or from the identity
            // about the other side's origin.
            if let (AnimatedValue::Transform(a), AnimatedValue::Transform(b)) = (&mut from, &mut to)
            {
                match (before.transform.is_some(), entry.transform.is_some()) {
                    (false, true) => a.origin = b.origin,
                    (true, false) => b.origin = a.origin,
                    _ => {}
                }
            }
            if !differ(&from, &to) {
                continue;
            }
            started.push(TransitionStart {
                node: entry.node,
                animation: Animation {
                    property: spec.property,
                    keyframes: vec![
                        Keyframe {
                            offset: 0.0,
                            value: from,
                        },
                        Keyframe {
                            offset: 1.0,
                            value: to,
                        },
                    ],
                    duration: spec.duration,
                    delay: spec.delay,
                    easing: spec.easing,
                    iterations: Iterations::Count(1.0),
                    direction: PlayDirection::Normal,
                    fill: FillMode::Backwards,
                    paused: false,
                    name: None,
                },
            });
        }
    }
    started
}

fn differ(a: &AnimatedValue, b: &AnimatedValue) -> bool {
    match (a, b) {
        (AnimatedValue::Opacity(a), AnimatedValue::Opacity(b)) => (a - b).abs() > 1e-4,
        _ => a != b,
    }
}

/// Which `@keyframes` blocks declare `property`.
fn declares(blocks: &[KeyframeBlock], property: AnimatedProperty) -> bool {
    blocks.iter().any(|block| {
        declared_names(&block.declarations)
            .iter()
            .any(|name| name == css_name(property))
    })
}

/// Whether every property a rule changes is one a renderer plays, and it changes at least
/// one. (`animation-timing-function` is allowed because it only shapes the segment after
/// its keyframe, which this crate does not model per keyframe.)
fn renderer_only(blocks: &[KeyframeBlock]) -> bool {
    let mut any = false;
    for block in blocks {
        for name in declared_names(&block.declarations) {
            if renderer_property(&name).is_some() {
                any = true;
            } else if name != "animation-timing-function" {
                return false;
            }
        }
    }
    any
}

/// Property names a declaration list sets, lower-cased.
fn declared_names(declarations: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    let mut take = |segment: &str| {
        if let Some((name, _)) = segment.split_once(':') {
            let name = name.trim().to_ascii_lowercase();
            if !name.is_empty() {
                names.push(name);
            }
        }
    };
    for (index, character) in declarations.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => depth -= 1,
            ';' if depth == 0 => {
                take(&declarations[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    take(&declarations[start..]);
    names
}

/// A CSS timing function as the display list carries it.
fn easing_of(function: &ComputedTimingFunction) -> Easing {
    match function {
        StyloTiming::Keyword(keyword) => match keyword {
            TimingKeyword::Linear => Easing::Linear,
            TimingKeyword::Ease => Easing::Ease,
            TimingKeyword::EaseIn => Easing::EaseIn,
            TimingKeyword::EaseOut => Easing::EaseOut,
            TimingKeyword::EaseInOut => Easing::EaseInOut,
        },
        StyloTiming::CubicBezier { x1, y1, x2, y2 } => Easing::CubicBezier(*x1, *y1, *x2, *y2),
        StyloTiming::Steps(count, position) => Easing::Steps {
            count: (*count).max(1) as u32,
            position: match position {
                StyloStep::JumpStart | StyloStep::Start => StepPosition::JumpStart,
                StyloStep::JumpEnd | StyloStep::End => StepPosition::JumpEnd,
                StyloStep::JumpNone => StepPosition::JumpNone,
                StyloStep::JumpBoth => StepPosition::JumpBoth,
            },
        },
        // `linear(0, 0.25 75%, 1)` is a piecewise curve; its endpoints are linear's.
        StyloTiming::LinearFunction(_) => Easing::Linear,
    }
}

// ---------------------------------------------------------------------------------------
// Reading @keyframes out of the stylesheets
// ---------------------------------------------------------------------------------------

fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        match rest[start + 2..].find("*/") {
            Some(end) => rest = &rest[start + 2 + end + 2..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// The index of the `}` that closes the `{` at `open`.
fn matching_brace(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (index, byte) in text.bytes().enumerate().skip(open) {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// Every `@keyframes` rule in `css`, wherever it is nested, by name.
pub(crate) fn parse_keyframes(css: &str) -> HashMap<String, Vec<KeyframeBlock>> {
    let css = strip_comments(css);
    let mut out = HashMap::new();
    let mut at = 0;
    while let Some(found) = css[at..].find('@') {
        let start = at + found;
        let rest = &css[start + 1..];
        let name_end = rest
            .find(|c: char| c.is_whitespace() || c == '{')
            .unwrap_or(rest.len());
        let rule = rest[..name_end].to_ascii_lowercase();
        let after = start + 1 + name_end;
        at = after;
        let is_keyframes =
            rule == "keyframes" || (rule.starts_with('-') && rule.ends_with("-keyframes"));
        if !is_keyframes {
            continue;
        }
        let Some(open) = css[after..].find('{').map(|found| after + found) else {
            break;
        };
        let name = css[after..open]
            .trim()
            .trim_matches(|c: char| c == '"' || c == '\'')
            .to_string();
        let Some(close) = matching_brace(&css, open) else {
            break;
        };
        out.insert(name, parse_blocks(&css[open + 1..close]));
        at = close + 1;
    }
    out
}

fn parse_blocks(body: &str) -> Vec<KeyframeBlock> {
    let mut blocks = Vec::new();
    let mut at = 0;
    while let Some(open) = body[at..].find('{').map(|found| at + found) {
        let Some(close) = matching_brace(body, open) else {
            break;
        };
        let declarations = body[open + 1..close].trim().to_string();
        for selector in body[at..open].split(',') {
            if let Some(offset) = keyframe_offset(selector.trim()) {
                blocks.push(KeyframeBlock {
                    offset,
                    declarations: declarations.clone(),
                });
            }
        }
        at = close + 1;
    }
    // Stable, so two blocks at one offset stay in source order and the later one wins.
    blocks.sort_by(|a, b| a.offset.total_cmp(&b.offset));
    blocks
}

fn keyframe_offset(selector: &str) -> Option<f32> {
    let selector = selector.to_ascii_lowercase();
    match selector.as_str() {
        "from" => Some(0.0),
        "to" => Some(1.0),
        other => {
            let percent: f32 = other.strip_suffix('%')?.trim().parse().ok()?;
            (0.0..=100.0).contains(&percent).then_some(percent / 100.0)
        }
    }
}

// ---------------------------------------------------------------------------------------
// Computing a keyframe's values with stylo
// ---------------------------------------------------------------------------------------

/// What a keyframe block sets, computed.
#[derive(Clone)]
struct BlockValues {
    opacity: f32,
    background: Rgba,
    /// `None` for a 3D transform.
    functions: Option<Vec<TransformFunction>>,
}

/// A document of one element, whose `style` attribute is set to a keyframe's declarations
/// so that stylo reads them the way it reads any author style: every unit, keyword and
/// function it knows, with no parser of this crate's to keep in step.
struct ValueProbe {
    doc: BaseDocument,
    node: usize,
    /// Answers already worked out, by declarations, font size and box size.
    known: HashMap<(String, u32, u32, u32), Option<BlockValues>>,
}

fn qualified(tag: &str, namespace: &str) -> QualName {
    QualName::new(None, Namespace::from(namespace), LocalName::from(tag))
}

impl ValueProbe {
    fn new() -> Self {
        const HTML: &str = "http://www.w3.org/1999/xhtml";
        let mut doc = BaseDocument::new(DocumentConfig {
            html_parser_provider: Some(Arc::new(HtmlProvider)),
            viewport: Some(Viewport::new(0, 0, 1.0, ColorScheme::Light)),
            base_url: Some(UNRESOLVED_BASE.to_string()),
            net_provider: Some(Arc::new(DummyNetProvider)),
            ..DocumentConfig::default()
        });
        let node = {
            let mut mutator = doc.mutate();
            let html = mutator.create_element(qualified("html", HTML), Vec::new());
            let head = mutator.create_element(qualified("head", HTML), Vec::new());
            let body = mutator.create_element(qualified("body", HTML), Vec::new());
            let probe = mutator.create_element(qualified("div", HTML), Vec::new());
            mutator.append_children(body, &[probe]);
            mutator.append_children(html, &[head, body]);
            mutator.append_children(0, &[html]);
            probe
        };
        Self {
            doc,
            node,
            known: HashMap::new(),
        }
    }

    /// The computed `opacity`, `background-color` and `transform` of an element with the
    /// font size `font_px` and these declarations, the transform resolved against a box of
    /// `width` by `height`.
    fn values(
        &mut self,
        declarations: &str,
        font_px: f32,
        width: f32,
        height: f32,
    ) -> Option<BlockValues> {
        let key = (
            declarations.to_string(),
            font_px.to_bits(),
            width.to_bits(),
            height.to_bits(),
        );
        if let Some(known) = self.known.get(&key) {
            return known.clone();
        }
        let values = self.compute(declarations, font_px, width, height);
        if self.known.len() >= 256 {
            self.known.clear();
        }
        self.known.insert(key, values.clone());
        values
    }

    fn compute(
        &mut self,
        declarations: &str,
        font_px: f32,
        width: f32,
        height: f32,
    ) -> Option<BlockValues> {
        let css = format!("font-size: {font_px}px; {declarations}");
        {
            let mut mutator = self.doc.mutate();
            mutator.set_attribute(self.node, qualified("style", ""), &css);
        }
        resolve_styles(&mut self.doc, 0.0);
        let node = self.doc.get_node(self.node)?;
        let style = node.primary_styles()?;
        let style: &ComputedValues = &style;
        Some(BlockValues {
            opacity: style.clone_opacity(),
            background: convert::resolve_color(&style.clone_background_color(), style),
            functions: functions_of(style, width, height),
        })
    }
}

// ---------------------------------------------------------------------------------------
// prefers-reduced-motion
// ---------------------------------------------------------------------------------------

/// stylo's media features, as blitz-dom configures them, include no
/// `prefers-reduced-motion`, so `@media (prefers-reduced-motion: reduce)` never matches
/// and neither does its opposite. The Host knows the user's setting, so it answers the
/// query itself: each such feature in `css` is replaced by a condition that is always true
/// or never is, which stays valid wherever the feature appeared (alone, after `and`,
/// inside a list).
pub(crate) fn rewrite_reduced_motion(css: &str, reduce: bool) -> String {
    const FEATURE: &str = "prefers-reduced-motion";
    const ALWAYS: &str = "(min-width: 0px)";
    const NEVER: &str = "(min-width: 99999999px)";
    let lower = css.to_ascii_lowercase();
    let mut out = String::with_capacity(css.len());
    let mut at = 0;
    while let Some(found) = lower[at..].find(FEATURE) {
        let hit = at + found;
        let end = hit + FEATURE.len();
        let open = lower[at..hit].rfind('(').map(|index| at + index);
        let close = lower[end..].find(')').map(|index| end + index);
        let replacement = match (open, close) {
            (Some(open), Some(close)) if lower[open + 1..hit].trim().is_empty() => {
                let condition: String = lower[end..close]
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect();
                match condition.as_str() {
                    "" | ":reduce" => Some((open, close, reduce)),
                    ":no-preference" => Some((open, close, !reduce)),
                    _ => None,
                }
            }
            _ => None,
        };
        match replacement {
            Some((open, close, truth)) => {
                out.push_str(&css[at..open]);
                out.push_str(if truth { ALWAYS } else { NEVER });
                at = close + 1;
            }
            None => {
                out.push_str(&css[at..end]);
                at = end;
            }
        }
    }
    out.push_str(&css[at..]);
    out
}
