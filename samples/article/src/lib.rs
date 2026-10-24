//! Article: a reading page with headings, wrapping paragraphs, lists, a quotation and a
//! code sample.

use dioxus_compose::html::prelude::*;

pub const STYLE: &str = r#"
*, *::before, *::after {
    box-sizing: border-box;
}

body {
    margin: 0;
    font-family: Georgia, serif;
    font-size: 16px;
    line-height: 1.5;
    color: #24292f;
    background: #ffffff;
}

.post {
    max-width: 640px;
    margin: 0 auto;
    padding: 32px 24px;
}

.kicker {
    margin: 0 0 8px;
    font-family: system-ui, sans-serif;
    font-size: 12px;
    line-height: 16px;
    text-transform: uppercase;
    color: #6e7781;
}

h1 {
    margin: 0 0 8px;
    font-size: 32px;
    line-height: 40px;
}

.byline {
    margin: 0 0 32px;
    font-size: 14px;
    line-height: 20px;
    color: #57606a;
}

h2 {
    margin: 32px 0 12px;
    font-size: 24px;
    line-height: 32px;
}

h3 {
    margin: 24px 0 8px;
    font-size: 20px;
    line-height: 28px;
}

p {
    margin: 0 0 16px;
}

ul, ol {
    margin: 0 0 16px;
    padding-left: 24px;
}

li {
    margin: 4px 0;
}

blockquote {
    margin: 24px 0;
    padding: 4px 0 4px 16px;
    border-left: 4px solid #d0d7de;
    color: #57606a;
    font-style: italic;
}

blockquote p {
    margin: 0;
}

pre {
    margin: 0 0 16px;
    padding: 12px 16px;
    border-radius: 6px;
    background: #f6f8fa;
    font-size: 14px;
    line-height: 20px;
}

pre, code {
    font-family: ui-monospace, Menlo, monospace;
}
"#;

/// The code sample under the last heading.
pub const SAMPLE: &str = "fn main() {
    let mut dom = HtmlDom::with_config(app, HtmlConfig::default());
    let list = dom.layout(480.0, 800.0, 1.0);
    println!(\"{} boxes\", list.entries.len());
}";

pub fn app() -> Element {
    rsx! {
        article { class: "post",
            header {
                p { id: "kicker", class: "kicker", "Field notes" }
                h1 { id: "headline", "Laying out a page without a browser" }
                p { id: "byline", class: "byline", "By Ada Lovelace, 3 October 2026" }
            }

            h2 { id: "why", "Why the Host lays out" }
            p { id: "first",
                "A Dioxus app written with HTML tags and CSS does not need a browser to be laid "
                "out. The Host runs the same style and layout engines a browser would, and hands "
                "the renderer nothing but rectangles, colours and runs of text."
            }
            p { id: "second",
                "Text is the one thing both sides must agree on. The Host asks a text measurer "
                "how wide every run is, and the renderer draws it with the same fonts, so a line "
                "never breaks in one place on one side and somewhere else on the other."
            }

            h3 { "What crosses the boundary" }
            ul { id: "crosses",
                li { id: "crosses-1", "Rectangles in CSS pixels" }
                li { id: "crosses-2", "Colours, borders and shadows" }
                li { id: "crosses-3", "Runs of text and their fonts" }
            }

            h3 { "What stays behind" }
            ol { id: "stays",
                li { id: "stays-1", "Selectors and the cascade" }
                li { id: "stays-2", "Property names" }
                li { id: "stays-3", "Every script on the page" }
            }

            blockquote {
                p { id: "quote",
                    "A page that looks the same everywhere is a page whose layout was "
                    "decided in one place."
                }
            }

            h3 { "Trying it" }
            pre { id: "sample", code { "{SAMPLE}" } }
        }
    }
}
