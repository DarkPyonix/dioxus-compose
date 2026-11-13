//! A `font-family` written through a custom property reaches the text that inherits it.
//!
//! A page keeps its font stacks in custom properties on `:root` (`--mono: "JetBrains Mono",
//! ui-monospace, Menlo, monospace`) and sets `font-family: var(--mono)` on a container. Every
//! text inside it, in a block child or an inline one, is measured and drawn in that stack,
//! as Chromium draws it: the measurer is asked with the list, the display list carries it,
//! and the Text the renderer gets has it as its `Font`.

use std::cell::RefCell;
use std::ffi::c_int;
use std::rc::Rc;

use dioxus_compose::Host;
use dioxus_compose::boundary::{RendererApi, install_renderer_api};
use dioxus_compose::fonts::{FontRef, FontRefs};
use dioxus_compose::html::prelude::*;
use dioxus_compose::html::{
    HtmlConfig, HtmlDom, TextLineHeight, TextMeasureRequest, TextMeasurer, TextMetrics,
};
use dioxus_compose::measure::{MeasureResult, RESULT_OK};
use dioxus_compose::protocol::{Mutation, PropertyValue, decode_batch};
use dioxus_compose::schema::{GenericFamily, PropertyKind};
use dioxus_compose::window::reset_window_size;

const SHEET: &str = "html, body { margin: 0; padding: 0; } \
     :root { --mono: \"JetBrains Mono\", ui-monospace, Menlo, monospace; \
             --font: \"Inter\", -apple-system, system-ui, sans-serif; } \
     body { font: 400 14px/1.5 var(--font); } \
     .code { font: 13px/22px var(--mono); }";

fn app() -> Element {
    rsx! {
        div { id: "diff", style: "font-family: var(--mono)",
            div { "fn main() {{}}" }
            span { "12" }
        }
        div { class: "code",
            div { "let x = 1;" }
        }
        div { "Inter text" }
    }
}

fn mono() -> Vec<String> {
    ["JetBrains Mono", "ui-monospace", "Menlo", "monospace"]
        .map(String::from)
        .to_vec()
}

/// Records the family list of every text it is asked about.
struct Recording(Rc<RefCell<Vec<(String, Vec<String>)>>>);

impl TextMeasurer for Recording {
    fn measure(&mut self, request: &TextMeasureRequest<'_>) -> TextMetrics {
        self.0
            .borrow_mut()
            .push((request.text.to_string(), request.style.font_family.clone()));
        let size = request.style.font_size;
        let line = match request.style.line_height {
            TextLineHeight::Normal => size * 1.25,
            TextLineHeight::Px(height) => height,
        };
        TextMetrics {
            width: request.text.chars().count() as f32 * size * 0.5,
            height: line,
            first_baseline: line * 0.8,
            line_count: 1,
        }
    }
}

#[test]
fn fr34_7_font_family_from_var_is_inherited() {
    let asked = Rc::new(RefCell::new(Vec::new()));
    let config = HtmlConfig {
        stylesheets: vec![SHEET.to_string()],
        measurer: Some(Box::new(Recording(asked.clone()))),
        ..HtmlConfig::default()
    };
    let mut dom = HtmlDom::with_config(app, config);
    dom.layout(400.0, 600.0, 1.0);

    let asked = asked.borrow();
    let family_of = |text: &str| {
        asked
            .iter()
            .find(|(asked, _)| asked == text)
            .map(|(_, family)| family.clone())
            .unwrap_or_else(|| panic!("the measurer was not asked about {text:?}: {asked:?}"))
    };
    for text in ["fn main() {}", "12", "let x = 1;"] {
        assert_eq!(
            family_of(text),
            mono(),
            "the measurer's family for {text:?}"
        );
    }
    assert_eq!(
        family_of("Inter text"),
        ["Inter", "-apple-system", "system-ui", "sans-serif"].map(String::from),
        "the body's family"
    );

    let list = dom.display_list().expect("laid out");
    let runs: Vec<_> = list
        .order()
        .into_iter()
        .filter_map(|node| list.get(node))
        .flat_map(|entry| entry.texts.iter())
        .collect();
    for text in ["fn main() {}", "12", "let x = 1;"] {
        let run = runs
            .iter()
            .find(|run| run.text == text)
            .unwrap_or_else(|| panic!("no text run {text:?}: {runs:#?}"));
        assert_eq!(
            run.style.font_family,
            mono(),
            "the drawn family of {text:?}"
        );
    }
}

extern "C" fn never_run() -> c_int {
    0
}

extern "C" fn no_frames() {}

/// Answers every question with one fixed line.
unsafe extern "C" fn fixed(
    _requests: *const u8,
    _length: u32,
    count: u32,
    results: *mut MeasureResult,
) -> i32 {
    for index in 0..count as usize {
        let result = MeasureResult {
            width: 60.0,
            height: 18.0,
            first_baseline: 14.0,
            line_count: 1,
            status: RESULT_OK,
            ..MeasureResult::default()
        };
        // SAFETY: the caller hands room for `count` results.
        unsafe { results.add(index).write(result) };
    }
    0
}

/// The Text the renderer gets for the inherited mono text carries the stack as its Font.
#[test]
fn fr34_7_font_family_from_var_reaches_the_plan_bridge() {
    let _ = install_renderer_api(RendererApi {
        run: never_run,
        request_frame: no_frames,
        measure: fixed,
    });
    fn cfg() -> HtmlConfig {
        HtmlConfig {
            stylesheets: vec![SHEET.to_string()],
            ..HtmlConfig::default()
        }
    }
    reset_window_size();
    let mut host = Host::html(app, cfg);
    let mutations = decode_batch(host.rebuild().unwrap()).unwrap();
    let expected = FontRefs::new([
        FontRef::System("JetBrains Mono".to_string()),
        FontRef::System("ui-monospace".to_string()),
        FontRef::System("Menlo".to_string()),
        FontRef::Generic(GenericFamily::Monospace),
    ]);
    let fonts: Vec<Vec<u8>> = mutations
        .iter()
        .filter_map(|mutation| match mutation {
            Mutation::SetProp {
                property: PropertyKind::Font,
                value: PropertyValue::Bytes(bytes),
                ..
            } => Some(bytes.to_vec()),
            _ => None,
        })
        .collect();
    let count = fonts
        .iter()
        .filter(|bytes| bytes.as_slice() == expected.as_bytes())
        .count();
    assert!(
        count >= 3,
        "the three mono texts carry the mono stack as their Font, {count} do: {fonts:?}"
    );
}
