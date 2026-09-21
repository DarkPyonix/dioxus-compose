//! Product statistics: a dial, a week of costs, and where the money went.
//!
//! This is the sample that pushes on `Canvas`. A dial with a tick ring and a marker, and a
//! bar chart with a day under each column, are pixels the widget vocabulary has no name
//! for, and they are the two most common things a statistics screen is made of. Both are
//! draw lists, and every colour in them is a role, so they follow the reader into dark and
//! come out in each design system's own accent.
//!
//! Unified, naming Cupertino: the reference is an iOS design.

mod charts;
mod spending;

use charts::Bar;
use dioxus_compose::prelude::*;
use spending::{
    ALL_TIME_CENTS, MOBILE_SHARE, RETURNING_SHARE, SOURCES, TODAY, WEEK, YESTERDAY, amount, cost,
    dearest, peak,
};

/// A phone design in a desktop window is still a phone design.
const PAGE_MEASURE: f32 = 420.0;

/// The square the dial is drawn into, and the box the week's columns get.
///
/// Numbers, because these are the proportions of a drawing. The space ladder answers how
/// far apart two things sit, not how large a picture is.
const DIAL_SIDE: f32 = 260.0;
/// The columns beside the dial, and the columns on the costs page.
///
/// Two sizes, because a draw list is in the canvas's own coordinates: the canvas is given
/// exactly these and the chart is drawn at exactly these, so nothing runs off the side and
/// nothing stops short of it.
const PANEL_CHART: (f32, f32) = (186.0, 150.0);
const PAGE_CHART: (f32, f32) = (340.0, 210.0);
const SOURCE_TILE: f32 = 150.0;

/// The two screens.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Page {
    Today,
    Costs,
}

impl Page {
    const STRIP: [Page; 2] = [Page::Today, Page::Costs];

    fn label(self) -> &'static str {
        match self {
            Page::Today => "Today",
            Page::Costs => "Costs",
        }
    }

    fn index(self) -> usize {
        Self::STRIP
            .iter()
            .position(|found| *found == self)
            .unwrap_or(0)
    }
}

/// The week as the chart wants it: heights against the dearest day, with that day filled.
fn week_bars() -> Vec<Bar> {
    let tallest = peak().max(1) as f32;
    let marked = dearest();
    WEEK.iter()
        .enumerate()
        .map(|(index, (day, cents))| Bar {
            day,
            height: *cents as f32 / tallest,
            filled: index == marked,
        })
        .collect()
}

/// The dial, with its reading stacked in the middle of it.
///
/// The number is a `Text` rather than a `TextAt` inside the draw list. A draw list's text
/// is placed at a point and takes a rung of the ladder, which is enough for a day label
/// under a column; a reading this size is the loudest thing on the screen and wants the
/// ladder's own metrics and the ellipsis behaviour that goes with a widget.
fn dial_card() -> Element {
    rsx! {
        Surface {
            fill_max_width: true,
            shape_role: ShapeRole::Large,
            padding_role: SpaceRole::Lg,
            dioxus_compose::Box {
                fill_max_width: true,
                alignment: Alignment::Center,
                Canvas {
                    width: DIAL_SIDE,
                    height: DIAL_SIDE,
                    commands: charts::dial(
                        DIAL_SIDE,
                        MOBILE_SHARE as f32 / 100.0,
                        ColorRole::OutlineVariant,
                        ColorRole::Tertiary,
                    ),
                }
                Column {
                    alignment: Alignment::Center,
                    Text { text: "{MOBILE_SHARE}%", type_role: TypeRole::Display }
                    Text {
                        text: "Mobile",
                        type_role: TypeRole::Label,
                        color: Paint::Role(ColorRole::OnSurfaceVariant),
                    }
                }
            }
        }
    }
}

/// The week's columns on a tinted panel, which is the shape the reference gives them.
///
/// The panel is `SecondaryContainer` and the chart's ink is its paired ink, so the chart
/// is guaranteed to be readable on it in every design system and in both schemes. Before
/// those roles existed the only way to say "a panel in a colour" was a literal, and a
/// literal is a colour the design system never sees.
fn costs_panel(title_role: TypeRole, (width, height): (f32, f32)) -> Element {
    let bars = week_bars();
    rsx! {
        Column {
            fill_max_width: true,
            background: Paint::Role(ColorRole::SecondaryContainer),
            shape_role: ShapeRole::Large,
            padding_role: SpaceRole::Md,
            space_role: SpaceRole::Sm,
            Text {
                text: "Popular costs",
                type_role: title_role,
                color: Paint::Role(ColorRole::OnSecondaryContainer),
            }
            Canvas {
                width,
                height,
                commands: charts::week(width, height, &bars, ColorRole::OnSecondaryContainer),
            }
        }
    }
}

/// The overview: the dial, the costs panel and a second reading beside it.
fn today_page() -> Element {
    rsx! {
        Column {
            fill_max_width: true,
            padding_role: SpaceRole::Md,
            space_role: SpaceRole::Md,

            {dial_card()}

            Row {
                fill_max_width: true,
                space_role: SpaceRole::Md,
                alignment: Alignment::TopStart,
                dioxus_compose::Box { weight: 2.0, {costs_panel(TypeRole::BodyStrong, PANEL_CHART)} }
                Column {
                    weight: 1.0,
                    space_role: SpaceRole::Sm,
                    Surface {
                        fill_max_width: true,
                        shape_role: ShapeRole::Large,
                        padding_role: SpaceRole::Md,
                        Text { text: "{RETURNING_SHARE}%", type_role: TypeRole::Title }
                    }
                    // The third accent, used as a fill rather than as a mark. The
                    // reference puts a plain block of colour here and that is all it is:
                    // a second reading that has not been written yet.
                    dioxus_compose::Box {
                        fill_max_width: true,
                        height: 64.0,
                        background: Paint::Role(ColorRole::TertiaryContainer),
                        shape_role: ShapeRole::Large,
                    }
                    Button {
                        text: "+",
                        fill_max_width: true,
                        variant: ButtonVariant::Tonal,
                        on_click: move |_| {
                            Message::new("Adding a source is not part of this sample").show();
                        },
                    }
                }
            }

            Separator {}

            Row {
                fill_max_width: true,
                space_role: SpaceRole::Md,
                alignment: Alignment::CenterStart,
                Text { text: "Today", type_role: TypeRole::BodyStrong }
                Text {
                    text: "{TODAY}",
                    type_role: TypeRole::Caption,
                    color: Paint::Role(ColorRole::OnSurfaceVariant),
                }
                Spacer { weight: 1.0 }
                Text {
                    text: "Yesterday",
                    type_role: TypeRole::Body,
                    color: Paint::Role(ColorRole::OnSurfaceVariant),
                }
                Text {
                    text: "{YESTERDAY}",
                    type_role: TypeRole::Caption,
                    color: Paint::Role(ColorRole::OnSurfaceVariant),
                }
            }
        }
    }
}

/// The costs in full: the whole page is the panel, and the sources run off the edge.
fn costs_page() -> Element {
    rsx! {
        Column {
            fill_max_width: true,
            fill_max_height: true,
            background: Paint::Role(ColorRole::SecondaryContainer),
            padding_role: SpaceRole::Md,
            space_role: SpaceRole::Md,

            Text {
                text: "Popular costs",
                type_role: TypeRole::Headline,
                color: Paint::Role(ColorRole::OnSecondaryContainer),
            }

            Canvas {
                width: PAGE_CHART.0,
                height: PAGE_CHART.1,
                commands: charts::week(
                    PAGE_CHART.0,
                    PAGE_CHART.1,
                    &week_bars(),
                    ColorRole::OnSecondaryContainer,
                ),
            }

            // Where the money went, one tile per service, running past the right edge.
            LazyRow {
                fill_max_width: true,
                height: SOURCE_TILE,
                item_count: SOURCES.len(),
                key_of: move |position: usize| SOURCES[position].name.to_owned(),
                item: move |position: usize| {
                    let source = SOURCES[position];
                    rsx! {
                        dioxus_compose::Box {
                            width: SOURCE_TILE,
                            fill_max_height: true,
                            padding_role: SpaceRole::Xs,
                            Column {
                                fill_max_width: true,
                                fill_max_height: true,
                                background: Paint::Role(ColorRole::TertiaryContainer),
                                shape_role: ShapeRole::Large,
                                padding_role: SpaceRole::Md,
                                space_role: SpaceRole::Xs,
                                Text {
                                    text: source.name,
                                    type_role: TypeRole::Label,
                                    color: Paint::Role(ColorRole::OnTertiaryContainer),
                                    max_lines: 1,
                                    overflow: TextOverflow::Ellipsis,
                                }
                                Spacer { weight: 1.0 }
                                Text {
                                    text: cost(source.cents),
                                    type_role: TypeRole::Title,
                                    color: Paint::Role(ColorRole::OnTertiaryContainer),
                                    max_lines: 1,
                                    overflow: TextOverflow::Ellipsis,
                                }
                            }
                        }
                    }
                },
            }

            Spacer { weight: 1.0 }

            Row {
                fill_max_width: true,
                space_role: SpaceRole::Sm,
                alignment: Alignment::CenterStart,
                Text {
                    text: "All time",
                    type_role: TypeRole::Body,
                    color: Paint::Role(ColorRole::OnSecondaryContainer),
                    weight: 1.0,
                }
                Text {
                    text: cost(ALL_TIME_CENTS),
                    type_role: TypeRole::Headline,
                    color: Paint::Role(ColorRole::OnSecondaryContainer),
                }
            }
        }
    }
}

fn app() -> Element {
    let window = use_window_size();
    let measure = if window.is_compact() {
        None
    } else {
        Some(PAGE_MEASURE)
    };
    let mut page = use_signal(|| Page::Today);

    rsx! {
        Column {
            fill_max_width: true,
            fill_max_height: true,
            background: Paint::Role(ColorRole::Background),

            TopAppBar {
                fill_max_width: true,
                Text { text: "Product statistics", type_role: TypeRole::Title, weight: 1.0 }
                Text {
                    text: "{amount(ALL_TIME_CENTS)} all time",
                    type_role: TypeRole::Label,
                    color: Paint::Role(ColorRole::OnSurfaceVariant),
                }
            }

            dioxus_compose::Box {
                fill_max_width: true,
                weight: 1.0,
                alignment: Alignment::TopCenter,
                Column {
                    width: measure,
                    fill_max_width: measure.is_none(),
                    fill_max_height: true,
                    dioxus_compose::Box {
                        fill_max_width: true,
                        padding_role: SpaceRole::Md,
                        Tabs {
                            fill_max_width: true,
                            selected_index: page().index(),
                            for choice in Page::STRIP {
                                Button {
                                    key: "{choice.label()}",
                                    text: choice.label(),
                                    variant: ButtonVariant::Text,
                                    on_click: move |_| page.set(choice),
                                }
                            }
                        }
                    }
                    match page() {
                        Page::Today => rsx! {
                            ScrollColumn { fill_max_width: true, fill_max_height: true, {today_page()} }
                        },
                        // The costs page is the panel, edge to edge, so it does not scroll
                        // inside something else: the tint has to reach the window's sides
                        // or it is a card pretending to be a page.
                        Page::Costs => costs_page(),
                    }
                }
            }
        }
    }
}

fn main() {
    dioxus_compose::LaunchBuilder::new()
        .with_theme(Theme::unified(DesignSystem::Cupertino))
        .launch(app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus_compose::Host;
    use dioxus_compose::protocol::{
        HostEvent, Mutation, PropertyValue, decode_batch, encode_event,
    };
    use dioxus_compose::schema::{EventPayload, PropertyKind, WidgetKind};

    fn first_frame() -> Vec<u8> {
        dioxus_compose::window::reset_window_size();
        let mut host = Host::new(app);
        let batch = host.rebuild().expect("the first frame failed").to_vec();
        dioxus_compose::window::reset_window_size();
        batch
    }

    fn texts(batch: &[u8]) -> Vec<String> {
        decode_batch(batch)
            .expect("the batch did not decode")
            .iter()
            .filter_map(|mutation| match mutation {
                Mutation::SetProp {
                    property: PropertyKind::Text,
                    value: PropertyValue::String(text),
                    ..
                } => Some((*text).to_owned()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_first_frame_encodes_without_a_protocol_error() {
        assert!(Host::new(app).rebuild().is_ok());
    }

    /// Both pages have to encode, not just the one the screen opens on.
    #[test]
    fn fr15_both_pages_encode() {
        dioxus_compose::window::reset_window_size();
        let mut host = Host::new(app);
        let first = host.rebuild().expect("the first frame failed").to_vec();
        for page in Page::STRIP {
            let mutations = decode_batch(&first).expect("the batch did not decode");
            let node = mutations
                .iter()
                .find_map(|mutation| match mutation {
                    Mutation::SetProp {
                        node_id,
                        property: PropertyKind::Text,
                        value: PropertyValue::String(text),
                    } if *text == page.label() => Some(*node_id),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("no segment called {}", page.label()));
            let handler = mutations
                .iter()
                .find_map(|mutation| match mutation {
                    Mutation::SetProp {
                        node_id,
                        property: PropertyKind::OnClick,
                        value: PropertyValue::Integer(handler),
                    } if *node_id == node => Some(*handler as u64),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{} cannot be pressed", page.label()));
            let mut bytes = Vec::new();
            encode_event(
                &HostEvent {
                    node_id: node,
                    handler_id: handler,
                    payload: EventPayload::Clicked,
                },
                &mut bytes,
            )
            .expect("the click did not encode");
            host.dispatch_event(&bytes)
                .unwrap_or_else(|error| panic!("{} failed: {error:?}", page.label()));
        }
        dioxus_compose::window::reset_window_size();
    }

    /// Both charts have to reach the Renderer as draw lists. A `Canvas` with no commands
    /// on it is a blank rectangle, and a blank rectangle is exactly what a chart that
    /// failed to build looks like.
    #[test]
    fn fr16_the_dial_and_the_week_both_carry_draw_lists() {
        let batch = first_frame();
        let mutations = decode_batch(&batch).expect("the batch did not decode");
        let canvases: Vec<u32> = mutations
            .iter()
            .filter_map(|mutation| match mutation {
                Mutation::Create {
                    node_id,
                    widget: WidgetKind::Canvas,
                } => Some(*node_id),
                _ => None,
            })
            .collect();
        assert_eq!(
            canvases.len(),
            2,
            "the overview should hold the dial and the week, and nothing else drawn"
        );
        for canvas in canvases {
            assert!(
                mutations.iter().any(|mutation| matches!(
                    mutation,
                    Mutation::SetProp {
                        node_id,
                        property: PropertyKind::Commands,
                        ..
                    } if *node_id == canvas
                )),
                "a canvas reached the Renderer with no draw list, so it is a blank box"
            );
        }
    }

    /// The reading in the middle of the dial and the reading the dial is drawn from are
    /// one number. Two of them is how a chart starts disagreeing with its own label.
    #[test]
    fn fr16_the_dial_says_the_number_it_is_drawn_from() {
        assert!(
            texts(&first_frame())
                .iter()
                .any(|text| text == &format!("{MOBILE_SHARE}%")),
            "the dial's reading is not written anywhere on it"
        );
    }

    /// The overview, in the design system it ships, in both schemes, at all three widths.
    #[test]
    fn fr16_the_overview_is_recorded_in_the_system_it_ships() {
        sample_frames::record_in("Statistics", &[DesignSystem::Cupertino], app, |_| {});
    }

    /// The costs page, which is the tinted one: a different picture, and the one where a
    /// chart drawn in the wrong ink would be invisible rather than merely wrong.
    #[test]
    fn fr13_the_costs_page_is_recorded() {
        sample_frames::record_in(
            "StatisticsCosts",
            &[DesignSystem::Cupertino],
            app,
            |screen| {
                assert!(
                    screen.press(Page::Costs.label()),
                    "the screen has no way to reach the costs page"
                );
                screen.fill_lists(5);
            },
        );
    }
}
