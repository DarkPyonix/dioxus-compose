//! Images: what the application tells this crate about them, and how an `<img>` gets its
//! natural size without anything being fetched.
//!
//! This crate never loads an image. A `src` attribute or a CSS `url()` is recorded as
//! written, resolved against the document's base URL when there is one, and the
//! application decides what it draws through an [`ImageResolver`]: it may have the bytes
//! already, fetch them itself, or refuse. Without a resolver an image draws nothing, and an
//! `<img>` takes whatever size its attributes and CSS give it.
//!
//! blitz-dom sizes an `<img>` from the decoded image it keeps on the element
//! (`SpecialElementData::Image`), which it fills in when its network provider delivers the
//! bytes. The document here is given a provider that never fetches, and the size comes
//! from the resolver instead: [`apply_natural_sizes`] writes an image of the size the
//! resolver answered, with no pixels, onto each `<img>` before layout, and blitz-dom's
//! replaced-element sizing reads only the width and height of it.

use std::sync::Arc;

use blitz_dom::node::{ImageData, RasterImageData, SpecialElementData};
use blitz_dom::{BaseDocument, NodeData};
use blitz_traits::net::Url;
use style::values::computed::url::ComputedUrl;

use crate::layout::local;

/// An image the application registered with the renderer, as the application numbers it.
/// Opaque to this crate: it is handed back in the plan and never looked into.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AssetId(pub u32);

/// The natural size of an image, in CSS pixels.
///
/// Whole pixels, because that is what blitz-dom's replaced-element sizing reads. An image
/// meant for a screen of two device pixels per CSS pixel is half its pixel size here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageSize {
    pub width: u32,
    pub height: u32,
}

impl ImageSize {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

/// What the application knows about the images a document names.
///
/// URLs are passed as the display list records them: as written, resolved against the
/// document's base URL when there is one.
pub trait ImageResolver {
    /// The asset that draws the image at `url`, or `None` when the application has none
    /// for it, in which case the image draws nothing. Asked while a plan is built, never
    /// during layout.
    fn resolve(&mut self, url: &str) -> Option<AssetId>;

    /// The natural size of the image at `url`, when the application knows it without
    /// fetching anything. Asked during layout: an `<img>` with no `width` or `height` of its
    /// own is laid out at this size, and `background-size` resolves against it. `None`, the
    /// default, leaves the image without a natural size.
    fn size(&mut self, _url: &str) -> Option<ImageSize> {
        None
    }
}

/// The base URL a document is given when the application names none.
///
/// blitz-dom resolves every `<img src>` against the document's base URL as soon as the
/// element is attached, and panics when it cannot. Its own default base is a `data:` URL,
/// against which no relative URL resolves, so `<img src="logo.png">` would take the process
/// down. This base resolves anything, and [`as_written`] takes it off again, so a relative
/// URL reaches the application as it was written.
pub(crate) const UNRESOLVED_BASE: &str = "dioxus-compose:/unresolved/";
const UNRESOLVED_SCHEME: &str = "dioxus-compose:";
const UNRESOLVED_PATH: &str = "/unresolved/";

/// A URL with [`UNRESOLVED_BASE`] taken off again.
///
/// A path relative to the document comes back as written, a path from the root keeps its
/// leading slash and a scheme-relative URL its two. A relative path that climbs above the
/// document (`../logo.png`) has nowhere to climb to and comes back from the root.
pub(crate) fn as_written(url: &str) -> String {
    match url.strip_prefix(UNRESOLVED_SCHEME) {
        None => url.to_string(),
        Some(rest) => rest
            .strip_prefix(UNRESOLVED_PATH)
            .unwrap_or(rest)
            .to_string(),
    }
}

/// The URL a CSS `url()` names. stylo resolves it against the document's base when it can;
/// when it cannot it keeps the text as written.
pub(crate) fn css_url(url: &ComputedUrl) -> Option<String> {
    let text = match url {
        ComputedUrl::Valid(url) => url.as_str().to_string(),
        ComputedUrl::Invalid(text) => text.as_str().to_string(),
    };
    let text = as_written(text.trim());
    (!text.is_empty()).then_some(text)
}

/// The images a layout pass may ask about, and the base their URLs resolve against.
pub(crate) struct ImageLookup<'a> {
    resolver: Option<&'a mut dyn ImageResolver>,
    base: Option<Url>,
}

impl<'a> ImageLookup<'a> {
    pub(crate) fn new(resolver: Option<&'a mut dyn ImageResolver>, base: Option<&str>) -> Self {
        Self {
            resolver,
            base: base.and_then(|base| Url::parse(base).ok()),
        }
    }

    /// The URL an `<img src>` names: resolved against the base when there is one, as
    /// written otherwise. `None` for an empty `src`, which names no image.
    pub(crate) fn source(&self, raw: &str) -> Option<String> {
        // HTML strips leading and trailing ASCII white space from a URL attribute.
        let raw = raw.trim_matches(|c: char| c.is_ascii_whitespace());
        if raw.is_empty() {
            return None;
        }
        let resolved = match &self.base {
            Some(base) => match base.join(raw) {
                Ok(url) => url.to_string(),
                Err(_) => raw.to_string(),
            },
            None => raw.to_string(),
        };
        Some(as_written(&resolved))
    }

    /// The natural size of the image at `url`, from the resolver; `None` without one.
    pub(crate) fn size(&mut self, url: &str) -> Option<ImageSize> {
        self.resolver.as_mut()?.size(url)
    }
}

/// The `src` of an `<img>` element.
pub(crate) fn img_src(node: &blitz_dom::Node) -> Option<&str> {
    let element = node.element_data()?;
    if !matches!(node.data, NodeData::Element(_)) || &*element.name.local != "img" {
        return None;
    }
    element.attr(local("src"))
}

/// Gives every `<img>` in the document the natural size the resolver answers for its
/// `src`, or none, before layout.
///
/// blitz-dom's replaced-element sizing (`compute_child_layout` in its `layout/mod.rs`)
/// reads the width and height of the `RasterImageData` on the element and nothing else, so
/// an image with no pixels at the answered size lays the element out exactly as the decoded
/// image would. Nothing is ever fetched: the document's network provider drops every
/// request, and an `<img>` the resolver cannot size keeps no image at all, as one whose
/// fetch failed would.
pub(crate) fn apply_natural_sizes(doc: &mut BaseDocument, lookup: &mut ImageLookup<'_>) {
    let images: Vec<(usize, Option<String>)> = doc
        .tree()
        .iter()
        .filter(|(_, node)| node.flags.is_in_document())
        .filter_map(|(id, node)| {
            let element = node.element_data()?;
            if !matches!(node.data, NodeData::Element(_)) || &*element.name.local != "img" {
                return None;
            }
            Some((id, img_src(node).and_then(|src| lookup.source(src))))
        })
        .collect();
    for (id, source) in images {
        let size = source.and_then(|url| lookup.size(&url));
        let data = match size {
            Some(size) => ImageData::Raster(RasterImageData::new(
                size.width,
                size.height,
                Arc::new(Vec::new()),
            )),
            None => ImageData::None,
        };
        if let Some(element) = doc
            .get_node_mut(id)
            .and_then(|node| node.element_data_mut())
        {
            element.special_data = SpecialElementData::Image(Box::new(data));
        }
    }
}
