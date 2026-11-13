//! Programmatic scrolling of the page's scroll containers, and what the page can read back.
//!
//! The renderer owns every scroll position. The page asks for a move through
//! `MountedData` (`scroll_to`, `scroll_to_with_options`, `scroll`), which queues a
//! [`ScrollRequest`] here; the runtime takes the queue after each render and writes the
//! requests into its batch. The renderer reports where a container ended up, the runtime
//! turns the report into the page's CSS pixels and hands it to
//! [`HtmlDom::scroll_changed`](crate::html::HtmlDom::scroll_changed), and
//! `get_scroll_offset` and `get_scroll_size` read the last one kept here.

use std::any::Any;
use std::collections::HashMap;

use compose_rust::schema::ScrollAlign;
use dioxus_html::HasScrollData;
use dioxus_html::{ScrollBehavior, ScrollLogicalPosition, ScrollToOptions};

use crate::html::NodeId;

/// Where a scroll container is, in the page's CSS pixels: the units the layout runs in,
/// before the zoom factor.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollPosition {
    /// How far the content is scrolled to the left (`scrollLeft`).
    pub x: f64,
    /// How far the content is scrolled up (`scrollTop`).
    pub y: f64,
    /// The width of the content, scrolled out of view or not (`scrollWidth`).
    pub scroll_width: f64,
    /// The height of the content (`scrollHeight`).
    pub scroll_height: f64,
    /// The width of the visible area (`clientWidth`).
    pub client_width: f64,
    /// The height of the visible area (`clientHeight`).
    pub client_height: f64,
}

/// One move the page asked for, waiting for the runtime to write it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ScrollRequest {
    /// Make `node` visible in every scroll container around it, innermost first.
    IntoView {
        node: NodeId,
        align_x: ScrollAlign,
        align_y: ScrollAlign,
        animate: bool,
    },
    /// Move the scroll container `node` to an offset in CSS pixels.
    To {
        node: NodeId,
        x: f64,
        y: f64,
        animate: bool,
    },
}

/// What the page's `MountedData` shares with the document.
#[derive(Default)]
pub(crate) struct Scrolls {
    /// The moves asked for since the runtime last took them, in order.
    pub(crate) requests: Vec<ScrollRequest>,
    /// The last position the renderer reported for each container.
    pub(crate) reported: HashMap<NodeId, ScrollPosition>,
    /// Each container's size from the last layout, for a container the renderer has not
    /// reported yet. Its offsets are zero.
    pub(crate) laid_out: HashMap<NodeId, ScrollPosition>,
}

impl Scrolls {
    /// The position `get_scroll_offset` and `get_scroll_size` answer with: the last report,
    /// or the layout's sizes at offset zero before the first one.
    pub(crate) fn position(&self, node: NodeId) -> Option<ScrollPosition> {
        self.reported
            .get(&node)
            .or_else(|| self.laid_out.get(&node))
            .copied()
    }
}

/// The alignments `scroll_to_with_options` asked for, as the renderer's.
///
/// `MountedData::scroll_to(behavior)` reaches the element as the options' defaults, block
/// `start` and inline `center`. That call means "make it visible", which is `nearest` on
/// both axes, so those two defaults together are read as it.
pub(crate) fn alignments(options: &ScrollToOptions) -> (ScrollAlign, ScrollAlign) {
    if matches!(
        (&options.vertical, &options.horizontal),
        (ScrollLogicalPosition::Start, ScrollLogicalPosition::Center)
    ) {
        return (ScrollAlign::Nearest, ScrollAlign::Nearest);
    }
    (align(&options.horizontal), align(&options.vertical))
}

fn align(position: &ScrollLogicalPosition) -> ScrollAlign {
    match position {
        ScrollLogicalPosition::Start => ScrollAlign::Start,
        ScrollLogicalPosition::Center => ScrollAlign::Center,
        ScrollLogicalPosition::End => ScrollAlign::End,
        ScrollLogicalPosition::Nearest => ScrollAlign::Nearest,
    }
}

/// Whether a move is animated: `smooth` is, `instant` jumps.
pub(crate) fn animated(behavior: ScrollBehavior) -> bool {
    behavior == ScrollBehavior::Smooth
}

/// What an `onscroll` handler receives.
#[derive(Clone, Copy)]
pub(crate) struct ScrollEventData(pub(crate) ScrollPosition);

impl HasScrollData for ScrollEventData {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn scroll_top(&self) -> f64 {
        self.0.y
    }

    fn scroll_left(&self) -> f64 {
        self.0.x
    }

    fn scroll_width(&self) -> i32 {
        self.0.scroll_width.round() as i32
    }

    fn scroll_height(&self) -> i32 {
        self.0.scroll_height.round() as i32
    }

    fn client_width(&self) -> i32 {
        self.0.client_width.round() as i32
    }

    fn client_height(&self) -> i32 {
        self.0.client_height.round() as i32
    }
}
