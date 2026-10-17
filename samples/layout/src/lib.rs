//! Layout: flexbox, CSS grid and positioning, one panel each under a sticky header.
//!
//! The page is 800px wide in the tests, every box is `border-box` and almost every size is
//! written in pixels, so a position can be worked out from this stylesheet alone.

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
    font-size: 14px;
    line-height: 24px;
    color: #1f2328;
    background: #f6f8fa;
}

.site-header {
    position: sticky;
    top: 0;
    z-index: 10;
    display: flex;
    align-items: center;
    height: 48px;
    padding: 0 16px;
    background: #24292f;
    color: #ffffff;
}

.panels {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 16px;
}

.panel {
    flex-shrink: 0;
    padding: 16px;
    background: #ffffff;
    border-radius: 8px;
}

.panel-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    height: 32px;
    margin-bottom: 12px;
}

.panel-head h2 {
    margin: 0;
    font-size: 18px;
    line-height: 24px;
}

.toggle {
    height: 32px;
    padding: 0 12px;
    border: 1px solid #d0d7de;
    border-radius: 6px;
    background: #f6f8fa;
    color: inherit;
    font: inherit;
    cursor: pointer;
}

.demo-stack {
    display: flex;
    flex-direction: column;
    gap: 12px;
}

.box {
    min-width: 0;
    padding: 0 8px;
    background: #ddf4ff;
    border-radius: 4px;
}

/* Flexbox */

.wrap-box {
    display: flex;
    flex-direction: row;
    flex-wrap: wrap;
    gap: 8px;
}

.wrap-box.column {
    flex-direction: column;
}

.wrap-box .box {
    width: 160px;
    height: 40px;
}

.grow-row, .shrink-row {
    display: flex;
    height: 40px;
}

#g1 { flex: 1 1 100px; }
#g2 { flex: 3 1 100px; }
#g3 { flex: 0 0 136px; }

#k1 { flex: 0 1 400px; }
#k2 { flex: 0 3 400px; }
#k3 { flex: 0 0 136px; }

.align-row {
    display: flex;
    height: 80px;
    background: #eaeef2;
}

.align-spread {
    justify-content: space-between;
    align-items: center;
}

.align-end {
    justify-content: center;
    align-items: flex-end;
}

.align-row .box {
    width: 60px;
    padding: 0;
}

#a1, #e1 { height: 20px; }
#a2, #e2 { height: 40px; }
#a3, #e3 { height: 60px; }

.order-row {
    display: flex;
    gap: 8px;
}

.order-row .box {
    width: 100px;
    height: 40px;
}

#o1 { order: 3; }
#o2 { order: 1; }
#o3 { order: 2; }

/* Grid */

.areas {
    display: grid;
    grid-template-areas:
        "head head"
        "nav main"
        "foot foot";
    grid-template-columns: 160px 1fr;
    grid-template-rows: 40px 120px 40px;
    gap: 8px;
}

.areas .box { padding: 8px; }
#area-head { grid-area: head; }
#area-nav { grid-area: nav; }
#area-main { grid-area: main; }
#area-foot { grid-area: foot; }

.card-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    grid-auto-rows: 60px;
    gap: 8px;
}

.span-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    grid-auto-rows: 40px;
    gap: 8px;
}

#span-a { grid-column: span 2; }
#span-c { grid-row: span 2; }

.line-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    grid-template-rows: repeat(3, 40px);
    gap: 8px;
}

#line-a { grid-column: 2 / 4; grid-row: 1 / 3; }
#line-b { grid-column: 1; grid-row: 3; }
#line-c { grid-column: 4 / 5; grid-row: 2 / 4; }

/* Positioning */

.stack {
    position: relative;
    height: 140px;
}

.layer {
    position: absolute;
    width: 100px;
    height: 100px;
    border-radius: 6px;
}

#layer-red { left: 0; top: 0; z-index: 3; background: #cf222e; }
#layer-green { left: 40px; top: 20px; z-index: 1; background: #1a7f37; }
#layer-blue { left: 80px; top: 40px; z-index: 2; background: #0969da; }

.anchor {
    position: relative;
    width: 300px;
    height: 100px;
    background: #eaeef2;
}

.anchor p {
    margin: 0;
}

#badge {
    position: absolute;
    right: 8px;
    bottom: 8px;
    width: 80px;
    height: 24px;
    background: #8250df;
    border-radius: 12px;
}

#pin {
    position: absolute;
    top: 10px;
    left: 50%;
    width: 40px;
    height: 20px;
    background: #bf8700;
}
"#;

pub fn app() -> Element {
    let mut column = use_signal(|| false);

    rsx! {
        div { id: "page", class: "page",
            header { id: "site-header", class: "site-header", "Layout lab" }
            main { class: "panels",
                section { id: "panel-flex", class: "panel",
                    div { class: "panel-head",
                        h2 { id: "title-flex", "Flexbox" }
                        button {
                            id: "toggle-direction",
                            class: "toggle",
                            r#type: "button",
                            onclick: move |_| column.set(!column()),
                            if column() { "Direction: column" } else { "Direction: row" }
                        }
                    }
                    div { class: "demo-stack",
                        div {
                            id: "wrap-box",
                            class: if column() { "wrap-box column" } else { "wrap-box" },
                            div { id: "w1", class: "box", "One" }
                            div { id: "w2", class: "box", "Two" }
                            div { id: "w3", class: "box", "Three" }
                            div { id: "w4", class: "box", "Four" }
                            div { id: "w5", class: "box", "Five" }
                        }
                        div { id: "grow-row", class: "grow-row",
                            div { id: "g1", class: "box", "1" }
                            div { id: "g2", class: "box", "3" }
                            div { id: "g3", class: "box", "fixed" }
                        }
                        div { id: "shrink-row", class: "shrink-row",
                            div { id: "k1", class: "box", "1" }
                            div { id: "k2", class: "box", "3" }
                            div { id: "k3", class: "box", "fixed" }
                        }
                        div { id: "align-spread", class: "align-row align-spread",
                            div { id: "a1", class: "box" }
                            div { id: "a2", class: "box" }
                            div { id: "a3", class: "box" }
                        }
                        div { id: "align-end", class: "align-row align-end",
                            div { id: "e1", class: "box" }
                            div { id: "e2", class: "box" }
                            div { id: "e3", class: "box" }
                        }
                        div { id: "order-row", class: "order-row",
                            div { id: "o1", class: "box", "A" }
                            div { id: "o2", class: "box", "B" }
                            div { id: "o3", class: "box", "C" }
                        }
                    }
                }
                section { id: "panel-grid", class: "panel",
                    div { class: "panel-head",
                        h2 { id: "title-grid", "Grid" }
                    }
                    div { class: "demo-stack",
                        div { id: "areas", class: "areas",
                            div { id: "area-head", class: "box", "Header" }
                            div { id: "area-nav", class: "box", "Nav" }
                            div { id: "area-main", class: "box", "Main" }
                            div { id: "area-foot", class: "box", "Footer" }
                        }
                        div { id: "card-grid", class: "card-grid",
                            div { id: "card-1", class: "box", "Card 1" }
                            div { id: "card-2", class: "box", "Card 2" }
                            div { id: "card-3", class: "box", "Card 3" }
                            div { id: "card-4", class: "box", "Card 4" }
                            div { id: "card-5", class: "box", "Card 5" }
                            div { id: "card-6", class: "box", "Card 6" }
                        }
                        div { id: "span-grid", class: "span-grid",
                            div { id: "span-a", class: "box", "a" }
                            div { id: "span-b", class: "box", "b" }
                            div { id: "span-c", class: "box", "c" }
                            div { id: "span-d", class: "box", "d" }
                            div { id: "span-e", class: "box", "e" }
                            div { id: "span-f", class: "box", "f" }
                        }
                        div { id: "line-grid", class: "line-grid",
                            div { id: "line-a", class: "box", "a" }
                            div { id: "line-b", class: "box", "b" }
                            div { id: "line-c", class: "box", "c" }
                        }
                    }
                }
                section { id: "panel-position", class: "panel",
                    div { class: "panel-head",
                        h2 { id: "title-position", "Position" }
                    }
                    div { class: "demo-stack",
                        div { id: "stack", class: "stack",
                            div { id: "layer-red", class: "layer" }
                            div { id: "layer-green", class: "layer" }
                            div { id: "layer-blue", class: "layer" }
                        }
                        div { id: "anchor", class: "anchor",
                            p { "Relative parent" }
                            div { id: "pin" }
                            div { id: "badge" }
                        }
                    }
                }
            }
        }
    }
}
