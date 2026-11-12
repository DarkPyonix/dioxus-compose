//! The canvas background: the background of the root element, or of `<body>` when the
//! root has none, paints the whole canvas rather than the box it was declared on (CSS
//! Backgrounds 3, section 2.11.2, "The Canvas Background and the Root Element").
//!
//! A browser fills the window with it, down past the end of a short page and all the way
//! down a long one, and does not paint it a second time over the element's own box. Images
//! and gradients in it are sized and placed against the root element's box, and repeat
//! over the whole canvas.
//!
//! Pages are laid out at 800 by 600 with `html` and `body` free of margins unless a test
//! says otherwise. Text is sized by a measurer that gives every character 10px and every
//! line 20px; no fixture here holds text worth measuring.

use dioxus_compose::Host;
use dioxus_compose::html::prelude::*;
use dioxus_compose::html::{
    AnimatedProperty, BackgroundImage, DisplayList, HtmlConfig, HtmlDom, LiteralColours, NodeEntry,
    PlanKey, PlanModifier, Rect, Rgba, TextMeasureRequest, TextMeasurer, TextMetrics, TileRepeat,
};
use dioxus_compose::protocol::{Mutation, decode_batch};
use dioxus_compose::schema::{Color, Modifier, Paint};

struct FixedAdvance;

impl TextMeasurer for FixedAdvance {
    fn measure(&mut self, request: &TextMeasureRequest<'_>) -> TextMetrics {
        TextMetrics {
            width: request.text.chars().count() as f32 * 10.0,
            height: 20.0,
            first_baseline: 15.0,
            line_count: 1,
        }
    }
}

const RED: Rgba = Rgba::new(255, 0, 0, 255);
const BLUE: Rgba = Rgba::new(0, 0, 255, 255);
const WHITE: Rgba = Rgba::new(255, 255, 255, 255);

fn config_with(css: &str) -> HtmlConfig {
    HtmlConfig {
        stylesheets: vec![format!("html, body {{ margin: 0; padding: 0; }}\n{css}")],
        measurer: Some(Box::new(FixedAdvance)),
        ..HtmlConfig::default()
    }
}

const ROOT_RED: &str = "html { background-color: rgb(255, 0, 0); }";
const BODY_WHITE: &str = "body { background: #ffffff; }";
const ROOT_GRADIENT: &str =
    "html { background-image: linear-gradient(rgb(255, 0, 0), rgb(0, 0, 255)); }";
const BODY_GLOW: &str = "body { background-color: rgb(255, 0, 0); \
                         animation: glow 2s linear infinite; } \
                         @keyframes glow { from { background-color: rgb(255, 0, 0); } \
                         to { background-color: rgb(0, 0, 255); } }";

fn root_red() -> HtmlConfig {
    config_with(ROOT_RED)
}

fn body_white() -> HtmlConfig {
    config_with(BODY_WHITE)
}

fn root_gradient() -> HtmlConfig {
    config_with(ROOT_GRADIENT)
}

fn body_glow() -> HtmlConfig {
    config_with(BODY_GLOW)
}

/// One block 100px tall, so the page is far shorter than the 600px window.
fn short_page() -> Element {
    rsx! {
        div { id: "content", style: "height: 100px" }
    }
}

/// One block 1500px tall, so the page runs well past the bottom of the window.
fn tall_page() -> Element {
    rsx! {
        div { id: "content", style: "height: 1500px" }
    }
}

fn laid_out(app: fn() -> Element, css: &str) -> HtmlDom {
    let mut dom = HtmlDom::with_config(app, config_with(css));
    dom.layout(800.0, 600.0, 1.0);
    dom
}

#[track_caller]
fn tagged<'a>(list: &'a DisplayList, tag: &str) -> &'a NodeEntry {
    list.entries
        .iter()
        .find(|entry| entry.tag == tag)
        .unwrap_or_else(|| panic!("no <{tag}> entry: {:#?}", list.entries))
}

#[track_caller]
fn assert_rect(actual: Rect, expected: Rect, what: &str) {
    let close = |a: f32, b: f32| (a - b).abs() < 0.01;
    assert!(
        close(actual.x, expected.x)
            && close(actual.y, expected.y)
            && close(actual.width, expected.width)
            && close(actual.height, expected.height),
        "{what}: expected {expected:?}, got {actual:?}"
    );
}

/// Whether `outer` covers all of `inner`.
fn covers(outer: Rect, inner: Rect) -> bool {
    outer.x <= inner.x + 0.01
        && outer.y <= inner.y + 0.01
        && outer.right() >= inner.right() - 0.01
        && outer.bottom() >= inner.bottom() - 0.01
}

fn page_backgrounds(dom: &mut HtmlDom) -> Vec<PlanModifier> {
    let plan = dom.plan(&mut LiteralColours).expect("laid out above");
    plan.root
        .modifiers
        .iter()
        .filter(|modifier| {
            matches!(
                modifier,
                PlanModifier::Background(_) | PlanModifier::BackgroundBrush { .. }
            )
        })
        .cloned()
        .collect()
}

fn node_backgrounds(dom: &mut HtmlDom, tag: &str) -> Vec<PlanModifier> {
    let node = tagged(dom.display_list().expect("laid out above"), tag).node;
    let plan = dom.plan(&mut LiteralColours).expect("laid out above");
    let Some(found) = plan.find(PlanKey::Node(node)) else {
        return Vec::new();
    };
    found
        .modifiers
        .iter()
        .filter(|modifier| {
            matches!(
                modifier,
                PlanModifier::Background(_) | PlanModifier::BackgroundBrush { .. }
            )
        })
        .cloned()
        .collect()
}

fn paint(colour: Rgba) -> Paint {
    Paint::Literal(Color::argb(
        (u32::from(colour.a) << 24)
            | (u32::from(colour.r) << 16)
            | (u32::from(colour.g) << 8)
            | u32::from(colour.b),
    ))
}

/// What the first batch of a Host running `app` gives each renderer node: its modifiers by
/// index, and the parent it was inserted into.
struct FirstFrame {
    modifiers: Vec<(u32, u16, Modifier)>,
    inserts: Vec<(u32, u32, u32)>,
    animated: Vec<u32>,
}

impl FirstFrame {
    fn of(app: fn() -> Element, config: fn() -> HtmlConfig) -> Self {
        let mut host = Host::html(app, config);
        let bytes = host.rebuild().expect("the first frame is built");
        let mut frame = FirstFrame {
            modifiers: Vec::new(),
            inserts: Vec::new(),
            animated: Vec::new(),
        };
        for record in decode_batch(bytes).expect("the batch decodes") {
            match record {
                Mutation::SetModifier {
                    node_id,
                    index,
                    modifier,
                } => frame.modifiers.push((node_id, index, modifier)),
                Mutation::Insert {
                    parent_id,
                    node_id,
                    index,
                } => frame.inserts.push((parent_id, node_id, index)),
                Mutation::StartAnimation(animation) => frame.animated.push(animation.node_id),
                _ => {}
            }
        }
        frame
    }

    /// The modifier list a node ends up with, by index, empties dropped.
    fn modifiers_of(&self, node: u32) -> Vec<Modifier> {
        let mut list: Vec<Modifier> = Vec::new();
        for (id, index, modifier) in &self.modifiers {
            if *id == node {
                let index = usize::from(*index);
                if list.len() <= index {
                    list.resize(index + 1, Modifier::Empty);
                }
                list[index] = modifier.clone();
            }
        }
        list.retain(|modifier| *modifier != Modifier::Empty);
        list
    }

    /// Every node given `modifier`.
    fn nodes_with(&self, modifier: &Modifier) -> Vec<u32> {
        let mut nodes: Vec<u32> = self
            .modifiers
            .iter()
            .filter(|(_, _, given)| given == modifier)
            .map(|(node, _, _)| *node)
            .collect();
        nodes.dedup();
        nodes
    }

    fn parent_of(&self, node: u32) -> Option<u32> {
        self.inserts
            .iter()
            .find(|(_, child, _)| *child == node)
            .map(|(parent, _, _)| *parent)
    }
}

/// `html { background-color }` paints the whole window, not the 100px the root's box is
/// tall, and the root's own box no longer paints it.
#[test]
fn fr34_root_background_fills_the_viewport() {
    let mut dom = laid_out(short_page, ROOT_RED);
    let list = dom.display_list().expect("laid out above").clone();
    let html = tagged(&list, "html");
    assert_rect(
        html.rect,
        Rect::new(0.0, 0.0, 800.0, 100.0),
        "the root's box",
    );

    let canvas = list.canvas.as_ref().expect("the root has a background");
    assert_eq!(canvas.source, html.node, "taken from <html>");
    assert_eq!(canvas.background, Some(RED));
    assert!(
        covers(canvas.area, Rect::new(0.0, 0.0, 800.0, 600.0)),
        "the canvas covers the viewport: {:?}",
        canvas.area
    );
    assert_eq!(
        html.background, None,
        "the root's box does not paint it again"
    );
    assert!(html.backgrounds.is_empty());

    assert_eq!(page_backgrounds(&mut dom), [PlanModifier::Background(RED)]);
    assert!(node_backgrounds(&mut dom, "html").is_empty());

    // The renderer draws it on the page, which is the window's size.
    let frame = FirstFrame::of(short_page, root_red);
    let painted = frame.nodes_with(&Modifier::Background(paint(RED)));
    assert_eq!(
        painted.len(),
        1,
        "one node paints red: {:#?}",
        frame.modifiers
    );
    assert_eq!(
        frame.modifiers_of(painted[0]),
        [
            Modifier::RequiredSize {
                width: 800.0,
                height: 600.0
            },
            Modifier::Background(paint(RED)),
        ]
    );
}

/// `<html>` with a transparent background and no image takes `<body>`'s, which is the
/// `samples/hello` case: its white stopped 100px down here (227px there) and now fills the
/// window. `<body>` does not paint it over its own box as well.
#[test]
fn fr34_body_background_propagates_when_html_has_none() {
    let mut dom = laid_out(short_page, BODY_WHITE);
    let list = dom.display_list().expect("laid out above").clone();
    let body = tagged(&list, "body");
    assert_rect(
        body.rect,
        Rect::new(0.0, 0.0, 800.0, 100.0),
        "the body's box",
    );

    let canvas = list.canvas.as_ref().expect("the body has a background");
    assert_eq!(canvas.source, body.node, "taken from <body>");
    assert_eq!(canvas.background, Some(WHITE));
    assert!(covers(canvas.area, Rect::new(0.0, 0.0, 800.0, 600.0)));
    assert_eq!(
        body.background, None,
        "the body's box does not paint it again"
    );
    assert_eq!(tagged(&list, "html").background, None);

    assert_eq!(
        page_backgrounds(&mut dom),
        [PlanModifier::Background(WHITE)]
    );
    assert!(node_backgrounds(&mut dom, "body").is_empty());

    let frame = FirstFrame::of(short_page, body_white);
    let painted = frame.nodes_with(&Modifier::Background(paint(WHITE)));
    assert_eq!(
        painted.len(),
        1,
        "one node paints white: {:#?}",
        frame.modifiers
    );
    assert_eq!(
        frame.modifiers_of(painted[0])[0],
        Modifier::RequiredSize {
            width: 800.0,
            height: 600.0
        },
        "the node that paints white is the window-sized page"
    );
}

/// When `<html>` has a background of its own, that is the canvas's, and `<body>` keeps
/// its own background over its own box.
#[test]
fn fr34_html_background_wins_over_body() {
    let css =
        "html { background-color: rgb(255, 0, 0); } body { background-color: rgb(0, 0, 255); }";
    let mut dom = laid_out(short_page, css);
    let list = dom.display_list().expect("laid out above").clone();
    let html = tagged(&list, "html");
    let body = tagged(&list, "body");

    let canvas = list.canvas.as_ref().expect("the root has a background");
    assert_eq!(canvas.source, html.node);
    assert_eq!(canvas.background, Some(RED));
    assert_eq!(html.background, None);
    assert_eq!(body.background, Some(BLUE), "the body paints its own box");

    assert_eq!(page_backgrounds(&mut dom), [PlanModifier::Background(RED)]);
    assert_eq!(
        node_backgrounds(&mut dom, "body"),
        [PlanModifier::Background(BLUE)]
    );
}

/// `<html>` with only an image, and no colour, still owns the canvas: `<body>`'s colour
/// stays on the body's box.
#[test]
fn fr34_html_image_alone_keeps_the_canvas_from_body() {
    let css = "html { background-image: linear-gradient(rgb(255, 0, 0), rgb(0, 0, 255)); } \
               body { background-color: rgb(0, 0, 255); }";
    let dom = laid_out(short_page, css);
    let list = dom.display_list().expect("laid out above");
    let canvas = list.canvas.as_ref().expect("the root has an image");
    assert_eq!(canvas.source, tagged(list, "html").node);
    assert_eq!(canvas.background, None);
    assert_eq!(canvas.backgrounds.len(), 1);
    assert_eq!(tagged(list, "body").background, Some(BLUE));
}

/// No background on either: the canvas stays transparent, as before, and the page node
/// paints nothing.
#[test]
fn fr34_canvas_stays_transparent_without_a_root_background() {
    let mut dom = laid_out(short_page, "#content { background-color: rgb(255, 0, 0); }");
    let list = dom.display_list().expect("laid out above").clone();
    assert_eq!(list.canvas, None);
    assert!(page_backgrounds(&mut dom).is_empty());
    assert_eq!(
        list.get(dom.element_by_id("content").expect("the block"))
            .expect("its entry")
            .background,
        Some(RED),
        "other boxes paint as before"
    );
}

/// On a page taller than the window, the canvas covers the whole document, and the page
/// node that paints it is the document's height.
#[test]
fn fr34_root_background_covers_the_whole_document() {
    let dom = laid_out(tall_page, BODY_WHITE);
    let list = dom.display_list().expect("laid out above");
    let canvas = list.canvas.as_ref().expect("the body has a background");
    assert!(
        covers(canvas.area, Rect::new(0.0, 0.0, 800.0, 1500.0)),
        "the canvas covers the document: {:?}",
        canvas.area
    );

    let frame = FirstFrame::of(tall_page, body_white);
    let painted = frame.nodes_with(&Modifier::Background(paint(WHITE)));
    assert_eq!(painted.len(), 1, "{:#?}", frame.modifiers);
    assert_eq!(
        frame.modifiers_of(painted[0])[0],
        Modifier::RequiredSize {
            width: 800.0,
            height: 1500.0
        }
    );
}

/// A gradient on the root is sized against the root's box (800 by 100 here) and repeats
/// over the whole canvas; the root's box does not draw it.
#[test]
fn fr34_root_gradient_paints_the_canvas() {
    let mut dom = laid_out(short_page, ROOT_GRADIENT);
    let list = dom.display_list().expect("laid out above").clone();
    let html = tagged(&list, "html");
    assert!(html.backgrounds.is_empty(), "{:#?}", html.backgrounds);

    let canvas = list.canvas.as_ref().expect("the root has an image");
    assert_eq!(canvas.source, html.node);
    assert_eq!(canvas.background, None);
    assert_eq!(canvas.backgrounds.len(), 1, "{:#?}", canvas.backgrounds);
    let layer = &canvas.backgrounds[0];
    assert!(
        matches!(layer.image, BackgroundImage::Linear(_)),
        "{layer:#?}"
    );
    assert_rect(
        layer.tile,
        Rect::new(0.0, 0.0, 800.0, 100.0),
        "the tile is the root's box",
    );
    assert!(
        covers(layer.area, Rect::new(0.0, 0.0, 800.0, 600.0)),
        "the painting area is the canvas: {:?}",
        layer.area
    );
    assert_eq!(layer.repeat_y, TileRepeat::Repeat);

    let page = page_backgrounds(&mut dom);
    assert_eq!(page.len(), 1, "{page:#?}");
    assert!(matches!(page[0], PlanModifier::BackgroundBrush { .. }));
    assert!(node_backgrounds(&mut dom, "html").is_empty());

    // The renderer gets one layer the page's size, inside the page, holding six 100px
    // tiles down the window.
    let frame = FirstFrame::of(short_page, root_gradient);
    let window = Modifier::RequiredSize {
        width: 800.0,
        height: 600.0,
    };
    let sized = frame.nodes_with(&window);
    let layer = sized
        .iter()
        .copied()
        .find(|node| {
            frame
                .parent_of(*node)
                .is_some_and(|parent| sized.contains(&parent))
        })
        .unwrap_or_else(|| {
            panic!(
                "a window-sized layer inside the page: {:#?}",
                frame.modifiers
            )
        });
    let tiles: Vec<u32> = frame
        .inserts
        .iter()
        .filter(|(parent, _, _)| *parent == layer)
        .map(|(_, child, _)| *child)
        .collect();
    assert_eq!(tiles.len(), 6, "{:#?}", frame.inserts);
}

/// A background image on `<body>` that moves to the canvas is placed against the root
/// element's box, as if `<html>` had declared it: with a 20px margin on the body, the tile
/// still starts at the root's corner.
#[test]
fn fr34_body_gradient_is_placed_against_the_root() {
    let css = "body { margin: 20px; \
               background-image: linear-gradient(rgb(255, 0, 0), rgb(0, 0, 255)); }";
    let dom = laid_out(short_page, css);
    let list = dom.display_list().expect("laid out above");
    let html = tagged(list, "html");
    let body = tagged(list, "body");
    assert_eq!(
        (body.rect.x, body.rect.width),
        (20.0, 760.0),
        "the body's box"
    );
    assert_eq!(
        (html.rect.x, html.rect.width),
        (0.0, 800.0),
        "the root's box"
    );
    assert!(body.backgrounds.is_empty(), "{:#?}", body.backgrounds);

    let canvas = list.canvas.as_ref().expect("the body has an image");
    assert_eq!(canvas.source, body.node);
    assert_eq!(canvas.backgrounds.len(), 1);
    assert_rect(
        canvas.backgrounds[0].tile,
        html.rect,
        "the tile is the root's box",
    );
}

/// A `@keyframes` animation of `<body>`'s background colour plays on the canvas, where the
/// colour is drawn, rather than on the body's box, which no longer draws it.
#[test]
fn fr34_body_background_animation_plays_on_the_canvas() {
    let mut dom = laid_out(short_page, BODY_GLOW);
    let list = dom.display_list().expect("laid out above").clone();
    let body = tagged(&list, "body");
    assert_eq!(
        list.canvas.as_ref().map(|canvas| canvas.source),
        Some(body.node)
    );

    let plan = dom.plan(&mut LiteralColours).expect("laid out above");
    assert!(
        plan.root
            .animations
            .iter()
            .any(|animation| animation.property == AnimatedProperty::BackgroundColor),
        "the page plays the colour: {:#?}",
        plan.root.animations
    );
    let on_body = plan
        .find(PlanKey::Node(body.node))
        .map(|node| node.animations.clone())
        .unwrap_or_default();
    assert!(
        on_body
            .iter()
            .all(|animation| animation.property != AnimatedProperty::BackgroundColor),
        "the body's box does not: {on_body:#?}"
    );

    let frame = FirstFrame::of(short_page, body_glow);
    let page = frame.nodes_with(&Modifier::RequiredSize {
        width: 800.0,
        height: 600.0,
    });
    assert_eq!(page.len(), 1, "{:#?}", frame.modifiers);
    assert_eq!(
        frame.animated, page,
        "the animation starts on the page node"
    );
}
