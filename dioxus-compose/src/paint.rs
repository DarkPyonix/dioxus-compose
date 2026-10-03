//! What a laid-out HTML document asks a renderer to draw.
//!
//! [`display_list`] is the neutral, flat list of absolute rectangles in paint order;
//! [`build`] reads it off a laid-out blitz-dom document; [`plan`] turns it into the tree of
//! drawing elements that goes to the renderer, and diffs one such tree against the last;
//! [`bridge`] writes a plan into compose-rust's batch, a whole tree first and only what
//! changed after that.

pub(crate) mod bridge;
pub(crate) mod build;
pub(crate) mod display_list;
pub(crate) mod plan;
