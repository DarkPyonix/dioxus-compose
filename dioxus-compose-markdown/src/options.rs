//! What an application can decide about how a document is drawn.

use std::borrow::Cow;
use std::sync::Arc;

/// Code blocks longer than this many lines start folded.
pub const DEFAULT_FOLD_LINES: usize = 20;

/// The words on the controls the crate draws.
///
/// The application owns its interface language, so every word is replaceable. The
/// defaults are English because something has to be shown when nobody says otherwise.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Labels {
    /// The control that copies a code block.
    pub copy: Cow<'static, str>,
    /// The control that unfolds a long code block. `{n}` is replaced with how many lines
    /// are hidden.
    pub show_more: Cow<'static, str>,
    /// The control that folds it again.
    pub show_less: Cow<'static, str>,
}

impl Default for Labels {
    fn default() -> Self {
        Self {
            copy: Cow::Borrowed("Copy"),
            show_more: Cow::Borrowed("Show {n} more lines"),
            show_less: Cow::Borrowed("Show less"),
        }
    }
}

impl Labels {
    pub(crate) fn show_more(&self, hidden: usize) -> String {
        self.show_more.replace("{n}", &hidden.to_string())
    }
}

/// Turns an image address into an asset the application registered.
///
/// The crate never fetches anything: agent output is untrusted, and whether to go to the
/// network for it is the application's decision. Without a resolver an image is shown as
/// its description and its address. With one, the resolver decides, and an image it
/// returns `None` for is shown the same way.
#[derive(Clone)]
pub struct ImageResolver(Arc<dyn Fn(&str) -> Option<u32> + Send + Sync>);

impl ImageResolver {
    /// `resolve` receives the address exactly as the document wrote it and returns the id
    /// of an asset registered with `dioxus_compose::asset` (or the Host), or `None`.
    pub fn new(resolve: impl Fn(&str) -> Option<u32> + Send + Sync + 'static) -> Self {
        Self(Arc::new(resolve))
    }

    pub fn resolve(&self, url: &str) -> Option<u32> {
        (self.0)(url)
    }
}

impl PartialEq for ImageResolver {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl std::fmt::Debug for ImageResolver {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ImageResolver(..)")
    }
}

/// How a document is read and drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct MarkdownOptions {
    /// Code blocks with more lines than this start folded to this many.
    pub fold_lines: usize,
    /// How deep quotes and lists nest, counted together, before deeper content is shown
    /// flat inside the deepest level.
    pub max_depth: usize,
    pub labels: Labels,
    pub image_resolver: Option<ImageResolver>,
    /// Whether code is coloured. Colouring always happens on a worker thread and arrives
    /// a frame or more after the code; turning it off leaves code as plain monospace,
    /// which is also what a browser build does.
    pub highlight: bool,
}

impl Default for MarkdownOptions {
    fn default() -> Self {
        Self {
            fold_lines: DEFAULT_FOLD_LINES,
            max_depth: crate::parse::DEFAULT_MAX_DEPTH,
            labels: Labels::default(),
            image_resolver: None,
            highlight: true,
        }
    }
}

/// What the copy control on a code block hands the application.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopyRequest {
    /// The fence's language word exactly as written, never normalised: `rs` stays `rs`.
    pub language: Option<String>,
    /// The code exactly as the document holds it, byte for byte: not coloured, not
    /// folded, and with any terminal escape sequences still in it.
    pub text: String,
}
