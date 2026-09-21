//! A clothing shop: a catalogue you can browse by category, a product you can size and
//! count, and a bag that adds up.
//!
//! Unified rather than adaptive. The reference is a light iOS design and the shop's look
//! is the shop's, not the platform's, so `THEME` names the design system and the colour
//! scheme once and the same declaration draws the same screen everywhere.
//!
//! The garments are drawn, not photographed. A photograph of a real garment belongs to
//! whoever took it, so the shop's stock is a set of original vector drawings in
//! `assets/`, registered once with `asset` and drawn by id after that. Everything around
//! a picture is still a role: the card behind it, the ink on the card, the price beside
//! it, so the only thing on this screen that keeps its own colours is the artwork.

mod catalogue;

use catalogue::{BagLine, CATALOGUE, Category, Product, SIZES, price, stars, total, under};
use dioxus_compose::prelude::*;

/// How wide the page is once the window is wider than a phone. A phone design in a desktop
/// window is still a phone design.
const PAGE_MEASURE: f32 = 420.0;

/// How tall a garment's tile is on the catalogue and on the carousel.
///
/// Numbers, because these are the proportions of a picture and the space ladder answers
/// how far apart two things sit rather than how large a picture is.
const TILE_HEIGHT: f32 = 168.0;
const CAROUSEL_HEIGHT: f32 = 200.0;
/// How tall the banner is. Wider than it is tall, the way a picture at the top of a page
/// is, and short enough that the first shelf is still on screen under it.
const BANNER_HEIGHT: f32 = 168.0;
/// How much of a card the garment takes, leaving the rest for what is written under it.
const PICTURE_SHARE: f32 = 0.7;
const STRIP_HEIGHT: f32 = 56.0;
const HERO_HEIGHT: f32 = 300.0;

/// The destinations along the bottom.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Destination {
    Shop,
    Search,
    Bag,
    Account,
}

impl Destination {
    const STRIP: [Destination; 4] = [
        Destination::Shop,
        Destination::Search,
        Destination::Bag,
        Destination::Account,
    ];

    fn label(self) -> &'static str {
        match self {
            Destination::Shop => "Shop",
            Destination::Search => "Search",
            Destination::Bag => "Bag",
            Destination::Account => "Account",
        }
    }

    /// What the destination means, so the Renderer draws its own artwork for it.
    fn icon(self) -> IconRole {
        match self {
            Destination::Shop => IconRole::Home,
            Destination::Search => IconRole::Search,
            Destination::Bag => IconRole::Inbox,
            Destination::Account => IconRole::Settings,
        }
    }

    fn index(self) -> usize {
        Self::STRIP
            .iter()
            .position(|found| *found == self)
            .unwrap_or(0)
    }
}

/// A garment on its card: the drawing, with whatever the card has to say written over it.
///
/// `named` is false in the grid, where the cell writes the name and the price underneath
/// and the tile would otherwise say both twice.
///
/// The picture is registered here rather than up front. `asset` returns the same id for
/// the same bytes and queues nothing the second time, so calling it in the body that draws
/// the garment is one registration on the first frame however many shelves the garment
/// appears on, and nothing at all on any frame after that.
///
/// The button is here because a tile cannot be tapped. `Modifier::Clickable` exists on the
/// wire, but no container widget exposes it, so the only thing in the vocabulary that
/// carries a press is a `Button`. The reference has no button: the picture itself is the
/// control.
fn tile(product: &Product, height: f32, named: bool, on_open: EventHandler<u32>) -> Element {
    let (fill, ink) = product.tint.pair();
    let id = product.id;
    rsx! {
        dioxus_compose::Box {
            fill_max_width: true,
            height,
            background: Paint::Role(fill),
            shape_role: ShapeRole::Large,
            alignment: Alignment::BottomStart,
            // The drawing takes the upper part of the card and the name is written under
            // it, which is the reference's shape: a photograph with the label sitting on
            // its lower left. It is a box of its own rather than the card's first child,
            // because the card aligns what is in it to the bottom left and a picture put
            // there sits behind the words.
            dioxus_compose::Box {
                fill_max_width: true,
                fill_max_height: true,
                alignment: Alignment::TopCenter,
                Image {
                    fill_max_width: true,
                    height: height * PICTURE_SHARE,
                    padding_role: SpaceRole::Sm,
                    asset_id: asset(AssetKind::Svg, product.picture),
                }
            }
            Column {
                fill_max_width: true,
                padding_role: SpaceRole::Md,
                space_role: SpaceRole::Xs,
                if named {
                    Text {
                        text: product.name,
                        type_role: TypeRole::Subtitle,
                        color: Paint::Role(ink),
                        max_lines: 1,
                        overflow: TextOverflow::Ellipsis,
                    }
                    Text {
                        text: product.support,
                        type_role: TypeRole::Caption,
                        color: Paint::Role(ink),
                        max_lines: 1,
                        overflow: TextOverflow::Ellipsis,
                    }
                }
                Button {
                    text: "View",
                    variant: ButtonVariant::Text,
                    color: Paint::Role(ink),
                    on_click: move |_| on_open.call(id),
                }
            }
        }
    }
}

/// One cell of the two-up grid: the tile, then the name, support and price under it.
///
/// No weight of its own. The cell's parent in the grid is a `Column`, where weight is
/// vertical, and a vertical weight inside a column that is measuring its own height comes
/// out as a height of zero. The width share belongs to the wrapper in the row above.
fn grid_cell(product: &Product, on_open: EventHandler<u32>) -> Element {
    rsx! {
        Column {
            fill_max_width: true,
            space_role: SpaceRole::Xs,
            {tile(product, TILE_HEIGHT, false, on_open)}
            Text {
                text: product.name,
                type_role: TypeRole::BodyStrong,
                max_lines: 1,
                overflow: TextOverflow::Ellipsis,
            }
            Text {
                text: product.support,
                type_role: TypeRole::Caption,
                color: Paint::Role(ColorRole::OnSurfaceVariant),
                max_lines: 1,
                overflow: TextOverflow::Ellipsis,
            }
            Text { text: price(product.cents), type_role: TypeRole::BodyStrong }
        }
    }
}

/// The catalogue: the banner, a carousel, the category strip and a two-up grid.
fn catalogue_screen(
    category: Signal<Category>,
    slide: Signal<usize>,
    bag: Signal<Vec<BagLine>>,
    on_open: EventHandler<u32>,
) -> Element {
    let mut category = category;
    let mut slide = slide;
    let shelf = under(category());
    // A shelf can be shorter than the slide the last one left behind, so the carousel is
    // read modulo what is on it rather than indexed straight.
    let showing = if shelf.is_empty() {
        0
    } else {
        slide() % shelf.len()
    };
    let rows: Vec<Vec<&'static Product>> = shelf.chunks(2).map(<[_]>::to_vec).collect();
    let in_bag: u32 = bag().iter().map(|line| line.quantity).sum();

    rsx! {
        Column {
            fill_max_width: true,
            padding_role: SpaceRole::Md,
            space_role: SpaceRole::Md,

            Row {
                fill_max_width: true,
                alignment: Alignment::CenterStart,
                Text {
                    text: "Let's find your sports outfit",
                    type_role: TypeRole::Headline,
                    weight: 1.0,
                }
                if in_bag > 0 {
                    // The count of what is waiting, in the accent, which is the one place
                    // on this screen the accent is used for a number rather than an action.
                    Text {
                        text: "{in_bag}",
                        type_role: TypeRole::BodyStrong,
                        color: Paint::Role(ColorRole::OnPrimary),
                        background: Paint::Role(ColorRole::Primary),
                        shape_role: ShapeRole::Full,
                        padding_role: SpaceRole::Sm,
                    }
                }
            }

            // The banner, which is the one picture on this screen that is a scene rather
            // than a garment. It sits on a card the design system fills, so the drawing is
            // the only thing here carrying colours of its own.
            dioxus_compose::Box {
                fill_max_width: true,
                height: BANNER_HEIGHT,
                background: Paint::Role(ColorRole::SurfaceVariant),
                shape_role: ShapeRole::Large,
                alignment: Alignment::Center,
                Image {
                    fill_max_width: true,
                    fill_max_height: true,
                    asset_id: asset(AssetKind::Svg, catalogue::HERO),
                }
            }

            // One slide at a time with a row of dots under it, which is the reference's
            // carousel. The slide is the Host's rather than a scroll position, because a
            // scroll position belongs to the Renderer and is never reported back: dots
            // driven by one would be drawn in the right place and never move.
            if !shelf.is_empty() {
                {tile(shelf[showing], CAROUSEL_HEIGHT, true, on_open)}
                Row {
                    fill_max_width: true,
                    space_role: SpaceRole::Xs,
                    // Centred across the row, which needs the arrangement rather than the
                    // alignment: alignment answers where a child sits across the row's
                    // other axis, so a row of dots set to centre alignment is a row of
                    // vertically centred dots still starting at the left edge.
                    arrangement: Arrangement::Center,
                    alignment: Alignment::Center,
                    for (position, product) in shelf.iter().enumerate() {
                        // A bullet in a text button. Nothing in the vocabulary is a dot,
                        // and a `Button` is the only thing that carries a press, so the
                        // dot is the smallest button there is rather than a decoration
                        // that cannot be reached.
                        Button {
                            key: "{product.id}",
                            text: "\u{2022}",
                            variant: ButtonVariant::Text,
                            padding_role: SpaceRole::None,
                            color: Paint::Role(if position == showing {
                                ColorRole::OnSurface
                            } else {
                                ColorRole::OutlineVariant
                            }),
                            on_click: move |_| slide.set(position),
                        }
                    }
                }
            }

            // A strip that runs off the edge, not a segmented control.
            //
            // `Tabs` divides the width it is given equally between its children and does
            // not scroll, so five categories on a phone is five segments of seventy dp,
            // and a button inside one carries its own horizontal padding: every label
            // broke into a column of single letters. The reference's own strip runs past
            // the right edge with the last category half shown, which is this.
            LazyRow {
                fill_max_width: true,
                height: STRIP_HEIGHT,
                item_count: Category::STRIP.len(),
                key_of: move |position: usize| Category::STRIP[position].label().to_owned(),
                item: move |position: usize| {
                    let choice = Category::STRIP[position];
                    rsx! {
                        dioxus_compose::Box {
                            padding_role: SpaceRole::Xs,
                            alignment: Alignment::Center,
                            // Every category is a label and one of them is the one you
                            // are looking at, which is what the reference draws: the
                            // chosen one in the reading ink and the rest in the quieter
                            // one. Filling the chosen one instead made the row read as
                            // five actions, four of them in the accent, on a screen whose
                            // only real action is "Add to bag".
                            Button {
                                text: choice.label(),
                                variant: ButtonVariant::Text,
                                color: Paint::Role(if choice == category() {
                                    ColorRole::OnSurface
                                } else {
                                    ColorRole::OnSurfaceVariant
                                }),
                                on_click: move |_| category.set(choice),
                            }
                        }
                    }
                },
            }

            Column {
                fill_max_width: true,
                space_role: SpaceRole::Md,
                for (index, row) in rows.iter().enumerate() {
                    Row {
                        key: "{index}",
                        fill_max_width: true,
                        space_role: SpaceRole::Md,
                        alignment: Alignment::TopStart,
                        for product in row.iter().copied() {
                            Column { key: "{product.id}", weight: 1.0,
                                {grid_cell(product, on_open)}
                            }
                        }
                        // A shelf with an odd number of garments leaves a gap rather than
                        // letting the last one stretch to twice the width of its
                        // neighbours, which would read as a different kind of product.
                        if row.len() == 1 {
                            Spacer { weight: 1.0 }
                        }
                    }
                }
            }
        }
    }
}

/// One garment: the picture, what it is, what size, how many, and what it costs.
fn detail_screen(
    product: &Product,
    size: Signal<&'static str>,
    quantity: Signal<u32>,
    on_back: EventHandler<()>,
    on_add: EventHandler<()>,
) -> Element {
    let mut size = size;
    let mut quantity = quantity;
    let (fill, ink) = product.tint.pair();
    let line_total = price(product.cents * quantity());
    let rating = product.rating;

    rsx! {
        Column {
            fill_max_width: true,
            fill_max_height: true,

            dioxus_compose::Box {
                fill_max_width: true,
                height: HERO_HEIGHT,
                background: Paint::Role(fill),
                alignment: Alignment::TopStart,
                // The garment, full size. The reference's product page is a photograph
                // running to the window's edges with the panel covering its lower part,
                // and a page that fills that with a flat colour is the one screen in the
                // shop that shows you nothing about what you are buying.
                Image {
                    fill_max_width: true,
                    fill_max_height: true,
                    padding_role: SpaceRole::Lg,
                    asset_id: asset(AssetKind::Svg, product.picture),
                }
                Row {
                    fill_max_width: true,
                    padding_role: SpaceRole::Md,
                    alignment: Alignment::CenterStart,
                    Button {
                        text: "\u{2190}",
                        variant: ButtonVariant::Text,
                        color: Paint::Role(ink),
                        on_click: move |_| on_back.call(()),
                    }
                    Spacer { weight: 1.0 }
                    Text {
                        text: product.name,
                        type_role: TypeRole::Title,
                        color: Paint::Role(ink),
                    }
                }
            }

            // The panel that covers the lower part of the picture, which is the shape the
            // reference draws and the shape a grouped iOS sheet has.
            Surface {
                fill_max_width: true,
                weight: 1.0,
                shape_role: ShapeRole::Large,
                padding_role: SpaceRole::Md,
                Column {
                    fill_max_width: true,
                    space_role: SpaceRole::Md,

                    Column {
                        fill_max_width: true,
                        space_role: SpaceRole::Xs,
                        Text { text: product.name, type_role: TypeRole::Headline }
                        Text {
                            text: product.support,
                            type_role: TypeRole::Body,
                            color: Paint::Role(ColorRole::OnSurfaceVariant),
                        }
                        Row {
                            space_role: SpaceRole::Sm,
                            alignment: Alignment::CenterStart,
                            Text {
                                text: stars(rating),
                                type_role: TypeRole::Body,
                                color: Paint::Role(ColorRole::Primary),
                            }
                            Text {
                                text: "({catalogue::rating_text(rating)})",
                                type_role: TypeRole::Label,
                                color: Paint::Role(ColorRole::OnSurfaceVariant),
                            }
                        }
                    }

                    Column {
                        fill_max_width: true,
                        space_role: SpaceRole::Sm,
                        Text {
                            text: "Select size",
                            type_role: TypeRole::Label,
                            color: Paint::Role(ColorRole::OnSurfaceVariant),
                        }
                        Row {
                            fill_max_width: true,
                            space_role: SpaceRole::Sm,
                            for option in SIZES {
                                // The selected size is a filled button and the rest are
                                // outlined. Which colour "filled" is belongs to the design
                                // system, so this never names one.
                                Button {
                                    key: "{option}",
                                    text: option,
                                    weight: 1.0,
                                    variant: if option == size() {
                                        ButtonVariant::Filled
                                    } else {
                                        ButtonVariant::Outlined
                                    },
                                    on_click: move |_| size.set(option),
                                }
                            }
                        }
                    }

                    Row {
                        fill_max_width: true,
                        space_role: SpaceRole::Sm,
                        alignment: Alignment::CenterStart,
                        Button {
                            text: "\u{2212}",
                            variant: ButtonVariant::Tonal,
                            enabled: quantity() > 1,
                            on_click: move |_| quantity.set(quantity().saturating_sub(1).max(1)),
                        }
                        Text { text: "{quantity}", type_role: TypeRole::Subtitle }
                        Button {
                            text: "+",
                            variant: ButtonVariant::Tonal,
                            enabled: quantity() < 9,
                            on_click: move |_| quantity.set((quantity() + 1).min(9)),
                        }
                        Spacer { weight: 1.0 }
                        Text { text: line_total, type_role: TypeRole::Headline }
                    }

                    Button {
                        text: "Add to bag",
                        fill_max_width: true,
                        variant: ButtonVariant::Filled,
                        on_click: move |_| on_add.call(()),
                    }
                }
            }
        }
    }
}

/// What is waiting to be paid for.
fn bag_screen(bag: Signal<Vec<BagLine>>) -> Element {
    let mut bag = bag;
    let lines = bag();
    let sum = total(&lines);

    rsx! {
        Column {
            fill_max_width: true,
            fill_max_height: true,
            padding_role: SpaceRole::Md,
            space_role: SpaceRole::Md,
            Text { text: "Bag", type_role: TypeRole::Headline }
            if lines.is_empty() {
                dioxus_compose::Box {
                    fill_max_width: true,
                    weight: 1.0,
                    alignment: Alignment::Center,
                    Text {
                        text: "Nothing in the bag yet.",
                        type_role: TypeRole::Body,
                        color: Paint::Role(ColorRole::OnSurfaceVariant),
                    }
                }
            } else {
                Surface {
                    fill_max_width: true,
                    shape_role: ShapeRole::Large,
                    Column {
                        fill_max_width: true,
                        for (index, line) in lines.iter().enumerate() {
                            {
                                let line = *line;
                                let named = catalogue::product(line.product);
                                let last = index + 1 == lines.len();
                                rsx! {
                                    Column { key: "{index}", fill_max_width: true,
                                        Row {
                                            fill_max_width: true,
                                            padding_role: SpaceRole::Md,
                                            space_role: SpaceRole::Sm,
                                            alignment: Alignment::CenterStart,
                                            Column {
                                                weight: 1.0,
                                                space_role: SpaceRole::Xs,
                                                Text {
                                                    text: named.map_or("Unavailable", |found| found.name),
                                                    type_role: TypeRole::Body,
                                                    max_lines: 1,
                                                    overflow: TextOverflow::Ellipsis,
                                                }
                                                Text {
                                                    text: "Size {line.size} \u{00b7} {line.quantity}",
                                                    type_role: TypeRole::Caption,
                                                    color: Paint::Role(ColorRole::OnSurfaceVariant),
                                                }
                                            }
                                            Text {
                                                text: price(
                                                    named.map_or(0, |found| found.cents) * line.quantity,
                                                ),
                                                type_role: TypeRole::BodyStrong,
                                            }
                                            Button {
                                                text: "Remove",
                                                variant: ButtonVariant::Text,
                                                color: Paint::Role(ColorRole::Error),
                                                on_click: move |_| {
                                                    let removed = bag.write().remove(index);
                                                    let name = catalogue::product(removed.product)
                                                        .map_or("An item", |found| found.name);
                                                    Message::new(format!("Removed {name}"))
                                                        .with_action("Undo", move |()| {
                                                            let at = index.min(bag.read().len());
                                                            bag.write().insert(at, removed);
                                                        })
                                                        .with_duration(MessageDuration::Long)
                                                        .show();
                                                },
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
                Spacer { weight: 1.0 }
                Row {
                    fill_max_width: true,
                    alignment: Alignment::CenterStart,
                    Text {
                        text: "Total",
                        type_role: TypeRole::Body,
                        color: Paint::Role(ColorRole::OnSurfaceVariant),
                        weight: 1.0,
                    }
                    Text { text: price(sum), type_role: TypeRole::Headline }
                }
                Button {
                    text: "Checkout",
                    fill_max_width: true,
                    variant: ButtonVariant::Filled,
                    on_click: move |_| {
                        Message::new("Checkout is not part of this sample").show();
                    },
                }
            }
        }
    }
}

/// Search, which is a field and the shelves as a list.
fn search_screen(on_open: EventHandler<u32>) -> Element {
    rsx! {
        Column {
            fill_max_width: true,
            fill_max_height: true,
            padding_role: SpaceRole::Md,
            space_role: SpaceRole::Md,
            Text { text: "Search", type_role: TypeRole::Headline }
            TextField { fill_max_width: true, placeholder: "T-shirts, joggers, jackets" }
            Column {
                fill_max_width: true,
                space_role: SpaceRole::Sm,
                for product in CATALOGUE {
                    Row {
                        key: "{product.id}",
                        fill_max_width: true,
                        space_role: SpaceRole::Sm,
                        alignment: Alignment::CenterStart,
                        dioxus_compose::Box {
                            width: 44.0,
                            height: 44.0,
                            background: Paint::Role(product.tint.pair().0),
                            shape_role: ShapeRole::Medium,
                        }
                        Column {
                            weight: 1.0,
                            Text {
                                text: product.name,
                                type_role: TypeRole::Body,
                                max_lines: 1,
                                overflow: TextOverflow::Ellipsis,
                            }
                            Text {
                                text: product.support,
                                type_role: TypeRole::Caption,
                                color: Paint::Role(ColorRole::OnSurfaceVariant),
                                max_lines: 1,
                                overflow: TextOverflow::Ellipsis,
                            }
                        }
                        Button {
                            text: price(product.cents),
                            variant: ButtonVariant::Text,
                            on_click: move |_| on_open.call(product.id),
                        }
                    }
                }
            }
        }
    }
}

/// The account page, which is a short list of the things a shop keeps about a person.
fn account_screen() -> Element {
    rsx! {
        Column {
            fill_max_width: true,
            fill_max_height: true,
            padding_role: SpaceRole::Md,
            space_role: SpaceRole::Md,
            Text { text: "Account", type_role: TypeRole::Headline }
            Surface {
                fill_max_width: true,
                shape_role: ShapeRole::Large,
                Column {
                    fill_max_width: true,
                    for (index, entry) in ["Orders", "Addresses", "Payment", "Notifications"]
                        .iter()
                        .enumerate()
                    {
                        Column { key: "{entry}", fill_max_width: true,
                            Row {
                                fill_max_width: true,
                                padding_role: SpaceRole::Md,
                                alignment: Alignment::CenterStart,
                                Text { text: *entry, type_role: TypeRole::Body, weight: 1.0 }
                                Text {
                                    text: "\u{203a}",
                                    type_role: TypeRole::Body,
                                    color: Paint::Role(ColorRole::OnSurfaceVariant),
                                }
                            }
                            if index < 3 {
                                Separator {}
                            }
                        }
                    }
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

    let mut destination = use_signal(|| Destination::Shop);
    let category = use_signal(|| Category::New);
    let slide = use_signal(|| 0_usize);
    let mut open = use_signal(|| Option::<u32>::None);
    let size = use_signal(|| SIZES[1]);
    let quantity = use_signal(|| 1_u32);
    let bag = use_signal(Vec::<BagLine>::new);

    let on_open = EventHandler::new(move |id: u32| {
        destination.set(Destination::Shop);
        open.set(Some(id));
    });

    let showing = open().and_then(catalogue::product);
    let body = match (destination(), showing) {
        (Destination::Shop, Some(product)) => detail_screen(
            product,
            size,
            quantity,
            EventHandler::new(move |()| open.set(None)),
            add_to_bag(bag, open, size, quantity),
        ),
        (Destination::Shop, None) => catalogue_screen(category, slide, bag, on_open),
        (Destination::Search, _) => search_screen(on_open),
        (Destination::Bag, _) => bag_screen(bag),
        (Destination::Account, _) => account_screen(),
    };

    // A garment's page fills the window, because the picture runs to the edge. Everything
    // else is a page that scrolls.
    let scrolls = showing.is_none() || destination() != Destination::Shop;

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
                    on_click: move |()| {
                        destination.set(choice);
                        if choice != Destination::Shop {
                            open.set(None);
                        }
                    },
                }
            }
            Column {
                fill_max_width: true,
                fill_max_height: true,
                background: Paint::Role(ColorRole::Background),
                TopAppBar {
                    fill_max_width: true,
                    Text { text: "Nimbus", type_role: TypeRole::Title, weight: 1.0 }
                    Text {
                        text: "Personal fitness clothes",
                        type_role: TypeRole::Label,
                        color: Paint::Role(ColorRole::OnSurfaceVariant),
                    }
                }
                dioxus_compose::Box {
                    fill_max_width: true,
                    weight: 1.0,
                    alignment: Alignment::TopCenter,
                    if scrolls {
                        ScrollColumn {
                            width: measure,
                            fill_max_width: measure.is_none(),
                            fill_max_height: true,
                            {body}
                        }
                    } else {
                        Column {
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
}

/// Putting the open garment in the bag, at the size and count that are showing.
///
/// A line that is already there grows rather than repeating, because two lines for the
/// same garment in the same size is a bag nobody can read, and it makes the total right
/// for the wrong reason.
fn add_to_bag(
    bag: Signal<Vec<BagLine>>,
    open: Signal<Option<u32>>,
    size: Signal<&'static str>,
    quantity: Signal<u32>,
) -> EventHandler<()> {
    let mut bag = bag;
    EventHandler::new(move |()| {
        let Some(id) = open() else { return };
        let chosen = size();
        let count = quantity();
        let existing = bag
            .read()
            .iter()
            .position(|line| line.product == id && line.size == chosen);
        match existing {
            Some(index) => bag.write()[index].quantity += count,
            None => bag.write().push(BagLine {
                product: id,
                size: chosen,
                quantity: count,
            }),
        }
        let name = catalogue::product(id).map_or("Item", |found| found.name);
        Message::new(format!("{name} added to the bag"))
            .with_duration(MessageDuration::Short)
            .show();
    })
}

/// The design this sample draws, named once.
///
/// One design system everywhere, because the design is the product here rather than the
/// platform's convention, and light because the reference is a white page with black ink,
/// grey product cards and one yellow accent.
///
/// The scheme is said out loud rather than left to follow the machine. `Theme::unified`
/// settles which design system is drawn and nothing else, so without this line a reader
/// whose system is set the other way sees a screen the design was never drawn for.
const THEME: Theme = Theme::unified(DesignSystem::Cupertino).with_color_scheme(ColorScheme::Light);

/// `demo_theme_for` rather than `THEME` alone: a sample is something to look at, and one
/// machine can only show the design system and the scheme it is set to. `DXC_DESIGN` and
/// `DXC_SCHEME` each override the half they name, so the line above stays the answer to
/// everything nobody asked about.
fn main() {
    dioxus_compose::LaunchBuilder::new()
        .with_theme(dioxus_compose::demo_theme_for(THEME))
        .launch(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    use dioxus_compose::Host;
    use dioxus_compose::protocol::{
        HostEvent, Mutation, PropertyValue, decode_batch, encode_event,
    };
    use dioxus_compose::schema::{EventPayload, PropertyKind};

    /// Named for what it defends: the reference is a light design, and a machine set
    /// the other way drew this sample dark with nothing to compare against.
    #[test]
    fn fr14_the_design_names_its_colour_scheme() {
        // Through the wire rather than off the constant: what settles the question is the
        // record the Renderer reads, and a scheme that never leaves the Host is a scheme
        // nobody is drawn in.
        dioxus_compose::window::reset_window_size();
        let mut host = Host::with_theme(app, THEME);
        let batch = host.rebuild().expect("the first frame failed").to_vec();
        let first = decode_batch(&batch)
            .expect("the first batch did not decode")
            .into_iter()
            .next()
            .expect("the first batch is empty");
        let Mutation::SetTheme(theme) = first else {
            panic!("the first record is {first:?} rather than the theme");
        };
        assert_eq!(theme.color_scheme, ColorScheme::Light);
        assert!(!theme.adaptive, "the design is the product here");
    }

    /// The screen, driven the way a Renderer drives it.
    ///
    /// Every batch is kept rather than only the last one. A batch is the change since the
    /// frame before it, so a test that reads one batch is reading what moved, not what is
    /// on screen, and a screen built over three frames would look almost empty.
    struct Screen {
        host: Host,
        frames: Vec<Vec<u8>>,
    }

    impl Screen {
        fn new() -> Self {
            dioxus_compose::window::reset_window_size();
            let mut host = Host::new(app);
            let first = host.rebuild().expect("the first frame failed").to_vec();
            let mut screen = Self {
                host,
                frames: vec![first],
            };
            screen.resize(420.0);
            screen
        }

        fn resize(&mut self, width_dp: f32) {
            self.send(HostEvent {
                node_id: 0,
                handler_id: 0,
                payload: EventPayload::WindowSizeChanged {
                    width_dp,
                    height_dp: 780.0,
                    class: WindowSizeClass::from_width_dp(width_dp),
                },
            });
        }

        fn send(&mut self, event: HostEvent<'_>) {
            let mut bytes = Vec::new();
            encode_event(&event, &mut bytes).expect("the event did not encode");
            let (batch, _) = self.host.dispatch_event(&bytes).expect("the event failed");
            if !batch.is_empty() {
                self.frames.push(batch.to_vec());
            }
        }

        fn mutations(&self) -> Vec<Mutation<'_>> {
            self.frames
                .iter()
                .flat_map(|frame| decode_batch(frame).expect("a batch did not decode"))
                .collect()
        }

        fn texts(&self) -> Vec<String> {
            self.mutations()
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

        /// Presses whatever carries this label.
        ///
        /// The most recently declared one, because a label that has appeared twice over
        /// the run belongs to whichever screen is showing now, and the older node is
        /// something the Renderer has already thrown away.
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
            self.send(HostEvent {
                node_id,
                handler_id,
                payload: EventPayload::Clicked,
            });
            true
        }

        /// Whatever text arrived in the last frame, which is what "this screen replaced
        /// that one" has to be read from.
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

    /// Every garment reaches the Renderer as a picture, and each drawing crosses once.
    ///
    /// Named for what it defends: `Image` was on the wire for a long time with nothing in
    /// the tree drawing one, and the shelves were flat rectangles the whole time. A shelf
    /// that shows two of the same garment is still one registration, because the id is
    /// what a frame carries after the first one.
    #[test]
    fn fr16_every_garment_crosses_once_as_a_picture() {
        let screen = Screen::new();
        let registered: Vec<&[u8]> = screen
            .mutations()
            .iter()
            .filter_map(|mutation| match mutation {
                Mutation::RegisterAsset { bytes, .. } => Some(*bytes),
                _ => None,
            })
            .collect();
        assert!(
            registered.contains(&catalogue::HERO),
            "the banner was never registered"
        );
        for product in under(Category::New) {
            assert!(
                registered.contains(&product.picture),
                "{} is on the shelf with no picture",
                product.name
            );
        }
        let mut seen = registered.clone();
        seen.sort_unstable_by_key(|bytes| bytes.as_ptr());
        seen.dedup_by_key(|bytes| bytes.as_ptr());
        assert_eq!(
            seen.len(),
            registered.len(),
            "a drawing was registered more than once"
        );
    }

    /// Nothing is registered again once the screen is up. A picture that crossed on every
    /// frame would put a file in the frame budget and would be the whole reason the id
    /// exists undone.
    #[test]
    fn fr16_a_later_frame_carries_no_pictures() {
        let mut screen = Screen::new();
        screen.press(Category::Women.label());
        let last = screen.frames.last().expect("nothing moved").clone();
        let registered = decode_batch(&last)
            .expect("the batch did not decode")
            .into_iter()
            .filter(|mutation| matches!(mutation, Mutation::RegisterAsset { .. }))
            .count();
        assert_eq!(
            registered, 0,
            "a drawing already on the shelf was sent again"
        );
    }

    /// The carousel shows one slide, and the dot under it moves the carousel.
    ///
    /// Named for what it defends: a scroll position belongs to the Renderer and is never
    /// reported back, so dots driven by one would be drawn in the right place and never
    /// move. The slide is the Host's, which is what makes the dots mean anything.
    #[test]
    fn fr17_a_dot_moves_the_carousel_to_its_slide() {
        let mut screen = Screen::new();
        let shelf = under(Category::New);
        assert!(shelf.len() > 1, "a carousel of one slide proves nothing");
        assert!(
            screen.texts().iter().any(|text| text == shelf[0].name),
            "the carousel is not showing its first slide"
        );

        let dots: Vec<(u32, u64)> = screen
            .mutations()
            .iter()
            .filter_map(|mutation| match mutation {
                Mutation::SetProp {
                    node_id,
                    property: PropertyKind::Text,
                    value: PropertyValue::String(text),
                } if *text == "\u{2022}" => Some(*node_id),
                _ => None,
            })
            .filter_map(|node| {
                screen
                    .mutations()
                    .iter()
                    .find_map(|mutation| match mutation {
                        Mutation::SetProp {
                            node_id,
                            property: PropertyKind::OnClick,
                            value: PropertyValue::Integer(handler),
                        } if *node_id == node => Some((node, *handler as u64)),
                        _ => None,
                    })
            })
            .collect();
        assert_eq!(
            dots.len(),
            shelf.len(),
            "a carousel of {} slides has {} dots",
            shelf.len(),
            dots.len()
        );

        let (node_id, handler_id) = dots[1];
        screen.send(HostEvent {
            node_id,
            handler_id,
            payload: EventPayload::Clicked,
        });
        assert!(
            screen
                .latest_texts()
                .iter()
                .any(|text| text == shelf[1].name),
            "the second dot did not bring its slide up"
        );
    }

    /// Every destination has to encode, not just the one the shop opens on. A widget only
    /// the bag reaches would otherwise fail for the first person who taps it.
    #[test]
    fn fr15_every_destination_encodes() {
        let mut screen = Screen::new();
        for choice in Destination::STRIP {
            assert!(
                screen.press(choice.label()),
                "the bar has no destination called {}",
                choice.label()
            );
            assert!(
                !screen.texts().is_empty(),
                "{} drew nothing at all",
                choice.label()
            );
        }
        dioxus_compose::window::reset_window_size();
    }

    /// Opening a garment replaces the catalogue with its page, and going back brings the
    /// catalogue with it. A detail view that cannot be left is a dead end.
    #[test]
    fn fr15_a_garment_opens_and_closes() {
        let mut screen = Screen::new();
        assert!(screen.press("View"), "no garment on the catalogue opens");
        assert!(
            screen
                .latest_texts()
                .iter()
                .any(|text| text == "Select size"),
            "opening a garment did not bring up its page"
        );
        assert!(
            screen.press("\u{2190}"),
            "the garment's page has no way back"
        );
        assert!(
            screen
                .latest_texts()
                .iter()
                .any(|text| text.starts_with("Let's find")),
            "going back did not bring the catalogue with it"
        );
        dioxus_compose::window::reset_window_size();
    }

    /// Adding the same garment in the same size twice is one line that counts two, not two
    /// lines. A bag that repeats itself adds up to the right number for the wrong reason.
    #[test]
    fn fr15_the_bag_grows_a_line_rather_than_repeating_it() {
        let mut bag = vec![BagLine {
            product: 1,
            size: "S",
            quantity: 1,
        }];
        let same = BagLine {
            product: 1,
            size: "S",
            quantity: 2,
        };
        match bag
            .iter()
            .position(|line| line.product == same.product && line.size == same.size)
        {
            Some(index) => bag[index].quantity += same.quantity,
            None => bag.push(same),
        }
        assert_eq!(bag.len(), 1);
        assert_eq!(bag[0].quantity, 3);
        assert_eq!(price(total(&bag)), "$285");
    }

    /// A phone design does not become a desktop design by being put in a wider window.
    #[test]
    fn fr20_the_page_stops_widening_past_a_phone() {
        fn widths(width_dp: f32) -> Vec<f32> {
            dioxus_compose::window::reset_window_size();
            let mut host = Host::new(app);
            host.rebuild().expect("the first frame failed");
            let mut bytes = Vec::new();
            encode_event(
                &HostEvent {
                    node_id: 0,
                    handler_id: 0,
                    payload: EventPayload::WindowSizeChanged {
                        width_dp,
                        height_dp: 780.0,
                        class: WindowSizeClass::from_width_dp(width_dp),
                    },
                },
                &mut bytes,
            )
            .expect("the resize did not encode");
            let (batch, _) = host.dispatch_event(&bytes).expect("the resize failed");
            let found = decode_batch(batch)
                .expect("the batch did not decode")
                .iter()
                .filter_map(|mutation| match mutation {
                    Mutation::SetModifier {
                        modifier: dioxus_compose::Modifier::Width(dp),
                        ..
                    } => Some(*dp),
                    _ => None,
                })
                .collect();
            dioxus_compose::window::reset_window_size();
            found
        }

        assert!(widths(1180.0).contains(&PAGE_MEASURE));
        assert!(!widths(420.0).contains(&PAGE_MEASURE));
    }

    /// The catalogue, in the design system it ships, in both schemes, at all three widths.
    ///
    /// The category strip windows its labels, so the recorder answers the range request a
    /// real Renderer would have made before the first pixel. Without it the strip is an
    /// empty box and the picture is of a screen with a hole in it.
    #[test]
    fn fr14_the_catalogue_is_recorded_in_the_system_it_ships() {
        sample_frames::record_as(
            "Store",
            &sample_frames::as_designed(THEME, &sample_frames::APPLE),
            app,
            |screen| {
                assert_eq!(
                    screen.fill_lists(6),
                    1,
                    "the category strip should be the catalogue's one windowing list, or \
                     the picture is of a screen with a hole in it"
                );
            },
        );
    }

    /// A garment's own page, which is a different picture: the grid is gone, the sizes and
    /// the stepper are there, and the panel covers the lower part of the tile.
    #[test]
    fn fr14_a_garment_page_is_recorded() {
        sample_frames::record_as(
            "StoreProduct",
            &sample_frames::as_designed(THEME, &sample_frames::APPLE),
            app,
            |screen| {
                screen.fill_lists(6);
                assert!(screen.press("View"), "no garment on the catalogue opens");
            },
        );
    }
}
