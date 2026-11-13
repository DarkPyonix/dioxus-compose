//! Flex rows whose items shrink, grow from a zero basis, or are pushed to the end, laid out
//! as Chromium lays them out, and laid out the same however often and in whatever order
//! the page is built.
//!
//! The pages are Ember's editor rows, cut down: a commit row (a file name, its folder, the
//! status letter), the terminal tab strip, and an extension row whose buttons sit at its
//! end. Every expected number is worked out from the CSS with a measurer that gives each
//! character the same advance.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use blitz_dom::DocumentConfig;
use blitz_html::HtmlDocument;
use blitz_traits::shell::{ColorScheme, Viewport};
use dioxus_compose::html::prelude::*;
use dioxus_compose::html::{
    BaseDocument, DisplayList, HtmlConfig, HtmlDom, Rect, TextMeasureRequest, TextMeasurer,
    TextMetrics, TextStyle, TextWhiteSpace, WidthConstraint, element_by_id, layout_document,
};
use dioxus_hooks::use_signal;

/// Every character `char_width` wide and every line `line_height` tall; lines break
/// greedily at spaces.
struct FakeMeasurer {
    char_width: f32,
    line_height: f32,
}

impl TextMeasurer for FakeMeasurer {
    fn measure(&mut self, request: &TextMeasureRequest<'_>) -> TextMetrics {
        let max_chars = match request.width {
            WidthConstraint::MaxContent => usize::MAX,
            WidthConstraint::MinContent => 0,
            WidthConstraint::AtMost(width) => (width / self.char_width).floor() as usize,
        };
        let mut lines: Vec<String> = Vec::new();
        let mut current = String::new();
        for word in request.text.split(' ') {
            if current.is_empty() {
                current.push_str(word);
            } else if current.chars().count() + 1 + word.chars().count() <= max_chars {
                current.push(' ');
                current.push_str(word);
            } else {
                lines.push(std::mem::take(&mut current));
                current.push_str(word);
            }
        }
        lines.push(current);
        let width = lines
            .iter()
            .map(|line| line.chars().count() as f32 * self.char_width)
            .fold(0.0, f32::max);
        TextMetrics {
            width,
            height: lines.len() as f32 * self.line_height,
            first_baseline: self.line_height * 0.75,
            line_count: lines.len() as u32,
        }
    }
}

type Key = (String, u32, bool, u8, u32);

fn key(request: &TextMeasureRequest<'_>) -> Key {
    let (constraint, width) = match request.width {
        WidthConstraint::MinContent => (0, 0.0f32),
        WidthConstraint::MaxContent => (1, 0.0),
        WidthConstraint::AtMost(width) => (2, width),
    };
    (
        request.text.to_string(),
        request.style.font_size.to_bits(),
        request.white_space.wrap,
        constraint,
        width.to_bits(),
    )
}

/// A measurer that behaves as the renderer's does: a question it has not been answered
/// yet gets an estimate (6px a character) and is kept, and `settle` answers every kept
/// question (7px a character), so the layout runs again with the answers.
struct SettlingMeasurer {
    known: HashMap<Key, TextMetrics>,
    asked: Vec<(Key, TextStyle, WidthConstraint, TextWhiteSpace)>,
}

impl SettlingMeasurer {
    fn new() -> Self {
        Self {
            known: HashMap::new(),
            asked: Vec::new(),
        }
    }
}

impl TextMeasurer for SettlingMeasurer {
    fn measure(&mut self, request: &TextMeasureRequest<'_>) -> TextMetrics {
        let key = key(request);
        if let Some(metrics) = self.known.get(&key) {
            return *metrics;
        }
        self.asked.push((
            key,
            request.style.clone(),
            request.width,
            request.white_space,
        ));
        FakeMeasurer {
            char_width: 6.0,
            line_height: 20.0,
        }
        .measure(request)
    }

    fn settle(&mut self) -> bool {
        if self.asked.is_empty() {
            return false;
        }
        for (key, style, width, white_space) in std::mem::take(&mut self.asked) {
            let request = TextMeasureRequest {
                text: &key.0,
                style: &style,
                width,
                white_space,
            };
            let metrics = FakeMeasurer {
                char_width: 7.0,
                line_height: 20.0,
            }
            .measure(&request);
            self.known.insert(key, metrics);
        }
        true
    }
}

fn parse(html: &str, width: u32, height: u32) -> BaseDocument {
    let config = DocumentConfig {
        viewport: Some(Viewport::new(width, height, 1.0, ColorScheme::Light)),
        ..DocumentConfig::default()
    };
    HtmlDocument::from_html(html, config).into_inner()
}

fn rect(list: &DisplayList, doc: &BaseDocument, id: &str) -> Rect {
    let node = element_by_id(doc, id).unwrap_or_else(|| panic!("no element with id {id}"));
    list.get(node)
        .unwrap_or_else(|| panic!("#{id} has no display list entry"))
        .rect
}

#[track_caller]
fn assert_rect(actual: Rect, x: f32, y: f32, width: f32, height: f32) {
    let close = |a: f32, b: f32| (a - b).abs() < 0.01;
    assert!(
        close(actual.x, x)
            && close(actual.y, y)
            && close(actual.width, width)
            && close(actual.height, height),
        "expected ({x}, {y}, {width}, {height}), got ({}, {}, {}, {})",
        actual.x,
        actual.y,
        actual.width,
        actual.height
    );
}

const COMMIT_ROW: &str = r#"<!DOCTYPE html><html><head><style>html, body { margin: 0; }</style></head><body>
<div id="row" style="display:flex;width:260px">
  <span id="name" style="flex:0 1 auto;white-space:nowrap;overflow:hidden">editor_scm.rs</span>
  <span id="folder" style="flex:1 2 0px;min-width:0;white-space:nowrap;overflow:hidden">ember/app/src/ui</span>
  <span id="letter" style="flex:none">M</span>
</div></body></html>"#;

/// Chromium fills the 260px row: the name keeps its 13 characters (91px), the letter its
/// 7px, and the folder, grown from a zero basis, takes the 162px between them and is cut.
#[test]
fn fr34_flex_basis_zero_folder_takes_the_rest_of_the_row() {
    let mut doc = parse(COMMIT_ROW, 800, 600);
    let mut measurer = FakeMeasurer {
        char_width: 7.0,
        line_height: 20.0,
    };
    let list = layout_document(&mut doc, &mut measurer);
    assert_rect(rect(&list, &doc, "row"), 0.0, 0.0, 260.0, 20.0);
    assert_rect(rect(&list, &doc, "name"), 0.0, 0.0, 91.0, 20.0);
    assert_rect(rect(&list, &doc, "folder"), 91.0, 0.0, 162.0, 20.0);
    assert_rect(rect(&list, &doc, "letter"), 253.0, 0.0, 7.0, 20.0);
}

/// The commit row with more room than it needs: the folder and the spacer both grow from
/// a zero basis and share what the name, the button and the letter leave, so the letter
/// sits at the row's end.
#[test]
fn fr34_flex_shrink_commit_row_fills_its_panel() {
    let html = r#"<!DOCTYPE html><html><head><style>html, body { margin: 0; }
.tree { width: 280px; padding: 8px; box-sizing: border-box; overflow: auto; }
.row { display: flex; align-items: center; gap: 3px; height: 30px; padding: 0 8px 0 20px; white-space: nowrap; min-width: 0; }
.spacer { flex: 1; min-width: 0; }
</style></head><body><div class="tree"><div role="group">
<div id="row" class="row"><img src="icon:file" width="15" height="15"><span id="name" style="overflow:hidden;flex:0 1 auto;min-width:20px">vcs.rs</span><span id="folder" style="overflow:hidden;flex:1 2 0px;min-width:0">ember/app</span><div class="spacer"></div><button style="width:20px;height:20px;flex:none;padding:0;border:0"></button><span id="letter" style="width:14px;flex:none">M</span></div>
</div></div></body></html>"#;
    let mut doc = parse(html, 800, 600);
    let mut measurer = FakeMeasurer {
        char_width: 7.0,
        line_height: 20.0,
    };
    let list = layout_document(&mut doc, &mut measurer);
    // The row is the tree's 264px content box. Its content box, 236px from x = 28, holds
    // the icon (15), the name (42), the button (20), the letter (14) and five 3px gaps:
    // 106px, which leaves 130px for the folder and the spacer, 65px each.
    assert_rect(rect(&list, &doc, "row"), 8.0, 8.0, 264.0, 30.0);
    assert_rect(rect(&list, &doc, "name"), 46.0, 13.0, 42.0, 20.0);
    assert_rect(rect(&list, &doc, "folder"), 91.0, 13.0, 65.0, 20.0);
    assert_rect(rect(&list, &doc, "letter"), 250.0, 13.0, 14.0, 20.0);
}

const TAB_STRIP_CSS: &str = r#"html, body { margin: 0; }
.ib { width: 28px; height: 28px; border: 0; display: grid; place-items: center; flex: none; padding: 0; }
.strip { display: flex; align-items: center; flex: none; min-width: 0; padding-right: 12px; width: 300px; }
.tabs { display: flex; align-items: center; gap: 4px; padding: 8px 12px 0; flex: 1 1 auto; min-width: 0; overflow-x: auto; }
.tabs > button { box-sizing: border-box; flex: 0 0 auto; max-width: 240px; display: flex; align-items: center; gap: 6px; height: 28px; padding: 0 12px; border: 1px solid #ccc; background: none; white-space: nowrap; }
.tabs > button > .ib { width: 18px; height: 18px; margin-right: -6px; }
"#;

/// A terminal tab is a flex row of its label and an 18px close icon pulled 6px into the
/// padding by a negative margin, `flex: none`. Its width is its content's: 1 + 12 of
/// border and padding each side, 21px of label, a 6px gap and 18 - 6 = 12px of icon, 65px.
/// Taffy summed the icon's contribution with a scale that does not hold for an item that
/// may not shrink, got a negative width, and the tab collapsed to its padding.
#[test]
fn fr34_flex_shrink_zero_icon_with_a_negative_margin_keeps_the_tab_wide() {
    let html = format!(
        r#"<!DOCTYPE html><html><head><style>{TAB_STRIP_CSS}</style></head><body>
<div class="strip"><div class="tabs">
<button id="one">zsh<span class="ib"></span></button>
<button id="two">npm<span class="ib"></span></button>
</div><button id="plus" class="ib">+</button></div></body></html>"#
    );
    let mut doc = parse(&html, 800, 600);
    let mut measurer = FakeMeasurer {
        char_width: 7.0,
        line_height: 20.0,
    };
    let list = layout_document(&mut doc, &mut measurer);
    assert_rect(rect(&list, &doc, "one"), 12.0, 8.0, 65.0, 28.0);
    assert_rect(rect(&list, &doc, "two"), 81.0, 8.0, 65.0, 28.0);
    // The strip's content box is 300px; the tab list grows into the 272px the + leaves,
    // and the + follows it.
    assert_rect(rect(&list, &doc, "plus"), 272.0, 4.0, 28.0, 28.0);
}

const ROW: &str = "display:flex; flex-direction:column; gap:2px; padding:8px 8px 8px 12px; width:100%; min-width:0; max-width:100%; overflow:hidden; box-sizing:border-box;";
const LINE: &str = "display:flex; align-items:center; gap:8px; width:100%; min-width:0; max-width:100%; overflow:hidden;";
const NAME: &str = "overflow:hidden; text-overflow:ellipsis; white-space:nowrap; min-width:0; flex:0 1 auto; display:block;";
const VERSION: &str = "font-size:12px; flex:none;";
const DESCRIPTION: &str = "font-size:12px; white-space:normal; overflow-wrap:anywhere; display:block; width:100%; min-width:0; max-width:100%;";
const META: &str = "font-size:12px; overflow:hidden; white-space:nowrap; display:block; width:100%; min-width:0; max-width:100%;";
const ACTIONS_SPACER: &str = "display:flex; align-items:center; gap:8px; width:100%;";
const ACTIONS_END: &str =
    "display:flex; align-items:center; justify-content:flex-end; gap:8px; width:100%;";
const ACTION: &str =
    "border:1px solid #ccc; height:24px; padding:0 8px; font-size:12px; flex:none;";

const PANEL_CSS: &str = r#"html, body { margin: 0; }
.panel { display: flex; flex-direction: column; min-height: 0; min-width: 0; overflow: hidden; width: 280px; height: 1000px; }
.tree { overflow: auto; flex: 1; padding: 8px; min-height: 0; }
.tn { display: flex; align-items: center; gap: 6px; height: 30px; padding: 0 8px 0 calc(4px + var(--d) * 16px); white-space: nowrap; min-width: 0; }
.spacer { flex: 1; min-width: 0; }
.ib { width: 28px; height: 28px; border: 0; display: grid; place-items: center; flex: none; padding: 0; }
.strip { display: flex; align-items: center; flex: none; min-width: 0; padding-right: 12px; }
.tabs { display: flex; align-items: center; gap: 4px; padding: 8px 12px 0; flex: 1 1 auto; min-width: 0; overflow-x: auto; }
.tabs > button { flex: 0 0 auto; max-width: 240px; display: flex; align-items: center; gap: 6px; height: 28px; padding: 0 12px; border: 1px solid #ccc; background: none; white-space: nowrap; }
.tabs > button > .ib { width: 18px; height: 18px; margin-right: -6px; }
"#;

/// The extensions, commits and tabs a stage of the page shows, so the same page can be
/// built in steps or at once.
static STAGE: AtomicUsize = AtomicUsize::new(0);
/// Whether the page's measurer estimates first and settles after.
static SETTLING: AtomicUsize = AtomicUsize::new(0);

struct Extension {
    name: &'static str,
    version: &'static str,
    description: &'static str,
    publisher: &'static str,
}

const EXTENSIONS: [Extension; 3] = [
    Extension {
        name: "Even Better TOML",
        version: "0.21.2",
        description: "Fully-featured TOML support",
        publisher: "tamasfe",
    },
    Extension {
        name: "rust-analyzer",
        version: "0.4.3116",
        description: "Rust language support for Visual Studio Code",
        publisher: "rust-lang",
    },
    Extension {
        name: "Prettier",
        version: "11.0.0",
        description: "Code formatter using prettier",
        publisher: "esbenp",
    },
];

fn extension_row(index: usize, extension: &Extension, enabled: bool, spacer: bool) -> Element {
    let toggle = if enabled { "Disable" } else { "Enable" };
    let actions = if spacer { ACTIONS_SPACER } else { ACTIONS_END };
    let kind = if spacer { "s" } else { "e" };
    rsx! {
        div { key: "{kind}{extension.name}", style: "{ROW}",
            div { style: "{LINE}",
                span { style: "{NAME}", "{extension.name}" }
                span { style: "{VERSION}", "{extension.version}" }
            }
            div { style: "{DESCRIPTION}", "{extension.description}" }
            div { style: "{META}", "{extension.publisher}" }
            div { id: "actions-{kind}{index}", style: "{actions}",
                if spacer { div { class: "spacer" } }
                button { id: "toggle-{kind}{index}", style: "{ACTION}", "{toggle}" }
                button { id: "remove-{kind}{index}", style: "{ACTION}", "Remove" }
            }
        }
    }
}

fn commit_row(name: &str, folder: &str) -> Element {
    rsx! {
        div { key: "{name}{folder}", class: "tn", style: "--d:1; gap:3px; padding-right:8px;",
            img { src: "icon:file", width: "15", height: "15" }
            span { style: "overflow:hidden; flex:0 1 auto; min-width:20px;", "{name}" }
            span { style: "font-size:11px; overflow:hidden; flex:1 2 0px; min-width:0;", "{folder}" }
            div { class: "spacer" }
            button { class: "ib", style: "width:20px; height:20px; flex:none;" }
            span { style: "width:14px; flex:none; text-align:center;", "M" }
        }
    }
}

fn tab(label: &str, on: bool) -> Element {
    rsx! {
        button { key: "{label}",
            img { src: "icon:code", width: "14", height: "14" }
            "{label}"
            if on { span { class: "ib" } }
            span { class: "ib" }
        }
    }
}

fn panel() -> Element {
    let mut bump = use_signal(|| 0usize);
    let _ = bump();
    let stage = STAGE.load(Ordering::SeqCst);
    let tabs: &[&str] = match stage {
        0 => &[],
        1 => &["zsh"],
        2 => &["zsh", "zsh 2"],
        _ => &["zsh", "zsh 2", "build", "server logs"],
    };
    let shown = match stage {
        0 => 0,
        1 => 1,
        2 | 3 => 2,
        _ => 3,
    };
    let files: &[(&str, &str)] = match stage {
        0 => &[],
        1 => &[("vcs.rs", "ember/app/src/ui")],
        _ => &[
            ("vcs.rs", "ember/app/src/ui"),
            ("diff_view.rs", "ember/app/src/ui/editor"),
            ("git.rs", "ember"),
        ],
    };
    rsx! {
        button { id: "next", style: "width:10px; height:10px", onclick: move |_| bump += 1 }
        section { class: "panel",
            div { class: "strip",
                div { class: "tabs", role: "tablist",
                    for (index, label) in tabs.iter().enumerate() { {tab(label, index == 0)} }
                }
                button { class: "ib", "+" }
            }
            div { class: "tree",
                div { role: "group",
                    div { class: "tn", style: "--d:0",
                        "Changes"
                        span { style: "margin-left:4px", "{files.len()}" }
                        div { class: "spacer" }
                        button { class: "ib", style: "width:22px; height:22px" }
                    }
                    for (name, folder) in files.iter() { {commit_row(name, folder)} }
                }
            }
            div { class: "tree", style: "overflow-x:hidden;",
                if shown == 0 { div { "Loading" } }
                for (index, extension) in EXTENSIONS.iter().take(shown).enumerate() {
                    {extension_row(index, extension, stage != 3 || index != 0, true)}
                }
                for (index, extension) in EXTENSIONS.iter().take(shown).enumerate() {
                    {extension_row(index, extension, true, false)}
                }
            }
        }
    }
}

fn panel_config() -> HtmlConfig {
    let measurer: Box<dyn TextMeasurer> = if SETTLING.load(Ordering::SeqCst) == 1 {
        Box::new(SettlingMeasurer::new())
    } else {
        Box::new(FakeMeasurer {
            char_width: 7.0,
            line_height: 20.0,
        })
    };
    HtmlConfig {
        stylesheets: vec![PANEL_CSS.to_string()],
        measurer: Some(measurer),
        ..HtmlConfig::default()
    }
}

fn boxes(dom: &mut HtmlDom) -> Vec<(String, [f32; 4])> {
    dom.layout(900.0, 1100.0, 1.0)
        .entries
        .iter()
        .map(|entry| {
            let rect = entry.rect;
            (
                entry.tag.to_string(),
                [rect.x, rect.y, rect.width, rect.height],
            )
        })
        .collect()
}

/// The page at `stage`, built at once in a fresh document.
fn fresh(stage: usize) -> HtmlDom {
    STAGE.store(stage, Ordering::SeqCst);
    HtmlDom::with_config(panel, panel_config())
}

/// Moves `dom` to `stage` the way a page changes: a handler runs and the page renders.
fn advance(dom: &mut HtmlDom, stage: usize) {
    STAGE.store(stage, Ordering::SeqCst);
    let next = dom.element_by_id("next").expect("the page has its button");
    dom.click_node(next);
    dom.render();
}

/// Both pages have the same element with this id at the same place.
fn rect_of(dom: &mut HtmlDom, id: &str) -> Rect {
    dom.layout(900.0, 1100.0, 1.0);
    let node = dom.element_by_id(id).unwrap_or_else(|| panic!("no #{id}"));
    dom.display_list()
        .and_then(|list| list.get(node))
        .unwrap_or_else(|| panic!("#{id} has no box"))
        .rect
}

/// The tests here share `STAGE` and `SETTLING`, so they run one at a time.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// An extension row ends in its buttons, pushed there by a `flex: 1` spacer or by
/// `justify-content: flex-end`, on every row: the one whose description fits a line and
/// the one whose description wraps, built at once or one step at a time, with text
/// measured at once or estimated first.
#[test]
fn fr34_a_flex_spacer_right_aligns_every_row() {
    let _serial = SERIAL.lock().unwrap_or_else(|poison| poison.into_inner());
    for settling in [0, 1] {
        SETTLING.store(settling, Ordering::SeqCst);
        let mut stepped = fresh(0);
        boxes(&mut stepped);
        for stage in 1..=4 {
            advance(&mut stepped, stage);
            let mut built = fresh(stage);
            let shown = match stage {
                1 => 1,
                2 | 3 => 2,
                _ => 3,
            };
            for dom in [&mut stepped, &mut built] {
                for kind in ["s", "e"] {
                    for index in 0..shown {
                        let actions = rect_of(dom, &format!("actions-{kind}{index}"));
                        let remove = rect_of(dom, &format!("remove-{kind}{index}"));
                        let toggle = rect_of(dom, &format!("toggle-{kind}{index}"));
                        assert!(
                            (remove.x + remove.width - (actions.x + actions.width)).abs() < 0.01,
                            "stage {stage}, row {kind}{index}: the buttons end at {} in a line \
                             that ends at {}",
                            remove.x + remove.width,
                            actions.x + actions.width
                        );
                        assert!(
                            (toggle.x + toggle.width + 8.0 - remove.x).abs() < 0.01,
                            "stage {stage}, row {kind}{index}: the buttons are not side by side"
                        );
                        // The row is the panel's: 280px less the tree's padding and the row's.
                        assert!(
                            (actions.width - 244.0).abs() < 0.01,
                            "stage {stage}, row {kind}{index}: the button line is {} wide",
                            actions.width
                        );
                    }
                }
            }
        }
    }
}

/// The same page lays out to the same boxes however it got there: built in one go, built
/// a step at a time, built in another order, built again in a fresh document, laid out
/// again with nothing changed, and with text measured at once or estimated first and
/// settled after.
#[test]
fn fr34_layout_is_the_same_every_run() {
    let _serial = SERIAL.lock().unwrap_or_else(|poison| poison.into_inner());
    for settling in [0, 1] {
        SETTLING.store(settling, Ordering::SeqCst);
        let orders: [&[usize]; 4] = [
            &[0, 1, 2, 3, 4],
            &[4, 1, 3, 0, 2],
            &[2, 4, 0, 3, 1],
            &[3, 3, 1, 1, 4],
        ];
        let mut expected: HashMap<usize, Vec<(String, [f32; 4])>> = HashMap::new();
        for stage in 0..=4 {
            let first = boxes(&mut fresh(stage));
            for _ in 0..3 {
                assert_eq!(
                    boxes(&mut fresh(stage)),
                    first,
                    "stage {stage} laid out differently in a second fresh document"
                );
            }
            expected.insert(stage, first);
        }
        for order in orders {
            let mut dom = fresh(order[0]);
            for (step, &stage) in order.iter().enumerate() {
                if step > 0 {
                    advance(&mut dom, stage);
                }
                let laid_out = boxes(&mut dom);
                assert_eq!(
                    laid_out, expected[&stage],
                    "stage {stage}, reached by {order:?}, differs from the page built at once \
                     (measurer settling: {settling})"
                );
                assert_eq!(
                    boxes(&mut dom),
                    laid_out,
                    "laying stage {stage} out again with nothing changed moved a box"
                );
            }
        }
    }
}
