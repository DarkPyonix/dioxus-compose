//! Dioxus screens written with HTML elements and CSS, laid out inside the Host.
//!
//! `blitz-dom` runs headless here: stylo resolves the CSS, Taffy lays out blocks, flex and
//! grid. The renderer never sees CSS. What it gets is a [`DisplayList`]: absolute
//! rectangles, colours, borders, radii, shadows, clips, text runs and form fields, in paint
//! order, keyed by node, with [`DisplayListDiff`] carrying only what changed after an
//! update.
//!
//! [`plan_from`] turns a display list into a tree of drawing elements (boxes placed at
//! offsets inside their containers, scroll containers, text, fields and images), and
//! [`diff`] says what changed between two such trees, node by node and modifier by
//! modifier.
//!
//! Nothing is fetched. An `<img src>` or a CSS `url()` is recorded as written, resolved
//! against the document's base URL when there is one, and what it draws is up to the
//! application's [`ImageResolver`]: it names the asset for a URL when the plan is built,
//! and may tell layout an image's natural size. Without one, an image draws nothing.
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

mod background;
mod build;
mod convert;
mod display_list;
mod dom;
mod image;
mod layout;
mod measure;
mod plan;
mod writer;

/// A node in the blitz-dom document. Stable for as long as the node is in the document.
pub type NodeId = usize;

pub use blitz_dom::BaseDocument;
pub use blitz_traits::shell::ColorScheme;
pub use display_list::{
    BackgroundImage, BackgroundLayer, Border, BorderLine, BoxShadow, Corners, DisplayList,
    DisplayListDiff, GradientStop, InputField, InputKind, LinearGradient, NodeEntry, ObjectFit,
    RadialGradient, Radius, Rect, ReplacedImage, Rgba, ScrollContainer, Sides, TextAlign, TextRun,
    TileRepeat,
};
pub use dom::{HtmlConfig, HtmlDom, element_by_id};
pub use image::{AssetId, ImageResolver, ImageSize};
pub use measure::{
    ParleyMeasurer, TextLineHeight, TextMeasureRequest, TextMeasurer, TextMetrics, TextStyle,
    TextWhiteSpace, WhiteSpaceCollapse, WidthConstraint,
};
pub use plan::{
    BorderSide, Brush, BrushId, ColourResolver, ColourUse, LiteralColours, ModifierSlot, Plan,
    PlanChange, PlanImage, PlanKey, PlanKind, PlanModifier, PlanNode, PlanText, PlanTextField,
    diff, plan_from, plan_from_images, plan_from_with,
};

/// Lays out a document that did not come from Dioxus (one parsed by `blitz-html`, say) at
/// the viewport it was configured with, and returns what to draw.
///
/// No image has a natural size, and `<img src>` is recorded as written. See
/// [`layout_document_with`].
pub fn layout_document(doc: &mut BaseDocument, measurer: &mut dyn TextMeasurer) -> DisplayList {
    layout_document_with(doc, measurer, None, None)
}

/// [`layout_document`], with natural image sizes from `images` and `<img src>` resolved
/// against `base_url`.
///
/// `base_url` should be the base the document was configured with, which blitz-dom does
/// not give back; CSS `url()` values have already been resolved against that by stylo.
/// Nothing is fetched here. Whether the document's own network provider fetches anything
/// is up to whoever configured it.
pub fn layout_document_with(
    doc: &mut BaseDocument,
    measurer: &mut dyn TextMeasurer,
    images: Option<&mut dyn ImageResolver>,
    base_url: Option<&str>,
) -> DisplayList {
    let mut lookup = image::ImageLookup::new(images, base_url);
    layout::resolve_styles(doc);
    image::apply_natural_sizes(doc, &mut lookup);
    let texts = layout::run_layout(doc, measurer);
    build::build_display_list(doc, &texts, &mut lookup)
}

/// What an app needs in scope to write `rsx!` with HTML elements.
pub mod prelude {
    pub use crate::{AssetId, HtmlConfig, HtmlDom, ImageResolver, ImageSize};
    pub use dioxus_core::{Element, VirtualDom};
    pub use dioxus_core_macro::rsx;
    pub use dioxus_html as dioxus_elements;
}
