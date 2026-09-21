//! What the shop sells, and what is in the bag.
//!
//! No prices in floating point. A price is a whole number of cents and is formatted once,
//! which is the only way a catalogue adds up to the same total twice.

use dioxus_compose::prelude::*;

/// The strip across the top of the catalogue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Category {
    New,
    Women,
    Men,
    Kids,
    Sale,
}

impl Category {
    pub const STRIP: [Category; 5] = [
        Category::New,
        Category::Women,
        Category::Men,
        Category::Kids,
        Category::Sale,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Category::New => "New",
            Category::Women => "Women",
            Category::Men => "Men",
            Category::Kids => "Kids",
            Category::Sale => "Sale",
        }
    }
}

/// The garments, drawn. Vector rather than raster, because a flat garment is paths and a
/// path file is a few hundred bytes that stays sharp at any tile size.
///
/// These are original drawings made for this sample. The reference is photography, which
/// is the one thing that cannot be written into a repository: a photograph of a real
/// garment belongs to whoever took it. So the shop draws its stock instead, and what the
/// sample demonstrates, that a picture crosses the boundary once and is drawn by id
/// afterwards, is the same either way.
///
/// A picture carries its own colours. That is what makes it a picture rather than a fill,
/// and it is the one exception to the rule the rest of this sample keeps: the card behind
/// the garment, the name under it and the price beside it are all roles, so everything
/// except the artwork still follows the reader into dark.
static TEE: &[u8] = include_bytes!("../assets/tee.svg");
static TANK: &[u8] = include_bytes!("../assets/tank.svg");
static JACKET: &[u8] = include_bytes!("../assets/jacket.svg");
static SHORTS: &[u8] = include_bytes!("../assets/shorts.svg");
static JOGGER: &[u8] = include_bytes!("../assets/jogger.svg");
static HOODIE: &[u8] = include_bytes!("../assets/hoodie.svg");
static KIT: &[u8] = include_bytes!("../assets/kit.svg");

/// The banner at the top of the catalogue, which is the one picture that is a scene rather
/// than a garment.
pub static HERO: &[u8] = include_bytes!("../assets/hero.svg");

/// The colour the card behind a garment is filled with.
///
/// The picture is the picture; this is the card it sits on, and the ink the card promises
/// to carry for the name and the price written over it. Three accent containers, which is
/// what those roles exist for: quiet fills that read as relatives of one another.
///
/// A literal colour was the other option and is worse. A literal is a colour the design
/// system never sees, so the shop would keep its pastel cards when the reader asked for
/// dark and the text on them would stop being readable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Tint {
    First,
    Second,
    Third,
}

impl Tint {
    /// The fill, and the ink that fill promises to carry.
    pub fn pair(self) -> (ColorRole, ColorRole) {
        match self {
            Tint::First => (ColorRole::PrimaryContainer, ColorRole::OnPrimaryContainer),
            Tint::Second => (
                ColorRole::SecondaryContainer,
                ColorRole::OnSecondaryContainer,
            ),
            Tint::Third => (ColorRole::TertiaryContainer, ColorRole::OnTertiaryContainer),
        }
    }
}

/// One thing for sale.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Product {
    pub id: u32,
    pub name: &'static str,
    pub support: &'static str,
    pub cents: u32,
    pub tint: Tint,
    pub category: Category,
    /// Tenths of a star, so the rating adds up and rounds the same way every time.
    pub rating: u32,
    /// The drawing of the garment, as the bytes of an SVG. Registered once with `asset`
    /// and drawn by id after that, so a shelf of eight garments costs eight registrations
    /// on the first frame and nothing on any frame after it.
    pub picture: &'static [u8],
}

/// The sizes every garment here comes in.
pub const SIZES: [&str; 5] = ["XS", "S", "M", "L", "XL"];

pub const CATALOGUE: [Product; 8] = [
    Product {
        id: 1,
        name: "Swoosh T-Shirt",
        support: "Women's light support",
        cents: 9500,
        tint: Tint::First,
        category: Category::New,
        picture: TEE,
        rating: 50,
    },
    Product {
        id: 2,
        name: "Pro Dri-Fit",
        support: "Men's tank top",
        cents: 7000,
        tint: Tint::Second,
        category: Category::New,
        picture: TANK,
        rating: 44,
    },
    Product {
        id: 3,
        name: "Windrunner",
        support: "Women's running jacket",
        cents: 12000,
        tint: Tint::Third,
        category: Category::Women,
        picture: JACKET,
        rating: 47,
    },
    Product {
        id: 4,
        name: "Tempo Short",
        support: "Women's 3 inch brief",
        cents: 4500,
        tint: Tint::First,
        category: Category::Women,
        picture: SHORTS,
        rating: 41,
    },
    Product {
        id: 5,
        name: "Flex Jogger",
        support: "Men's tapered fit",
        cents: 8500,
        tint: Tint::Second,
        category: Category::Men,
        picture: JOGGER,
        rating: 46,
    },
    Product {
        id: 6,
        name: "Court Vision",
        support: "Men's training tee",
        cents: 5500,
        tint: Tint::Third,
        category: Category::Men,
        picture: TEE,
        rating: 39,
    },
    Product {
        id: 7,
        name: "Little Runner",
        support: "Kids' all-weather set",
        cents: 6000,
        tint: Tint::First,
        category: Category::Kids,
        picture: KIT,
        rating: 48,
    },
    Product {
        id: 8,
        name: "Legacy Hoodie",
        support: "Last season, half price",
        cents: 4000,
        tint: Tint::Third,
        category: Category::Sale,
        picture: HOODIE,
        rating: 43,
    },
];

pub fn product(id: u32) -> Option<&'static Product> {
    CATALOGUE.iter().find(|found| found.id == id)
}

/// What the catalogue shows under one tab.
pub fn under(category: Category) -> Vec<&'static Product> {
    CATALOGUE
        .iter()
        .filter(|product| product.category == category)
        .collect()
}

/// A price, from cents. Whole amounts drop the cents, which is how a shelf label is
/// written and how the reference writes it.
pub fn price(cents: u32) -> String {
    if cents % 100 == 0 {
        format!("${}", thousands(cents / 100))
    } else {
        format!("${}.{:02}", thousands(cents / 100), cents % 100)
    }
}

/// Groups of three from the right. A bag of six jackets is a four figure total, and a four
/// figure total without a separator is a number people misread.
fn thousands(value: u32) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (position, digit) in digits.chars().enumerate() {
        if position > 0 && (digits.len() - position) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// Five glyphs, filled up to the rating and hollow after it.
///
/// A row of five icons is what the reference draws, and an icon cannot be placed from
/// application code: the `Icon` widget takes an id the Host registered, and `IconRole` only
/// reaches the tree through a navigation destination. Characters are what is left, and they
/// are sized by the type ladder like any other text, so they at least follow the design
/// system's scale.
pub fn stars(rating: u32) -> String {
    let filled = (rating + 5) / 10;
    let mut out = String::with_capacity(5 * 3);
    for step in 0..5 {
        out.push(if step < filled {
            '\u{2605}'
        } else {
            '\u{2606}'
        });
    }
    out
}

/// The rating as it is written beside the stars: one decimal, from tenths.
///
/// Tenths rather than a float, so "4.7" is a number the catalogue stores and not the
/// nearest binary fraction to it printed back.
pub fn rating_text(rating: u32) -> String {
    format!("{}.{}", rating / 10, rating % 10)
}

/// One line of the bag.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BagLine {
    pub product: u32,
    pub size: &'static str,
    pub quantity: u32,
}

/// What the bag comes to.
pub fn total(lines: &[BagLine]) -> u32 {
    lines
        .iter()
        .filter_map(|line| product(line.product).map(|found| found.cents * line.quantity))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr13_every_product_uses_a_role_rather_than_a_colour() {
        for product in CATALOGUE {
            let (fill, ink) = product.tint.pair();
            assert_ne!(fill, ink, "{} fills and inks with one role", product.name);
        }
    }

    #[test]
    fn a_price_reads_the_way_a_shelf_label_does() {
        assert_eq!(price(9500), "$95");
        assert_eq!(price(9550), "$95.50");
        assert_eq!(price(119250), "$1,192.50");
        assert_eq!(price(0), "$0");
    }

    #[test]
    fn a_bag_totals_its_lines_and_their_quantities() {
        let lines = [
            BagLine {
                product: 1,
                size: "S",
                quantity: 2,
            },
            BagLine {
                product: 2,
                size: "M",
                quantity: 1,
            },
        ];
        assert_eq!(total(&lines), 9500 * 2 + 7000);
        assert_eq!(price(total(&lines)), "$260");
    }

    /// A line naming something the shop does not sell contributes nothing rather than
    /// panicking. A bag restored from an old save is exactly that case.
    #[test]
    fn a_line_for_a_product_that_is_gone_is_worth_nothing() {
        let lines = [BagLine {
            product: 999,
            size: "S",
            quantity: 3,
        }];
        assert_eq!(total(&lines), 0);
    }

    #[test]
    fn fr13_the_rating_is_written_from_tenths_rather_than_a_float() {
        assert_eq!(rating_text(50), "5.0");
        assert_eq!(rating_text(47), "4.7");
        assert_eq!(rating_text(0), "0.0");
    }

    #[test]
    fn fr13_the_rating_rounds_to_whole_stars() {
        assert_eq!(stars(50), "\u{2605}\u{2605}\u{2605}\u{2605}\u{2605}");
        assert_eq!(stars(44), "\u{2605}\u{2605}\u{2605}\u{2605}\u{2606}");
        assert_eq!(stars(0), "\u{2606}\u{2606}\u{2606}\u{2606}\u{2606}");
    }

    /// Every tab has something on its shelf. A tab that shows nothing reads as a shop
    /// that has run out rather than as a filter with no matches.
    #[test]
    fn every_category_has_something_on_its_shelf() {
        for category in Category::STRIP {
            assert!(
                !under(category).is_empty(),
                "{} is an empty shelf",
                category.label()
            );
        }
    }
}
