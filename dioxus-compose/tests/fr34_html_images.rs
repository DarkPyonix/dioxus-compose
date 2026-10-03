//! Images in HTML screens: `<img>` elements and CSS background images and gradients, in
//! the display list and in the plan.
//!
//! Nothing here is ever fetched. The application answers for images through an
//! `ImageResolver`; these tests use one that knows a fixed list of URLs and records every
//! question it is asked. Expected numbers follow from the CSS in each fixture and the
//! natural sizes the resolver answers, worked out by hand.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use blitz_dom::DocumentConfig;
use blitz_dom::node::ImageData;
use blitz_html::HtmlDocument;
use blitz_traits::net::Url;
use blitz_traits::shell::{ColorScheme, Viewport};
use dioxus_compose::html::prelude::*;
use dioxus_compose::html::{
    AssetId, BackgroundImage, BackgroundLayer, BaseDocument, DisplayList, GradientStop, HtmlConfig,
    HtmlDom, ImageResolver, ImageSize, LiteralColours, ModifierSlot, NodeEntry, PlanChange,
    PlanKey, PlanKind, PlanModifier, Rect, Rgba, TextMeasureRequest, TextMeasurer, TextMetrics,
    TileRepeat, diff, element_by_id, layout_document_with, plan_from_images,
};
use dioxus_core::ScopeId;

/// Every character is ten pixels wide and every line twenty tall. These fixtures hold no
/// text worth measuring; the measurer only keeps Parley and the system's fonts out of it.
struct FakeMeasurer;

impl TextMeasurer for FakeMeasurer {
    fn measure(&mut self, request: &TextMeasureRequest<'_>) -> TextMetrics {
        TextMetrics {
            width: request.text.chars().count() as f32 * 10.0,
            height: 20.0,
            first_baseline: 15.0,
            line_count: 1,
        }
    }
}

/// Every question an [`ImageResolver`] was asked, in order.
#[derive(Default, Debug)]
struct Questions {
    sizes: Vec<String>,
    assets: Vec<String>,
}

/// Knows a fixed set of URLs, each with a natural size and an asset, and writes down every
/// question.
#[derive(Clone)]
struct KnownImages {
    known: Vec<(&'static str, ImageSize, AssetId)>,
    questions: Rc<RefCell<Questions>>,
}

impl KnownImages {
    fn new(known: Vec<(&'static str, ImageSize, AssetId)>) -> Self {
        Self {
            known,
            questions: Rc::new(RefCell::new(Questions::default())),
        }
    }

    fn find(&self, url: &str) -> Option<&(&'static str, ImageSize, AssetId)> {
        self.known.iter().find(|(known, _, _)| *known == url)
    }
}

impl ImageResolver for KnownImages {
    fn resolve(&mut self, url: &str) -> Option<AssetId> {
        self.questions.borrow_mut().assets.push(url.to_string());
        self.find(url).map(|(_, _, asset)| *asset)
    }

    fn size(&mut self, url: &str) -> Option<ImageSize> {
        self.questions.borrow_mut().sizes.push(url.to_string());
        self.find(url).map(|(_, size, _)| *size)
    }
}

/// Knows no image at all, and writes down every question.
struct NoImages {
    questions: Rc<RefCell<Questions>>,
}

impl ImageResolver for NoImages {
    fn resolve(&mut self, url: &str) -> Option<AssetId> {
        self.questions.borrow_mut().assets.push(url.to_string());
        None
    }

    fn size(&mut self, url: &str) -> Option<ImageSize> {
        self.questions.borrow_mut().sizes.push(url.to_string());
        None
    }
}

const RED: Rgba = Rgba::new(255, 0, 0, 255);
const LIME: Rgba = Rgba::new(0, 255, 0, 255);
const BLUE: Rgba = Rgba::new(0, 0, 255, 255);

#[track_caller]
fn assert_close(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() < 0.01,
        "{what}: expected {expected}, got {actual}"
    );
}

#[track_caller]
fn assert_rect(actual: Rect, expected: Rect, what: &str) {
    assert_close(actual.x, expected.x, &format!("{what} x"));
    assert_close(actual.y, expected.y, &format!("{what} y"));
    assert_close(actual.width, expected.width, &format!("{what} width"));
    assert_close(actual.height, expected.height, &format!("{what} height"));
}

#[track_caller]
fn assert_stops(actual: &[GradientStop], expected: &[(f32, Rgba)]) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "stops: expected {expected:?}, got {actual:?}"
    );
    for (stop, (offset, color)) in actual.iter().zip(expected) {
        assert_close(stop.offset, *offset, "stop offset");
        assert_eq!(stop.color, *color, "stop colour at {offset}");
    }
}

/// Parses and lays out `html` in an 800 by 600 viewport, with natural sizes from `images`.
fn layout_html(html: &str, images: Option<&mut dyn ImageResolver>) -> (BaseDocument, DisplayList) {
    let config = DocumentConfig {
        viewport: Some(Viewport::new(800, 600, 1.0, ColorScheme::Light)),
        ..DocumentConfig::default()
    };
    let mut doc = HtmlDocument::from_html(html, config).into_inner();
    let list = layout_document_with(&mut doc, &mut FakeMeasurer, images, None);
    (doc, list)
}

#[track_caller]
fn entry<'a>(list: &'a DisplayList, doc: &BaseDocument, id: &str) -> &'a NodeEntry {
    let node = element_by_id(doc, id).unwrap_or_else(|| panic!("no element with id {id}"));
    list.get(node)
        .unwrap_or_else(|| panic!("#{id} is not in the display list"))
}

#[track_caller]
fn only_layer(entry: &NodeEntry) -> &BackgroundLayer {
    assert_eq!(
        entry.backgrounds.len(),
        1,
        "one background layer: {:#?}",
        entry.backgrounds
    );
    &entry.backgrounds[0]
}

fn margin_free(images: Option<Box<dyn ImageResolver>>) -> HtmlConfig {
    HtmlConfig {
        stylesheets: vec!["html, body { margin: 0; padding: 0; }".to_string()],
        measurer: Some(Box::new(FakeMeasurer)),
        images,
        ..HtmlConfig::default()
    }
}

const LOGO: &str = "images/logo.png";

fn logo() -> Element {
    rsx! {
        div { style: "display: flex; align-items: flex-start",
            img { id: "logo", src: "images/logo.png" }
        }
    }
}

/// The resolver's answer for the natural size is what layout sizes an `<img>` by, and the
/// asset it names is what the plan draws.
#[test]
fn fr34_img_with_resolver_size_is_laid_out_at_it_and_planned_as_that_asset() {
    let images = KnownImages::new(vec![(LOGO, ImageSize::new(40, 30), AssetId(7))]);
    let questions = images.questions.clone();
    let mut dom = HtmlDom::with_config(logo, margin_free(Some(Box::new(images))));
    dom.layout(400.0, 300.0, 1.0);
    let logo = dom.element_by_id("logo").unwrap();

    let list = dom.display_list().unwrap();
    let entry = list.get(logo).expect("the <img> is in the display list");
    assert_rect(entry.rect, Rect::new(0.0, 0.0, 40.0, 30.0), "<img> box");
    let image = entry
        .image
        .as_ref()
        .expect("an <img> with a src has an image");
    assert_eq!(
        image.source, LOGO,
        "a relative src without a base is as written"
    );
    assert_eq!(image.natural_size, Some((40.0, 30.0)));
    assert_rect(
        image.image_rect,
        Rect::new(0.0, 0.0, 40.0, 30.0),
        "drawn image",
    );
    assert!(
        questions.borrow().assets.is_empty(),
        "layout never asks for an asset: {:?}",
        questions.borrow()
    );
    assert!(questions.borrow().sizes.iter().any(|url| url == LOGO));

    let plan = dom.plan(&mut LiteralColours).expect("laid out above");
    let node = plan
        .find(PlanKey::Image(logo))
        .unwrap_or_else(|| panic!("the <img> draws an image node:\n{plan:#?}"));
    let PlanKind::Image(drawn) = &node.kind else {
        panic!("an <img> is an Image: {node:#?}");
    };
    assert_eq!(drawn.asset, AssetId(7));
    assert_eq!(drawn.source, LOGO);
    assert_rect(
        drawn.draw,
        Rect::new(0.0, 0.0, 40.0, 30.0),
        "image in its node",
    );
    assert_eq!(
        node.modifier(ModifierSlot::RequiredSize),
        Some(&PlanModifier::RequiredSize {
            width: 40.0,
            height: 30.0
        })
    );
    assert_eq!(node.modifier(ModifierSlot::Clip), None);
    assert_eq!(
        plan.parent_of(PlanKey::Image(logo))
            .map(|parent| parent.key),
        Some(PlanKey::Node(logo))
    );
    assert_eq!(questions.borrow().assets, vec![LOGO.to_string()]);
}

/// A file on disk that is a real PNG, named by an absolute `file:` URL: anything that
/// fetched it would decode it and give the `<img>` its natural size.
fn real_png_url() -> String {
    let path = std::fs::canonicalize(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../samples/chat/assets/icon.png"
    ))
    .expect("the chat sample's icon is checked in");
    Url::from_file_path(&path)
        .expect("an absolute path")
        .to_string()
}

fn real_png() -> Element {
    let src = real_png_url();
    rsx! {
        div { style: "display: flex; align-items: flex-start",
            img { id: "icon", src: "{src}" }
        }
    }
}

/// Without a resolver an `<img>` has no natural size, so it takes the size layout gives
/// it, and nothing reads the file its `src` names. A resolver handed to the plan later is
/// asked for the asset once and never for a size: layout did not have it.
#[test]
fn fr34_img_without_resolver_draws_nothing_and_fetches_nothing() {
    let url = real_png_url();
    let mut dom = HtmlDom::with_config(real_png, margin_free(None));
    dom.layout(400.0, 300.0, 1.0);
    let icon = dom.element_by_id("icon").unwrap();

    let element = dom
        .document()
        .get_node(icon)
        .and_then(|node| node.element_data())
        .expect("the <img> is an element");
    assert!(
        matches!(element.image_data(), None | Some(ImageData::None)),
        "no image was loaded into the document"
    );

    let plan = dom.plan(&mut LiteralColours).unwrap();
    assert!(
        plan.find(PlanKey::Image(icon)).is_none(),
        "without a resolver the image draws nothing:\n{plan:#?}"
    );

    let list = dom.display_list().unwrap();
    let entry = list.get(icon).expect("the <img> is in the display list");
    assert_rect(
        entry.rect,
        Rect::new(0.0, 0.0, 0.0, 0.0),
        "<img> with no size",
    );
    let image = entry.image.as_ref().expect("the src is still recorded");
    assert_eq!(image.source, url);
    assert_eq!(image.natural_size, None);

    let questions = Rc::new(RefCell::new(Questions::default()));
    let mut none = NoImages {
        questions: questions.clone(),
    };
    let plan = plan_from_images(list, &mut LiteralColours, Some(&mut none));
    assert!(plan.find(PlanKey::Image(icon)).is_none());
    assert!(
        questions.borrow().sizes.is_empty(),
        "nothing asked for a size during layout: {:?}",
        questions.borrow()
    );
    assert_eq!(questions.borrow().assets, vec![url]);
}

#[test]
fn fr34_linear_gradient_background_is_recorded_with_resolved_stops_and_angle() {
    let (doc, list) = layout_html(
        r#"<!DOCTYPE html><html><head><style>
html, body { margin: 0; }
div { height: 100px; }
#to-right { width: 200px;
  background-image: linear-gradient(to right, red, blue 25%, lime); }
#angle { width: 100px; background-image: linear-gradient(45deg, red, lime, blue); }
#corner { width: 200px; background-image: linear-gradient(to top right, red, blue); }
</style></head><body><div id="to-right"></div><div id="angle"></div><div id="corner"></div>
</body></html>"#,
        None,
    );

    let layer = only_layer(entry(&list, &doc, "to-right"));
    assert_rect(
        layer.area,
        Rect::new(0.0, 0.0, 200.0, 100.0),
        "painting area",
    );
    assert_rect(layer.tile, Rect::new(0.0, 0.0, 200.0, 100.0), "tile");
    assert_eq!(layer.repeat_x, TileRepeat::Repeat);
    let BackgroundImage::Linear(gradient) = &layer.image else {
        panic!("a linear gradient: {layer:#?}");
    };
    assert_close(gradient.angle, 90.0, "to right");
    assert_close(gradient.start.0, 0.0, "start x");
    assert_close(gradient.start.1, 50.0, "start y");
    assert_close(gradient.end.0, 200.0, "end x");
    assert_close(gradient.end.1, 50.0, "end y");
    assert!(!gradient.repeating);
    assert_stops(&gradient.stops, &[(0.0, RED), (0.25, BLUE), (1.0, LIME)]);

    // 45deg in a square: the line runs corner to corner, sqrt(2) * 100 long, and the
    // middle stop without a position goes halfway.
    let layer = only_layer(entry(&list, &doc, "angle"));
    let BackgroundImage::Linear(gradient) = &layer.image else {
        panic!("a linear gradient: {layer:#?}");
    };
    assert_close(gradient.angle, 45.0, "45deg");
    assert_close(gradient.start.0, 0.0, "start x");
    assert_close(gradient.start.1, 100.0, "start y");
    assert_close(gradient.end.0, 100.0, "end x");
    assert_close(gradient.end.1, 0.0, "end y");
    assert_stops(&gradient.stops, &[(0.0, RED), (0.5, LIME), (1.0, BLUE)]);

    // Towards a corner of a 200 by 100 box: perpendicular to the other diagonal, so
    // atan(100 / 200) from vertical.
    let layer = only_layer(entry(&list, &doc, "corner"));
    let BackgroundImage::Linear(gradient) = &layer.image else {
        panic!("a linear gradient: {layer:#?}");
    };
    assert_close(gradient.angle, 26.565_052, "to top right");
}

#[test]
fn fr34_radial_gradient_background_resolves_centre_and_radii() {
    let (doc, list) = layout_html(
        r#"<!DOCTYPE html><html><head><style>
html, body { margin: 0; }
div { width: 200px; height: 100px; }
#circle { background-image: radial-gradient(circle closest-side at 30px 40px, red, blue); }
#ellipse { background-image: radial-gradient(red, blue); }
</style></head><body><div id="circle"></div><div id="ellipse"></div></body></html>"#,
        None,
    );

    // The closest side to (30, 40) is the left edge, 30 away.
    let layer = only_layer(entry(&list, &doc, "circle"));
    let BackgroundImage::Radial(gradient) = &layer.image else {
        panic!("a radial gradient: {layer:#?}");
    };
    assert!(gradient.circle);
    assert_close(gradient.center.0, 30.0, "centre x");
    assert_close(gradient.center.1, 40.0, "centre y");
    assert_close(gradient.radius_x, 30.0, "radius");
    assert_close(gradient.radius_y, 30.0, "radius");
    assert_stops(&gradient.stops, &[(0.0, RED), (1.0, BLUE)]);

    // The default ellipse reaches the farthest corner with the proportions of the
    // farthest sides, 100 by 50: radii 100 * sqrt(2) by 50 * sqrt(2).
    let layer = only_layer(entry(&list, &doc, "ellipse"));
    let BackgroundImage::Radial(gradient) = &layer.image else {
        panic!("a radial gradient: {layer:#?}");
    };
    assert!(!gradient.circle);
    assert_close(gradient.center.0, 100.0, "centre x");
    assert_close(gradient.center.1, 50.0, "centre y");
    assert_close(gradient.radius_x, 141.421_36, "radius x");
    assert_close(gradient.radius_y, 70.710_68, "radius y");
}

#[test]
fn fr34_background_size_cover_contain_and_position_resolve_to_rects() {
    let mut images = KnownImages::new(vec![("tile.png", ImageSize::new(100, 50), AssetId(3))]);
    let (doc, list) = layout_html(
        r#"<!DOCTYPE html><html><head><style>
html, body { margin: 0; }
div { position: absolute; left: 0; top: 0; width: 300px; height: 300px;
  background-image: url(tile.png); }
#cover { background-size: cover; background-position: center; }
#contain { background-size: contain; background-position: center; background-repeat: no-repeat; }
#corner { background-size: 50px auto; background-position: right 10px bottom 20px; }
#boxes { width: 100px; height: 100px; padding: 10px; border: 5px solid black;
  background-size: contain; background-origin: content-box; background-clip: padding-box;
  background-repeat: no-repeat; }
</style></head><body><div id="cover"></div><div id="contain"></div><div id="corner"></div>
<div id="boxes"></div></body></html>"#,
        Some(&mut images),
    );

    // 100 by 50 covering 300 by 300 scales by 6 to 600 by 300, centred: 150 off each side.
    let layer = only_layer(entry(&list, &doc, "cover"));
    assert_eq!(layer.image, BackgroundImage::Url("tile.png".to_string()));
    assert_rect(layer.area, Rect::new(0.0, 0.0, 300.0, 300.0), "cover area");
    assert_rect(
        layer.tile,
        Rect::new(-150.0, 0.0, 600.0, 300.0),
        "cover tile",
    );
    assert_eq!(layer.repeat_x, TileRepeat::Repeat);
    assert_eq!(layer.repeat_y, TileRepeat::Repeat);

    // Contained it scales by 3 to 300 by 150, centred: 75 from the top.
    let layer = only_layer(entry(&list, &doc, "contain"));
    assert_rect(
        layer.tile,
        Rect::new(0.0, 75.0, 300.0, 150.0),
        "contain tile",
    );
    assert_eq!(layer.repeat_x, TileRepeat::NoRepeat);
    assert_eq!(layer.repeat_y, TileRepeat::NoRepeat);

    // 50 wide keeps the 2:1 ratio, 25 tall; 10 from the right and 20 from the bottom.
    let layer = only_layer(entry(&list, &doc, "corner"));
    assert_rect(
        layer.tile,
        Rect::new(240.0, 255.0, 50.0, 25.0),
        "positioned tile",
    );

    // Border box 130 by 130; padding box inset 5; content box inset 15. Contained in the
    // 100 by 100 content box at its top-left, painted only inside the padding box.
    let layer = only_layer(entry(&list, &doc, "boxes"));
    assert_rect(
        layer.area,
        Rect::new(5.0, 5.0, 120.0, 120.0),
        "padding-box clip",
    );
    assert_rect(
        layer.tile,
        Rect::new(15.0, 15.0, 100.0, 50.0),
        "content-box origin",
    );
}

fn fitted() -> Element {
    rsx! {
        div { style: "display: flex; align-items: flex-start",
            img { id: "contain", src: "images/logo.png", style: "width: 100px; height: 100px; object-fit: contain" }
            img { id: "cover", src: "images/logo.png", style: "width: 100px; height: 100px; object-fit: cover" }
        }
    }
}

#[test]
fn fr34_img_object_fit_resolves_where_the_image_is_drawn() {
    let images = KnownImages::new(vec![(LOGO, ImageSize::new(40, 30), AssetId(7))]);
    let mut dom = HtmlDom::with_config(fitted, margin_free(Some(Box::new(images))));
    dom.layout(400.0, 300.0, 1.0);
    let contain = dom.element_by_id("contain").unwrap();
    let cover = dom.element_by_id("cover").unwrap();
    let plan = dom.plan(&mut LiteralColours).unwrap();

    // 40 by 30 in 100 by 100: contain scales by 2.5 to 100 by 75, centred 12.5 down.
    let node = plan.find(PlanKey::Image(contain)).expect("contain draws");
    let PlanKind::Image(drawn) = &node.kind else {
        panic!("an Image: {node:#?}");
    };
    assert_rect(drawn.draw, Rect::new(0.0, 12.5, 100.0, 75.0), "contain");
    assert_eq!(node.modifier(ModifierSlot::Clip), None);

    // Cover scales by 10/3 to 133.33 by 100, centred 16.67 to the left, and is clipped.
    let node = plan.find(PlanKey::Image(cover)).expect("cover draws");
    let PlanKind::Image(drawn) = &node.kind else {
        panic!("an Image: {node:#?}");
    };
    assert_rect(
        drawn.draw,
        Rect::new(-16.666_666, 0.0, 133.333_33, 100.0),
        "cover",
    );
    assert_eq!(node.modifier(ModifierSlot::Clip), Some(&PlanModifier::Clip));
}

fn urls() -> Element {
    rsx! {
        div { style: "display: flex; align-items: flex-start",
            img { id: "relative", src: "img/a.png" }
            img { id: "rooted", src: "/b.png" }
            div { id: "background", style: "width: 10px; height: 10px; background-image: url(bg.png)" }
        }
    }
}

/// With a base URL, relative URLs are resolved against it; without one they reach the
/// application as written. Either way a relative `src` does not take the process down.
#[test]
fn fr34_image_urls_resolve_against_the_base_or_stay_as_written() {
    let mut dom = HtmlDom::with_config(urls, margin_free(None));
    let list = dom.layout(400.0, 300.0, 1.0).clone();
    let source = |dom: &HtmlDom, list: &DisplayList, id: &str| {
        let node = dom.element_by_id(id).unwrap();
        list.get(node)
            .and_then(|entry| entry.image.as_ref())
            .map(|image| image.source.clone())
    };
    let background = |dom: &HtmlDom, list: &DisplayList| {
        let node = dom.element_by_id("background").unwrap();
        list.get(node)
            .map(|entry| entry.backgrounds[0].image.clone())
    };
    assert_eq!(
        source(&dom, &list, "relative").as_deref(),
        Some("img/a.png")
    );
    assert_eq!(source(&dom, &list, "rooted").as_deref(), Some("/b.png"));
    assert_eq!(
        background(&dom, &list),
        Some(BackgroundImage::Url("bg.png".to_string()))
    );

    let mut dom = HtmlDom::with_config(
        urls,
        HtmlConfig {
            base_url: Some("https://example.com/app/".to_string()),
            ..margin_free(None)
        },
    );
    let list = dom.layout(400.0, 300.0, 1.0).clone();
    assert_eq!(
        source(&dom, &list, "relative").as_deref(),
        Some("https://example.com/app/img/a.png")
    );
    assert_eq!(
        source(&dom, &list, "rooted").as_deref(),
        Some("https://example.com/b.png")
    );
    assert_eq!(
        background(&dom, &list),
        Some(BackgroundImage::Url(
            "https://example.com/app/bg.png".to_string()
        ))
    );
}

thread_local! {
    static SECOND_STOP_IS_LIME: Cell<bool> = const { Cell::new(false) };
}

fn gradient_bar() -> Element {
    let second = if SECOND_STOP_IS_LIME.with(Cell::get) {
        "rgb(0, 255, 0)"
    } else {
        "rgb(0, 0, 255)"
    };
    rsx! {
        div { id: "first", style: "width: 100px; height: 20px; background-color: rgb(0, 0, 0)" }
        div {
            id: "bar",
            style: "width: 100px; height: 20px; background-color: rgb(0, 0, 0); background-image: linear-gradient(to right, rgb(255, 0, 0), {second})",
        }
    }
}

/// Changing one stop of a gradient is one change: the brush the node's background slot
/// names. The background colour, the box and every other node stay as they were, and the
/// new brush is the only one the renderer has not registered yet.
#[test]
fn fr34_gradient_stop_colour_change_is_one_plan_change() {
    SECOND_STOP_IS_LIME.with(|lime| lime.set(false));
    let mut dom = HtmlDom::with_config(gradient_bar, margin_free(None));
    dom.layout(400.0, 300.0, 1.0);
    let before = dom.plan(&mut LiteralColours).unwrap();
    let bar = dom.element_by_id("bar").unwrap();
    assert_eq!(before.brushes.len(), 1, "one gradient, one brush");
    assert!(
        matches!(
            before
                .find(PlanKey::Node(bar))
                .and_then(|node| node.modifier(ModifierSlot::Background)),
            Some(PlanModifier::Background(_))
        ),
        "the colour stays a plain Background under the gradient"
    );

    SECOND_STOP_IS_LIME.with(|lime| lime.set(true));
    dom.virtual_dom_mut().mark_dirty(ScopeId::APP);
    dom.render();
    dom.layout(400.0, 300.0, 1.0);
    let after = dom.plan(&mut LiteralColours).unwrap();

    let changes = diff(&before, &after);
    assert_eq!(changes.len(), 1, "one change: {changes:#?}");
    let PlanChange::ModifierSet {
        key,
        slot,
        modifier:
            PlanModifier::BackgroundBrush {
                brush,
                area,
                tile,
                repeat_x,
                repeat_y,
            },
    } = &changes[0]
    else {
        panic!("the change sets the background brush: {changes:#?}");
    };
    assert_eq!(*key, PlanKey::Node(bar));
    assert_eq!(*slot, ModifierSlot::BackgroundBrush(0));
    assert_rect(
        *area,
        Rect::new(0.0, 0.0, 100.0, 20.0),
        "area, from the node",
    );
    assert_rect(
        *tile,
        Rect::new(0.0, 0.0, 100.0, 20.0),
        "tile, from the node",
    );
    assert_eq!(
        (*repeat_x, *repeat_y),
        (TileRepeat::Repeat, TileRepeat::Repeat)
    );

    let added = after.brushes_not_in(&before);
    assert_eq!(added.len(), 1, "one brush to register: {added:#?}");
    assert_eq!(added[0].0, *brush);
    let dioxus_compose::html::Brush::LinearGradient { stops, .. } = added[0].1 else {
        panic!("the new brush is the gradient: {:#?}", added[0].1);
    };
    assert_stops(stops, &[(0.0, RED), (1.0, LIME)]);
    assert_eq!(
        before.brushes_not_in(&after).len(),
        1,
        "the old gradient can be released"
    );
}
