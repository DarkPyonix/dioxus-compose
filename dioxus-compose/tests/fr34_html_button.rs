//! `<button>` laid out the way Chromium lays it out, and the font Chromium gives form
//! controls.
//!
//! Chromium puts a button's content in an anonymous box that is centred in the button's
//! content box, top to bottom, and its user-agent sheet sets `text-align: center` on
//! buttons, so a label sits in the middle of the button both ways. A button that is a flex
//! or grid container lays its children out as one, with no centring of its own. Buttons,
//! inputs, selects and text areas take `font: -webkit-small-control` from that sheet
//! (13.333px) instead of inheriting the page's font, unless the page sets theirs.
//!
//! Text is sized by a measurer that advances half the font size per character and makes a
//! line 1.25 times the font size when `line-height` is `normal`, and the CSS line height
//! otherwise.

use dioxus_compose::html::prelude::*;
use dioxus_compose::html::{
    HtmlConfig, HtmlDom, NodeEntry, NodeId, Rect, TextLineHeight, TextMeasureRequest, TextMeasurer,
    TextMetrics, TextRun, WidthConstraint,
};

/// The font size Chromium gives form controls: `-webkit-small-control`, 13.333px.
const CONTROL_FONT: f32 = 40.0 / 3.0;

struct HalfEm;

impl TextMeasurer for HalfEm {
    fn measure(&mut self, request: &TextMeasureRequest<'_>) -> TextMetrics {
        let style = request.style;
        let line = match style.line_height {
            TextLineHeight::Normal => style.font_size * 1.25,
            TextLineHeight::Px(height) => height,
        };
        let natural = request.text.chars().count() as f32 * style.font_size * 0.5;
        let width = match request.width {
            WidthConstraint::AtMost(width) => natural.min(width.max(0.0)),
            _ => natural,
        };
        TextMetrics {
            width,
            height: line,
            first_baseline: line * 0.8,
            line_count: 1,
        }
    }
}

fn open(app: fn() -> Element) -> HtmlDom {
    let config = HtmlConfig {
        stylesheets: vec![
            "html, body { margin: 0; padding: 0; } \
             body { font-family: serif; font-size: 16px; line-height: 24px; }"
                .to_string(),
        ],
        measurer: Some(Box::new(HalfEm)),
        ..HtmlConfig::default()
    };
    let mut dom = HtmlDom::with_config(app, config);
    dom.layout(400.0, 600.0, 1.0);
    dom
}

#[track_caller]
fn node(dom: &HtmlDom, id: &str) -> NodeId {
    dom.element_by_id(id)
        .unwrap_or_else(|| panic!("no element with id {id}"))
}

#[track_caller]
fn entry<'a>(dom: &'a HtmlDom, id: &str) -> &'a NodeEntry {
    let node = node(dom, id);
    dom.display_list()
        .expect("laid out")
        .get(node)
        .unwrap_or_else(|| panic!("#{id} has no display list entry"))
}

#[track_caller]
fn label<'a>(dom: &'a HtmlDom, id: &str) -> &'a TextRun {
    let entry = entry(dom, id);
    assert_eq!(entry.texts.len(), 1, "#{id} has one text run: {entry:#?}");
    &entry.texts[0]
}

#[track_caller]
fn close(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() < 0.01,
        "{what}: expected {expected}, got {actual}"
    );
}

#[track_caller]
fn rect_close(actual: Rect, expected: (f32, f32, f32, f32), what: &str) {
    let (x, y, width, height) = expected;
    close(actual.x, x, &format!("{what} x"));
    close(actual.y, y, &format!("{what} y"));
    close(actual.width, width, &format!("{what} width"));
    close(actual.height, height, &format!("{what} height"));
}

fn buttons() -> Element {
    rsx! {
        div {
            style: "display: flex; flex-direction: column; align-items: flex-start;",
            // 100 by 32 with no padding or border: the label is centred both ways.
            button {
                id: "fixed",
                style: "width: 100px; height: 32px; padding: 0; border: 0;",
                "Toggle"
            }
            // Sized by its label across, 40px tall: centred top to bottom inside the
            // padding and border.
            button {
                id: "padded",
                style: "height: 40px; padding: 4px 10px; border: 1px solid #000;",
                "Add"
            }
            // The page sets the button's font, so it is the page's.
            button {
                id: "inherit",
                style: "font: inherit; width: 100px; height: 40px; padding: 0; border: 0;",
                "Go"
            }
            // A flex container: its children are flex items, not centred by the button.
            button {
                id: "flex",
                style: "display: flex; align-items: flex-start; width: 100px; height: 32px; \
                        padding: 0; border: 0;",
                span { id: "flex-label", "Go" }
            }
        }
    }
}

/// A fixed-size button with no padding puts its one text run in the middle of its box:
/// "Toggle" at 13.333px is 6 * 6.667 = 40px wide and 16.667px tall, so it starts
/// (100 - 40) / 2 = 30px in and (32 - 16.667) / 2 = 7.667px down.
#[test]
fn fr34_button_centres_its_label_in_a_fixed_box() {
    let dom = open(buttons);
    let button = entry(&dom, "fixed").rect;
    rect_close(button, (0.0, 0.0, 100.0, 32.0), "#fixed");
    let run = label(&dom, "fixed");
    rect_close(
        run.rect,
        (
            30.0,
            (32.0 - CONTROL_FONT * 1.25) / 2.0,
            40.0,
            CONTROL_FONT * 1.25,
        ),
        "the label of #fixed",
    );
}

/// A button taller than its label centres the label inside its padding and border.
/// "Add" is 3 * 6.667 = 20px wide, so the button is 20 + 20 + 2 = 42px wide;
/// the label starts at 1 + 10 = 11 and, in the 40 - 10 = 30px tall content box,
/// (30 - 16.667) / 2 = 6.667px below its top at 5: at 32 + 5 + 6.667 = 43.667.
#[test]
fn fr34_button_centres_its_label_inside_padding() {
    let dom = open(buttons);
    let button = entry(&dom, "padded").rect;
    rect_close(button, (0.0, 32.0, 42.0, 40.0), "#padded");
    let line = CONTROL_FONT * 1.25;
    let run = label(&dom, "padded");
    rect_close(
        run.rect,
        (11.0, 32.0 + 5.0 + (30.0 - line) / 2.0, 20.0, line),
        "the label of #padded",
    );
}

/// Buttons take Chromium's control font, 13.333px with a `normal` line height, rather than
/// the body's 16px on 24px lines; a button whose font the page sets keeps the page's.
#[test]
fn fr34_button_takes_the_control_font_unless_the_page_sets_it() {
    let dom = open(buttons);
    let run = label(&dom, "fixed");
    close(run.style.font_size, CONTROL_FONT, "the button's font size");
    assert_eq!(run.style.line_height, TextLineHeight::Normal);
    assert!(
        !run.style.font_family.iter().any(|family| family == "serif"),
        "the button does not inherit the body's font family: {:?}",
        run.style.font_family
    );

    let inherited = label(&dom, "inherit");
    close(inherited.style.font_size, 16.0, "font: inherit");
    assert_eq!(inherited.style.line_height, TextLineHeight::Px(24.0));
    // "Go" at 16px is 16px wide on a 24px line, centred in 100 by 40.
    rect_close(
        inherited.rect,
        (42.0, 72.0 + 8.0, 16.0, 24.0),
        "the label of #inherit",
    );
}

/// A button that is a flex container lays its items out as flex does: `align-items:
/// flex-start` keeps the label at the top, and nothing centres it.
#[test]
fn fr34_flex_button_is_not_centred() {
    let dom = open(buttons);
    let button = entry(&dom, "flex").rect;
    let label = entry(&dom, "flex-label").rect;
    close(label.y, button.y, "the flex item's top");
    close(label.x, button.x, "the flex item's left");
}

fn boxes() -> Element {
    rsx! {
        div {
            style: "display: flex; flex-direction: column; align-items: flex-start;",
            // An icon-only button: one inline box and no text.
            button {
                id: "icon",
                style: "width: 48px; height: 48px; padding: 0; border: 0;",
                span { id: "glyph", style: "display: inline-block; width: 16px; height: 16px;" }
            }
            // A block inside: the anonymous box holding it is centred top to bottom.
            button {
                id: "block",
                style: "width: 120px; height: 60px; padding: 0; border: 0;",
                div { id: "inner", style: "width: 20px; height: 20px;" }
            }
            // Content taller than the button starts at the top of the content box.
            button {
                id: "tall",
                style: "width: 120px; height: 20px; padding: 0; border: 0;",
                div { id: "tall-inner", style: "width: 20px; height: 40px;" }
            }
        }
    }
}

/// An icon-only button centres the icon both ways: 16px in a 48px square is 16px in from
/// each side. The line around the icon may be a little taller than it, so the vertical
/// position is allowed a pixel either way.
#[test]
fn fr34_button_centres_an_inline_box() {
    let dom = open(boxes);
    let button = entry(&dom, "icon").rect;
    let glyph = entry(&dom, "glyph").rect;
    close(glyph.x - button.x, 16.0, "the icon's left inset");
    let middle = glyph.y + glyph.height / 2.0 - button.y;
    assert!(
        (middle - 24.0).abs() <= 1.0,
        "the icon's middle is about 24px down the button, got {middle}: {glyph:?} in {button:?}"
    );
}

/// A block inside a button is centred top to bottom: 20px in 60px is 20px down. A block
/// is not moved across by `text-align`.
#[test]
fn fr34_button_centres_a_block_child_vertically() {
    let dom = open(boxes);
    let button = entry(&dom, "block").rect;
    let inner = entry(&dom, "inner").rect;
    rect_close(inner, (button.x, button.y + 20.0, 20.0, 20.0), "#inner");

    // Content taller than its button is not pushed up past the top.
    let button = entry(&dom, "tall").rect;
    let inner = entry(&dom, "tall-inner").rect;
    close(inner.y, button.y, "the tall block's top");
}

fn controls() -> Element {
    rsx! {
        input { id: "field", value: "Ada" }
        textarea { id: "area" }
    }
}

/// Inputs and text areas take the control font too.
#[test]
fn fr34_inputs_take_the_control_font() {
    let dom = open(controls);
    for id in ["field", "area"] {
        let field = entry(&dom, id)
            .input
            .as_ref()
            .unwrap_or_else(|| panic!("#{id} is a form field"));
        close(
            field.style.font_size,
            CONTROL_FONT,
            &format!("#{id} font size"),
        );
        assert_eq!(field.style.line_height, TextLineHeight::Normal, "#{id}");
    }
}
