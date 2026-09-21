//! Pictures: registering the bytes once and drawing them by id.
//!
//! An asset is not part of the tree. It is bytes that have to reach the Renderer before
//! anything can draw them and have to outlive the frame that does, which is the opposite
//! of a node: a node is re-sent whenever it changes and points at nothing after the call
//! that carried it.
//!
//! So an asset is registered and forgotten, the way a message is posted and forgotten. It
//! rides out on the batch the current call produces, and everything after that is the id.

use crate::schema::AssetKind;
use std::cell::RefCell;

/// One registration, waiting for the batch that carries it.
pub(crate) struct PendingAsset {
    pub(crate) asset_id: u32,
    pub(crate) kind: AssetKind,
    pub(crate) bytes: &'static [u8],
}

thread_local! {
    /// What has been registered, so the same picture asked for twice is one registration.
    ///
    /// Keyed on where the bytes are and how many there are rather than on their contents.
    /// A component body that draws a picture calls this on every render, which is the
    /// natural way to write it, and comparing contents would put a scan of the whole file
    /// in the frame. Two `&'static [u8]` at the same address of the same length are the
    /// same bytes.
    static REGISTERED: RefCell<Vec<(*const u8, usize, u32)>> = const {
        RefCell::new(Vec::new())
    };
    static QUEUE: RefCell<Vec<PendingAsset>> = const { RefCell::new(Vec::new()) };
}

/// Registers one picture and returns the id an [`crate::Image`] draws it by.
///
/// ```ignore
/// static HERO: &[u8] = include_bytes!("../assets/hero.svg");
///
/// rsx! { Image { asset_id: asset(AssetKind::Svg, HERO), height: 240.0 } }
/// ```
///
/// Call it wherever the picture is drawn, including in a component body that runs every
/// render. The second call with the same bytes returns the first call's id and queues
/// nothing, so a screen that draws the same picture every frame registers it once.
///
/// The bytes are `'static` because a registration is carried by whichever batch the
/// current call produces, which is not always the call this was made in, and because
/// `include_bytes!` is what an application almost always has.
pub fn asset(kind: AssetKind, bytes: &'static [u8]) -> u32 {
    let key = (bytes.as_ptr(), bytes.len());
    let existing = REGISTERED.with_borrow(|registered| {
        registered
            .iter()
            .find(|(ptr, len, _)| (*ptr, *len) == key)
            .map(|(_, _, id)| *id)
    });
    if let Some(id) = existing {
        return id;
    }
    // Ids start at one. Zero is what a node with no asset property reads as, so handing it
    // out would make "no picture" and "the first picture" the same value on the wire.
    let asset_id = REGISTERED.with_borrow_mut(|registered| {
        let asset_id = registered.len() as u32 + 1;
        registered.push((key.0, key.1, asset_id));
        asset_id
    });
    QUEUE.with_borrow_mut(|queue| {
        queue.push(PendingAsset {
            asset_id,
            kind,
            bytes,
        });
    });
    asset_id
}

/// Hands every queued registration to `emit` and empties the queue.
///
/// The buffer is moved out and put back, so the steady state, where nothing new has been
/// registered, touches an empty `Vec` and allocates nothing.
pub(crate) fn drain(mut emit: impl FnMut(&PendingAsset)) {
    let mut taken = QUEUE.with_borrow_mut(std::mem::take);
    if taken.is_empty() {
        return;
    }
    for pending in &taken {
        emit(pending);
    }
    taken.clear();
    QUEUE.with_borrow_mut(|queue| {
        if queue.is_empty() {
            *queue = taken;
        }
    });
}

/// Forgets every registration and every id. Used between tests and when a new `Host` takes
/// over the thread.
///
/// A new Host means a Renderer whose cache is empty, so an id handed out by the one before
/// it names nothing. Keeping the table would leave a screen drawing pictures that were
/// never registered.
#[doc(hidden)]
pub fn reset_assets() {
    REGISTERED.with_borrow_mut(Vec::clear);
    QUEUE.with_borrow_mut(Vec::clear);
}
