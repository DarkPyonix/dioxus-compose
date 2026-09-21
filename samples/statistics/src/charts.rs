//! The two drawings: a dial and a week of bars.
//!
//! Both are `DrawList`s rather than widgets, because a chart is pixels the widget
//! vocabulary has no name for. Every colour in them is a `Paint::Role`, so a dial drawn on
//! a panel follows the reader into dark along with everything around it, and a dial drawn
//! in another design system comes out in that system's accent rather than in this file's
//! idea of blue.
//!
//! Neither takes a colour argument for the ink and both take one for the ground. A chart
//! sitting on the page and the same chart sitting on a tinted panel need different ink,
//! and which panel it is on is something only the screen knows.

use dioxus_compose::prelude::*;
use dioxus_compose::{DrawList, DrawListBuilder};

/// How much of the dial's radius a tick takes.
const TICK_LENGTH: f32 = 0.12;
/// How many ticks go round the dial. Twelve, like a clock face, which is what the
/// reference draws and what a reader can count without counting.
const TICKS: usize = 12;

/// A dial reading `fraction` of the way round, with a marker where the reading is.
///
/// The ticks are drawn as short lines rather than as an arc with dashes, because an arc
/// command has one sweep and a dashed stroke is not something the draw vocabulary can
/// say. Twelve lines is the same picture and costs twelve records once.
pub fn dial(size: f32, fraction: f32, ink: ColorRole, marker: ColorRole) -> DrawList {
    let middle = size / 2.0;
    let outer = middle * 0.94;
    let inner = outer * (1.0 - TICK_LENGTH);
    let mut list = DrawListBuilder::with_capacity(TICKS + 2, 0);
    for step in 0..TICKS {
        // From the top, clockwise, so the marker and the ticks agree about where zero is.
        let angle =
            (step as f32 / TICKS as f32) * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
        let (sin, cos) = angle.sin_cos();
        list = list.line(
            Paint::Role(ink),
            middle + cos * inner,
            middle + sin * inner,
            middle + cos * outer,
            middle + sin * outer,
            2.0,
        );
    }
    let angle = fraction.clamp(0.0, 1.0) * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
    let (sin, cos) = angle.sin_cos();
    let reading = (outer + inner) / 2.0;
    list.circle(
        Paint::Role(marker),
        middle + cos * reading,
        middle + sin * reading,
        size * 0.022,
        0.0,
    )
    .build()
}

/// One column of the week's chart.
#[derive(Clone, Copy, Debug)]
pub struct Bar {
    pub day: &'static str,
    /// Nought to one, against the tallest column of the week.
    pub height: f32,
    /// The one column the chart is about. The rest are drawn as outlines.
    pub filled: bool,
}

/// A week of columns with the day under each.
///
/// `width` is the box the drawing is given, not a box of its own choosing: a draw list is
/// in the canvas's own coordinates and knows nothing about how wide the canvas turned out,
/// so a chart drawn at three hundred inside a canvas that filled two hundred runs off the
/// side, and one drawn at three hundred inside a canvas that filled four stops short. The
/// caller passes the same number to both.
///
/// The columns that are not the subject are drawn as outlines rather than as a paler fill.
/// A paler fill would mean a colour between the ink and the ground, and there is no role
/// for "the ink, quieter": the vocabulary has fills and it has inks, and half of an ink is
/// a literal. An outline is the same distinction said in stroke width, which every design
/// system can draw and no design system has to invent a colour for.
pub fn week(width: f32, height: f32, bars: &[Bar], ink: ColorRole) -> DrawList {
    if bars.is_empty() {
        return DrawListBuilder::with_capacity(0, 0).build();
    }
    // The label sits under the columns, so the columns get what is left. The baseline is
    // set three quarters of the way down that band rather than at the bottom of it,
    // because a baseline at the bottom puts the descenders outside the box and the row of
    // days comes out with its tails shaved off.
    let label_band = height * 0.22;
    let plot = height - label_band;
    let baseline = plot + label_band * 0.62;
    let slot = width / bars.len() as f32;
    let bar_width = slot * 0.46;
    let radius = bar_width / 2.0;

    let mut list = DrawListBuilder::with_capacity(bars.len() * 2, bars.len() * 4);
    for (index, bar) in bars.iter().enumerate() {
        let column = bar.height.clamp(0.0, 1.0) * plot;
        // Never shorter than a full round cap, or a quiet day is drawn as a sliver that
        // reads as a rendering fault rather than as a small number.
        let column = column.max(bar_width);
        let x = index as f32 * slot + (slot - bar_width) / 2.0;
        list = list.round_rect(
            Paint::Role(ink),
            x,
            plot - column,
            bar_width,
            column,
            radius,
            if bar.filled { 0.0 } else { 2.0 },
        );
        list = list.text_at(
            Paint::Role(ink),
            bar.day,
            index as f32 * slot + slot / 2.0,
            baseline,
            TypeRole::Caption,
        );
    }
    list.build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus_compose::DrawCommand;

    #[test]
    fn fr16_a_dial_draws_a_tick_for_every_step_and_one_marker() {
        let list = dial(200.0, 0.55, ColorRole::OutlineVariant, ColorRole::Tertiary);
        let commands = list.decode().expect("the dial did not decode");
        assert_eq!(commands.len(), TICKS + 1);
        assert_eq!(
            commands
                .iter()
                .filter(|command| matches!(command, DrawCommand::Circle { .. }))
                .count(),
            1,
            "a dial has one reading on it"
        );
    }

    /// Every colour in a chart is a role. A literal is a colour the design system never
    /// sees, so a chart full of them keeps its light-mode palette when the reader asks for
    /// dark and the panel around it changes underneath.
    #[test]
    fn fr13_no_command_in_either_chart_carries_a_literal_colour() {
        let charts = [
            dial(200.0, 0.55, ColorRole::OutlineVariant, ColorRole::Tertiary),
            week(
                300.0,
                160.0,
                &[
                    Bar {
                        day: "Mon",
                        height: 0.3,
                        filled: false,
                    },
                    Bar {
                        day: "Tue",
                        height: 1.0,
                        filled: true,
                    },
                ],
                ColorRole::OnSurface,
            ),
        ];
        for chart in charts {
            for command in chart.decode().expect("a chart did not decode") {
                assert!(
                    matches!(command.paint(), Paint::Role(_)),
                    "{command:?} is painted with something other than a role"
                );
            }
        }
    }

    /// A day with nothing spent on it still has to be a column somebody can see. A bar
    /// scaled straight from zero is a one pixel line, which reads as a fault.
    #[test]
    fn fr16_a_quiet_day_is_still_drawn_as_a_column() {
        let list = week(
            300.0,
            160.0,
            &[Bar {
                day: "Mon",
                height: 0.0,
                filled: false,
            }],
            ColorRole::OnSurface,
        );
        let commands = list.decode().expect("the week did not decode");
        let drawn = commands
            .iter()
            .find_map(|command| match command {
                DrawCommand::RoundRect { height, width, .. } => Some((width, height)),
                _ => None,
            })
            .expect("no column was drawn at all");
        assert!(
            drawn.1 >= drawn.0,
            "a column of {} by {} is thinner than it is tall",
            drawn.0,
            drawn.1
        );
    }

    /// An empty week is an empty drawing rather than a panic or a division by zero.
    #[test]
    fn a_week_with_no_days_draws_nothing() {
        let list = week(300.0, 160.0, &[], ColorRole::OnSurface);
        assert!(list.decode().expect("it did not decode").is_empty());
    }
}
