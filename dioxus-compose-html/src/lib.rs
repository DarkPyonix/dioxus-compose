//! Dioxus screens written with HTML elements and CSS, laid out inside the Host.
//!
//! `blitz-dom` runs headless here: stylo resolves the CSS, Taffy lays out blocks, flex and
//! grid. The renderer never sees CSS. What it gets is a [`DisplayList`]: absolute
//! rectangles, colours, borders, radii, shadows, clips, text runs and form fields, in paint
//! order, keyed by node, with [`DisplayListDiff`] carrying only what changed after an
//! update.
//!
//! [`plan_from`] turns a display list into a tree of drawing elements (boxes placed at
//! offsets inside their containers, scroll containers, text and fields), and [`diff`] says
//! what changed between two such trees, node by node and modifier by modifier.
//!
//! Text is sized by a [`TextMeasurer`] the caller supplies, so the engine that draws the
//! text can also be the one that decides how much room it takes. [`ParleyMeasurer`] is the
//! default.
//!
//! JavaScript is never run. A `<script>` element is an ordinary element that the
//! user-agent stylesheet hides.
//!
//! ```ignore
//! use dioxus_compose_html::prelude::*;
//!
//! fn app() -> Element {
//!     rsx! { div { style: "display: flex; gap: 8px", span { "Hello" } } }
//! }
//!
//! let mut dom = HtmlDom::new(app);
//! let list = dom.layout(800.0, 600.0, 2.0);
//! ```

mod build;
mod convert;
mod display_list;
mod dom;
mod layout;
mod measure;
mod plan;
mod writer;

/// A node in the blitz-dom document. Stable for as long as the node is in the document.
pub type NodeId = usize;

pub use blitz_dom::BaseDocument;
pub use blitz_traits::shell::ColorScheme;
pub use display_list::{
    Border, BorderLine, BoxShadow, Corners, DisplayList, DisplayListDiff, InputField, InputKind,
    NodeEntry, Radius, Rect, Rgba, ScrollContainer, Sides, TextAlign, TextRun,
};
pub use dom::{HtmlConfig, HtmlDom, element_by_id};
pub use measure::{
    ParleyMeasurer, TextLineHeight, TextMeasureRequest, TextMeasurer, TextMetrics, TextStyle,
    TextWhiteSpace, WhiteSpaceCollapse, WidthConstraint,
};
pub use plan::{
    BorderSide, ColourResolver, ColourUse, LiteralColours, ModifierSlot, Plan, PlanChange,
    PlanImage, PlanKey, PlanKind, PlanModifier, PlanNode, PlanText, PlanTextField, diff, plan_from,
    plan_from_with,
};

/// Lays out a document that did not come from Dioxus (one parsed by `blitz-html`, say) at
/// the viewport it was configured with, and returns what to draw.
pub fn layout_document(doc: &mut BaseDocument, measurer: &mut dyn TextMeasurer) -> DisplayList {
    layout::resolve_styles(doc);
    let texts = layout::run_layout(doc, measurer);
    build::build_display_list(doc, &texts)
}

/// What an app needs in scope to write `rsx!` with HTML elements.
pub mod prelude {
    pub use crate::{HtmlConfig, HtmlDom};
    pub use dioxus_core::{Element, VirtualDom};
    pub use dioxus_core_macro::rsx;
    pub use dioxus_html as dioxus_elements;
}
