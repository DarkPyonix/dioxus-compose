//! The page's scroll requests on their way to the renderer, and the renderer's reports on
//! their way back.
//!
//! The renderer owns every scroll position and scrolls in its own physical pixels. A CSS
//! pixel is one dp of the page, which the renderer draws at the zoom factor `k` and then
//! at the screen's density, so one CSS pixel is `k × density` renderer pixels. The Host is
//! told `k` and is not told the density. Every report carries the container's viewport in
//! renderer pixels, and the bridge knows the size it gave that container in CSS pixels, so
//! the density is read off the first report. An offset asked for before any report waits
//! for it; a request to bring an element into view names the element's node and needs no
//! conversion, so it goes out at once.

use compose_rust::Batch;
use compose_rust::schema::{Rect, ScrollAlign, ScrollIntoView, ScrollTarget, ScrollTo};

use crate::dom::{HtmlDom, ScrollRequest};
use crate::html::{NodeId, PlanKey};
use crate::paint::bridge::PlanBridge;

use super::page_zoom;

/// A density this close to a multiple of an eighth is taken to be that multiple. Every
/// common screen density is one (1, 1.25, 1.5, 2, 2.625, 3), and the renderer rounds the
/// viewport to whole pixels, which leaves the measured ratio a little off.
const DENSITY_SNAP: f64 = 0.01;

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
    /// Moves to an offset asked for before the first report, which is what says how many
    /// renderer pixels a CSS pixel is.
    held: Vec<ScrollRequest>,
    /// Renderer pixels per dp, before the zoom factor.
    density: Option<f64>,
}

impl ScrollSync {
    /// Renderer pixels per CSS pixel at the current zoom, once a report has said.
    fn scale(&self) -> Option<f64> {
        self.density.map(|density| density * f64::from(page_zoom()))
    }

    /// Writes the moves the page asked for into `batch`. Called after the bridge has
    /// written the frame's plan, so a container made in this frame has its node.
    pub(crate) fn write(&mut self, dom: &mut HtmlDom, bridge: &PlanBridge, batch: &mut Batch) {
        dom.take_scroll_requests(&mut self.taken);
        if self.taken.is_empty() && (self.held.is_empty() || self.density.is_none()) {
            return;
        }
        let scale = self.scale();
        let mut requests = std::mem::take(&mut self.held);
        requests.append(&mut self.taken);
        for request in requests.drain(..) {
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
                    let Some(scale) = scale else {
                        self.held.push(request);
                        continue;
                    };
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
        self.taken = requests;
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
        let key = if horizontal {
            PlanKey::ScrollRow(node)
        } else {
            PlanKey::ScrollColumn(node)
        };
        if let Some(size) = bridge.required_size(key) {
            self.learn_density(size, &report);
        }
        let scale = self.scale().unwrap_or_else(|| f64::from(page_zoom()));
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

    /// Reads the density off a report: the viewport in renderer pixels over the size the
    /// container was given, along its longer side, which rounding moves the least.
    fn learn_density(&mut self, (width, height): (f32, f32), report: &Report) {
        let (pixels, css) = if height >= width {
            (report.height, f64::from(height))
        } else {
            (report.width, f64::from(width))
        };
        let k = f64::from(page_zoom());
        if !(css > 0.0 && pixels > 0.0 && pixels.is_finite()) {
            return;
        }
        let measured = pixels / (css * k);
        let snapped = (measured * 8.0).round() / 8.0;
        self.density = Some(
            if snapped > 0.0 && (measured - snapped).abs() <= DENSITY_SNAP * snapped {
                snapped
            } else {
                measured
            },
        );
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
