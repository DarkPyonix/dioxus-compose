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
//!   pixels;
//! - a motion of `transform` with a 3D keyframe is sent as 4x4 matrices
//!   (`AnimatedProperty::Transform3D`), each keyframe's transform multiplied out without
//!   its origin, which the node's 3D transform carries. A renderer interpolates two
//!   matrices by taking them apart into a move, a scale, a skew, a perspective and a
//!   rotation, and turns the short way round, so a segment whose function lists line up
//!   and that turns half a turn or more is cut into segments that each turn less. The
//!   keyframes added between are the function lists interpolated one function at a time,
//!   as CSS interpolates them, and the timing function is cut at the same places so that
//!   the pieces together run the curve the segment had.

use std::f32::consts::PI;

use compose_rust::schema::{
    self as wire, AnimationEvents, Color, ColorInterpolation, KeyframeValue, Paint, PlayState,
    Timing, TransformFunction as WireFunction,
};

use crate::paint::display_list::{
    AnimatedProperty, AnimatedValue, Animation, Easing, FillMode, Iterations, Matrix2D, Matrix3D,
    PlayDirection, StepPosition, Transform3D, TransformFunction, TransformFunction3D,
};
use crate::paint::transform::interpolate_3d;

/// The wire property a CSS property is animated as.
pub(crate) fn wire_property(property: AnimatedProperty) -> wire::AnimatedProperty {
    match property {
        AnimatedProperty::Opacity => wire::AnimatedProperty::Alpha,
        AnimatedProperty::Transform => wire::AnimatedProperty::Transform,
        AnimatedProperty::Color => wire::AnimatedProperty::Color,
        AnimatedProperty::BackgroundColor => wire::AnimatedProperty::Background,
    }
}

/// The wire property `animation` is played as: a motion of `transform` with a 3D keyframe
/// is a matrix animation, and any other is its property's.
pub(crate) fn wire_property_of(animation: &Animation) -> wire::AnimatedProperty {
    if is_3d(animation) {
        wire::AnimatedProperty::Transform3D
    } else {
        wire_property(animation.property)
    }
}

/// Whether `animation` is a motion of `transform` with a 3D keyframe.
fn is_3d(animation: &Animation) -> bool {
    animation.property == AnimatedProperty::Transform
        && animation
            .keyframes
            .iter()
            .any(|keyframe| matches!(keyframe.value, AnimatedValue::Transform3D(_)))
}

/// The CSS name of the property, which is what `transitionend`'s `propertyName` carries.
pub(crate) fn css_name(property: AnimatedProperty) -> &'static str {
    match property {
        AnimatedProperty::Opacity => "opacity",
        AnimatedProperty::Transform => "transform",
        AnimatedProperty::Color => "color",
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
    let (keyframes, origin) = if is_3d(animation) {
        // The origin is the node's 3D transform's, so the animation carries none.
        (matrix_keyframes(animation, size)?, (0.0, 0.0))
    } else {
        function_keyframes(animation, size)?
    };
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
    let colour = matches!(
        animation.property,
        AnimatedProperty::Color | AnimatedProperty::BackgroundColor
    );
    Some(wire::Animation {
        node_id,
        animation_id,
        property: wire_property_of(animation),
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

/// The keyframes of an animation that is not a 3D motion, and the origin of its
/// transform as a fraction of the node's size (the centre for any other property).
fn function_keyframes(
    animation: &Animation,
    size: (f32, f32),
) -> Option<(Vec<wire::Keyframe>, (f32, f32))> {
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
            // Never here: an animation with one of these is a 3D motion.
            AnimatedValue::Transform3D(_) => return None,
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

    let origin = transforms.first().map_or((0.5, 0.5), |transform| {
        let fraction = |origin: f32, extent: f32| {
            if extent > 0.0 { origin / extent } else { 0.5 }
        };
        (
            fraction(transform.origin.0, size.0),
            fraction(transform.origin.1, size.1),
        )
    });
    Some((keyframes, origin))
}

// ---------------------------------------------------------------------------------------
// A motion through a 3D transform, as 4x4 matrices
// ---------------------------------------------------------------------------------------

/// The most segments one segment is cut into. A record has room for many more, but a
/// segment that turns this many half turns (over 180000 degrees) is not one a page means to
/// show turn by turn; its pieces are left to turn more than half a turn each.
const MOST_PIECES: usize = 1024;

/// A keyframe added inside a segment: where it stands, as a share of the segment's time
/// (`at`) and of the way from its first keyframe's functions to its last's (`progress`),
/// and the timing of the piece that starts there.
#[derive(Clone, Copy, Debug)]
struct Piece {
    at: f32,
    progress: f32,
    timing: Timing,
}

/// The keyframes of a motion of `transform` with a 3D keyframe, as matrices about the
/// origin of the node's 3D transform.
fn matrix_keyframes(animation: &Animation, size: (f32, f32)) -> Option<Vec<wire::Keyframe>> {
    let is_transition = animation.name.is_none();
    let like = animation
        .keyframes
        .iter()
        .find_map(|keyframe| match &keyframe.value {
            AnimatedValue::Transform3D(transform) => Some(transform.clone()),
            _ => None,
        })?;
    // The display list makes every keyframe of such a motion 3D; a flat one is taken as
    // the 3D transform it is, in the same context as the others.
    let frames: Vec<Transform3D> = animation
        .keyframes
        .iter()
        .map(|keyframe| match &keyframe.value {
            AnimatedValue::Transform3D(transform) => Some(transform.clone()),
            AnimatedValue::Transform(flat) => Some(Transform3D::wrapped(
                flat.functions
                    .iter()
                    .map(TransformFunction3D::from_2d)
                    .collect(),
                flat.origin,
                like.outer,
                like.inner,
                like.backface_hidden,
            )),
            AnimatedValue::Opacity(_) | AnimatedValue::Colour(_) => None,
        })
        .collect::<Option<_>>()?;

    // The node's 3D transform is the value the motion rests at: the end of a transition,
    // and the keyframe an `@keyframes` animation starts from.
    let rests_at_end = is_transition
        || matches!(
            animation.direction,
            PlayDirection::Reverse | PlayDirection::AlternateReverse
        );
    let reference = if rests_at_end {
        frames[frames.len() - 1].origin
    } else {
        frames[0].origin
    };
    // A box with no area has no fraction for its origin, so its 3D transform is sent with
    // the origin folded into the matrix, and so is every keyframe.
    let fold_origin = !(size.0 > 0.0
        && size.1 > 0.0
        && (reference.0 / size.0).is_finite()
        && (reference.1 / size.1).is_finite());
    let place = |matrix: Matrix3D, origin: (f32, f32)| -> Matrix3D {
        let mut matrix = matrix;
        if origin != reference {
            // The same transform about another point: move that point onto the
            // reference, apply, move it back.
            let (dx, dy) = (origin.0 - reference.0, origin.1 - reference.1);
            matrix = Matrix3D::translation(dx, dy, 0.0)
                .times(&matrix)
                .times(&Matrix3D::translation(-dx, -dy, 0.0));
        }
        if fold_origin {
            matrix = Matrix3D::translation(reference.0, reference.1, 0.0)
                .times(&matrix)
                .times(&Matrix3D::translation(-reference.0, -reference.1, 0.0));
        }
        matrix
    };

    let mut out = Vec::with_capacity(frames.len());
    for (index, from) in frames.iter().enumerate() {
        let start = animation.keyframes[index].offset;
        let Some(to) = frames.get(index + 1) else {
            out.push(matrix_keyframe(
                start,
                Timing::Linear,
                place(from.matrix, from.origin),
            ));
            break;
        };
        let end = animation.keyframes[index + 1].offset;
        for piece in pieces(from, to, animation.easing) {
            let (matrix, origin) = if piece.progress == 0.0 {
                (from.matrix, from.origin)
            } else if piece.progress == 1.0 {
                (to.matrix, to.origin)
            } else {
                (
                    between(from, to, piece.progress),
                    if piece.progress < 0.5 {
                        from.origin
                    } else {
                        to.origin
                    },
                )
            };
            out.push(matrix_keyframe(
                start + (end - start) * piece.at,
                piece.timing,
                place(matrix, origin),
            ));
        }
    }
    // A transition that changes course carries on from where it was.
    if is_transition && let Some(first) = out.first_mut() {
        first.from_presented = true;
    }
    let finite = out.iter().all(|keyframe| match &keyframe.value {
        KeyframeValue::Matrix3D(m) => m.iter().all(|value| value.is_finite()),
        _ => false,
    });
    finite.then_some(out)
}

fn matrix_keyframe(offset: f32, timing: Timing, matrix: Matrix3D) -> wire::Keyframe {
    wire::Keyframe {
        offset,
        timing,
        from_presented: false,
        value: KeyframeValue::Matrix3D(matrix.0),
    }
}

/// The pieces the segment from `from` to `to` is played in: one with the segment's own
/// timing, unless the two function lists line up and turn half a turn or more between
/// them. Then the segment is cut where each piece turns less than half a turn, so the
/// renderer's matrix interpolation, which turns the short way, goes the way CSS goes.
fn pieces(from: &Transform3D, to: &Transform3D, easing: Easing) -> Vec<Piece> {
    let segment = timing(easing);
    let whole = vec![Piece {
        at: 0.0,
        progress: 0.0,
        timing: segment,
    }];
    let Some(turn) = turn_between(&from.functions, &to.functions) else {
        return whole;
    };
    let count = pieces_for(turn);
    if count < 2 {
        return whole;
    }
    match segment {
        Timing::Linear => (0..count)
            .map(|index| {
                let share = index as f32 / count as f32;
                Piece {
                    at: share,
                    progress: share,
                    timing: Timing::Linear,
                }
            })
            .collect(),
        Timing::CubicBezier { x1, y1, x2, y2 } => bezier_pieces([x1, y1, x2, y2], turn),
        Timing::Steps { count, position } => step_pieces(count, position),
        // This crate sends no spring for a CSS timing function.
        Timing::Spring { .. } => whole,
    }
}

/// How many pieces a turn of `turn` radians is cut into so that each turns less than half
/// a turn. A turn a hair under half a turn is cut too: the short way round is not a
/// direction a renderer can be relied on to find there.
fn pieces_for(turn: f32) -> usize {
    if !turn.is_finite() || turn <= 0.0 {
        return 1;
    }
    let count = (turn / PI + 1e-3).floor();
    if count >= MOST_PIECES as f32 {
        MOST_PIECES
    } else {
        count as usize + 1
    }
}

/// The largest turn, in radians, between two function lists that line up, of the
/// rotations CSS interpolates by their angle; `None` when the lists do not line up and are
/// interpolated as matrices, as CSS does then too.
fn turn_between(from: &[TransformFunction3D], to: &[TransformFunction3D]) -> Option<f32> {
    let (from, to) = aligned_3d(from, to)?;
    let mut turn: f32 = 0.0;
    for (a, b) in from.iter().zip(&to) {
        if let (
            TransformFunction3D::Rotate(ax, ay, az, angle_a),
            TransformFunction3D::Rotate(bx, by, bz, angle_b),
        ) = (*a, *b)
            && let Some((_, from_angle, to_angle)) =
                common_axis([ax, ay, az, angle_a], [bx, by, bz, angle_b])
        {
            turn = turn.max((to_angle - from_angle).abs());
        }
    }
    Some(turn)
}

/// The function that changes nothing, of the same kind.
fn neutral_3d(function: &TransformFunction3D) -> TransformFunction3D {
    match *function {
        TransformFunction3D::Translate(..) => TransformFunction3D::Translate(0.0, 0.0, 0.0),
        TransformFunction3D::Rotate(x, y, z, _) => TransformFunction3D::Rotate(x, y, z, 0.0),
        TransformFunction3D::Scale(..) => TransformFunction3D::Scale(1.0, 1.0, 1.0),
        TransformFunction3D::Skew(..) => TransformFunction3D::Skew(0.0, 0.0),
        TransformFunction3D::Perspective(_) => TransformFunction3D::Perspective(f32::INFINITY),
        TransformFunction3D::Matrix(_) => TransformFunction3D::Matrix(Matrix3D::IDENTITY),
    }
}

/// Two function lists made to line up the way the 2D ones are: an empty list is the other
/// with every function neutral, and otherwise both have the same kinds in the same order.
fn aligned_3d(
    from: &[TransformFunction3D],
    to: &[TransformFunction3D],
) -> Option<(Vec<TransformFunction3D>, Vec<TransformFunction3D>)> {
    match (from.is_empty(), to.is_empty()) {
        (true, true) => Some((Vec::new(), Vec::new())),
        (true, false) => Some((to.iter().map(neutral_3d).collect(), to.to_vec())),
        (false, true) => Some((from.to_vec(), from.iter().map(neutral_3d).collect())),
        (false, false) => {
            let lined_up = from.len() == to.len()
                && from
                    .iter()
                    .zip(to)
                    .all(|(a, b)| std::mem::discriminant(a) == std::mem::discriminant(b));
            lined_up.then(|| (from.to_vec(), to.to_vec()))
        }
    }
}

/// The axis two `rotate3d()` functions share and their angles about it, when CSS
/// interpolates them by their angle: their axes point the same way, or one of them turns
/// nothing and the other's axis is taken (the z axis when neither turns). `None` when both
/// turn about different axes, which CSS interpolates as matrices.
fn common_axis(from: [f32; 4], to: [f32; 4]) -> Option<([f32; 3], f32, f32)> {
    let unit = |[x, y, z, _]: [f32; 4]| {
        let length = (x * x + y * y + z * z).sqrt();
        (length > 0.0 && length.is_finite()).then(|| [x / length, y / length, z / length])
    };
    // An axis of no length turns nothing, whatever the angle.
    let turning = |function: [f32; 4]| unit(function).filter(|_| function[3] != 0.0);
    match (turning(from), turning(to)) {
        (Some(a), Some(b)) => {
            let dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
            (dot > 1.0 - 1e-6).then_some((a, from[3], to[3]))
        }
        (Some(a), None) => Some((a, from[3], 0.0)),
        (None, Some(b)) => Some((b, 0.0, to[3])),
        (None, None) => Some(([0.0, 0.0, 1.0], 0.0, 0.0)),
    }
}

/// The transform `progress` of the way from `from` to `to`, interpolated one function at a
/// time, inside the context (`transform-origin` depth, parent perspective) of the nearer
/// end. Lists that do not line up are interpolated as matrices.
fn between(from: &Transform3D, to: &Transform3D, progress: f32) -> Matrix3D {
    let Some((a, b)) = aligned_3d(&from.functions, &to.functions) else {
        return interpolate_3d(&from.matrix, &to.matrix, progress);
    };
    let product = a.iter().zip(&b).fold(Matrix3D::IDENTITY, |total, (a, b)| {
        total.times(&function_between(a, b, progress))
    });
    let near = if progress < 0.5 { from } else { to };
    near.outer.times(&product).times(&near.inner)
}

/// One function `progress` of the way from `a` to `b`, which are of the same kind.
fn function_between(a: &TransformFunction3D, b: &TransformFunction3D, progress: f32) -> Matrix3D {
    let mix = |from: f32, to: f32| from + (to - from) * progress;
    match (*a, *b) {
        (
            TransformFunction3D::Translate(ax, ay, az),
            TransformFunction3D::Translate(bx, by, bz),
        ) => Matrix3D::translation(mix(ax, bx), mix(ay, by), mix(az, bz)),
        (TransformFunction3D::Scale(ax, ay, az), TransformFunction3D::Scale(bx, by, bz)) => {
            Matrix3D::scaling(mix(ax, bx), mix(ay, by), mix(az, bz))
        }
        (TransformFunction3D::Skew(ax, ay), TransformFunction3D::Skew(bx, by)) => {
            Matrix3D::from_2d(&Matrix2D::skew(mix(ax, bx), mix(ay, by)))
        }
        (
            TransformFunction3D::Rotate(ax, ay, az, angle_a),
            TransformFunction3D::Rotate(bx, by, bz, angle_b),
        ) => match common_axis([ax, ay, az, angle_a], [bx, by, bz, angle_b]) {
            Some((axis, from, to)) => Matrix3D::rotation(axis[0], axis[1], axis[2], mix(from, to)),
            None => interpolate_3d(&a.matrix(), &b.matrix(), progress),
        },
        // A perspective, a matrix, or two kinds that do not match: as matrices.
        _ => interpolate_3d(&a.matrix(), &b.matrix(), progress),
    }
}

/// One coordinate of the CSS cubic Bézier with inner control values `c1` and `c2` (its
/// ends are 0 and 1) at the curve's parameter `t`.
fn bezier_at(c1: f32, c2: f32, t: f32) -> f32 {
    let u = 1.0 - t;
    3.0 * u * u * t * c1 + 3.0 * u * t * t * c2 + t * t * t
}

/// The parameter at which a coordinate that never falls (`c1` and `c2` in 0..=1) reaches
/// `value`.
fn bezier_solve(c1: f32, c2: f32, value: f32) -> f32 {
    let (mut low, mut high) = (0.0f32, 1.0f32);
    for _ in 0..64 {
        let middle = (low + high) / 2.0;
        if bezier_at(c1, c2, middle) < value {
            low = middle;
        } else {
            high = middle;
        }
    }
    (low + high) / 2.0
}

/// The control points of the part of a cubic Bézier between parameters `t0` and `t1`, by
/// de Casteljau's construction.
fn sub_curve(points: [[f32; 2]; 4], t0: f32, t1: f32) -> [[f32; 2]; 4] {
    let lerp =
        |a: [f32; 2], b: [f32; 2], t: f32| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
    // The curve up to `t`, and the curve after it.
    let split = |p: [[f32; 2]; 4], t: f32| {
        let (a, b, c) = (
            lerp(p[0], p[1], t),
            lerp(p[1], p[2], t),
            lerp(p[2], p[3], t),
        );
        let (d, e) = (lerp(a, b, t), lerp(b, c, t));
        let f = lerp(d, e, t);
        ([p[0], a, d, f], [f, e, c, p[3]])
    };
    if t1 <= 0.0 {
        return [points[0]; 4];
    }
    let (before, _) = split(points, t1);
    let (_, part) = split(before, t0 / t1);
    part
}

/// Cuts a `cubic-bezier()` segment that turns `turn` radians into pieces that each turn
/// less than half a turn, each piece timed by its part of the curve stretched back to the
/// unit square, so that the pieces run the curve the segment had.
fn bezier_pieces([x1, y1, x2, y2]: [f32; 4], turn: f32) -> Vec<Piece> {
    let points = [[0.0, 0.0], [x1, y1], [x2, y2], [1.0, 1.0]];
    let rises = (0.0..=1.0).contains(&y1) && (0.0..=1.0).contains(&y2);
    let cuts: Vec<f32> = if rises {
        // The progress never falls, so the curve is cut where it reaches each equal share.
        let count = pieces_for(turn);
        (0..=count)
            .map(|index| match index {
                0 => 0.0,
                _ if index == count => 1.0,
                _ => bezier_solve(y1, y2, index as f32 / count as f32),
            })
            .collect()
    } else {
        // A curve that overshoots turns further than its ends say. The curve lies inside
        // the hull of its control points, so pieces are added until each piece's hull
        // spans less than half a turn.
        let low = 0.0f32.min(y1).min(y2);
        let high = 1.0f32.max(y1).max(y2);
        let mut count = pieces_for(turn * (high - low)).max(2);
        loop {
            let cuts: Vec<f32> = (0..=count)
                .map(|index| index as f32 / count as f32)
                .collect();
            let fits = cuts.windows(2).all(|pair| {
                let part = sub_curve(points, pair[0], pair[1]);
                let (low, high) = part.iter().fold((f32::MAX, f32::MIN), |(low, high), p| {
                    (low.min(p[1]), high.max(p[1]))
                });
                (high - low) * turn < PI * (1.0 - 1e-3)
            });
            if fits || count >= MOST_PIECES {
                break cuts;
            }
            count += 1;
        }
    };
    cuts.windows(2)
        .map(|pair| {
            let part = sub_curve(points, pair[0], pair[1]);
            let width = part[3][0] - part[0][0];
            let rise = part[3][1] - part[0][1];
            // A piece whose ends are at the same progress shows nothing between them that a
            // timing could stretch; it is played straight.
            let timing = if width > 1e-6 && rise.abs() > 1e-6 {
                Timing::CubicBezier {
                    x1: ((part[1][0] - part[0][0]) / width).clamp(0.0, 1.0),
                    y1: (part[1][1] - part[0][1]) / rise,
                    x2: ((part[2][0] - part[0][0]) / width).clamp(0.0, 1.0),
                    y2: (part[2][1] - part[0][1]) / rise,
                }
            } else {
                Timing::Linear
            };
            Piece {
                at: if pair[0] == 0.0 { 0.0 } else { part[0][0] },
                progress: if pair[0] == 0.0 { 0.0 } else { part[0][1] },
                timing,
            }
        })
        .collect()
}

/// A `steps()` segment as one keyframe per step, each held for its step with a `steps(1)`
/// timing, so that every step shows the function lists interpolated to its own progress.
fn step_pieces(count: u32, position: wire::StepPosition) -> Vec<Piece> {
    let steps = count.max(1) as usize;
    let one = |position| Timing::Steps { count: 1, position };
    let share = |index: usize, of: usize| index as f32 / of as f32;
    match position {
        // Each step shows its own keyframe, the first at the start.
        wire::StepPosition::JumpEnd => (0..steps)
            .map(|index| Piece {
                at: share(index, steps),
                progress: share(index, steps),
                timing: one(wire::StepPosition::JumpEnd),
            })
            .collect(),
        // Each step shows the keyframe after it, so the first step is already one along.
        wire::StepPosition::JumpStart => (0..steps)
            .map(|index| Piece {
                at: share(index, steps),
                progress: share(index, steps),
                timing: one(wire::StepPosition::JumpStart),
            })
            .collect(),
        // The first step shows the start and the last the end.
        wire::StepPosition::JumpNone => (0..steps)
            .map(|index| Piece {
                at: share(index, steps),
                progress: share(index, steps.max(2) - 1).min(1.0),
                timing: one(wire::StepPosition::JumpEnd),
            })
            .collect(),
        // Neither the start nor the end is a step: step `k` shows `k + 1` shares of
        // `steps + 1`. The last keyframe is the last step's, at the segment's end, and the
        // segment's end follows it at the same offset, which is what the end shows.
        wire::StepPosition::JumpBoth => (0..=steps)
            .map(|index| Piece {
                at: share(index, steps).min(1.0),
                progress: share(index, steps + 1),
                timing: one(wire::StepPosition::JumpStart),
            })
            .collect(),
    }
}
