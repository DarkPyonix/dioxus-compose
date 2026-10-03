//! Background layers and `<img>` placement, read from stylo's computed values and resolved
//! to rectangles, angles and stop offsets.
//!
//! Follows CSS Backgrounds 3 (sizing, positioning, repeating, the painting and positioning
//! areas) and CSS Images 3 (gradient lines, ending shapes, colour stop fixup, `object-fit`).

use blitz_dom::Node;
use style::properties::ComputedValues;
use style::properties::generated::longhands::background_clip::single_value::computed_value::T as BackgroundClip;
use style::properties::generated::longhands::background_origin::single_value::computed_value::T as BackgroundOrigin;
use style::properties::generated::longhands::object_fit::computed_value::T as StyloObjectFit;
use style::values::computed::{
    BackgroundSize, Color, Gradient, Image, Length, LengthPercentage, LineDirection,
    NonNegativeLength, NonNegativeLengthPercentage,
};
use style::values::generics::background::GenericBackgroundSize;
use style::values::generics::image::{
    Circle, Ellipse, EndingShape, GenericGradient, GradientCompatMode, GradientFlags, GradientItem,
    ShapeExtent,
};
use style::values::generics::length::LengthPercentageOrAuto;
use style::values::specified::background::BackgroundRepeatKeyword;
use style::values::specified::position::{HorizontalPositionKeyword, VerticalPositionKeyword};

use crate::convert;
use crate::display_list::{
    BackgroundImage, BackgroundLayer, GradientStop, LinearGradient, ObjectFit, RadialGradient,
    Rect, ReplacedImage, Rgba, TileRepeat,
};
use crate::image::{ImageLookup, css_url, img_src};
use crate::layout::local;

/// The three boxes of an element, in document coordinates.
pub(crate) struct Boxes {
    pub border: Rect,
    pub padding: Rect,
    pub content: Rect,
}

/// What a layer's image turned out to be once `image-set()` has picked its candidate.
enum Source<'a> {
    Url {
        url: String,
        /// Natural size in CSS pixels, when the resolver knows it.
        natural: Option<(f32, f32)>,
    },
    Gradient(&'a Gradient),
}

/// The `background-image` layers of a box, in CSS order (the first is on top), each
/// resolved against the box. Layers that draw nothing (`none`, a zero-sized tile, an empty
/// painting area) are left out.
pub(crate) fn background_layers(
    style: &ComputedValues,
    boxes: &Boxes,
    lookup: &mut ImageLookup<'_>,
) -> Vec<BackgroundLayer> {
    let images = style.clone_background_image();
    let clips = style.clone_background_clip();
    let origins = style.clone_background_origin();
    let sizes = style.clone_background_size();
    let repeats = style.clone_background_repeat();
    let xs = style.clone_background_position_x();
    let ys = style.clone_background_position_y();

    // CSS repeats the shorter lists to match the number of images.
    fn nth<T>(list: &[T], index: usize) -> Option<&T> {
        (!list.is_empty()).then(|| &list[index % list.len()])
    }

    let mut layers = Vec::new();
    for (index, image) in images.0.iter().enumerate() {
        let Some(source) = source_of(image, 1.0, lookup) else {
            continue;
        };
        let area = match nth(&clips.0, index)
            .copied()
            .unwrap_or(BackgroundClip::BorderBox)
        {
            BackgroundClip::BorderBox => boxes.border,
            BackgroundClip::PaddingBox => boxes.padding,
            BackgroundClip::ContentBox => boxes.content,
        };
        let positioning = match nth(&origins.0, index)
            .copied()
            .unwrap_or(BackgroundOrigin::PaddingBox)
        {
            BackgroundOrigin::BorderBox => boxes.border,
            BackgroundOrigin::PaddingBox => boxes.padding,
            BackgroundOrigin::ContentBox => boxes.content,
        };
        if area.width <= 0.0 || area.height <= 0.0 {
            continue;
        }
        let natural = match &source {
            Source::Url { natural, .. } => *natural,
            Source::Gradient(_) => None,
        };
        let size = nth(&sizes.0, index);
        let (mut tile_width, mut tile_height, auto) = tile_size(size, natural, &positioning);
        let (repeat_x, repeat_y) = nth(&repeats.0, index)
            .map(|repeat| (repeat.0, repeat.1))
            .unwrap_or((
                BackgroundRepeatKeyword::Repeat,
                BackgroundRepeatKeyword::Repeat,
            ));

        // `round` resizes the tile so a whole number of tiles fills the positioning area.
        // When only one axis rounds and the other is `auto`, the other keeps the ratio.
        let rounded = |tile: f32, room: f32| {
            if tile <= 0.0 || room <= 0.0 {
                return tile;
            }
            let count = (room / tile).round().max(1.0);
            room / count
        };
        let round_x = repeat_x == BackgroundRepeatKeyword::Round;
        let round_y = repeat_y == BackgroundRepeatKeyword::Round;
        if round_x {
            let before = tile_width;
            tile_width = rounded(tile_width, positioning.width);
            if !round_y && auto.1 && before > 0.0 {
                tile_height *= tile_width / before;
            }
        }
        if round_y {
            let before = tile_height;
            tile_height = rounded(tile_height, positioning.height);
            if !round_x && auto.0 && before > 0.0 {
                tile_width *= tile_height / before;
            }
        }
        if tile_width <= 0.0 || tile_height <= 0.0 {
            continue;
        }

        let resolve = |position: Option<&LengthPercentage>, room: f32| {
            position.map_or(0.0, |position| position.resolve(Length::new(room)).px())
        };
        let mut x = positioning.x + resolve(nth(&xs.0, index), positioning.width - tile_width);
        let mut y = positioning.y + resolve(nth(&ys.0, index), positioning.height - tile_height);

        let axis = |keyword: BackgroundRepeatKeyword, tile: f32, room: f32| match keyword {
            BackgroundRepeatKeyword::NoRepeat => (TileRepeat::NoRepeat, false),
            BackgroundRepeatKeyword::Repeat | BackgroundRepeatKeyword::Round => {
                (TileRepeat::Repeat, false)
            }
            BackgroundRepeatKeyword::Space => {
                let count = (room / tile).floor();
                if count >= 2.0 {
                    let gap = (room - count * tile) / (count - 1.0);
                    (TileRepeat::Space { gap }, true)
                } else {
                    // Room for fewer than two: one tile, placed by `background-position`.
                    (TileRepeat::NoRepeat, false)
                }
            }
        };
        let (repeat_x, spaced_x) = axis(repeat_x, tile_width, positioning.width);
        let (repeat_y, spaced_y) = axis(repeat_y, tile_height, positioning.height);
        if spaced_x {
            x = positioning.x;
        }
        if spaced_y {
            y = positioning.y;
        }

        let tile = Rect::new(x, y, tile_width, tile_height);
        let image = match source {
            Source::Url { url, .. } => BackgroundImage::Url(url),
            Source::Gradient(gradient) => {
                match resolve_gradient(gradient, tile_width, tile_height, style) {
                    Some(image) => image,
                    None => continue,
                }
            }
        };
        layers.push(BackgroundLayer {
            image,
            area,
            tile,
            repeat_x,
            repeat_y,
        });
    }
    layers
}

/// The image a layer draws: a URL with its natural size, or a gradient. `image-set()`
/// becomes the candidate stylo selected, its natural size divided by its resolution.
fn source_of<'a>(
    image: &'a Image,
    density: f32,
    lookup: &mut ImageLookup<'_>,
) -> Option<Source<'a>> {
    match image {
        Image::Url(url) => {
            let url = css_url(url)?;
            let natural = lookup
                .size(&url)
                .map(|size| (size.width as f32 / density, size.height as f32 / density));
            Some(Source::Url { url, natural })
        }
        Image::Gradient(gradient) => Some(Source::Gradient(gradient)),
        Image::ImageSet(set) => {
            let item = set.items.get(set.selected_index)?;
            let density = item.resolution.dppx();
            source_of(
                &item.image,
                if density > 0.0 { density } else { 1.0 },
                lookup,
            )
        }
        // `none`; and `cross-fade()`, `light-dark()` and paint worklets, which this display
        // list does not describe.
        _ => None,
    }
}

/// The tile size `background-size` gives an image with the given natural size (none for a
/// gradient, or an image the resolver could not size) in the positioning area. Also says
/// which of the two dimensions was `auto`.
fn tile_size(
    size: Option<&BackgroundSize>,
    natural: Option<(f32, f32)>,
    area: &Rect,
) -> (f32, f32, (bool, bool)) {
    let ratio = natural.and_then(|(w, h)| (w > 0.0 && h > 0.0).then_some(w / h));
    match size {
        Some(GenericBackgroundSize::Cover) | Some(GenericBackgroundSize::Contain) => {
            let cover = matches!(size, Some(GenericBackgroundSize::Cover));
            match (natural, ratio) {
                (Some((w, h)), Some(_)) => {
                    let sx = area.width / w;
                    let sy = area.height / h;
                    let scale = if cover { sx.max(sy) } else { sx.min(sy) };
                    (w * scale, h * scale, (false, false))
                }
                // No ratio to keep: the image fills the area.
                _ => (area.width, area.height, (false, false)),
            }
        }
        Some(GenericBackgroundSize::ExplicitSize { width, height }) => {
            let resolve = |value: &LengthPercentageOrAuto<_>, basis: f32| match value {
                LengthPercentageOrAuto::LengthPercentage(length) => {
                    let length: &NonNegativeLengthPercentage = length;
                    Some(length.0.resolve(Length::new(basis)).px())
                }
                LengthPercentageOrAuto::Auto => None,
            };
            let w = resolve(width, area.width);
            let h = resolve(height, area.height);
            let auto = (w.is_none(), h.is_none());
            let (w, h) = match (w, h) {
                (Some(w), Some(h)) => (w, h),
                (Some(w), None) => match (ratio, natural) {
                    (Some(ratio), _) => (w, w / ratio),
                    (None, Some((_, nh))) => (w, nh),
                    (None, None) => (w, area.height),
                },
                (None, Some(h)) => match (ratio, natural) {
                    (Some(ratio), _) => (h * ratio, h),
                    (None, Some((nw, _))) => (nw, h),
                    (None, None) => (area.width, h),
                },
                (None, None) => match natural {
                    Some((nw, nh)) => (nw, nh),
                    None => (area.width, area.height),
                },
            };
            (w, h, auto)
        }
        None => match natural {
            Some((nw, nh)) => (nw, nh, (true, true)),
            None => (area.width, area.height, (true, true)),
        },
    }
}

/// A gradient resolved for a tile of `width` by `height`. `None` for a conic gradient,
/// which this display list does not describe.
fn resolve_gradient(
    gradient: &Gradient,
    width: f32,
    height: f32,
    style: &ComputedValues,
) -> Option<BackgroundImage> {
    match gradient {
        GenericGradient::Linear {
            direction,
            items,
            flags,
            compat_mode,
            ..
        } => {
            let angle = line_angle(direction, *compat_mode, width, height);
            let radians = angle.to_radians();
            let (sin, cos) = radians.sin_cos();
            let length = (width * sin).abs() + (height * cos).abs();
            let center = (width / 2.0, height / 2.0);
            let half = (sin * length / 2.0, -cos * length / 2.0);
            Some(BackgroundImage::Linear(LinearGradient {
                angle,
                start: (center.0 - half.0, center.1 - half.1),
                end: (center.0 + half.0, center.1 + half.1),
                stops: resolve_stops(items, length, style),
                repeating: flags.contains(GradientFlags::REPEATING),
            }))
        }
        GenericGradient::Radial {
            shape,
            position,
            items,
            flags,
            ..
        } => {
            let center = (
                position.horizontal.resolve(Length::new(width)).px(),
                position.vertical.resolve(Length::new(height)).px(),
            );
            let (circle, radius_x, radius_y) = ending_shape(shape, center, width, height);
            Some(BackgroundImage::Radial(RadialGradient {
                circle,
                center,
                radius_x,
                radius_y,
                stops: resolve_stops(items, radius_x, style),
                repeating: flags.contains(GradientFlags::REPEATING),
            }))
        }
        GenericGradient::Conic { .. } => None,
    }
}

/// The gradient line's angle in degrees, clockwise from up, in `0..360`.
fn line_angle(
    direction: &LineDirection,
    compat: GradientCompatMode,
    width: f32,
    height: f32,
) -> f32 {
    // The prefixed syntaxes name where the line starts rather than where it goes, and
    // measure angles counter-clockwise from the right.
    let legacy = compat != GradientCompatMode::Modern;
    let corner = height.atan2(width).to_degrees();
    let angle = match direction {
        LineDirection::Angle(angle) if legacy => 90.0 - angle.degrees(),
        LineDirection::Angle(angle) => angle.degrees(),
        LineDirection::Vertical(VerticalPositionKeyword::Top) => 0.0,
        LineDirection::Vertical(VerticalPositionKeyword::Bottom) => 180.0,
        LineDirection::Horizontal(HorizontalPositionKeyword::Right) => 90.0,
        LineDirection::Horizontal(HorizontalPositionKeyword::Left) => 270.0,
        // Towards a corner, the line is perpendicular to the diagonal between the other
        // two corners, so its angle depends on the tile's proportions.
        LineDirection::Corner(horizontal, vertical) => {
            use HorizontalPositionKeyword::{Left, Right};
            use VerticalPositionKeyword::{Bottom, Top};
            match (horizontal, vertical) {
                (Right, Top) => corner,
                (Right, Bottom) => 180.0 - corner,
                (Left, Bottom) => 180.0 + corner,
                (Left, Top) => 360.0 - corner,
            }
        }
    };
    let angle = match direction {
        LineDirection::Angle(_) => angle,
        _ if legacy => angle + 180.0,
        _ => angle,
    };
    angle.rem_euclid(360.0)
}

/// The radii of a radial gradient's ending shape around `center` in a tile of `width` by
/// `height`, and whether it is a circle.
fn ending_shape(
    shape: &EndingShape<NonNegativeLength, NonNegativeLengthPercentage>,
    center: (f32, f32),
    width: f32,
    height: f32,
) -> (bool, f32, f32) {
    let (cx, cy) = center;
    let near_x = cx.abs().min((width - cx).abs());
    let far_x = cx.abs().max((width - cx).abs());
    let near_y = cy.abs().min((height - cy).abs());
    let far_y = cy.abs().max((height - cy).abs());
    let corners = [
        (cx, cy),
        (width - cx, cy),
        (cx, height - cy),
        (width - cx, height - cy),
    ]
    .map(|(dx, dy)| (dx.abs(), dy.abs()));
    let distance = |(dx, dy): (f32, f32)| (dx * dx + dy * dy).sqrt();
    let closest_corner = corners
        .iter()
        .copied()
        .min_by(|a, b| distance(*a).total_cmp(&distance(*b)))
        .unwrap_or((0.0, 0.0));
    let farthest_corner = corners
        .iter()
        .copied()
        .max_by(|a, b| distance(*a).total_cmp(&distance(*b)))
        .unwrap_or((0.0, 0.0));
    let normal = |extent: ShapeExtent| match extent {
        ShapeExtent::Contain => ShapeExtent::ClosestSide,
        ShapeExtent::Cover => ShapeExtent::FarthestCorner,
        other => other,
    };
    // An ellipse through a corner, with the proportions the matching side keyword gives.
    let through = |(dx, dy): (f32, f32), (sx, sy): (f32, f32)| {
        if sx <= 0.0 || sy <= 0.0 {
            return (0.0, 0.0);
        }
        let a = (dx * dx + (dy * sx / sy).powi(2)).sqrt();
        (a, a * sy / sx)
    };
    match shape {
        EndingShape::Circle(Circle::Radius(radius)) => {
            let radius = radius.0.px();
            (true, radius, radius)
        }
        EndingShape::Circle(Circle::Extent(extent)) => {
            let radius = match normal(*extent) {
                ShapeExtent::ClosestSide => near_x.min(near_y),
                ShapeExtent::FarthestSide => far_x.max(far_y),
                ShapeExtent::ClosestCorner => distance(closest_corner),
                _ => distance(farthest_corner),
            };
            (true, radius, radius)
        }
        EndingShape::Ellipse(Ellipse::Radii(rx, ry)) => (
            false,
            rx.0.resolve(Length::new(width)).px(),
            ry.0.resolve(Length::new(height)).px(),
        ),
        EndingShape::Ellipse(Ellipse::Extent(extent)) => {
            let (rx, ry) = match normal(*extent) {
                ShapeExtent::ClosestSide => (near_x, near_y),
                ShapeExtent::FarthestSide => (far_x, far_y),
                ShapeExtent::ClosestCorner => through(closest_corner, (near_x, near_y)),
                _ => through(farthest_corner, (far_x, far_y)),
            };
            (false, rx, ry)
        }
    }
}

/// A colour stop or an interpolation hint, its position as a fraction of the line.
enum Item {
    Stop { color: Rgba, offset: Option<f32> },
    Hint(f32),
}

/// How many stops stand in for the curve an interpolation hint puts between two stops.
const HINT_SAMPLES: usize = 8;

/// The stops with every position resolved, the way CSS Images 3 fixes them up: a first
/// stop without a position is at 0 and a last one at 1, a position before an earlier one
/// moves up to it, and stops without positions share the room between their neighbours
/// evenly. An interpolation hint becomes extra stops along the curve it describes, since a
/// renderer's gradient takes stops only.
fn resolve_stops(
    items: &[GradientItem<Color, LengthPercentage>],
    length: f32,
    style: &ComputedValues,
) -> Vec<GradientStop> {
    let fraction = |position: &LengthPercentage| {
        let px = position.resolve(Length::new(length)).px();
        if length > 0.0 { px / length } else { 0.0 }
    };
    let mut list: Vec<Item> = items
        .iter()
        .map(|item| match item {
            GradientItem::SimpleColorStop(color) => Item::Stop {
                color: convert::resolve_color(color, style),
                offset: None,
            },
            GradientItem::ComplexColorStop { color, position } => Item::Stop {
                color: convert::resolve_color(color, style),
                offset: Some(fraction(position)),
            },
            GradientItem::InterpolationHint(position) => Item::Hint(fraction(position)),
        })
        .collect();

    let stop_indices: Vec<usize> = list
        .iter()
        .enumerate()
        .filter(|(_, item)| matches!(item, Item::Stop { .. }))
        .map(|(index, _)| index)
        .collect();
    let (Some(&first), Some(&last)) = (stop_indices.first(), stop_indices.last()) else {
        return Vec::new();
    };
    for (index, default) in [(first, 0.0), (last, 1.0)] {
        if let Item::Stop { offset, .. } = &mut list[index] {
            offset.get_or_insert(default);
        }
    }
    // Never decreasing.
    let mut highest = f32::NEG_INFINITY;
    for item in list.iter_mut() {
        match item {
            Item::Stop {
                offset: Some(offset),
                ..
            } => {
                *offset = offset.max(highest);
                highest = *offset;
            }
            Item::Hint(offset) => *offset = offset.max(highest),
            _ => {}
        }
    }
    // Runs without positions share the room between the stops around them.
    let mut at = 0;
    while at < stop_indices.len() {
        let index = stop_indices[at];
        let missing = matches!(list[index], Item::Stop { offset: None, .. });
        if !missing {
            at += 1;
            continue;
        }
        let before = stop_offset(&list[stop_indices[at - 1]]);
        let mut end = at;
        while matches!(list[stop_indices[end]], Item::Stop { offset: None, .. }) {
            end += 1;
        }
        let after = stop_offset(&list[stop_indices[end]]);
        let count = (end - at + 1) as f32;
        for (step, &index) in stop_indices[at..end].iter().enumerate() {
            if let Item::Stop { offset, .. } = &mut list[index] {
                *offset = Some(before + (after - before) * (step as f32 + 1.0) / count);
            }
        }
        at = end;
    }

    let mut stops: Vec<GradientStop> = Vec::new();
    let mut pending_hint: Option<f32> = None;
    for item in &list {
        match item {
            Item::Hint(offset) => pending_hint = Some(*offset),
            Item::Stop { color, offset } => {
                let offset = offset.unwrap_or(0.0);
                if let (Some(hint), Some(previous)) = (pending_hint.take(), stops.last().copied()) {
                    hinted(
                        &mut stops,
                        previous,
                        GradientStop {
                            offset,
                            color: *color,
                        },
                        hint,
                    );
                }
                stops.push(GradientStop {
                    offset,
                    color: *color,
                });
            }
        }
    }
    stops
}

fn stop_offset(item: &Item) -> f32 {
    match item {
        Item::Stop { offset, .. } => offset.unwrap_or(0.0),
        Item::Hint(offset) => *offset,
    }
}

/// Stops between `from` and `to` that follow the curve an interpolation hint at `hint`
/// describes: the colour halfway between the two sits at the hint.
fn hinted(stops: &mut Vec<GradientStop>, from: GradientStop, to: GradientStop, hint: f32) {
    let span = to.offset - from.offset;
    if span <= 0.0 {
        return;
    }
    let ratio = ((hint - from.offset) / span).clamp(0.0, 1.0);
    if (ratio - 0.5).abs() < 1e-4 {
        // A hint halfway is the same as no hint.
        return;
    }
    for sample in 1..HINT_SAMPLES {
        let t = sample as f32 / HINT_SAMPLES as f32;
        let weight = if ratio <= 0.0 {
            1.0
        } else if ratio >= 1.0 {
            0.0
        } else {
            t.powf(0.5f32.ln() / ratio.ln())
        };
        stops.push(GradientStop {
            offset: from.offset + span * t,
            color: mix(from.color, to.color, weight),
        });
    }
}

/// `a` and `b` mixed `weight` of the way to `b`, in premultiplied sRGB as CSS interpolates
/// gradient colours.
fn mix(a: Rgba, b: Rgba, weight: f32) -> Rgba {
    let alpha_a = a.a as f32 / 255.0;
    let alpha_b = b.a as f32 / 255.0;
    let alpha = alpha_a + (alpha_b - alpha_a) * weight;
    let channel = |ca: u8, cb: u8| {
        if alpha <= 0.0 {
            return 0;
        }
        let pa = ca as f32 * alpha_a;
        let pb = cb as f32 * alpha_b;
        ((pa + (pb - pa) * weight) / alpha)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Rgba {
        r: channel(a.r, b.r),
        g: channel(a.g, b.g),
        b: channel(a.b, b.b),
        a: (alpha * 255.0).round().clamp(0.0, 255.0) as u8,
    }
}

/// The image of an `<img>` element: its source and where it is drawn in the content box.
/// `None` without a `src`.
pub(crate) fn replaced_image(
    node: &Node,
    style: &ComputedValues,
    content: Rect,
    lookup: &ImageLookup<'_>,
) -> Option<ReplacedImage> {
    let source = lookup.source(img_src(node)?)?;
    let element = node.element_data()?;
    let alt = element.attr(local("alt")).map(str::to_string);
    // The size layout used: what the resolver answered, written onto the element before
    // layout. Reading it back keeps the drawn image and the laid-out box in agreement.
    let natural = element
        .raster_image_data()
        .map(|image| (image.width as f32, image.height as f32))
        .filter(|(width, height)| *width > 0.0 && *height > 0.0);
    let fit = match style.clone_object_fit() {
        StyloObjectFit::Fill => ObjectFit::Fill,
        StyloObjectFit::Contain => ObjectFit::Contain,
        StyloObjectFit::Cover => ObjectFit::Cover,
        StyloObjectFit::None => ObjectFit::None,
        StyloObjectFit::ScaleDown => ObjectFit::ScaleDown,
    };
    let image_rect = match natural {
        None => content,
        Some((natural_width, natural_height)) => {
            let scale_x = content.width / natural_width;
            let scale_y = content.height / natural_height;
            let (width, height) = match fit {
                ObjectFit::Fill => (content.width, content.height),
                ObjectFit::Contain => {
                    let scale = scale_x.min(scale_y);
                    (natural_width * scale, natural_height * scale)
                }
                ObjectFit::Cover => {
                    let scale = scale_x.max(scale_y);
                    (natural_width * scale, natural_height * scale)
                }
                ObjectFit::None => (natural_width, natural_height),
                ObjectFit::ScaleDown => {
                    let scale = scale_x.min(scale_y).min(1.0);
                    (natural_width * scale, natural_height * scale)
                }
            };
            let position = style.clone_object_position();
            Rect::new(
                content.x
                    + position
                        .horizontal
                        .resolve(Length::new(content.width - width))
                        .px(),
                content.y
                    + position
                        .vertical
                        .resolve(Length::new(content.height - height))
                        .px(),
                width,
                height,
            )
        }
    };
    Some(ReplacedImage {
        source,
        alt,
        content_rect: content,
        image_rect,
        fit,
        natural_size: natural,
    })
}
