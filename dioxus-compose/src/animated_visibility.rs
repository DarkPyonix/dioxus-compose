//! `AnimatedVisibility`: a child that fades, slides and collapses in and out as a flag
//! changes.
//!
//! The Renderer plays every motion. This component decides only when the child is declared
//! and when it is taken away: it stays declared through the exit animation, and the end of
//! that animation, reported by the Renderer as an animation event, is what removes it.
//! Nothing here is a timer.

use crate as dioxus_elements;
use crate::hooks::{Animated, use_animation_event, use_reduced_motion};
use compose_rust::{AnimationEventKind, AnimationSpec, MotionRole, ReducedMotion};
use dioxus_core::{AttributeValue, Element};
use dioxus_core_macro::{Props, component, rsx};
use dioxus_hooks::use_signal;
use dioxus_signals::{ReadableExt, WritableExt};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

thread_local! {
    /// Which instance an exit animation belongs to, by the animation's name. Written when
    /// the animation is started and read when the Renderer reports its end.
    static WATCHED: RefCell<HashMap<u32, u64>> = RefCell::new(HashMap::new());
    static NEXT_KEY: Cell<u64> = const { Cell::new(1) };
}

/// Remembers that the animation called `animation_id` belongs to the instance `key`.
pub(crate) fn watch_animation(animation_id: u32, key: u64) {
    WATCHED.with_borrow_mut(|watched| {
        watched.insert(animation_id, key);
    });
}

fn watcher_of(animation_id: u32) -> Option<u64> {
    WATCHED.with_borrow(|watched| watched.get(&animation_id).copied())
}

fn forget(animation_id: u32) {
    WATCHED.with_borrow_mut(|watched| {
        watched.remove(&animation_id);
    });
}

fn next_key() -> u64 {
    NEXT_KEY.with(|next| {
        let key = next.get();
        next.set(key + 1);
        key
    })
}

/// What plays when the child appears.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Enter {
    fade: bool,
    slide: Option<(f32, f32)>,
    expand: bool,
}

impl Enter {
    /// Nothing plays: the child is there.
    pub const NONE: Self = Self {
        fade: false,
        slide: None,
        expand: false,
    };

    /// From transparent to opaque.
    pub const fn fade() -> Self {
        Self {
            fade: true,
            ..Self::NONE
        }
    }

    /// From `offset` (dp, x and y) to its place.
    pub const fn slide(offset: (f32, f32)) -> Self {
        Self {
            slide: Some(offset),
            ..Self::NONE
        }
    }

    /// The container grows to the child's height with the theme's motion.
    pub const fn expand_vertically() -> Self {
        Self {
            expand: true,
            ..Self::NONE
        }
    }

    /// Both this and `other` play.
    pub const fn and(self, other: Self) -> Self {
        Self {
            fade: self.fade || other.fade,
            slide: match other.slide {
                Some(offset) => Some(offset),
                None => self.slide,
            },
            expand: self.expand || other.expand,
        }
    }
}

impl Default for Enter {
    /// A fade and a vertical expansion, as Compose's `AnimatedVisibility` does.
    fn default() -> Self {
        Self::fade().and(Self::expand_vertically())
    }
}

/// What plays when the child leaves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Exit {
    fade: bool,
    slide: Option<(f32, f32)>,
    shrink: bool,
}

impl Exit {
    /// Nothing plays: the child is gone.
    pub const NONE: Self = Self {
        fade: false,
        slide: None,
        shrink: false,
    };

    /// From opaque to transparent.
    pub const fn fade() -> Self {
        Self {
            fade: true,
            ..Self::NONE
        }
    }

    /// From its place to `offset` (dp, x and y).
    pub const fn slide(offset: (f32, f32)) -> Self {
        Self {
            slide: Some(offset),
            ..Self::NONE
        }
    }

    /// The container shrinks to nothing with the theme's motion.
    pub const fn shrink_vertically() -> Self {
        Self {
            shrink: true,
            ..Self::NONE
        }
    }

    /// Both this and `other` play.
    pub const fn and(self, other: Self) -> Self {
        Self {
            fade: self.fade || other.fade,
            slide: match other.slide {
                Some(offset) => Some(offset),
                None => self.slide,
            },
            shrink: self.shrink || other.shrink,
        }
    }
}

impl Default for Exit {
    /// A fade and a vertical shrink, as Compose's `AnimatedVisibility` does.
    fn default() -> Self {
        Self::fade().and(Self::shrink_vertically())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// The child is not declared.
    Hidden,
    /// The child is declared and shown, or on its way in.
    Open,
    /// The child is declared and on its way out. It is removed when the Renderer reports
    /// that the animation has ended.
    Leaving,
}

struct State {
    phase: Cell<Phase>,
    /// Whether the child that is declared appeared with a starting value to play from.
    from_start: Cell<bool>,
    /// Names this instance's exit animation in the animation events.
    key: u64,
}

/// Shows `children` while `visible` is true, and plays `enter` and `exit` as it changes.
///
/// ```ignore
/// rsx! {
///     AnimatedVisibility {
///         visible: open(),
///         enter: Enter::fade().and(Enter::expand_vertically()),
///         exit: Exit::fade().and(Exit::shrink_vertically()),
///         ToolCallGroup {}
///     }
/// }
/// ```
///
/// The fade and the slide are played by the Renderer as animations of the child's opacity
/// and offset, and this component is not rendered again while they play. The height
/// changes with the container's motion role, so a sibling below is pushed and pulled
/// smoothly. On the way out the child stays declared until the fade or the slide has
/// ended, then it is removed and the container closes over the space it held.
///
/// With reduced motion on, nothing plays: the child is declared and removed at once. A
/// child that is visible from the start is not animated in.
#[component]
pub fn AnimatedVisibility(
    visible: bool,
    #[props(default)] enter: Enter,
    #[props(default)] exit: Exit,
    /// The length and curve of the fade and the slide. The standard motion when left out.
    #[props(default)]
    spec: Option<AnimationSpec>,
    /// The motion role of the container's height change. The standard role when left out.
    #[props(default)]
    motion: Option<MotionRole>,
    children: Element,
) -> Element {
    let reduced = use_reduced_motion() == ReducedMotion::On;
    let mut changed = use_signal(|| 0_u32);
    let _ = *changed.read();
    let state = dioxus_core::use_hook(|| {
        Rc::new(State {
            phase: Cell::new(if visible { Phase::Open } else { Phase::Hidden }),
            from_start: Cell::new(false),
            key: next_key(),
        })
    });

    let heard = state.clone();
    use_animation_event(move |moment| {
        if moment.kind != AnimationEventKind::End
            || watcher_of(moment.animation_id) != Some(heard.key)
        {
            return;
        }
        forget(moment.animation_id);
        if heard.phase.get() == Phase::Leaving {
            heard.phase.set(Phase::Hidden);
            heard.from_start.set(false);
            *changed.write() += 1;
        }
    });

    let leaves_with_motion = exit.fade || exit.slide.is_some();
    match (state.phase.get(), visible) {
        (Phase::Hidden, true) => {
            state.phase.set(Phase::Open);
            state.from_start.set(!reduced);
        }
        (Phase::Open, false) => {
            if reduced || !leaves_with_motion {
                state.phase.set(Phase::Hidden);
                state.from_start.set(false);
            } else {
                state.phase.set(Phase::Leaving);
            }
        }
        (Phase::Leaving, true) => state.phase.set(Phase::Open),
        (Phase::Leaving, false) if reduced => {
            state.phase.set(Phase::Hidden);
            state.from_start.set(false);
        }
        _ => {}
    }

    let phase = state.phase.get();
    let leaving = phase == Phase::Leaving;
    let spec = spec.unwrap_or(AnimationSpec::STANDARD);
    let from_start = state.from_start.get();
    let container_motion = ((enter.expand || exit.shrink) && !reduced)
        .then(|| {
            let role: u16 = motion.unwrap_or(MotionRole::Standard).into();
            i64::from(role)
        });

    let alpha = (!reduced).then(|| Animated::<f32> {
        target: if leaving && exit.fade { 0.0 } else { 1.0 },
        spec,
        enter_from: (from_start && enter.fade).then_some(0.0),
        watch: if leaving && exit.fade { state.key } else { 0 },
    });
    let offset = (!reduced).then(|| Animated::<(f32, f32)> {
        target: if leaving {
            exit.slide.unwrap_or((0.0, 0.0))
        } else {
            (0.0, 0.0)
        },
        spec,
        enter_from: if from_start { enter.slide } else { None },
        watch: if leaving && !exit.fade && exit.slide.is_some() {
            state.key
        } else {
            0
        },
    });

    rsx! {
        column { motion: container_motion,
            if phase != Phase::Hidden {
                column {
                    animated_alpha: alpha.map(AttributeValue::any_value),
                    animated_offset: offset.map(AttributeValue::any_value),
                    {children}
                }
            }
        }
    }
}
