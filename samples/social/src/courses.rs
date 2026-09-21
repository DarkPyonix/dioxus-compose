//! The courses, the categories, and the scenes drawn on their cards.

use dioxus_compose::prelude::*;
use dioxus_compose::{DrawList, DrawListBuilder};

/// Which accent family a card's scene is drawn from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Palette {
    First,
    Second,
    Third,
}

impl Palette {
    /// The strong colour, the quiet fill it sits on, and the ink that reads on that fill.
    pub fn roles(self) -> (ColorRole, ColorRole, ColorRole) {
        match self {
            Palette::First => (
                ColorRole::Primary,
                ColorRole::PrimaryContainer,
                ColorRole::OnPrimaryContainer,
            ),
            Palette::Second => (
                ColorRole::Secondary,
                ColorRole::SecondaryContainer,
                ColorRole::OnSecondaryContainer,
            ),
            Palette::Third => (
                ColorRole::Tertiary,
                ColorRole::TertiaryContainer,
                ColorRole::OnTertiaryContainer,
            ),
        }
    }
}

/// One course.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Course {
    pub id: u32,
    pub title: &'static str,
    pub sessions: u32,
    pub minutes: u32,
    pub palette: Palette,
    /// Which shelf it belongs to.
    pub shelf: Shelf,
    /// What its scene is arranged from.
    pub seed: u32,
}

/// Which part of the app a course shows up in.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Shelf {
    ForYou,
    Meditate,
    Sleep,
}

pub const COURSES: [Course; 9] = [
    Course {
        id: 1,
        title: "Brief beginner meditation",
        sessions: 3,
        minutes: 3,
        palette: Palette::First,
        shelf: Shelf::ForYou,
        seed: 11,
    },
    Course {
        id: 2,
        title: "Seven days to improve your productivity",
        sessions: 7,
        minutes: 12,
        palette: Palette::Second,
        shelf: Shelf::ForYou,
        seed: 23,
    },
    Course {
        id: 3,
        title: "Seven days to prepare your day",
        sessions: 7,
        minutes: 9,
        palette: Palette::Third,
        shelf: Shelf::ForYou,
        seed: 37,
    },
    Course {
        id: 4,
        title: "Basics of meditation",
        sessions: 7,
        minutes: 10,
        palette: Palette::First,
        shelf: Shelf::Meditate,
        seed: 53,
    },
    Course {
        id: 5,
        title: "Control your breathing",
        sessions: 7,
        minutes: 8,
        palette: Palette::Second,
        shelf: Shelf::Meditate,
        seed: 67,
    },
    Course {
        id: 6,
        title: "Improve focus",
        sessions: 10,
        minutes: 15,
        palette: Palette::Third,
        shelf: Shelf::Meditate,
        seed: 79,
    },
    Course {
        id: 7,
        title: "The rear light",
        sessions: 1,
        minutes: 21,
        palette: Palette::First,
        shelf: Shelf::Sleep,
        seed: 97,
    },
    Course {
        id: 8,
        title: "Imaginary ride",
        sessions: 1,
        minutes: 21,
        palette: Palette::Second,
        shelf: Shelf::Sleep,
        seed: 113,
    },
    Course {
        id: 9,
        title: "Private planet",
        sessions: 1,
        minutes: 21,
        palette: Palette::Third,
        shelf: Shelf::Sleep,
        seed: 131,
    },
];

pub fn on(shelf: Shelf) -> Vec<&'static Course> {
    COURSES
        .iter()
        .filter(|course| course.shelf == shelf)
        .collect()
}

pub fn course(id: u32) -> Option<&'static Course> {
    COURSES.iter().find(|found| found.id == id)
}

/// How a course is described under its title.
pub fn sessions_label(course: &Course) -> String {
    let word = if course.sessions == 1 {
        "session"
    } else {
        "sessions"
    };
    format!("{} {word} \u{00b7} {} min", course.sessions, course.minutes)
}

/// A repeatable sequence from a seed, so a scene is the same scene every frame.
fn wobble(seed: u32, step: u32) -> f32 {
    let mixed = seed
        .wrapping_mul(1_664_525)
        .wrapping_add(step.wrapping_mul(1_013_904_223))
        .wrapping_mul(2_246_822_519);
    (mixed >> 8) as f32 / (1_u32 << 24) as f32
}

/// The illustration on a course's card: a sky, a sun, a hill and a frond.
///
/// The reference's cards carry drawn illustrations of people. A draw list has lines, arcs,
/// circles and rounded rectangles, and no way to reach a picture at all: `Image` takes an
/// id the Host registered and an application only has the tree. So this is the part of the
/// idea a draw list can hold, built from the card's own accent family, which means it
/// follows the reader into dark instead of staying the colour it was drawn.
pub fn scene(width: f32, height: f32, seed: u32, palette: Palette) -> DrawList {
    let (strong, quiet, ink) = palette.roles();
    let a = wobble(seed, 1);
    let b = wobble(seed, 2);
    let c = wobble(seed, 3);

    DrawListBuilder::with_capacity(5, 0)
        // The sky.
        .rect(Paint::Role(quiet), 0.0, 0.0, width, height, 0.0)
        // The hill: a filled disc whose middle sits below the bottom edge, so what is
        // left inside the picture is a dome. A stroked arc was the first attempt and came
        // out as a ring: an arc's stroke is centred on its radius, so a thick one bulges
        // out of the shape on both sides and reads as a donut rather than as a horizon.
        .circle(
            Paint::Role(strong),
            width * (0.22 + c * 0.5),
            height * 1.34,
            height * 1.02,
            0.0,
        )
        // The sun over it, in the ink, which is dark enough to read as a disc rather than
        // as a second hill.
        .circle(
            Paint::Role(ink),
            width * (0.18 + a * 0.5),
            height * (0.2 + b * 0.14),
            height * 0.11,
            0.0,
        )
        // A frond, as two strokes leaning out of the hill.
        .line(
            Paint::Role(ink),
            width * (0.72 + a * 0.12),
            height * 0.92,
            width * (0.82 + b * 0.1),
            height * 0.42,
            2.0,
        )
        .line(
            Paint::Role(ink),
            width * (0.78 + a * 0.1),
            height * 0.7,
            width * (0.9 + c * 0.06),
            height * 0.52,
            2.0,
        )
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every shelf has courses on it. An empty shelf reads as an app that failed to load
    /// rather than as a shelf nobody has filled.
    #[test]
    fn every_shelf_has_courses_on_it() {
        for shelf in [Shelf::ForYou, Shelf::Meditate, Shelf::Sleep] {
            assert!(!on(shelf).is_empty(), "{shelf:?} is empty");
        }
    }

    #[test]
    fn a_single_session_course_is_described_in_the_singular() {
        let one = course(7).expect("no course 7");
        assert_eq!(sessions_label(one), "1 session \u{00b7} 21 min");
        let many = course(2).expect("no course 2");
        assert_eq!(sessions_label(many), "7 sessions \u{00b7} 12 min");
    }

    /// Every id is its own. Two courses sharing one is a shelf where opening either gives
    /// you the first.
    #[test]
    fn every_course_has_an_id_of_its_own() {
        for (index, first) in COURSES.iter().enumerate() {
            for second in &COURSES[index + 1..] {
                assert_ne!(first.id, second.id, "{} repeats an id", second.title);
            }
        }
    }

    /// A scene is the same scene every time it is drawn. One that reshuffles on each
    /// redraw flickers, and one built from real randomness cannot be tested.
    #[test]
    fn fr16_a_scene_is_the_same_every_time_it_is_drawn() {
        let once = scene(200.0, 120.0, 11, Palette::First);
        let again = scene(200.0, 120.0, 11, Palette::First);
        assert_eq!(once.as_bytes(), again.as_bytes());
    }

    /// Two courses have to look different, or a shelf reads as one card repeated.
    #[test]
    fn fr16_two_seeds_draw_two_different_scenes() {
        let first = scene(200.0, 120.0, 11, Palette::First);
        let second = scene(200.0, 120.0, 23, Palette::First);
        assert_ne!(first.as_bytes(), second.as_bytes());
    }

    /// Every colour in a scene is a role.
    #[test]
    fn fr13_no_scene_carries_a_literal_colour() {
        for palette in [Palette::First, Palette::Second, Palette::Third] {
            for command in scene(200.0, 120.0, 11, palette)
                .decode()
                .expect("a scene did not decode")
            {
                assert!(
                    matches!(command.paint(), Paint::Role(_)),
                    "{command:?} is painted with something other than a role"
                );
            }
        }
    }

    /// A scene is a whole picture rather than an empty rectangle: the sky, the sun, the
    /// hill and the two strokes of the frond.
    #[test]
    fn fr16_a_scene_draws_every_part_of_itself() {
        let commands = scene(200.0, 120.0, 11, Palette::First)
            .decode()
            .expect("it did not decode");
        assert_eq!(commands.len(), 5);
    }
}
