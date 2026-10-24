//! The gallery example in an 800 by 600 viewport, with images answered by a resolver that
//! knows a fixed list of URLs. Nothing is fetched.
//!
//! Every number follows from `gallery::STYLE` and the natural sizes below:
//!
//! - The header is 160px tall and 800px wide. The avatar has no size of its own, so it is
//!   laid out at its natural 48 by 48, at the 32px padding and centred: y = (160 - 48) / 2.
//! - The grid's content box is 800 - 2 x 16 = 768px wide from x = 16, y = 160 + 16 = 176.
//!   `repeat(auto-fill, minmax(160px, 1fr))` with 12px gaps fits four columns
//!   (4 x 160 + 3 x 12 = 676; five would need 848), which share the 768 - 36 = 732px
//!   equally: 183px each, at x = 16, 211, 406 and 601. Rows are 140px, 12px apart, at
//!   y = 176 and 328.
//! - The first tile spans two columns, 2 x 183 + 12 = 378px. The other six tiles fill the
//!   rest of the first row and the second row in order.

use dioxus_compose::html::{
    AssetId, BackgroundImage, Brush, HtmlDom, ImageSize, LiteralColours, ModifierSlot, ObjectFit,
    PlanKey, PlanKind, PlanModifier, Rgba,
};
use sample_html_gallery as gallery;
use sample_html_support::{
    Images, Measurer, assert_close, assert_rect, centre, config_with_images, entry, node,
    plan_node, text_of,
};

const POSTCARD: &str = "textures/postcard.png";

fn images() -> Images {
    Images::new(vec![
        ("avatars/me.png", ImageSize::new(48, 48), AssetId(1)),
        ("photos/tram.jpg", ImageSize::new(600, 400), AssetId(10)),
        ("photos/tiles.jpg", ImageSize::new(300, 300), AssetId(11)),
        ("photos/river.jpg", ImageSize::new(400, 300), AssetId(12)),
        ("photos/castle.jpg", ImageSize::new(300, 400), AssetId(13)),
        ("photos/bakery.jpg", ImageSize::new(300, 300), AssetId(14)),
        ("maps/route.png", ImageSize::new(400, 400), AssetId(15)),
        (POSTCARD, ImageSize::new(200, 100), AssetId(20)),
    ])
}

fn laid_out() -> HtmlDom {
    let mut dom = HtmlDom::with_config(
        gallery::app,
        config_with_images(gallery::STYLE, Measurer::new(), images()),
    );
    dom.layout(800.0, 600.0, 1.0);
    dom
}

#[test]
fn fr34_gallery_grid_cells_follow_auto_fill_columns() {
    let dom = laid_out();

    assert_rect(
        entry(&dom, "grid").rect,
        0.0,
        160.0,
        800.0,
        324.0,
        "the grid",
    );
    let expected = [
        ("tile-0", 16.0, 176.0, 378.0),
        ("tile-1", 406.0, 176.0, 183.0),
        ("tile-2", 601.0, 176.0, 183.0),
        ("tile-3", 16.0, 328.0, 183.0),
        ("tile-4", 211.0, 328.0, 183.0),
        ("tile-5", 406.0, 328.0, 183.0),
        ("postcard", 601.0, 328.0, 183.0),
    ];
    for (id, x, y, width) in expected {
        assert_rect(entry(&dom, id).rect, x, y, width, 140.0, id);
    }
}

/// The spanning tile is exactly two columns and the gap between them, and the image in it
/// fills it: `width: 100%; height: 100%` of the tile.
#[test]
fn fr34_gallery_spanning_item_covers_two_columns() {
    let dom = laid_out();
    let wide = entry(&dom, "tile-0").rect;
    let next = entry(&dom, "tile-1").rect;
    assert_eq!(wide.width, 2.0 * 183.0 + 12.0);
    assert_eq!(
        next.x - wide.right(),
        12.0,
        "one gap after the spanning tile"
    );
    assert_rect(
        entry(&dom, "photo-0").rect,
        16.0,
        176.0,
        378.0,
        140.0,
        "the wide photo",
    );
}

/// An `<img>` without a size of its own is laid out at the size the resolver answers.
/// Inside a tile, `object-fit` decides where the image goes:
///
/// - `cover`: 600 by 400 into 378 by 140 scales by max(378 / 600, 140 / 400) = 0.63 to
///   378 by 252, centred 56px above the tile's top.
/// - `contain`: 400 by 400 into 183 by 140 scales by 140 / 400 = 0.35 to 140 by 140,
///   centred (183 - 140) / 2 = 21.5px from the tile's left.
#[test]
fn fr34_gallery_images_are_laid_out_at_resolver_sizes() {
    let dom = laid_out();

    let avatar = entry(&dom, "avatar");
    assert_rect(avatar.rect, 32.0, 56.0, 48.0, 48.0, "the avatar");
    let image = avatar.image.as_ref().expect("the avatar is an image");
    assert_eq!(image.source, "avatars/me.png");
    assert_eq!(image.natural_size, Some((48.0, 48.0)));
    assert_rect(
        image.image_rect,
        32.0,
        56.0,
        48.0,
        48.0,
        "the avatar's image",
    );

    let tram = entry(&dom, "photo-0").image.as_ref().expect("an image");
    assert_eq!(tram.fit, ObjectFit::Cover);
    assert_eq!(tram.natural_size, Some((600.0, 400.0)));
    assert_rect(
        tram.content_rect,
        16.0,
        176.0,
        378.0,
        140.0,
        "the tram's box",
    );
    assert_rect(
        tram.image_rect,
        16.0,
        120.0,
        378.0,
        252.0,
        "the tram, covering",
    );

    let route = entry(&dom, "photo-5").image.as_ref().expect("an image");
    assert_eq!(route.fit, ObjectFit::Contain);
    assert_rect(
        route.image_rect,
        427.5,
        328.0,
        140.0,
        140.0,
        "the route, contained",
    );
}

/// The header's gradient runs left to right across its 800 by 160 box: from (0, 80) to
/// (800, 80), #4f46e5 to #06b6d4. In the plan it is a brush the header's node paints.
#[test]
fn fr34_gallery_gradient_header_is_recorded() {
    let mut dom = laid_out();
    let indigo = Rgba::new(79, 70, 229, 255);
    let cyan = Rgba::new(6, 182, 212, 255);

    let hero = entry(&dom, "hero");
    assert_eq!(hero.backgrounds.len(), 1, "{:#?}", hero.backgrounds);
    let layer = &hero.backgrounds[0];
    let BackgroundImage::Linear(gradient) = &layer.image else {
        panic!("a linear gradient: {layer:#?}");
    };
    assert_close(gradient.angle, 90.0, "`to right`");
    assert_close(gradient.start.0, 0.0, "start x");
    assert_close(gradient.start.1, 80.0, "start y");
    assert_close(gradient.end.0, 800.0, "end x");
    assert_close(gradient.end.1, 80.0, "end y");
    assert_eq!(gradient.stops.len(), 2);
    assert_eq!(
        (gradient.stops[0].offset, gradient.stops[0].color),
        (0.0, indigo)
    );
    assert_eq!(
        (gradient.stops[1].offset, gradient.stops[1].color),
        (1.0, cyan)
    );
    assert_rect(
        layer.area,
        0.0,
        0.0,
        800.0,
        160.0,
        "the header's painting area",
    );

    let plan = dom.plan(&mut LiteralColours).expect("laid out above");
    let header = plan_node(&plan, PlanKey::Node(node(&dom, "hero")));
    let Some(PlanModifier::BackgroundBrush { brush, area, .. }) =
        header.modifier(ModifierSlot::BackgroundBrush(0))
    else {
        panic!("the header paints a brush: {header:#?}");
    };
    assert_rect(
        *area,
        0.0,
        0.0,
        800.0,
        160.0,
        "the brush's area, from the header's corner",
    );
    let Some(Brush::LinearGradient {
        start,
        end,
        stops,
        repeating,
    }) = plan.brushes.get(brush)
    else {
        panic!("the plan lists the header's gradient: {:#?}", plan.brushes);
    };
    assert_close(start.0, 0.0, "brush start x");
    assert_close(start.1, 80.0, "brush start y");
    assert_close(end.0, 800.0, "brush end x");
    assert_close(end.1, 80.0, "brush end y");
    assert!(!repeating);
    assert_eq!(stops.len(), 2);
}

/// Every photo is an image node drawing the asset the resolver named for its URL, and the
/// postcard's CSS `url()` background is an image brush. The postcard, 200 by 100, covers
/// its 183 by 140 tile at max(183 / 200, 140 / 100) = 1.4: 280 by 140, centred
/// (183 - 280) / 2 = -48.5px from the tile's left.
#[test]
fn fr34_gallery_assets_are_in_the_plan() {
    let mut dom = laid_out();

    let postcard = entry(&dom, "postcard");
    assert_eq!(postcard.backgrounds.len(), 1, "{:#?}", postcard.backgrounds);
    let layer = &postcard.backgrounds[0];
    assert_eq!(layer.image, BackgroundImage::Url(POSTCARD.to_string()));
    assert_rect(
        layer.tile,
        552.5,
        328.0,
        280.0,
        140.0,
        "the postcard's tile",
    );

    let plan = dom.plan(&mut LiteralColours).expect("laid out above");
    let expected = [
        ("avatar", 1, "avatars/me.png"),
        ("photo-0", 10, "photos/tram.jpg"),
        ("photo-1", 11, "photos/tiles.jpg"),
        ("photo-2", 12, "photos/river.jpg"),
        ("photo-3", 13, "photos/castle.jpg"),
        ("photo-4", 14, "photos/bakery.jpg"),
        ("photo-5", 15, "maps/route.png"),
    ];
    for (id, asset, source) in expected {
        let image = plan_node(&plan, PlanKey::Image(node(&dom, id)));
        let PlanKind::Image(drawn) = &image.kind else {
            panic!("#{id} draws an image: {image:#?}");
        };
        assert_eq!(drawn.asset, AssetId(asset), "#{id}");
        assert_eq!(drawn.source, source, "#{id}");
    }

    // Covering spills outside the tile and is clipped; containing does not.
    let tram = plan_node(&plan, PlanKey::Image(node(&dom, "photo-0")));
    let PlanKind::Image(drawn) = &tram.kind else {
        unreachable!("checked above")
    };
    assert_rect(drawn.draw, 0.0, -56.0, 378.0, 252.0, "the tram in its node");
    assert_eq!(tram.modifier(ModifierSlot::Clip), Some(&PlanModifier::Clip));
    let route = plan_node(&plan, PlanKey::Image(node(&dom, "photo-5")));
    let PlanKind::Image(drawn) = &route.kind else {
        unreachable!("checked above")
    };
    assert_rect(drawn.draw, 21.5, 0.0, 140.0, 140.0, "the route in its node");
    assert_eq!(route.modifier(ModifierSlot::Clip), None);

    let card = plan_node(&plan, PlanKey::Node(node(&dom, "postcard")));
    let Some(PlanModifier::BackgroundBrush { brush, tile, .. }) =
        card.modifier(ModifierSlot::BackgroundBrush(0))
    else {
        panic!("the postcard paints its image: {card:#?}");
    };
    assert_eq!(plan.brushes.get(brush), Some(&Brush::Image(AssetId(20))));
    assert_rect(
        *tile,
        -48.5,
        0.0,
        280.0,
        140.0,
        "the postcard's tile in its node",
    );
}

/// Clicking a photo selects it: the header shows its caption.
#[test]
fn fr34_gallery_clicking_a_tile_selects_it() {
    let mut dom = laid_out();
    assert_eq!(text_of(entry(&dom, "subtitle")), "6 photos and a postcard");

    let (x, y) = centre(entry(&dom, "tile-1").rect);
    assert_eq!(dom.click(x, y), Some(node(&dom, "tile-1")));
    dom.render();
    dom.layout(800.0, 600.0, 1.0);
    assert_eq!(text_of(entry(&dom, "subtitle")), "Azulejos");
}
