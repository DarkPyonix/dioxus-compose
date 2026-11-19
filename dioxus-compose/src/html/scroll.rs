//! The page's scroll requests on their way to the renderer, and the renderer's reports on
//! their way back.
//!
//! The renderer owns every scroll position and scrolls in its own physical pixels. A CSS
//! pixel is one dp of the page, which the renderer draws at the zoom factor `k` and then
//! at the screen's density, so one CSS pixel is `k × density` renderer pixels. Both arrive
//! from the renderer (`ZoomChanged` and `DensityChanged`), so a request to move to an
//! offset goes out in the frame it was asked in, and a request to bring an element into
//! view names the element's node and needs no conversion.

use compose_rust::Batch;
use compose_rust::schema::{Rect, ScrollAlign, ScrollIntoView, ScrollTarget, ScrollTo};

use crate::dom::{HtmlDom, ScrollRequest};
use crate::html::{NodeId, PlanKey};
use crate::paint::bridge::PlanBridge;

use super::page_zoom;

/// Renderer pixels per dp of the screen, as the renderer reported it. One before the first
/// report.
fn page_density() -> f64 {
    let scale = f64::from(compose_rust::density());
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

/// Renderer pixels per CSS pixel at the current zoom and screen density.
fn page_scale() -> f64 {
    page_density() * f64::from(page_zoom())
}

/// What one `ScrollChanged` report says, in renderer pixels.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Report {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) max_x: f64,
    pub(crate) max_y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

/// What the runtime keeps between frames for the page's scrolling.
#[derive(Default)]
pub(crate) struct ScrollSync {
    /// The requests taken from the document this frame. Kept to reuse its capacity.
    taken: Vec<ScrollRequest>,
}

impl ScrollSync {
    /// Writes the moves the page asked for into `batch`. Called after the bridge has
    /// written the frame's plan, so a container made in this frame has its node.
    pub(crate) fn write(&mut self, dom: &mut HtmlDom, bridge: &PlanBridge, batch: &mut Batch) {
        dom.take_scroll_requests(&mut self.taken);
        if self.taken.is_empty() {
            return;
        }
        let scale = page_scale();
        for request in self.taken.drain(..) {
            match request {
                ScrollRequest::IntoView {
                    node,
                    align_x,
                    align_y,
                    animate,
                } => into_view(dom, bridge, batch, node, (align_x, align_y), animate),
                ScrollRequest::To {
                    node,
                    x,
                    y,
                    animate,
                } => {
                    let column = bridge.node_id(PlanKey::ScrollColumn(node));
                    let row = bridge.node_id(PlanKey::ScrollRow(node));
                    if column.is_none() && row.is_none() {
                        // Not a scroll container: nothing to move.
                        continue;
                    }
                    // A box that scrolls both ways is two containers, one for each axis,
                    // and each is told only its own axis.
                    if let Some(column) = column {
                        batch.scroll_to(
                            column,
                            ScrollTo {
                                x_px: None,
                                y_px: Some(y * scale),
                                item: 0,
                                animate,
                            },
                        );
                    }
                    if let Some(row) = row {
                        batch.scroll_to(
                            row,
                            ScrollTo {
                                x_px: Some(x * scale),
                                y_px: None,
                                item: 0,
                                animate,
                            },
                        );
                    }
                }
            }
        }
    }

    /// Takes in the renderer's report on one axis of the scroll container `node`, and
    /// hands the page the container's position in CSS pixels.
    pub(crate) fn report(
        &mut self,
        dom: &mut HtmlDom,
        bridge: &PlanBridge,
        node: NodeId,
        horizontal: bool,
        report: Report,
    ) {
        let scale = page_scale();
        let mut position = dom.scroll_position(node).unwrap_or_default();
        // The axis the container scrolls is its own. A box that scrolls one way only has
        // one container, whose report also says the other axis.
        let both = bridge.node_id(PlanKey::ScrollColumn(node)).is_some()
            && bridge.node_id(PlanKey::ScrollRow(node)).is_some();
        if horizontal || !both {
            position.x = report.x / scale;
            position.scroll_width = (report.max_x.max(0.0) + report.width) / scale;
            position.client_width = report.width / scale;
        }
        if !horizontal || !both {
            position.y = report.y / scale;
            position.scroll_height = (report.max_y.max(0.0) + report.height) / scale;
            position.client_height = report.height / scale;
        }
        dom.scroll_changed(node, position);
    }
}

/// Writes one request for each scroll container that moves `node`, innermost first, then
/// one for the scroll the page itself sits in, as a browser's `scrollIntoView` moves the
/// viewport too. Each names the node `node` is drawn as, or the nearest box around it the
/// container holds.
fn into_view(
    dom: &HtmlDom,
    bridge: &PlanBridge,
    batch: &mut Batch,
    node: NodeId,
    (align_x, align_y): (ScrollAlign, ScrollAlign),
    animate: bool,
) {
    let Some((boxes, containers)) = dom.scroll_chain(node) else {
        return;
    };
    let request = |target: u32| ScrollIntoView {
        target: ScrollTarget::Node(target, Rect::default()),
        align_x,
        align_y,
        animate,
    };
    for container in containers {
        // The row of a box that scrolls both ways is inside its column.
        for key in [
            PlanKey::ScrollRow(container),
            PlanKey::ScrollColumn(container),
        ] {
            let Some(id) = bridge.node_id(key) else {
                continue;
            };
            let target = boxes.iter().find_map(|&candidate| {
                let candidate = PlanKey::Node(candidate);
                if bridge.is_inside(candidate, key) {
                    bridge.node_id(candidate)
                } else {
                    None
                }
            });
            if let Some(target) = target {
                batch.scroll_into_view(id, request(target));
            }
        }
    }
    if let Some(page) = bridge.root_node()
        && let Some(target) = boxes
            .iter()
            .find_map(|&candidate| bridge.node_id(PlanKey::Node(candidate)))
    {
        batch.scroll_into_view(page, request(target));
    }
}
