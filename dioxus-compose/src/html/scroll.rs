//! The page's scroll requests on their way to the renderer, and the renderer's reports on
//! their way back.
//!
//! The renderer owns every scroll position and scrolls in its own physical pixels. A CSS
//! pixel is one dp of the page, which the renderer draws at the zoom factor `k` and then
//! at the screen's density, so one CSS pixel is `k × density` renderer pixels. Both arrive
//! from the renderer (`ZoomChanged` and `DensityChanged`). A request to bring an element
//! into view names the element's node and needs no conversion, so it goes out in the frame
//! it was asked in. A request to move to an offset is held only while the screen's density
//! has not been reported yet (such as a scroll asked from `onmounted` during the first
//! batch, before the renderer's `DensityReporter` side effect has run), and goes out as
//! soon as the density is known.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

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
pub(crate) struct ScrollSync {
    /// Whether the screen's density is known: either a `DensityChanged` report arrived
    /// (which already set a non-1.0 density before the runtime started, or notified our
    /// subscription), or the renderer sent a `ScrollChanged` report (which only happens
    /// after the first composition, when `DensityReporter`'s side effect has already run).
    density_known: Arc<AtomicBool>,
    /// Offset requests made before the screen's density was known. Written on the first
    /// frame after it is known.
    held: Vec<ScrollRequest>,
    /// The requests taken from the document this frame. Kept to reuse its capacity.
    taken: Vec<ScrollRequest>,
}

impl ScrollSync {
    pub(crate) fn new() -> Self {
        let initial = compose_rust::density();
        let known = initial.is_finite()
            && initial > 0.0
            && initial != compose_rust::density::DEFAULT_DENSITY;
        Self {
            density_known: Arc::new(AtomicBool::new(known)),
            held: Vec::new(),
            taken: Vec::new(),
        }
    }

    /// Subscribes to `compose_rust::density` so that a density report marks the density
    /// as known and wakes a frame to flush any held scroll offset requests.
    pub(crate) fn subscribe_density(&self) -> compose_rust::density::DensitySubscription {
        let flag = self.density_known.clone();
        compose_rust::density::subscribe(Arc::new(move || {
            flag.store(true, Ordering::Relaxed);
        }))
    }

    fn is_density_known(&self) -> bool {
        if self.density_known.load(Ordering::Relaxed) {
            return true;
        }
        let scale = compose_rust::density();
        if scale.is_finite() && scale > 0.0 && scale != compose_rust::density::DEFAULT_DENSITY {
            self.density_known.store(true, Ordering::Relaxed);
            return true;
        }
        false
    }

    /// Writes the moves the page asked for into `batch`. Called after the bridge has
    /// written the frame's plan, so a container made in this frame has its node.
    pub(crate) fn write(&mut self, dom: &mut HtmlDom, bridge: &PlanBridge, batch: &mut Batch) {
        dom.take_scroll_requests(&mut self.taken);
        if self.held.is_empty() && self.taken.is_empty() {
            return;
        }
        if !self.is_density_known() {
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
                        if bridge.node_id(PlanKey::ScrollColumn(node)).is_some()
                            || bridge.node_id(PlanKey::ScrollRow(node)).is_some()
                        {
                            self.held.push(ScrollRequest::To {
                                node,
                                x,
                                y,
                                animate,
                            });
                        }
                    }
                }
            }
            return;
        }
        let scale = page_scale();
        for request in self.held.drain(..).chain(self.taken.drain(..)) {
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
        // Any scroll report from the renderer arrives after the first composition, when
        // DensityReporter's side effect has already reported the window's density.
        self.density_known.store(true, Ordering::Relaxed);
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
