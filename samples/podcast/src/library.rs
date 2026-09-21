//! The shows, the episodes, and the two drawings a podcast screen is made of.

use dioxus_compose::prelude::*;
use dioxus_compose::{DrawList, DrawListBuilder};

/// One show.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Show {
    pub name: &'static str,
    pub host: &'static str,
    pub followers: u32,
    /// Which of the three accent families the show's artwork is built from.
    pub family: Family,
}

/// The accent family a piece of artwork is drawn from.
///
/// Cover art is a photograph or an illustration in the reference, and neither can be
/// declared from application code: `Image` takes an id the Host registered and an
/// application only has the tree. What a draw list can do is build a mark out of the
/// design system's own accents, which is what these three are for. A show is recognised by
/// its family and its shape rather than by a picture, and unlike a picture it follows the
/// reader into dark.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Family {
    First,
    Second,
    Third,
}

impl Family {
    /// The strong colour, the quiet fill it sits on, and the ink that reads on that fill.
    pub fn roles(self) -> (ColorRole, ColorRole, ColorRole) {
        match self {
            Family::First => (
                ColorRole::Primary,
                ColorRole::PrimaryContainer,
                ColorRole::OnPrimaryContainer,
            ),
            Family::Second => (
                ColorRole::Secondary,
                ColorRole::SecondaryContainer,
                ColorRole::OnSecondaryContainer,
            ),
            Family::Third => (
                ColorRole::Tertiary,
                ColorRole::TertiaryContainer,
                ColorRole::OnTertiaryContainer,
            ),
        }
    }
}

/// One episode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Episode {
    pub id: u32,
    pub show: usize,
    pub season: u32,
    pub number: u32,
    pub title: &'static str,
    pub blurb: &'static str,
    /// Whole seconds, so a position and a length are the same kind of number.
    pub seconds: u32,
    pub hearts: u32,
    pub comments: u32,
    pub shares: u32,
    /// What the artwork is built from, and what the waveform is drawn from.
    pub seed: u32,
}

pub const SHOWS: [Show; 3] = [
    Show {
        name: "Heavyweight",
        host: "Andrew Biletski",
        followers: 25_400,
        family: Family::First,
    },
    Show {
        name: "The Researcher",
        host: "Mira Okafor",
        followers: 12_100,
        family: Family::Second,
    },
    Show {
        name: "Sunpath",
        host: "Joel Iwu",
        followers: 8_300,
        family: Family::Third,
    },
];

pub const EPISODES: [Episode; 5] = [
    Episode {
        id: 1,
        show: 0,
        season: 1,
        number: 2,
        title: "Unstoppable ambition",
        blurb: "In this empowering episode we explore the mindset and habits that turn \
                ambition into momentum, and how to keep going once the novelty has worn \
                off.",
        seconds: 752,
        hearts: 198,
        comments: 54,
        shares: 32,
        seed: 17,
    },
    Episode {
        id: 2,
        show: 1,
        season: 2,
        number: 4,
        title: "Fuel your fire",
        blurb: "We dive into the spark that fuels lasting motivation, and how to tap into \
                your own drive when nothing external is pushing you.",
        seconds: 1_284,
        hearts: 142,
        comments: 31,
        shares: 18,
        seed: 41,
    },
    Episode {
        id: 3,
        show: 2,
        season: 1,
        number: 9,
        title: "The long quiet",
        blurb: "What happens in the months nobody writes about, and why the people who \
                last are rarely the ones who started loudest.",
        seconds: 2_015,
        hearts: 88,
        comments: 12,
        shares: 7,
        seed: 73,
    },
    Episode {
        id: 4,
        show: 0,
        season: 1,
        number: 3,
        title: "Room to be wrong",
        blurb: "On building the kind of working life where changing your mind costs you \
                nothing.",
        seconds: 1_640,
        hearts: 301,
        comments: 77,
        shares: 44,
        seed: 5,
    },
    Episode {
        id: 5,
        show: 1,
        season: 2,
        number: 5,
        title: "Small and finished",
        blurb: "Why a small thing that exists beats a large thing that is nearly ready, \
                every time, for everyone involved.",
        seconds: 934,
        hearts: 64,
        comments: 9,
        shares: 3,
        seed: 29,
    },
];

pub fn episode(id: u32) -> Option<&'static Episode> {
    EPISODES.iter().find(|found| found.id == id)
}

pub fn show_of(episode: &Episode) -> &'static Show {
    &SHOWS[episode.show]
}

/// A length or a position, as minutes and seconds.
pub fn clock(seconds: u32) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// A count, shortened once it stops being a number people read digit by digit.
pub fn short_count(value: u32) -> String {
    if value < 1_000 {
        return value.to_string();
    }
    let thousands = value / 1_000;
    let remainder = (value % 1_000) / 100;
    if remainder == 0 {
        format!("{thousands}k")
    } else {
        format!("{thousands}.{remainder}k")
    }
}

/// A repeatable sequence from a seed.
///
/// A waveform has to look irregular and has to be the same irregular shape every frame: a
/// chart that reshuffles itself on each redraw is a chart that flickers, and a chart built
/// from a real random source cannot be tested. This is the smallest thing that is both.
fn wobble(seed: u32, step: u32) -> f32 {
    let mixed = seed
        .wrapping_mul(1_664_525)
        .wrapping_add(step.wrapping_mul(1_013_904_223))
        .wrapping_mul(2_246_822_519);
    (mixed >> 8) as f32 / (1_u32 << 24) as f32
}

/// The waveform behind a player: one column per sample, the played part in one colour and
/// the rest in another.
///
/// The reference draws a few hundred hair-thin columns. This draws `columns` of them,
/// because every column is a record and a frame's worth of records is the budget the whole
/// project is built around. Forty reads as a waveform and costs forty records once, held
/// in an attribute that only re-crosses the boundary when it changes.
pub fn waveform(
    width: f32,
    height: f32,
    columns: usize,
    seed: u32,
    played: f32,
    played_role: ColorRole,
    rest_role: ColorRole,
) -> DrawList {
    if columns == 0 {
        return DrawListBuilder::with_capacity(0, 0).build();
    }
    let slot = width / columns as f32;
    let bar = slot * 0.5;
    let middle = height / 2.0;
    let mut list = DrawListBuilder::with_capacity(columns, 0);
    for index in 0..columns {
        // Never shorter than the column is wide, or a quiet moment is a speck that reads
        // as a rendering fault.
        let tall = (wobble(seed, index as u32) * height).max(bar);
        let role = if (index as f32 + 0.5) / columns as f32 <= played.clamp(0.0, 1.0) {
            played_role
        } else {
            rest_role
        };
        list = list.round_rect(
            Paint::Role(role),
            index as f32 * slot + (slot - bar) / 2.0,
            middle - tall / 2.0,
            bar,
            tall,
            bar / 2.0,
            0.0,
        );
    }
    list.build()
}

/// A show's artwork: overlapping marks built from one accent family.
///
/// Three shapes, arranged from the seed, so two shows are told apart by their family and
/// by where their marks sit. It is not a photograph and does not pretend to be one.
pub fn artwork(size: f32, seed: u32, family: Family) -> DrawList {
    let (strong, quiet, ink) = family.roles();
    let a = wobble(seed, 1);
    let b = wobble(seed, 2);
    DrawListBuilder::with_capacity(4, 0)
        .rect(Paint::Role(quiet), 0.0, 0.0, size, size, 0.0)
        .circle(
            Paint::Role(strong),
            size * (0.28 + a * 0.20),
            size * (0.30 + b * 0.16),
            size * 0.30,
            0.0,
        )
        .round_rect(
            Paint::Role(ink),
            size * (0.42 + b * 0.16),
            size * 0.46,
            size * 0.34,
            size * 0.40,
            size * 0.08,
            0.0,
        )
        .circle(
            Paint::Role(strong),
            size * 0.22,
            size * (0.68 + a * 0.10),
            size * 0.12,
            0.0,
        )
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus_compose::DrawCommand;

    #[test]
    fn a_clock_reads_as_minutes_and_seconds() {
        assert_eq!(clock(752), "12:32");
        assert_eq!(clock(59), "0:59");
        assert_eq!(clock(0), "0:00");
        assert_eq!(clock(3_601), "60:01");
    }

    #[test]
    fn a_count_is_shortened_once_it_stops_being_read_digit_by_digit() {
        assert_eq!(short_count(999), "999");
        assert_eq!(short_count(1_000), "1k");
        assert_eq!(short_count(25_400), "25.4k");
        assert_eq!(short_count(12_100), "12.1k");
    }

    /// A waveform that reshuffles itself on every redraw is a waveform that flickers, and
    /// one built from a real random source cannot be tested at all.
    #[test]
    fn fr16_a_waveform_is_the_same_shape_every_time_it_is_drawn() {
        let once = waveform(
            300.0,
            80.0,
            40,
            17,
            0.4,
            ColorRole::Primary,
            ColorRole::OutlineVariant,
        );
        let again = waveform(
            300.0,
            80.0,
            40,
            17,
            0.4,
            ColorRole::Primary,
            ColorRole::OutlineVariant,
        );
        assert_eq!(once.as_bytes(), again.as_bytes());
    }

    /// Two episodes have to look different, or a shelf of them reads as one thing five
    /// times.
    #[test]
    fn fr16_two_seeds_draw_two_different_waveforms() {
        let first = waveform(
            300.0,
            80.0,
            40,
            17,
            0.4,
            ColorRole::Primary,
            ColorRole::OutlineVariant,
        );
        let second = waveform(
            300.0,
            80.0,
            40,
            41,
            0.4,
            ColorRole::Primary,
            ColorRole::OutlineVariant,
        );
        assert_ne!(first.as_bytes(), second.as_bytes());
    }

    /// The played part of the waveform is what says how far in you are, so it has to grow
    /// with the position rather than being drawn once and left.
    #[test]
    fn fr16_the_played_part_of_the_waveform_grows_with_the_position() {
        fn played(at: f32) -> usize {
            waveform(
                300.0,
                80.0,
                40,
                17,
                at,
                ColorRole::Primary,
                ColorRole::OutlineVariant,
            )
            .decode()
            .expect("it did not decode")
            .iter()
            .filter(|command| command.paint() == Paint::Role(ColorRole::Primary))
            .count()
        }
        assert_eq!(played(0.0), 0);
        assert_eq!(played(1.0), 40);
        assert!(played(0.25) < played(0.75));
    }

    /// Nothing drawn here is a literal colour.
    #[test]
    fn fr13_no_drawing_carries_a_literal_colour() {
        let drawings = [
            waveform(
                300.0,
                80.0,
                40,
                17,
                0.4,
                ColorRole::Primary,
                ColorRole::OutlineVariant,
            ),
            artwork(200.0, 17, Family::First),
        ];
        for drawing in drawings {
            for command in drawing.decode().expect("a drawing did not decode") {
                assert!(
                    matches!(command.paint(), Paint::Role(_)),
                    "{command:?} is painted with something other than a role"
                );
            }
        }
    }

    /// An empty waveform is an empty drawing rather than a division by zero.
    #[test]
    fn a_waveform_with_no_columns_draws_nothing() {
        let list = waveform(
            300.0,
            80.0,
            0,
            17,
            0.4,
            ColorRole::Primary,
            ColorRole::OutlineVariant,
        );
        assert!(list.decode().expect("it did not decode").is_empty());
    }

    /// A quiet moment is still a column somebody can see.
    #[test]
    fn fr16_a_quiet_moment_is_still_drawn() {
        for command in waveform(
            300.0,
            80.0,
            40,
            17,
            0.4,
            ColorRole::Primary,
            ColorRole::OutlineVariant,
        )
        .decode()
        .expect("it did not decode")
        {
            if let DrawCommand::RoundRect { width, height, .. } = command {
                assert!(
                    height >= width,
                    "a column of {width} by {height} is thinner than it is tall"
                );
            }
        }
    }

    /// Every episode names a show that exists, so a shelf never has a row with no name on
    /// it.
    #[test]
    fn every_episode_belongs_to_a_show() {
        for found in EPISODES {
            assert!(found.show < SHOWS.len(), "{} has no show", found.title);
            assert!(found.seconds > 0, "{} has no length", found.title);
        }
    }
}
