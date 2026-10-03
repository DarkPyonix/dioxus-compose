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
//! Events go back to the Dioxus handlers of the DOM nodes they land on: clicks through
//! [`HtmlDom::click`], with what a browser does after a click (a checkbox ticks, a submit
//! button submits its form), and the `input`, `change` and `submit` events of form controls
//! through [`HtmlDom::input`], [`HtmlDom::change`], [`HtmlDom::select`],
//! [`HtmlDom::check`] and [`HtmlDom::submit`], which a renderer calls with what the user
//! committed in its own fields. Keys go to the focused element's `keydown`, `keypress` and
//! `keyup` handlers through [`HtmlDom::key_down`] and [`HtmlDom::key_up`], and focus to its
//! `focus`, `blur`, `focusin` and `focusout` handlers through [`HtmlDom::focus`] and
//! [`HtmlDom::blur`]. `prefers-color-scheme` is answered with the configured scheme until
//! [`HtmlDom::set_color_scheme`] changes it.
//!
//! [`launch`] runs such an app in a window, the way [`crate::launch`] runs one written with
//! the Compose widgets: every frame that changes something is laid out, planned, and
//! written to compose-rust by a [`PlanBridge`], which sends only what differs from the
//! frame before. [`HtmlRuntime`] is that path as a compose-rust runtime, for a Host built
//! by hand.
//!
//! The names live here rather than at the crate root because the Compose widget path
//! already uses some of them there for something else: [`Brush`], [`ColorScheme`] and
//! [`TextAlign`] are the HTML path's own, and the crate root's are the widgets'. The code
//! behind them is in the crate's `dom`, `layout` and `paint` modules.
//!
//! ```ignore
//! use dioxus_compose::html::prelude::*;
//!
//! fn app() -> Element {
//!     rsx! { div { style: "display: flex; gap: 8px", span { "Hello" } } }
//! }
//!
//! let mut dom = HtmlDom::new(app);
//! let list = dom.layout(800.0, 600.0, 2.0);
//! ```

use crate::dom;
use crate::layout::{self, image};
use crate::paint::build;

mod runtime;

/// A node in the blitz-dom document. Stable for as long as the node is in the document.
pub type NodeId = usize;

pub use crate::dom::{HtmlConfig, HtmlDom, element_by_id};
pub use crate::layout::image::{AssetId, ImageResolver, ImageSize};
pub use crate::layout::measure::{
    ParleyMeasurer, TextLineHeight, TextMeasureRequest, TextMeasurer, TextMetrics, TextStyle,
    TextWhiteSpace, WhiteSpaceCollapse, WidthConstraint,
};
pub use crate::paint::bridge::{BridgeEvent, BridgeHandler, ClickTargets, PlanBridge, widget_for};
pub use crate::paint::display_list::{
    BackgroundImage, BackgroundLayer, Border, BorderLine, BoxShadow, Corners, DisplayList,
    DisplayListDiff, GradientStop, InputField, InputKind, LinearGradient, NodeEntry, ObjectFit,
    RadialGradient, Radius, Rect, ReplacedImage, Rgba, ScrollContainer, Sides, TextAlign,
    TextDecoration, TextRun, TileRepeat,
};
pub use crate::paint::plan::{
    BorderSide, Brush, BrushId, ColourResolver, ColourUse, LiteralColours, ModifierSlot, Plan,
    PlanChange, PlanDropdown, PlanImage, PlanKey, PlanKind, PlanModifier, PlanNode, PlanText,
    PlanTextField, diff, plan_from, plan_from_images, plan_from_with,
};
pub use blitz_dom::BaseDocument;
pub use blitz_traits::shell::ColorScheme;
pub use runtime::{HtmlRuntime, launch, runtime_for};

/// Adds the user-agent rules this crate relies on for form controls to a document that
/// did not come from an [`HtmlDom`]: a `<select>` is a box of its own, and its options are
/// listed by the renderer's dropdown rather than drawn in the page. Call it once, before
/// the first layout. An [`HtmlDom`] does this for its own document.
pub fn add_form_control_styles(doc: &mut BaseDocument) {
    doc.add_user_agent_stylesheet(dom::FORM_CONTROLS_CSS);
}

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
///
/// This is not the crate's own [`prelude`](crate::prelude): that one names the Compose
/// widgets as `dioxus_elements`, and this one names the HTML elements, so an `rsx!` block
/// takes one or the other.
pub mod prelude {
    pub use super::{AssetId, HtmlConfig, HtmlDom, ImageResolver, ImageSize, launch};
    pub use dioxus_core::{Element, VirtualDom};
    pub use dioxus_core_macro::rsx;
    pub use dioxus_html as dioxus_elements;
    // The crates `rsx!` expands into references to, under the names it expands into, for
    // the same reason the crate's own prelude carries them: an application that added only
    // `dioxus-compose` has neither in its dependency graph by name.
    pub use dioxus_core;
    pub use dioxus_signals;
}
