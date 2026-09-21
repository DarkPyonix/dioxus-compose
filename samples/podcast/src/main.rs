//! A podcast hub: new episodes, an episode you can open, and a player.
//!
//! The drawing this sample is about is the waveform. Every design system's player has one,
//! nothing in the widget vocabulary is one, and it is the clearest case of a `Canvas`
//! earning its place: forty columns, the played part in the accent and the rest in the
//! outline, built once into an attribute that only crosses the boundary when it changes.
//!
//! Unified, naming Cupertino: the reference is an iOS design.

mod library;

use dioxus_compose::prelude::*;
use library::{
    EPISODES, Episode, Family, SHOWS, Show, artwork, clock, episode, short_count, show_of, waveform,
};

/// A phone design in a desktop window is still a phone design.
const PAGE_MEASURE: f32 = 420.0;

/// How large each drawing is. Numbers, because these are the proportions of a picture.
const COVER_LARGE: f32 = 300.0;
const COVER_SMALL: f32 = 72.0;
const WAVE: (f32, f32) = (340.0, 72.0);
const WAVE_SMALL: (f32, f32) = (300.0, 48.0);
/// How many columns a waveform is drawn with.
///
/// The reference draws a few hundred hair-thin ones. Forty reads as a waveform and costs
/// forty records, which is a budget a frame can carry without anyone thinking about it.
const WAVE_COLUMNS: usize = 40;

/// Where the player picks up, in seconds. Part way in, because a player sitting at zero
/// shows a waveform with nothing played and says nothing about what the drawing does.
const START_SECONDS: u32 = 326;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Destination {
    New,
    Shows,
    You,
}

impl Destination {
    const STRIP: [Destination; 3] = [Destination::New, Destination::Shows, Destination::You];

    fn label(self) -> &'static str {
        match self {
            Destination::New => "New",
            Destination::Shows => "Shows",
            Destination::You => "You",
        }
    }

    fn icon(self) -> IconRole {
        match self {
            Destination::New => IconRole::Home,
            Destination::Shows => IconRole::Search,
            Destination::You => IconRole::Inbox,
        }
    }

    fn index(self) -> usize {
        Self::STRIP
            .iter()
            .position(|found| *found == self)
            .unwrap_or(0)
    }
}

/// A show's artwork at some size, as a drawing rather than a photograph.
fn cover(size: f32, seed: u32, family: Family) -> Element {
    rsx! {
        Canvas {
            width: size,
            height: size,
            shape_role: ShapeRole::Large,
            commands: artwork(size, seed, family),
        }
    }
}

/// One episode as a card: the artwork, a waveform either side of it, and what can be done
/// to it.
fn episode_card(found: &Episode, on_play: EventHandler<u32>) -> Element {
    let show = show_of(found);
    let (strong, _, _) = show.family.roles();
    let id = found.id;
    rsx! {
        Surface {
            fill_max_width: true,
            shape_role: ShapeRole::Large,
            padding_role: SpaceRole::Md,
            Column {
                fill_max_width: true,
                space_role: SpaceRole::Sm,
                Text {
                    text: "S{found.season}E{found.number}: {found.title}",
                    type_role: TypeRole::Subtitle,
                    text_align: TextAlign::Center,
                    fill_max_width: true,
                }
                Row {
                    fill_max_width: true,
                    space_role: SpaceRole::Sm,
                    alignment: Alignment::Center,
                    Canvas {
                        weight: 1.0,
                        height: WAVE_SMALL.1,
                        commands: waveform(
                            WAVE_SMALL.0 / 2.0,
                            WAVE_SMALL.1,
                            WAVE_COLUMNS / 2,
                            found.seed,
                            1.0,
                            ColorRole::OutlineVariant,
                            ColorRole::OutlineVariant,
                        ),
                    }
                    {cover(COVER_SMALL, found.seed, show.family)}
                    Canvas {
                        weight: 1.0,
                        height: WAVE_SMALL.1,
                        commands: waveform(
                            WAVE_SMALL.0 / 2.0,
                            WAVE_SMALL.1,
                            WAVE_COLUMNS / 2,
                            found.seed.wrapping_add(1),
                            0.0,
                            ColorRole::OutlineVariant,
                            ColorRole::OutlineVariant,
                        ),
                    }
                }
                Text {
                    text: found.blurb,
                    type_role: TypeRole::Body,
                    color: Paint::Role(ColorRole::OnSurfaceVariant),
                    max_lines: 3,
                    overflow: TextOverflow::Ellipsis,
                }
                Row {
                    fill_max_width: true,
                    space_role: SpaceRole::Sm,
                    alignment: Alignment::CenterStart,
                    Text {
                        text: "\u{2665} {short_count(found.hearts)}",
                        type_role: TypeRole::Label,
                        color: Paint::Role(strong),
                    }
                    // Words rather than marks. A comment bubble and a share arrow are
                    // icons, and an icon cannot be placed from application code: `Icon`
                    // takes an id the Host registered, and `IconRole` only reaches the
                    // tree through a navigation destination. A heart has a character that
                    // every font carries; the other two do not, and the ones that come
                    // closest arrive as an empty box on most systems.
                    Text {
                        text: "{short_count(found.comments)} replies",
                        type_role: TypeRole::Label,
                        color: Paint::Role(ColorRole::OnSurfaceVariant),
                    }
                    Text {
                        text: "{short_count(found.shares)} shares",
                        type_role: TypeRole::Label,
                        color: Paint::Role(ColorRole::OnSurfaceVariant),
                    }
                    Spacer { weight: 1.0 }
                    Button {
                        text: "\u{25b6}",
                        variant: ButtonVariant::Filled,
                        on_click: move |_| on_play.call(id),
                    }
                }
            }
        }
    }
}

/// A show as a row, with a follow button.
fn show_row(show: &Show, followed: bool, on_toggle: EventHandler<()>) -> Element {
    let (_, quiet, ink) = show.family.roles();
    rsx! {
        Row {
            fill_max_width: true,
            background: Paint::Role(quiet),
            shape_role: ShapeRole::Full,
            padding_role: SpaceRole::Sm,
            space_role: SpaceRole::Sm,
            alignment: Alignment::CenterStart,
            {cover(48.0, show.followers, show.family)}
            Column {
                weight: 1.0,
                Text {
                    text: show.name,
                    type_role: TypeRole::BodyStrong,
                    color: Paint::Role(ink),
                    max_lines: 1,
                    overflow: TextOverflow::Ellipsis,
                }
                Text {
                    text: "{short_count(show.followers)} followers",
                    type_role: TypeRole::Caption,
                    color: Paint::Role(ink),
                }
            }
            Button {
                text: if followed { "Following" } else { "Follow" },
                variant: if followed {
                    ButtonVariant::Outlined
                } else {
                    ButtonVariant::Filled
                },
                on_click: move |_| on_toggle.call(()),
            }
        }
    }
}

/// The shelf: a row of covers, then the newest episode in full, then who to follow.
fn new_page(
    followed: Signal<Vec<usize>>,
    on_play: EventHandler<u32>,
    on_open: EventHandler<u32>,
) -> Element {
    let mut followed = followed;
    let following = followed();
    rsx! {
        Column {
            fill_max_width: true,
            padding_role: SpaceRole::Md,
            space_role: SpaceRole::Md,

            Text { text: "New episodes", type_role: TypeRole::Headline }

            // The shelf across, windowed. A cover is a `Canvas`, so a shelf of them is a
            // shelf of drawings that only exist while they are on screen.
            LazyRow {
                fill_max_width: true,
                height: COVER_SMALL + 16.0,
                item_count: EPISODES.len(),
                key_of: move |position: usize| EPISODES[position].id.to_string(),
                item: move |position: usize| {
                    let found = EPISODES[position];
                    rsx! {
                        dioxus_compose::Box {
                            padding_role: SpaceRole::Xs,
                            Button {
                                text: "",
                                variant: ButtonVariant::Text,
                                padding_role: SpaceRole::None,
                                on_click: move |_| on_open.call(found.id),
                            }
                            {cover(COVER_SMALL, found.seed, SHOWS[found.show].family)}
                        }
                    }
                },
            }

            {episode_card(&EPISODES[0], on_play)}

            Text {
                text: "Shows you may like",
                type_role: TypeRole::Label,
                color: Paint::Role(ColorRole::OnSurfaceVariant),
            }
            Column {
                fill_max_width: true,
                space_role: SpaceRole::Sm,
                for (index, show) in SHOWS.iter().enumerate() {
                    {
                        let is_followed = following.contains(&index);
                        rsx! {
                            dioxus_compose::Box { key: "{show.name}", fill_max_width: true,
                                {show_row(show, is_followed, EventHandler::new(move |()| {
                                    let mut list = followed.write();
                                    match list.iter().position(|found| *found == index) {
                                        Some(at) => {
                                            list.remove(at);
                                        }
                                        None => list.push(index),
                                    }
                                }))}
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Every episode, shortest first, as a plain list.
fn shows_page(on_play: EventHandler<u32>) -> Element {
    let mut ordered: Vec<&'static Episode> = EPISODES.iter().collect();
    ordered.sort_by_key(|found| found.seconds);
    rsx! {
        Column {
            fill_max_width: true,
            padding_role: SpaceRole::Md,
            space_role: SpaceRole::Md,
            Text { text: "Shows", type_role: TypeRole::Headline }
            Surface {
                fill_max_width: true,
                shape_role: ShapeRole::Large,
                Column {
                    fill_max_width: true,
                    for (index, found) in ordered.iter().enumerate() {
                        {
                            let found: &'static Episode = found;
                            let last = index + 1 == EPISODES.len();
                            rsx! {
                                Column { key: "{found.id}", fill_max_width: true,
                                    Row {
                                        fill_max_width: true,
                                        padding_role: SpaceRole::Md,
                                        space_role: SpaceRole::Sm,
                                        alignment: Alignment::CenterStart,
                                        {cover(44.0, found.seed, SHOWS[found.show].family)}
                                        Column {
                                            weight: 1.0,
                                            Text {
                                                text: found.title,
                                                type_role: TypeRole::Body,
                                                max_lines: 1,
                                                overflow: TextOverflow::Ellipsis,
                                            }
                                            Text {
                                                text: "{SHOWS[found.show].name} \u{00b7} {clock(found.seconds)}",
                                                type_role: TypeRole::Caption,
                                                color: Paint::Role(ColorRole::OnSurfaceVariant),
                                            }
                                        }
                                        Button {
                                            text: "\u{25b6}",
                                            variant: ButtonVariant::Tonal,
                                            on_click: move |_| on_play.call(found.id),
                                        }
                                    }
                                    if !last {
                                        Separator {}
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// What you follow and what you have listened to.
fn you_page(followed: Vec<usize>) -> Element {
    let listened: u32 = EPISODES.iter().map(|found| found.seconds).sum();
    rsx! {
        Column {
            fill_max_width: true,
            padding_role: SpaceRole::Md,
            space_role: SpaceRole::Md,
            Text { text: "You", type_role: TypeRole::Headline }
            Row {
                fill_max_width: true,
                space_role: SpaceRole::Md,
                Column {
                    weight: 1.0,
                    background: Paint::Role(ColorRole::PrimaryContainer),
                    shape_role: ShapeRole::Large,
                    padding_role: SpaceRole::Md,
                    Text {
                        text: "{followed.len()}",
                        type_role: TypeRole::Display,
                        color: Paint::Role(ColorRole::OnPrimaryContainer),
                    }
                    Text {
                        text: "shows followed",
                        type_role: TypeRole::Label,
                        color: Paint::Role(ColorRole::OnPrimaryContainer),
                    }
                }
                Column {
                    weight: 1.0,
                    background: Paint::Role(ColorRole::TertiaryContainer),
                    shape_role: ShapeRole::Large,
                    padding_role: SpaceRole::Md,
                    Text {
                        text: clock(listened),
                        type_role: TypeRole::Display,
                        color: Paint::Role(ColorRole::OnTertiaryContainer),
                    }
                    Text {
                        text: "in the queue",
                        type_role: TypeRole::Label,
                        color: Paint::Role(ColorRole::OnTertiaryContainer),
                    }
                }
            }
        }
    }
}

/// The player: the whole page in the show's accent, with the transport at the bottom.
fn player_page(found: &Episode, position: Signal<u32>, on_back: EventHandler<()>) -> Element {
    let mut position = position;
    let show = show_of(found);
    let (strong, quiet, ink) = show.family.roles();
    let played = position() as f32 / found.seconds.max(1) as f32;
    let id_seed = found.seed;
    let length = found.seconds;

    rsx! {
        Column {
            fill_max_width: true,
            fill_max_height: true,
            background: Paint::Role(quiet),
            padding_role: SpaceRole::Md,
            space_role: SpaceRole::Md,

            Row {
                fill_max_width: true,
                alignment: Alignment::CenterStart,
                Button {
                    text: "\u{2190}",
                    variant: ButtonVariant::Text,
                    color: Paint::Role(ink),
                    on_click: move |_| on_back.call(()),
                }
                Spacer { weight: 1.0 }
                Text {
                    text: show.name,
                    type_role: TypeRole::BodyStrong,
                    color: Paint::Role(ink),
                }
                Spacer { weight: 1.0 }
                Button {
                    text: "Share",
                    variant: ButtonVariant::Text,
                    color: Paint::Role(ink),
                    on_click: move |_| {
                        Message::new("Sharing is not part of this sample").show();
                    },
                }
            }

            dioxus_compose::Box {
                fill_max_width: true,
                alignment: Alignment::Center,
                {cover(COVER_LARGE, id_seed, show.family)}
            }

            Column {
                fill_max_width: true,
                space_role: SpaceRole::Xs,
                Row {
                    fill_max_width: true,
                    space_role: SpaceRole::Sm,
                    alignment: Alignment::CenterStart,
                    Text {
                        text: show.host,
                        type_role: TypeRole::Label,
                        color: Paint::Role(ink),
                        weight: 1.0,
                    }
                    Text {
                        text: "\u{2605} 4.7",
                        type_role: TypeRole::Label,
                        color: Paint::Role(strong),
                    }
                }
                Text {
                    text: found.title,
                    type_role: TypeRole::Title,
                    color: Paint::Role(ink),
                }
                Text {
                    text: found.blurb,
                    type_role: TypeRole::Body,
                    color: Paint::Role(ink),
                    max_lines: 3,
                    overflow: TextOverflow::Ellipsis,
                }
            }

            Canvas {
                width: WAVE.0,
                height: WAVE.1,
                commands: waveform(
                    WAVE.0,
                    WAVE.1,
                    WAVE_COLUMNS,
                    id_seed,
                    played,
                    strong,
                    ColorRole::Outline,
                ),
            }
            Row {
                fill_max_width: true,
                alignment: Alignment::CenterStart,
                Text {
                    text: clock(position()),
                    type_role: TypeRole::Caption,
                    color: Paint::Role(ink),
                    weight: 1.0,
                }
                Text {
                    text: clock(length),
                    type_role: TypeRole::Caption,
                    color: Paint::Role(ink),
                }
            }

            Spacer { weight: 1.0 }

            Row {
                fill_max_width: true,
                space_role: SpaceRole::Sm,
                alignment: Alignment::Center,
                Button {
                    text: "\u{21ba} 15",
                    variant: ButtonVariant::Text,
                    color: Paint::Role(ink),
                    on_click: move |_| position.set(position().saturating_sub(15)),
                }
                Button {
                    text: "\u{25b6}",
                    variant: ButtonVariant::Filled,
                    on_click: move |_| {
                        Message::new("Playback is not part of this sample").show();
                    },
                }
                Button {
                    text: "15 \u{21bb}",
                    variant: ButtonVariant::Text,
                    color: Paint::Role(ink),
                    on_click: move |_| position.set((position() + 15).min(length)),
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

    let mut destination = use_signal(|| Destination::New);
    let mut playing = use_signal(|| Option::<u32>::None);
    let position = use_signal(|| START_SECONDS);
    let followed = use_signal(|| vec![0_usize]);

    let on_play = EventHandler::new(move |id: u32| playing.set(Some(id)));

    // The player covers everything, including the bar along the bottom, because a full
    // screen player is what the reference draws and a bar under it would be a way out of
    // a screen whose only way out is its own back button.
    if let Some(found) = playing().and_then(episode) {
        return rsx! {
            dioxus_compose::Box {
                fill_max_width: true,
                fill_max_height: true,
                alignment: Alignment::TopCenter,
                Column {
                    width: measure,
                    fill_max_width: measure.is_none(),
                    fill_max_height: true,
                    {player_page(
                        found,
                        position,
                        EventHandler::new(move |()| playing.set(None)),
                    )}
                }
            }
        };
    }

    let body = match destination() {
        Destination::New => new_page(followed, on_play, on_play),
        Destination::Shows => shows_page(on_play),
        Destination::You => you_page(followed()),
    };

    rsx! {
        Navigation {
            fill_max_width: true,
            fill_max_height: true,
            selected_index: destination().index(),
            for choice in Destination::STRIP {
                NavigationItem {
                    key: "{choice.label()}",
                    text: choice.label(),
                    icon: choice.icon(),
                    on_click: move |()| destination.set(choice),
                }
            }
            Column {
                fill_max_width: true,
                fill_max_height: true,
                background: Paint::Role(ColorRole::Background),
                dioxus_compose::Box {
                    fill_max_width: true,
                    fill_max_height: true,
                    alignment: Alignment::TopCenter,
                    ScrollColumn {
                        width: measure,
                        fill_max_width: measure.is_none(),
                        fill_max_height: true,
                        {body}
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

    /// The screen, driven the way a Renderer drives it. Every batch is kept, because a
    /// batch is the change since the frame before it rather than what is on screen.
    struct Screen {
        host: Host,
        frames: Vec<Vec<u8>>,
    }

    impl Screen {
        fn new() -> Self {
            dioxus_compose::window::reset_window_size();
            let mut host = Host::new(app);
            let first = host.rebuild().expect("the first frame failed").to_vec();
            Self {
                host,
                frames: vec![first],
            }
        }

        fn mutations(&self) -> Vec<Mutation<'_>> {
            self.frames
                .iter()
                .flat_map(|frame| decode_batch(frame).expect("a batch did not decode"))
                .collect()
        }

        fn press(&mut self, label: &str) -> bool {
            let found = {
                let mutations = self.mutations();
                let node = mutations.iter().rev().find_map(|mutation| match mutation {
                    Mutation::SetProp {
                        node_id,
                        property: PropertyKind::Text,
                        value: PropertyValue::String(text),
                    } if *text == label => Some(*node_id),
                    _ => None,
                });
                node.and_then(|node| {
                    mutations.iter().rev().find_map(|mutation| match mutation {
                        Mutation::SetProp {
                            node_id,
                            property: PropertyKind::OnClick,
                            value: PropertyValue::Integer(handler),
                        } if *node_id == node => Some((node, *handler as u64)),
                        _ => None,
                    })
                })
            };
            let Some((node_id, handler_id)) = found else {
                return false;
            };
            let mut bytes = Vec::new();
            encode_event(
                &HostEvent {
                    node_id,
                    handler_id,
                    payload: EventPayload::Clicked,
                },
                &mut bytes,
            )
            .expect("the click did not encode");
            let (batch, _) = self.host.dispatch_event(&bytes).expect("the click failed");
            if !batch.is_empty() {
                self.frames.push(batch.to_vec());
            }
            true
        }

        fn latest_texts(&self) -> Vec<String> {
            let Some(frame) = self.frames.last() else {
                return Vec::new();
            };
            decode_batch(frame)
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
    }

    #[test]
    fn the_first_frame_encodes_without_a_protocol_error() {
        assert!(Host::new(app).rebuild().is_ok());
    }

    /// Every destination has to encode, not just the one the app opens on.
    #[test]
    fn fr15_every_destination_encodes() {
        let mut screen = Screen::new();
        for choice in Destination::STRIP {
            assert!(
                screen.press(choice.label()),
                "the bar has no destination called {}",
                choice.label()
            );
            // Nothing is asserted about what came back. Pressing the destination the app
            // already opened on changes nothing, and a frame with nothing in it is the
            // right answer to that rather than a failure.
        }
        dioxus_compose::window::reset_window_size();
    }

    /// Opening the player replaces the whole screen and its back button returns.
    #[test]
    fn fr15_the_player_opens_and_closes() {
        let mut screen = Screen::new();
        assert!(screen.press("\u{25b6}"), "nothing on the shelf plays");
        assert!(
            screen
                .latest_texts()
                .iter()
                .any(|text| text == EPISODES[0].title),
            "the player did not come up"
        );
        assert!(screen.press("\u{2190}"), "the player has no way back");
        assert!(
            screen
                .latest_texts()
                .iter()
                .any(|text| text == "New episodes"),
            "closing the player did not bring the shelf back"
        );
        dioxus_compose::window::reset_window_size();
    }

    /// Skipping back moves the readout.
    ///
    /// It does not necessarily move the waveform, and that is the point rather than a gap
    /// in the test. Forty columns over twelve minutes is eighteen seconds a column, so
    /// most fifteen second skips leave every column exactly as it was and the draw list
    /// compares equal to the one already on screen. Nothing about it crosses the boundary,
    /// which is what an attribute comparison is for. The waveform's own behaviour when the
    /// position does move a column is checked where it is built, in
    /// `fr16_the_played_part_of_the_waveform_grows_with_the_position`.
    #[test]
    fn fr4_skipping_back_moves_the_readout() {
        let mut screen = Screen::new();
        assert!(screen.press("\u{25b6}"), "nothing on the shelf plays");
        let before = clock(START_SECONDS);
        assert!(
            screen.latest_texts().iter().any(|text| *text == before),
            "the player does not say where it is"
        );
        assert!(screen.press("\u{21ba} 15"), "the player cannot skip back");
        assert!(
            screen
                .latest_texts()
                .iter()
                .any(|text| *text == clock(START_SECONDS - 15)),
            "skipping back left the readout where it was"
        );
        dioxus_compose::window::reset_window_size();
    }

    /// Every drawing on the shelf has to reach the Renderer with commands on it. A canvas
    /// with none is a blank square, and on this screen most of the artwork is canvases.
    #[test]
    fn fr16_every_canvas_on_the_shelf_carries_a_draw_list() {
        dioxus_compose::window::reset_window_size();
        let batch = Host::new(app)
            .rebuild()
            .expect("the first frame failed")
            .to_vec();
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
        assert!(!canvases.is_empty(), "the shelf draws nothing at all");
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
                "a canvas reached the Renderer with no draw list"
            );
        }
        dioxus_compose::window::reset_window_size();
    }

    /// The shelf, in the design system it ships, in both schemes, at all three widths.
    #[test]
    fn fr16_the_shelf_is_recorded_in_the_system_it_ships() {
        sample_frames::record_in("Podcast", &[DesignSystem::Cupertino], app, |screen| {
            assert_eq!(
                screen.fill_lists(5),
                1,
                "the shelf should hold exactly one windowing list"
            );
        });
    }

    /// The player, which is the picture the waveform is for.
    #[test]
    fn fr16_the_player_is_recorded() {
        sample_frames::record_in("PodcastPlayer", &[DesignSystem::Cupertino], app, |screen| {
            screen.fill_lists(5);
            assert!(screen.press("\u{25b6}"), "nothing on the shelf plays");
        });
    }
}
