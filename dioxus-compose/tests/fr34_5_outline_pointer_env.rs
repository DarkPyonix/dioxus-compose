//! The workbench CSS the Host can finish alone: `outline`, `pointer-events: none` and
//! `env()`. Expected numbers follow from the CSS in each fixture, never from the layout.

use blitz_dom::DocumentConfig;
use blitz_html::HtmlDocument;
use blitz_traits::shell::{ColorScheme, Viewport};
use dioxus_compose::html::prelude::*;
use dioxus_compose::html::{
    BaseDocument, HtmlConfig, HtmlDom, PlanKey, PlanModifier, Rect, Rgba, Sides,
    TextMeasureRequest, TextMeasurer, TextMetrics, WidthConstraint, element_by_id, layout_document,
    plan_from,
};

struct FakeMeasurer;

impl TextMeasurer for FakeMeasurer {
    fn measure(&mut self, request: &TextMeasureRequest<'_>) -> TextMetrics {
        let chars = request.text.chars().count() as f32;
        let width = match request.width {
            WidthConstraint::AtMost(limit) => (chars * 10.0).min(limit),
            _ => chars * 10.0,
        };
        TextMetrics {
            width,
            height: 20.0,
            first_baseline: 15.0,
            line_count: 1,
        }
    }
}

fn open(html: &str) -> BaseDocument {
    let config = DocumentConfig {
        viewport: Some(Viewport::new(800, 600, 1.0, ColorScheme::Light)),
        ..DocumentConfig::default()
    };
    HtmlDocument::from_html(html, config).into_inner()
}

const BLUE: Rgba = Rgba::new(0, 0, 255, 255);

fn outline_modifiers(html: &str, id: &str) -> Vec<PlanModifier> {
    let mut doc = open(html);
    let list = layout_document(&mut doc, &mut FakeMeasurer);
    let plan = plan_from(&list);
    let node = element_by_id(&doc, id).unwrap_or_else(|| panic!("no element with id {id}"));
    plan.find(PlanKey::Outline(node))
        .unwrap_or_else(|| panic!("#{id} has no outline in the plan:\n{plan:#?}"))
        .modifiers
        .clone()
}

#[test]
fn fr34_5_outline_drawn_outside_border_box() {
    let html = r#"<!DOCTYPE html><html><head><style>
html, body { margin: 0; }
#a { position: absolute; left: 50px; top: 40px; width: 100px; height: 50px;
     outline: 3px solid rgb(0, 0, 255); outline-offset: 2px; }
#b { position: absolute; left: 300px; top: 40px; width: 100px; height: 50px;
     border-radius: 10px; outline: 2px solid rgb(0, 0, 255); }
</style></head><body><div id="a"></div><div id="b"></div></body></html>"#;

    // The line sits 2px outside the border box and is 3px wide, so the box that carries
    // it reaches 5px past every edge of the 100 by 50 box at (50, 40).
    let a = outline_modifiers(html, "a");
    assert!(
        a.contains(&PlanModifier::Offset { x: 45.0, y: 35.0 }),
        "{a:?}"
    );
    assert!(
        a.contains(&PlanModifier::RequiredSize {
            width: 110.0,
            height: 60.0
        }),
        "{a:?}"
    );
    assert!(
        a.contains(&PlanModifier::Border {
            width: 3.0,
            color: BLUE
        }),
        "{a:?}"
    );

    // The outline takes no room: the box keeps its CSS size.
    let mut doc = open(html);
    let list = layout_document(&mut doc, &mut FakeMeasurer);
    let entry = list.get(element_by_id(&doc, "a").unwrap()).unwrap();
    assert_eq!((entry.rect.width, entry.rect.height), (100.0, 50.0));

    // The outline follows the border radius: the outer edge is the radius plus the line.
    let b = outline_modifiers(html, "b");
    assert!(b.contains(&PlanModifier::Shape { radius: 12.0 }), "{b:?}");
}

#[test]
fn fr34_5_outline_negative_offset() {
    let html = r#"<!DOCTYPE html><html><head><style>
html, body { margin: 0; }
#a { position: absolute; left: 50px; top: 40px; width: 100px; height: 50px;
     outline: 2px solid rgb(0, 0, 255); outline-offset: -4px; }
</style></head><body><div id="a"></div></body></html>"#;

    // The inner edge of the line is 4px inside the border box and the line is 2px wide,
    // so the box that carries it is 2px inside every edge.
    let a = outline_modifiers(html, "a");
    assert!(
        a.contains(&PlanModifier::Offset { x: 52.0, y: 42.0 }),
        "{a:?}"
    );
    assert!(
        a.contains(&PlanModifier::RequiredSize {
            width: 96.0,
            height: 46.0
        }),
        "{a:?}"
    );
}

#[test]
fn fr34_5_pointer_events_none_skips_hit() {
    let html = r#"<!DOCTYPE html><html><head><style>
html, body { margin: 0; }
#outer { position: absolute; left: 0; top: 0; width: 100px; height: 100px;
         pointer-events: none; }
#inner { position: absolute; left: 0; top: 0; width: 20px; height: 20px;
         pointer-events: auto; }
#plain { position: absolute; left: 200px; top: 0; width: 50px; height: 50px; }
</style></head><body>
<div id="outer"><div id="inner"></div></div><div id="plain"></div>
</body></html>"#;
    let mut doc = open(html);
    let list = layout_document(&mut doc, &mut FakeMeasurer);
    let outer = element_by_id(&doc, "outer").unwrap();
    let inner = element_by_id(&doc, "inner").unwrap();
    let plain = element_by_id(&doc, "plain").unwrap();

    // A point on the child that opts back in hits the child.
    assert_eq!(list.hit_test(10.0, 10.0), Some(inner));
    // A point on the box alone skips it.
    assert_ne!(list.hit_test(60.0, 60.0), Some(outer));
    // A box that does not set the property is hit as before.
    assert_eq!(list.hit_test(225.0, 25.0), Some(plain));
}

fn titlebar_app() -> Element {
    rsx! {
        div { id: "bar" }
    }
}

fn bar_rect(config: HtmlConfig) -> Rect {
    let mut dom = HtmlDom::with_config(titlebar_app, config);
    let list = dom.layout(800.0, 600.0, 1.0).clone();
    let node = dom.element_by_id("bar").expect("the bar exists");
    list.get(node).expect("the bar has an entry").rect
}

const BAR_CSS: &str = "html, body { margin: 0; } #bar { position: absolute; \
    left: env(titlebar-area-x, 1px); top: env(titlebar-area-y, 2px); \
    width: env(titlebar-area-width, 111px); height: env(titlebar-area-height, 10px); }";

#[test]
fn fr34_5_env_titlebar_area_resolves() {
    // With a title bar area, the four variables are its values.
    let rect = bar_rect(HtmlConfig {
        stylesheets: vec![BAR_CSS.to_string()],
        measurer: Some(Box::new(FakeMeasurer)),
        titlebar_area: Some(Rect::new(70.0, 4.0, 300.0, 32.0)),
        ..HtmlConfig::default()
    });
    assert_eq!(rect, Rect::new(70.0, 4.0, 300.0, 32.0));

    // Without one, each `env()` takes its CSS fallback.
    let rect = bar_rect(HtmlConfig {
        stylesheets: vec![BAR_CSS.to_string()],
        measurer: Some(Box::new(FakeMeasurer)),
        ..HtmlConfig::default()
    });
    assert_eq!(rect, Rect::new(1.0, 2.0, 111.0, 10.0));
}

#[test]
fn fr34_5_env_safe_area_insets_resolve() {
    let css = "html, body { margin: 0; } #bar { position: absolute; \
        left: env(safe-area-inset-left, 0px); top: env(SAFE-AREA-INSET-TOP); \
        width: calc(100px + env(safe-area-inset-right, 0px)); \
        height: env(safe-area-inset-bottom, 9px); }";
    let rect = bar_rect(HtmlConfig {
        stylesheets: vec![css.to_string()],
        measurer: Some(Box::new(FakeMeasurer)),
        safe_area_insets: Some(Sides {
            top: 20.0,
            right: 5.0,
            bottom: 34.0,
            left: 7.0,
        }),
        ..HtmlConfig::default()
    });
    assert_eq!(rect, Rect::new(7.0, 20.0, 105.0, 34.0));
}
