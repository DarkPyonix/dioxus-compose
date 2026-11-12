//! `onresize`: after a layout pass, each element with a `resize` listener whose content box
//! size differs from the one it was last told about receives one event, the way a
//! `ResizeObserver` reports. Its first layout counts as a change.
//!
//! The sizes are the page's own CSS pixels: the layout runs in a viewport of the window
//! over the zoom factor, so nothing here multiplies by it.

use std::any::Any;
use std::collections::HashMap;

use blitz_dom::BaseDocument;
use dioxus_core::ElementId;
use dioxus_html::geometry::PixelsSize;
use dioxus_html::geometry::euclid::Size2D;
use dioxus_html::{HasResizeData, ResizeResult};

use super::writer::WriterState;
use crate::html::NodeId;

/// What a resize event this document sends carries.
#[derive(Clone, Copy)]
pub(crate) struct ResizeEventData {
    content: (f32, f32),
    border: (f32, f32),
}

impl HasResizeData for ResizeEventData {
    fn get_border_box_size(&self) -> ResizeResult<PixelsSize> {
        Ok(Size2D::new(
            f64::from(self.border.0),
            f64::from(self.border.1),
        ))
    }
    fn get_content_box_size(&self) -> ResizeResult<PixelsSize> {
        Ok(Size2D::new(
            f64::from(self.content.0),
            f64::from(self.content.1),
        ))
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// The content box size each observed element was last told about, and a buffer reused
/// for the events of one pass so that a pass with nothing to report allocates nothing.
#[derive(Default)]
pub(crate) struct ResizeObservers {
    reported: HashMap<NodeId, (f32, f32)>,
    pending: Vec<(ElementId, ResizeEventData)>,
}

impl ResizeObservers {
    /// Compares every listening element's content box with the size last reported and
    /// queues an event for each that differs. Returns the queued events, which the caller
    /// drains.
    pub(crate) fn collect(
        &mut self,
        doc: &BaseDocument,
        state: &WriterState,
    ) -> &mut Vec<(ElementId, ResizeEventData)> {
        self.pending.clear();
        let mut observed = 0;
        for (node, names) in state.listening() {
            if !names.contains(&"resize") {
                continue;
            }
            let Some(element) = state.element_of(node) else {
                continue;
            };
            let Some(data) = doc.get_node(node).filter(|n| n.flags.is_in_document()) else {
                continue;
            };
            observed += 1;
            let layout = &data.final_layout;
            let border = (layout.size.width, layout.size.height);
            let content = (
                (border.0
                    - layout.padding.left
                    - layout.padding.right
                    - layout.border.left
                    - layout.border.right)
                    .max(0.0),
                (border.1
                    - layout.padding.top
                    - layout.padding.bottom
                    - layout.border.top
                    - layout.border.bottom)
                    .max(0.0),
            );
            if self.reported.get(&node) == Some(&content) {
                continue;
            }
            self.reported.insert(node, content);
            self.pending
                .push((element, ResizeEventData { content, border }));
        }
        if self.reported.len() > observed {
            // An element stopped listening or left the document: forget it, so that it
            // is reported again, as on a first layout, if it comes back.
            self.reported.retain(|node, _| {
                state.listeners(*node).contains(&"resize")
                    && doc
                        .get_node(*node)
                        .is_some_and(|n| n.flags.is_in_document())
            });
        }
        &mut self.pending
    }

    /// The buffer [`ResizeObservers::collect`] fills, to hand an emptied one back.
    pub(crate) fn collect_buffer(&mut self) -> &mut Vec<(ElementId, ResizeEventData)> {
        &mut self.pending
    }
}
