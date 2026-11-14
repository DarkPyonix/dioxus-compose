//! Hello: a heading, a paragraph with bold and italic words, and a link that counts how
//! often it was clicked.
//!
//! [`launch`] opens it in a window: the Host lays the page out and compose-rust draws it.
//! The same page runs in an Android Activity, a browser page and an iOS bundle, through
//! the entry points declared at the end of this file.

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

/// What the window lays the page out with: the stylesheet, and text measured by Parley.
pub fn config() -> HtmlConfig {
    HtmlConfig {
        stylesheets: vec![STYLE.to_string()],
        ..HtmlConfig::default()
    }
}

/// Opens the page in a window. Does not return while it is open.
pub fn launch() {
    launch_builder().launch(app);
}

/// How the page starts, in one place because four entry points need it: the desktop
/// binary, an Android Activity, a browser page and an iOS bundle. Each of them gets the
/// same window and the same stylesheet, so the page is the same page wherever it runs.
fn launch_builder() -> dioxus_compose::LaunchBuilder {
    dioxus_compose::LaunchBuilder::new()
        .with_window(
            dioxus_compose::schema::Window::new()
                .with_title("Hello")
                .with_size(800, 600),
        )
        .with_html(config)
}

// The platforms where the page is not a program. Android's Activity and the browser's page
// own the loop and call an entry point that registers the root component; iOS starts at a
// C `main` (ios/main.c) that hands over to `launch`. Each macro compiles into nothing off
// its own platform, and they are declared unconditionally so a desktop build still checks
// that the page can be built for the other three.
dioxus_compose::android_main!({ launch_builder() }, app);
dioxus_compose::web_main!({ launch_builder() }, app);
dioxus_compose::ios_main!(launch);
