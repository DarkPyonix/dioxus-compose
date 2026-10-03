//! Gallery: a photo grid in CSS grid, under a header painted with a gradient.
//!
//! Images are named by URL and never fetched here: what draws them is up to the
//! application's image resolver.

use dioxus_compose::html::prelude::*;
use dioxus_hooks::use_signal;
use dioxus_signals::WritableExt;

pub const STYLE: &str = r#"
*, *::before, *::after {
    box-sizing: border-box;
}

body {
    margin: 0;
    font-family: system-ui, sans-serif;
    font-size: 16px;
    line-height: 24px;
    color: #1f2328;
    background: #ffffff;
}

.hero {
    display: flex;
    align-items: center;
    gap: 16px;
    height: 160px;
    padding: 0 32px;
    background-image: linear-gradient(to right, #4f46e5, #06b6d4);
    color: #ffffff;
}

.avatar {
    border-radius: 50%;
}

.hero h1 {
    margin: 0;
    font-size: 28px;
    line-height: 36px;
}

.hero p {
    margin: 0;
}

.grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    grid-auto-rows: 140px;
    gap: 12px;
    padding: 16px;
}

.tile {
    margin: 0;
    border-radius: 8px;
    overflow: hidden;
    background: #eaeef2;
    cursor: pointer;
}

.tile.wide {
    grid-column: span 2;
}

.tile img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
}

.tile img.whole {
    object-fit: contain;
}

.postcard {
    background-image: url(textures/postcard.png);
    background-size: cover;
    background-position: center;
}
"#;

/// One photo in the grid.
#[derive(Clone, Copy, PartialEq)]
pub struct Photo {
    pub src: &'static str,
    pub caption: &'static str,
    /// Takes two columns.
    pub wide: bool,
    /// Shown whole (`object-fit: contain`) instead of filling its tile.
    pub whole: bool,
}

pub const PHOTOS: [Photo; 6] = [
    Photo {
        src: "photos/tram.jpg",
        caption: "Tram 28 on the hill",
        wide: true,
        whole: false,
    },
    Photo {
        src: "photos/tiles.jpg",
        caption: "Azulejos",
        wide: false,
        whole: false,
    },
    Photo {
        src: "photos/river.jpg",
        caption: "The Tagus at dusk",
        wide: false,
        whole: false,
    },
    Photo {
        src: "photos/castle.jpg",
        caption: "Castle walls",
        wide: false,
        whole: false,
    },
    Photo {
        src: "photos/bakery.jpg",
        caption: "Pasteis de nata",
        wide: false,
        whole: false,
    },
    Photo {
        src: "maps/route.png",
        caption: "Where we walked",
        wide: false,
        whole: true,
    },
];

pub fn app() -> Element {
    let mut selected = use_signal(|| None::<usize>);

    let subtitle = match selected() {
        Some(index) => PHOTOS[index].caption.to_string(),
        None => format!("{} photos and a postcard", PHOTOS.len()),
    };

    rsx! {
        header { id: "hero", class: "hero",
            img { id: "avatar", class: "avatar", src: "avatars/me.png", alt: "Me" }
            div {
                h1 { "Summer in Lisbon" }
                p { id: "subtitle", "{subtitle}" }
            }
        }
        section { id: "grid", class: "grid",
            for (index, photo) in PHOTOS.into_iter().enumerate() {
                figure {
                    key: "{photo.src}",
                    id: "tile-{index}",
                    class: if photo.wide { "tile wide" } else { "tile" },
                    onclick: move |_| selected.set(Some(index)),
                    img {
                        id: "photo-{index}",
                        class: if photo.whole { "whole" } else { "" },
                        src: photo.src,
                        alt: photo.caption,
                    }
                }
            }
            div { id: "postcard", class: "tile postcard" }
        }
    }
}
