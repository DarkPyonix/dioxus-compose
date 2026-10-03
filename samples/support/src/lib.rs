//! What the samples' tests share: a text measurer whose answers can be worked out by hand,
//! an image resolver that knows a fixed list of images, and lookups and assertions over
//! the display list.
//!
//! The measurer gives every character an advance of half the font size, plus any
//! `letter-spacing`, and every line the CSS line height (1.25 times the font size for
//! `line-height: normal`). Lines break greedily at spaces and at every `\n`. So at 16px a
//! character is 8px wide, at 14px 7px, at 32px 16px.

use std::cell::RefCell;
use std::rc::Rc;

use dioxus_compose::html::{
    AssetId, DisplayList, HtmlConfig, HtmlDom, ImageResolver, ImageSize, NodeEntry, NodeId, Plan,
    PlanKey, PlanNode, Rect, TextLineHeight, TextMeasureRequest, TextMeasurer, TextMetrics,
    TextStyle, TextWhiteSpace, WidthConstraint,
};

/// One question the measurer was asked.
#[derive(Clone, Debug)]
pub struct Request {
    pub text: String,
    pub font_size: f32,
    pub width: WidthConstraint,
    pub white_space: TextWhiteSpace,
}

/// Measures text with a fixed advance per character and remembers every request.
#[derive(Default)]
pub struct Measurer {
    requests: Rc<RefCell<Vec<Request>>>,
}

impl Measurer {
    pub fn new() -> Self {
        Self::default()
    }

    /// The requests this measurer will record, readable after it has been handed to a
    /// document.
    pub fn requests(&self) -> Rc<RefCell<Vec<Request>>> {
        self.requests.clone()
    }
}

/// How wide one character is.
pub fn advance(style: &TextStyle) -> f32 {
    style.font_size * 0.5 + style.letter_spacing
}

/// How tall one line is.
pub fn line_height(style: &TextStyle) -> f32 {
    match style.line_height {
        TextLineHeight::Px(px) => px,
        TextLineHeight::Normal => style.font_size * 1.25,
    }
}

/// Greedy line breaking at spaces, within `max_chars` characters per line.
fn break_lines(text: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut current = String::new();
        let mut started = false;
        for word in paragraph.split(' ') {
            if !started {
                current.push_str(word);
                started = true;
            } else if current.chars().count() + 1 + word.chars().count() <= max_chars {
                current.push(' ');
                current.push_str(word);
            } else {
                lines.push(std::mem::take(&mut current));
                current.push_str(word);
            }
        }
        lines.push(current);
    }
    lines
}

impl TextMeasurer for Measurer {
    fn measure(&mut self, request: &TextMeasureRequest<'_>) -> TextMetrics {
        self.requests.borrow_mut().push(Request {
            text: request.text.to_string(),
            font_size: request.style.font_size,
            width: request.width,
            white_space: request.white_space,
        });
        let advance = advance(request.style);
        let line = line_height(request.style);
        let max_chars = match request.width {
            WidthConstraint::MaxContent => usize::MAX,
            WidthConstraint::MinContent => 0,
            // The small allowance keeps 432 / 8 from coming out as 53.999.
            WidthConstraint::AtMost(width) => (width / advance + 1e-3).floor() as usize,
        };
        let lines = break_lines(request.text, max_chars);
        let width = lines
            .iter()
            .map(|line| line.chars().count() as f32 * advance)
            .fold(0.0, f32::max);
        TextMetrics {
            width,
            height: lines.len() as f32 * line,
            first_baseline: line * 0.75,
            line_count: lines.len() as u32,
        }
    }
}

/// The configuration every sample test uses: the app's stylesheet and the measurer.
pub fn config(style: &str, measurer: Measurer) -> HtmlConfig {
    HtmlConfig {
        stylesheets: vec![style.to_string()],
        measurer: Some(Box::new(measurer)),
        ..HtmlConfig::default()
    }
}

/// [`config`] with an image resolver.
pub fn config_with_images(
    style: &str,
    measurer: Measurer,
    images: impl ImageResolver + 'static,
) -> HtmlConfig {
    HtmlConfig {
        images: Some(Box::new(images)),
        ..config(style, measurer)
    }
}

/// Every question an [`Images`] resolver was asked, in order.
#[derive(Default, Debug)]
pub struct Questions {
    pub sizes: Vec<String>,
    pub assets: Vec<String>,
}

/// Knows a fixed set of images by URL, each with a natural size and an asset. Nothing is
/// fetched: the answers are the list.
#[derive(Clone)]
pub struct Images {
    known: Vec<(&'static str, ImageSize, AssetId)>,
    pub questions: Rc<RefCell<Questions>>,
}

impl Images {
    pub fn new(known: Vec<(&'static str, ImageSize, AssetId)>) -> Self {
        Self {
            known,
            questions: Rc::default(),
        }
    }

    fn find(&self, url: &str) -> Option<&(&'static str, ImageSize, AssetId)> {
        self.known.iter().find(|(known, _, _)| *known == url)
    }
}

impl ImageResolver for Images {
    fn resolve(&mut self, url: &str) -> Option<AssetId> {
        self.questions.borrow_mut().assets.push(url.to_string());
        self.find(url).map(|(_, _, asset)| *asset)
    }

    fn size(&mut self, url: &str) -> Option<ImageSize> {
        self.questions.borrow_mut().sizes.push(url.to_string());
        self.find(url).map(|(_, size, _)| *size)
    }
}

/// The node of the element whose `id` is `id`.
#[track_caller]
pub fn node(dom: &HtmlDom, id: &str) -> NodeId {
    dom.element_by_id(id)
        .unwrap_or_else(|| panic!("no element with id {id}"))
}

/// The display list entry of the element whose `id` is `id`, from the last layout.
#[track_caller]
pub fn entry<'a>(dom: &'a HtmlDom, id: &str) -> &'a NodeEntry {
    let node = node(dom, id);
    dom.display_list()
        .expect("the document has been laid out")
        .get(node)
        .unwrap_or_else(|| panic!("#{id} (node {node}) has no display list entry"))
}

/// Every entry with the tag `tag`, in paint order.
pub fn entries_with_tag<'a>(list: &'a DisplayList, tag: &str) -> Vec<&'a NodeEntry> {
    list.entries
        .iter()
        .filter(|entry| entry.tag == tag)
        .collect()
}

/// The text an entry draws, its runs joined in order.
pub fn text_of(entry: &NodeEntry) -> String {
    entry
        .texts
        .iter()
        .map(|run| run.text.as_str())
        .collect::<Vec<_>>()
        .join("")
}

/// The plan node under `key`.
#[track_caller]
pub fn plan_node(plan: &Plan, key: PlanKey) -> &PlanNode {
    plan.find(key)
        .unwrap_or_else(|| panic!("{key:?} is not in the plan:\n{plan:#?}"))
}

#[track_caller]
pub fn assert_close(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() < 0.01,
        "{what}: expected {expected}, got {actual}"
    );
}

#[track_caller]
pub fn assert_rect(actual: Rect, x: f32, y: f32, width: f32, height: f32, what: &str) {
    let close = |a: f32, b: f32| (a - b).abs() < 0.01;
    assert!(
        close(actual.x, x)
            && close(actual.y, y)
            && close(actual.width, width)
            && close(actual.height, height),
        "{what}: expected ({x}, {y}, {width}, {height}), got ({}, {}, {}, {})",
        actual.x,
        actual.y,
        actual.width,
        actual.height
    );
}

/// The centre of a rectangle, where a test clicks.
pub fn centre(rect: Rect) -> (f32, f32) {
    (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
}
