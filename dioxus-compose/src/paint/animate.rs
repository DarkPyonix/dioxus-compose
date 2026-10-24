//! CSS transitions and `@keyframes` animations as compose-rust's `StartAnimation` records.
//!
//! The display list describes a motion once, in CSS terms (seconds, radians, a CSS timing
//! function). This module turns one such [`Animation`] into the wire record a renderer
//! plays on its own frame clock, so the Host sends it once and does nothing per frame.
//!
//! What the wire takes differs from the display list in a few places:
//!
//! - times are milliseconds, angles are degrees, and `infinite` is an infinite float;
//! - a timing function belongs to each keyframe and shapes the segment after it, where CSS
//!   gives one function to the whole animation: it is repeated on every keyframe but the
//!   last;
//! - a transform keyframe is a list of functions, the same kinds in the same order in every
//!   keyframe. A list that does not line up with the others is padded with identity
//!   functions when it is empty, and otherwise every keyframe is sent as one matrix;
//! - a transform's origin is a fraction of the node's size, where the display list has
//!   pixels.

use compose_rust::schema::{
    self as wire, AnimationEvents, Color, ColorInterpolation, KeyframeValue, Paint, PlayState,
    Timing, TransformFunction as WireFunction,
};

use crate::paint::display_list::{
    AnimatedProperty, AnimatedValue, Animation, Easing, FillMode, Iterations, Matrix2D,
    PlayDirection, StepPosition, TransformFunction,
};

/// The wire property a CSS property is animated as.
pub(crate) fn wire_property(property: AnimatedProperty) -> wire::AnimatedProperty {
    match property {
        AnimatedProperty::Opacity => wire::AnimatedProperty::Alpha,
        AnimatedProperty::Transform => wire::AnimatedProperty::Transform,
        AnimatedProperty::BackgroundColor => wire::AnimatedProperty::Background,
    }
}

/// The CSS name of the property, which is what `transitionend`'s `propertyName` carries.
pub(crate) fn css_name(property: AnimatedProperty) -> &'static str {
    match property {
        AnimatedProperty::Opacity => "opacity",
        AnimatedProperty::Transform => "transform",
        AnimatedProperty::BackgroundColor => "background-color",
    }
}

fn timing(easing: Easing) -> Timing {
    let bezier = |x1: f32, y1: f32, x2: f32, y2: f32| Timing::CubicBezier {
        x1: x1.clamp(0.0, 1.0),
        y1,
        x2: x2.clamp(0.0, 1.0),
        y2,
    };
    match easing {
        Easing::Linear => Timing::Linear,
        Easing::Ease => bezier(0.25, 0.1, 0.25, 1.0),
        Easing::EaseIn => bezier(0.42, 0.0, 1.0, 1.0),
        Easing::EaseOut => bezier(0.0, 0.0, 0.58, 1.0),
        Easing::EaseInOut => bezier(0.42, 0.0, 0.58, 1.0),
        Easing::CubicBezier(x1, y1, x2, y2) => bezier(x1, y1, x2, y2),
        Easing::Steps { count, position } => {
            let (position, least) = match position {
                StepPosition::JumpStart => (wire::StepPosition::JumpStart, 1),
                StepPosition::JumpEnd => (wire::StepPosition::JumpEnd, 1),
                StepPosition::JumpNone => (wire::StepPosition::JumpNone, 2),
                StepPosition::JumpBoth => (wire::StepPosition::JumpBoth, 1),
            };
            Timing::Steps {
                count: count.max(least),
                position,
            }
        }
    }
}

fn direction(direction: PlayDirection) -> wire::PlaybackDirection {
    match direction {
        PlayDirection::Normal => wire::PlaybackDirection::Normal,
        PlayDirection::Reverse => wire::PlaybackDirection::Reverse,
        PlayDirection::Alternate => wire::PlaybackDirection::Alternate,
        PlayDirection::AlternateReverse => wire::PlaybackDirection::AlternateReverse,
    }
}

fn fill(fill: FillMode) -> wire::FillMode {
    match fill {
        FillMode::None => wire::FillMode::None,
        FillMode::Forwards => wire::FillMode::Forwards,
        FillMode::Backwards => wire::FillMode::Backwards,
        FillMode::Both => wire::FillMode::Both,
    }
}

fn function(function: &TransformFunction) -> WireFunction {
    match *function {
        TransformFunction::Translate(x, y) => WireFunction::translate(x, y),
        TransformFunction::Rotate(radians) => WireFunction::rotate(radians.to_degrees()),
        TransformFunction::Scale(x, y) => WireFunction::scale(x, y),
        TransformFunction::Skew(x, y) => WireFunction::skew(x.to_degrees(), y.to_degrees()),
        TransformFunction::Matrix(m) => WireFunction::matrix(m.a, m.b, m.c, m.d, m.e, m.f),
    }
}

/// The function of the same kind that changes nothing.
fn neutral(function: &TransformFunction) -> TransformFunction {
    match function {
        TransformFunction::Translate(..) => TransformFunction::Translate(0.0, 0.0),
        TransformFunction::Rotate(_) => TransformFunction::Rotate(0.0),
        TransformFunction::Scale(..) => TransformFunction::Scale(1.0, 1.0),
        TransformFunction::Skew(..) => TransformFunction::Skew(0.0, 0.0),
        TransformFunction::Matrix(_) => TransformFunction::Matrix(Matrix2D::IDENTITY),
    }
}

fn same_kind(a: &TransformFunction, b: &TransformFunction) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

/// The function lists of the transform keyframes, made to line up, or `None` when they
/// cannot be.
fn aligned(lists: &[&[TransformFunction]]) -> Option<Vec<Vec<TransformFunction>>> {
    let reference = lists.iter().find(|list| !list.is_empty())?;
    let mut out = Vec::with_capacity(lists.len());
    for list in lists {
        if list.is_empty() {
            out.push(reference.iter().map(neutral).collect());
        } else if list.len() == reference.len()
            && list
                .iter()
                .zip(reference.iter())
                .all(|(a, b)| same_kind(a, b))
        {
            out.push(list.to_vec());
        } else {
            return None;
        }
    }
    Some(out)
}

fn paint(color: crate::paint::display_list::Rgba) -> Paint {
    Paint::Literal(Color::argb(
        (u32::from(color.a) << 24)
            | (u32::from(color.r) << 16)
            | (u32::from(color.g) << 8)
            | u32::from(color.b),
    ))
}

fn matrix_function(matrix: &Matrix2D) -> WireFunction {
    WireFunction::matrix(matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f)
}

/// The wire record that plays `animation` on `node_id`, in `slot`. `size` is the node's
/// border box, which a transform's origin is a fraction of. `events` says which moments
/// come back. `None` when a number would not be a valid record.
pub(crate) fn to_wire(
    node_id: u32,
    animation_id: u32,
    slot: u8,
    animation: &Animation,
    size: (f32, f32),
    events: AnimationEvents,
) -> Option<wire::Animation<'static>> {
    if animation.keyframes.len() < 2 {
        return None;
    }
    let count = animation.keyframes.len();
    let is_transition = animation.name.is_none();

    let transforms: Vec<&crate::paint::display_list::Transform2D> = animation
        .keyframes
        .iter()
        .filter_map(|keyframe| match &keyframe.value {
            AnimatedValue::Transform(transform) => Some(transform),
            _ => None,
        })
        .collect();
    let lists = if animation.property == AnimatedProperty::Transform {
        if transforms.len() != count {
            return None;
        }
        let lists: Vec<&[TransformFunction]> = transforms
            .iter()
            .map(|transform| transform.functions.as_slice())
            .collect();
        aligned(&lists)
    } else {
        None
    };

    let mut keyframes = Vec::with_capacity(count);
    for (index, keyframe) in animation.keyframes.iter().enumerate() {
        let value = match &keyframe.value {
            AnimatedValue::Opacity(alpha) => KeyframeValue::Alpha(*alpha),
            AnimatedValue::Colour(color) => KeyframeValue::Paint(paint(*color)),
            AnimatedValue::Transform(transform) => {
                let functions: Vec<WireFunction> = match &lists {
                    Some(lists) => lists[index].iter().map(function).collect(),
                    None => vec![matrix_function(&transform.matrix)],
                };
                KeyframeValue::Transform(std::borrow::Cow::Owned(functions))
            }
        };
        keyframes.push(wire::Keyframe {
            offset: keyframe.offset,
            timing: if index + 1 < count {
                timing(animation.easing)
            } else {
                Timing::Linear
            },
            // A transition that changes course carries on from where it was.
            from_presented: is_transition && index == 0,
            value,
        });
    }

    let colour = animation.property == AnimatedProperty::BackgroundColor;
    let origin = transforms.first().map_or((0.5, 0.5), |transform| {
        let fraction = |origin: f32, extent: f32| {
            if extent > 0.0 { origin / extent } else { 0.5 }
        };
        (
            fraction(transform.origin.0, size.0),
            fraction(transform.origin.1, size.1),
        )
    });
    let iterations = match animation.iterations {
        Iterations::Count(count) => count,
        Iterations::Infinite => f32::INFINITY,
    };
    let numbers = [
        animation.duration,
        animation.delay,
        iterations.min(f32::MAX),
        origin.0,
        origin.1,
    ];
    if numbers.iter().any(|number| !number.is_finite()) {
        return None;
    }
    Some(wire::Animation {
        node_id,
        animation_id,
        property: wire_property(animation.property),
        slot,
        direction: direction(animation.direction),
        fill: fill(animation.fill),
        play_state: if animation.paused {
            PlayState::Paused
        } else {
            PlayState::Running
        },
        interpolation: colour.then_some(ColorInterpolation::SrgbPremultiplied),
        start_time_nanos: 0,
        delay_ms: animation.delay * 1000.0,
        duration_ms: animation.duration * 1000.0,
        iterations,
        origin_x: origin.0,
        origin_y: origin.1,
        events,
        keyframes: std::borrow::Cow::Owned(keyframes),
    })
}
