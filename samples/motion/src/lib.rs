//! Motion: CSS transforms, a transition a button starts, looping keyframes, and a width
//! that animates.
//!
//! The first row is five boxes with a `transform` each (`rotate`, `scale`, `skewX`, a
//! rotation about the top-left corner, and three functions in a row), every one with a
//! smaller box inside it that turns with it. Layout does not know about a transform: the
//! boxes keep the places the flex row gave them, and a pointer lands on the shape that is
//! drawn.
//!
//! The second row is a box that fades, changes colour and slides when a button toggles its
//! `lit` class, by a transition of `opacity`, `background-color` and `transform`. The
//! third is two loops, a spinner and a pulse, written with `@keyframes`. These move
//! nothing about layout, so a renderer plays them and the Host does nothing per frame.
//!
//! The fourth is a bar whose `width` transitions when a button is pressed. A width changes
//! layout (the button beside the bar is pushed along), so the Host ticks it, frame by frame,
//! for as long as it runs.

use dioxus_compose::html::prelude::*;
use dioxus_hooks::use_signal;
use dioxus_signals::{ReadableExt, WritableExt};

pub const STYLE: &str = r#"
* { box-sizing: border-box; }
html, body { margin: 0; }
body { font-family: system-ui, sans-serif; font-size: 16px; color: #0f172a; background-color: #f8fafc; }

.page { padding: 24px; }
.row { display: flex; align-items: flex-start; gap: 80px; height: 120px; padding: 20px 0; }

.cell { flex: none; width: 80px; height: 80px; padding: 10px; background-color: #6366f1; }
.inner { width: 20px; height: 20px; background-color: #f59e0b; }
#rotate { transform: rotate(15deg); }
#scale { transform: scale(1.5); }
#skew { transform: skewX(10deg); }
#origin { transform: rotate(45deg); transform-origin: top left; }
#combo { transform: translate(10px, 5px) rotate(30deg) scale(0.5); }

button {
    flex: none;
    width: 100px;
    height: 32px;
    padding: 0;
    border: 0;
    border-radius: 6px;
    background-color: #334155;
    color: #f8fafc;
}

.fader {
    flex: none;
    width: 80px;
    height: 80px;
    background-color: #94a3b8;
    opacity: 0.3;
    transition: opacity 0.4s ease, background-color 0.4s ease, transform 0.4s ease;
}
.fader.lit {
    background-color: #22c55e;
    opacity: 1;
    transform: translateX(20px) rotate(10deg);
}

@keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
}
@keyframes pulse {
    from { opacity: 0.3; }
    to { opacity: 1; }
}
.spinner {
    flex: none;
    width: 40px;
    height: 40px;
    background-color: #ef4444;
    animation: spin 2s linear infinite;
}
.pulse {
    flex: none;
    width: 80px;
    height: 40px;
    background-color: #0ea5e9;
    animation: pulse 1s ease-in-out 0.5s infinite alternate;
}

.meter {
    flex: none;
    width: 100px;
    height: 24px;
    background-color: #6366f1;
    transition: width 0.3s linear;
}
.meter.wide { width: 300px; }

#status { margin: 0; height: 20px; }

@media (prefers-reduced-motion: reduce) {
    .fader, .meter { transition: none; }
    .spinner, .pulse { animation: none; }
}
"#;

pub fn app() -> Element {
    let mut last = use_signal(|| "nothing");
    let mut lit = use_signal(|| false);
    let mut wide = use_signal(|| false);
    let status = format!("Last click: {}", last.cloned());
    let fader = if lit.cloned() { "fader lit" } else { "fader" };
    let meter = if wide.cloned() { "meter wide" } else { "meter" };

    rsx! {
        div { class: "page",
            div { id: "shapes", class: "row",
                div { id: "rotate", class: "cell", onclick: move |_| last.set("rotate"),
                    div { id: "rotate-inner", class: "inner" }
                }
                div { id: "scale", class: "cell", onclick: move |_| last.set("scale"),
                    div { id: "scale-inner", class: "inner" }
                }
                div { id: "skew", class: "cell",
                    div { id: "skew-inner", class: "inner" }
                }
                div { id: "origin", class: "cell", onclick: move |_| last.set("origin"),
                    div { id: "origin-inner", class: "inner" }
                }
                div { id: "combo", class: "cell",
                    div { id: "combo-inner", class: "inner" }
                }
            }
            div { id: "fade-row", class: "row",
                div { id: "fader", class: "{fader}" }
                button { id: "toggle", onclick: move |_| lit.set(!lit.cloned()), "Toggle" }
            }
            div { id: "loop-row", class: "row",
                div { id: "spinner", class: "spinner" }
                div { id: "pulse", class: "pulse" }
            }
            div { id: "meter-row", class: "row",
                div { id: "meter", class: "{meter}" }
                button { id: "grow", onclick: move |_| wide.set(!wide.cloned()), "Grow" }
            }
            p { id: "status", "{status}" }
        }
    }
}
