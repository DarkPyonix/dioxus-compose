//! Hello: a heading, a paragraph with bold and italic words, and a link that counts how
//! often it was clicked.

use dioxus_compose::html::prelude::*;
use dioxus_hooks::use_signal;

pub const STYLE: &str = r#"
body {
    margin: 0;
    font-family: system-ui, sans-serif;
    font-size: 16px;
    line-height: 1.5;
    color: #1f2328;
    background: #ffffff;
}

.page {
    padding: 40px 48px;
}

h1 {
    margin: 0 0 16px;
    font-size: 32px;
    line-height: 40px;
}

p {
    margin: 12px 0;
}

.link {
    color: #0969da;
    text-decoration: underline;
    cursor: pointer;
}

.status {
    color: #57606a;
}
"#;

pub fn app() -> Element {
    let mut hellos = use_signal(|| 0u32);

    let status = match hellos() {
        0 => "Nobody has said hello yet".to_string(),
        1 => "You said hello once".to_string(),
        n => format!("You said hello {n} times"),
    };

    rsx! {
        main { class: "page",
            h1 { id: "title", "Hello, Dioxus" }
            p { id: "intro",
                "This page is "
                strong { id: "bold", "HTML" }
                " and "
                em { id: "italic", "CSS" }
                ", laid out by the Host."
            }
            p { id: "action",
                a {
                    id: "greet",
                    class: "link",
                    href: "#",
                    onclick: move |event| {
                        event.prevent_default();
                        hellos += 1;
                    },
                    "Say hello"
                }
            }
            p { id: "status", class: "status", "{status}" }
        }
    }
}
