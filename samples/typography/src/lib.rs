//! Typography: a type specimen page with one section per text feature, and a button that
//! cycles the specimen's font size.

use dioxus_compose::html::prelude::*;
use dioxus_hooks::use_signal;
use dioxus_signals::WritableExt;

pub const STYLE: &str = r#"
*, *::before, *::after {
    box-sizing: border-box;
}

body {
    margin: 0;
    font-family: system-ui, sans-serif;
    font-size: 16px;
    line-height: 20px;
    color: #1f2328;
    background: #ffffff;
}

.page {
    padding: 32px;
}

h1 {
    margin: 0 0 16px;
    font-size: 32px;
    line-height: 40px;
}

h2 {
    margin: 32px 0 8px;
    font-size: 20px;
    line-height: 28px;
}

.cap {
    margin: 12px 0 4px;
    font-size: 12px;
    line-height: 16px;
    color: #57606a;
}

.demo {
    margin: 0;
    background: #f6f8fa;
}

.w120 { width: 120px; }
.w160 { width: 160px; }
.w200 { width: 200px; }
.w240 { width: 240px; }

.controls {
    display: flex;
    align-items: center;
    gap: 12px;
}

.cycle {
    padding: 8px 16px;
    font-size: 14px;
    line-height: 20px;
    color: #ffffff;
    background: #0969da;
    border: 0;
    border-radius: 6px;
    cursor: pointer;
}

.specimen {
    margin: 12px 0 0;
    line-height: 1.5;
    background: #ddf4ff;
}
.size-small { font-size: 14px; }
.size-medium { font-size: 16px; }
.size-large { font-size: 20px; }

.scale { line-height: normal; background: #f6f8fa; margin: 0; }
.fs-12 { font-size: 12px; }
.fs-16 { font-size: 16px; }
.fs-20 { font-size: 20px; }
.fs-24 { font-size: 24px; }
.fs-32 { font-size: 32px; }
.fs-40 { font-size: 40px; }

.regular { font-weight: 400; }
.bold { font-weight: 700; }
.light { font-weight: 300; }
.italic { font-style: italic; }

.lh-unitless { line-height: 1.5; }
.lh-px { line-height: 30px; }
.lh-normal { line-height: normal; }

.ls-px { letter-spacing: 2px; }
.ls-em { letter-spacing: 0.25em; }

.align-left { text-align: left; }
.align-center { text-align: center; }
.align-right { text-align: right; }
.align-justify { text-align: justify; }

.upper { text-transform: uppercase; }
.capitalize { text-transform: capitalize; }

.ws-normal { white-space: normal; }
.ws-nowrap { white-space: nowrap; }
.ws-pre { white-space: pre; }
.ws-pre-wrap { white-space: pre-wrap; }

.wb-break-all { word-break: break-all; }
.wb-anywhere { overflow-wrap: anywhere; }
.wb-break-word { overflow-wrap: break-word; }

.ellipsis {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
}

.tag::before { content: "["; }
.tag::after { content: "]"; }

.list {
    margin: 0 0 12px;
    padding-left: 40px;
    background: #f6f8fa;
}
.list li {
    margin: 0;
    line-height: 20px;
}
.square { list-style-type: square; }
.decimal { list-style-type: decimal; }
.alpha { list-style-type: lower-alpha; }
.roman { list-style-type: upper-roman; }

.underline { text-decoration: underline; }
.strike { text-decoration: line-through; }
"#;

/// The font sizes the button cycles through: a name for its label and the class that sets
/// the size.
const SIZES: [(&str, &str); 3] = [
    ("small", "size-small"),
    ("medium", "size-medium"),
    ("large", "size-large"),
];

const PANGRAM: &str = "Pack my box with five dozen liquor jugs";
const LONG_WORD: &str = "Supercalifragilisticexpialidocious";
const WRAP_SAMPLE: &str = "alpha beta gamma delta epsilon zeta";

pub fn app() -> Element {
    let mut size = use_signal(|| 1usize);
    let (name, class) = SIZES[size()];

    rsx! {
        main { class: "page",
            h1 { id: "title", "Type specimen" }

            div { class: "controls",
                button {
                    id: "size-button",
                    class: "cycle",
                    onclick: move |_| {
                        let next = (size() + 1) % SIZES.len();
                        size.set(next);
                    },
                    "Size: {name}"
                }
            }
            p { id: "specimen", class: "specimen w200 {class}", "{PANGRAM}" }

            section { id: "sizes",
                h2 { "Font size" }
                div { id: "scale-12", class: "scale fs-12", "Hamburgefonstiv" }
                div { id: "scale-16", class: "scale fs-16", "Hamburgefonstiv" }
                div { id: "scale-20", class: "scale fs-20", "Hamburgefonstiv" }
                div { id: "scale-24", class: "scale fs-24", "Hamburgefonstiv" }
                div { id: "scale-32", class: "scale fs-32", "Hamburgefonstiv" }
                div { id: "scale-40", class: "scale fs-40", "Hamburgefonstiv" }
            }

            section { id: "faces",
                h2 { "Weight and style" }
                div { id: "weight-300", class: "demo light", "Light 300" }
                div { id: "weight-400", class: "demo regular", "Regular 400" }
                div { id: "weight-700", class: "demo bold", "Bold 700" }
                div { id: "italic", class: "demo italic", "Italic" }
            }

            section { id: "spacing",
                h2 { "Line height" }
                div { class: "cap", "unitless 1.5" }
                div { id: "lh-unitless", class: "demo w120 lh-unitless", "aaa bbb ccc ddd eee fff" }
                div { class: "cap", "30px" }
                div { id: "lh-px", class: "demo w120 lh-px", "aaa bbb ccc ddd eee fff" }
                div { class: "cap", "normal" }
                div { id: "lh-normal", class: "demo w120 lh-normal", "aaa bbb ccc ddd eee fff" }

                h2 { "Letter spacing" }
                div { class: "cap", "0" }
                div { id: "ls-zero", class: "demo w120", "aaa bbb ccc ddd eee fff" }
                div { class: "cap", "2px" }
                div { id: "ls-px", class: "demo w120 ls-px", "aaa bbb ccc ddd eee fff" }
                div { class: "cap", "0.25em" }
                div { id: "ls-em", class: "demo w120 ls-em", "aaa bbb ccc ddd eee fff" }
            }

            section { id: "alignment",
                h2 { "Alignment" }
                div { id: "align-left", class: "demo w240 align-left", "Align me" }
                div { id: "align-center", class: "demo w240 align-center", "Align me" }
                div { id: "align-right", class: "demo w240 align-right", "Align me" }
                div {
                    id: "align-justify",
                    class: "demo w240 align-justify",
                    "Justify spreads the words of every line across the full width"
                }
            }

            section { id: "transform",
                h2 { "Text transform" }
                div { id: "tt-upper", class: "demo upper", "type specimen" }
                div { id: "tt-capitalize", class: "demo capitalize", "type specimen" }
            }

            section { id: "whitespace",
                h2 { "White space" }
                div { class: "cap", "normal" }
                div { id: "ws-normal", class: "demo w160 ws-normal", "{WRAP_SAMPLE}" }
                div { class: "cap", "nowrap" }
                div { id: "ws-nowrap", class: "demo w160 ws-nowrap", "{WRAP_SAMPLE}" }
                div { class: "cap", "pre" }
                div {
                    id: "ws-pre",
                    class: "demo w160 ws-pre",
                    "first line\n    indented line\nlast"
                }
                div { class: "cap", "pre-wrap" }
                div {
                    id: "ws-pre-wrap",
                    class: "demo w160 ws-pre-wrap",
                    "alpha beta gamma delta epsilon\nzeta"
                }
            }

            section { id: "breaking",
                h2 { "Word break and overflow wrap" }
                div { class: "cap", "default" }
                div { id: "wb-default", class: "demo w120 wb-default", "{LONG_WORD}" }
                div { class: "cap", "word-break: break-all" }
                div { id: "wb-break-all", class: "demo w120 wb-break-all", "{LONG_WORD}" }
                div { class: "cap", "overflow-wrap: anywhere" }
                div { id: "wb-anywhere", class: "demo w120 wb-anywhere", "{LONG_WORD}" }
                div { class: "cap", "overflow-wrap: break-word" }
                div { id: "wb-break-word", class: "demo w120 wb-break-word", "{LONG_WORD}" }
            }

            section { id: "overflow",
                h2 { "Text overflow" }
                div {
                    id: "ellipsis",
                    class: "demo w160 ellipsis",
                    "A title that is far too long for its box"
                }
            }

            section { id: "generated",
                h2 { "Generated content" }
                div { id: "tag", class: "demo tag", "note" }
            }

            section { id: "lists",
                h2 { "List markers" }
                ul { id: "list-square", class: "list square w240",
                    li { id: "square-1", "Square marker" }
                    li { id: "square-2", "Second item" }
                    li { id: "square-3", "Third item" }
                }
                ol { id: "list-decimal", class: "list decimal w240",
                    li { id: "decimal-1", "Decimal marker" }
                    li { id: "decimal-2", "Second item" }
                    li { id: "decimal-3", "Third item" }
                }
                ol { id: "list-alpha", class: "list alpha w240",
                    li { id: "alpha-1", "Lower alpha marker" }
                    li { id: "alpha-2", "Second item" }
                }
                ol { id: "list-roman", class: "list roman w240",
                    li { id: "roman-1", "Upper roman marker" }
                    li { id: "roman-2", "Second item" }
                }
            }

            section { id: "decoration",
                h2 { "Text decoration" }
                div { id: "deco-underline", class: "demo underline", "Underlined text" }
                div { id: "deco-strike", class: "demo strike", "Struck text" }
            }
        }
    }
}
