//! Images: what the application tells this crate about them, and how an `<img>` gets its
//! natural size without anything being fetched.
//!
//! This crate never loads an image. A `src` attribute or a CSS `url()` is recorded as
//! written, resolved against the document's base URL when there is one, and the
//! application decides what it draws through an [`ImageResolver`]: it may have the bytes
//! already, fetch them itself, or refuse. Without a resolver no picture is drawn. An `<img>`
//! with no picture is drawn the way Chromium draws an image it could not load: with alt text,
//! as the broken-image icon and that text, sized by them on each axis its CSS leaves `auto`;
//! with an empty `alt`, as nothing, at whatever size its attributes and CSS give it.
//!
//! blitz-dom sizes an `<img>` from the decoded image it keeps on the element
//! (`SpecialElementData::Image`), which it fills in when its network provider delivers the
//! bytes. The document here is given a provider that never fetches, and the size comes
//! from the resolver instead: [`apply_natural_sizes`] writes an image of the size the
//! resolver answered, with no pixels, onto each `<img>` before layout, and blitz-dom's
//! replaced-element sizing reads only the width and height of it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use blitz_dom::node::{ImageData, RasterImageData, SpecialElementData};
use blitz_dom::{BaseDocument, NodeData};
use blitz_traits::net::Url;
use style::values::computed::url::ComputedUrl;

use compose_rust::schema::AssetKind;

use crate::layout::local;
use crate::paint::display_list::Rgba;

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
    /// for it, in which case no picture is drawn: an `<img>` with alt text shows the
    /// broken-image icon and its alt text instead, and one without draws nothing. Asked
    /// while a plan is built, never during layout.
    fn resolve(&mut self, url: &str) -> Option<AssetId>;

    /// The natural size of the image at `url`, when the application knows it without
    /// fetching anything. Asked during layout: an `<img>` with no `width` or `height` of its
    /// own is laid out at this size, and `background-size` resolves against it. `None`, the
    /// default, leaves the image without a natural size, and an `<img>` without one is laid
    /// out as Chromium lays out a broken image: by its alt text and the broken-image icon,
    /// or at no size when its `alt` is empty.
    fn size(&mut self, _url: &str) -> Option<ImageSize> {
        None
    }

    /// The source of the image at `url`, when it is an SVG that should take the `<img>`
    /// element's CSS `color` wherever it says `currentColor`, the way an inline `<svg>`
    /// does in a browser. Asked while a plan is built, before [`resolve`](Self::resolve).
    ///
    /// An SVG that names `currentColor` is registered with the renderer once per colour it
    /// is drawn in, with every `currentColor` written as that colour, and drawn from that
    /// registration; [`resolve`](Self::resolve) is not asked for it. A change of the
    /// element's `color` draws the registration for the new colour. An SVG that does not
    /// name `currentColor`, and `None`, the default, leave the image to
    /// [`resolve`](Self::resolve).
    fn svg(&mut self, _url: &str) -> Option<&'static [u8]> {
        None
    }
}

/// How many colours one SVG keeps registered at once. A colour animation draws a new colour
/// on every frame, and each one is a registration the renderer keeps a picture for, so past
/// this many the least recently drawn is let go.
pub(crate) const COLOURS_PER_SVG: usize = 16;

/// One colour an SVG is registered in, and the plan that last drew it.
struct Tint {
    color: [u8; 4],
    id: u32,
    drawn_in: u64,
}

/// What is known about one source SVG: whether it can be tinted at all, and the colours it
/// is registered in.
#[derive(Default)]
struct Source {
    untintable: bool,
    tints: Vec<Tint>,
}

#[derive(Default)]
struct Tinted {
    /// Counts plan builds, so a colour drawn in the plan being built is never let go.
    plan: u64,
    sources: HashMap<(usize, usize), Source>,
}

thread_local! {
    static TINTED: RefCell<Tinted> = RefCell::new(Tinted::default());
}

/// Marks the start of a plan build. Every colour drawn from here until the next call
/// belongs to this plan and stays registered while it is being built.
pub(crate) fn begin_plan() {
    TINTED.with_borrow_mut(|tinted| tinted.plan += 1);
}

/// `svg` with every `currentColor` written as `color`, registered with the renderer, or
/// `None` when `svg` is not UTF-8 or never names `currentColor`.
///
/// The answer is kept per source and colour, so a plan built again for the same image in
/// the same colour neither rewrites the bytes nor looks through the renderer's registry.
/// Each source keeps at most [`COLOURS_PER_SVG`] colours: a new one past that releases the
/// one drawn longest ago, unless every one of them is drawn in the current plan, in which
/// case the plan's colours all stay until a later plan stops drawing some.
pub(crate) fn current_color_asset(svg: &'static [u8], color: Rgba) -> Option<AssetId> {
    let key = (svg.as_ptr() as usize, svg.len());
    let color = [color.r, color.g, color.b, color.a];
    TINTED.with_borrow_mut(|tinted| {
        let plan = tinted.plan;
        let source = tinted.sources.entry(key).or_default();
        if source.untintable {
            return None;
        }
        if let Some(tint) = source.tints.iter_mut().find(|tint| tint.color == color) {
            tint.drawn_in = plan;
            return Some(AssetId(tint.id));
        }
        let rgba = Rgba::new(color[0], color[1], color[2], color[3]);
        let Some(text) = std::str::from_utf8(svg)
            .ok()
            .and_then(|text| with_current_color(text, rgba))
        else {
            source.untintable = true;
            return None;
        };
        if source.tints.len() >= COLOURS_PER_SVG {
            let oldest = source
                .tints
                .iter()
                .enumerate()
                .filter(|(_, tint)| tint.drawn_in < plan)
                .min_by_key(|(_, tint)| tint.drawn_in)
                .map(|(at, _)| at);
            if let Some(at) = oldest {
                let gone = source.tints.swap_remove(at);
                compose_rust::asset::release_owned(gone.id, AssetKind::Svg);
            }
        }
        let id = compose_rust::asset::register_owned(AssetKind::Svg, text.into_bytes());
        source.tints.push(Tint {
            color,
            id,
            drawn_in: plan,
        });
        Some(AssetId(id))
    })
}

/// `text` with every `currentColor`, in any case as CSS allows, written as `color` in hex,
/// or `None` when it has none.
pub(crate) fn with_current_color(text: &str, color: Rgba) -> Option<String> {
    const KEYWORD: &str = "currentcolor";
    // Lowering ASCII keeps every byte where it was, so a match in the lowered text is at
    // the same place in the original.
    let lowered = text.to_ascii_lowercase();
    if !lowered.contains(KEYWORD) {
        return None;
    }
    let hex = if color.a == u8::MAX {
        format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
    } else {
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            color.r, color.g, color.b, color.a
        )
    };
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for (start, _) in lowered.match_indices(KEYWORD) {
        out.push_str(&text[at..start]);
        out.push_str(&hex);
        at = start + KEYWORD.len();
    }
    out.push_str(&text[at..]);
    Some(out)
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

/// The side of the broken-image icon an `<img>` shows beside its alt text, in CSS pixels:
/// the size Chromium draws it at.
pub(crate) const BROKEN_IMAGE_ICON: f32 = 16.0;

/// The text an `<img>` shows in the place of an image it has nothing to draw with: its `alt`
/// with white space collapsed.
///
/// An `<img>` that names a source and has no natural size (the resolver answered none, or
/// there is no resolver) is drawn the way Chromium draws an image it could not load: the
/// broken-image icon and the alt text, as a box of text rather than a picture. `None` for an
/// image with a natural size, for one with no `src`, and for one whose `alt` is empty or
/// missing, which stands for nothing and keeps the size its CSS gives it.
pub(crate) fn fallback_alt(node: &blitz_dom::Node) -> Option<String> {
    let src = img_src(node)?;
    if src
        .trim_matches(|c: char| c.is_ascii_whitespace())
        .is_empty()
    {
        return None;
    }
    let element = node.element_data()?;
    let sized = element
        .raster_image_data()
        .is_some_and(|image| image.width > 0 && image.height > 0);
    if sized {
        return None;
    }
    let alt = element
        .attr(local("alt"))?
        .split_ascii_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!alt.is_empty()).then_some(alt)
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
