//! Syntax colouring, done on a worker thread and delivered as colour roles.
//!
//! The highlighter is regular-expression based, so its cost grows with the number of
//! lines, and a block of a few thousand lines would spend the whole frame budget on its
//! own. It therefore never runs on the UI thread: a code block asks for its colours, shows
//! plain monospace until they arrive, and then changes one property, its runs.
//!
//! The highlighter's own colour themes are not used. Grammars name what each piece of
//! code is (`comment.line`, `string.quoted.double`, `entity.name.function`), and the table
//! below turns those names into the code colour roles, which the design system resolves.

use crate::code::CodeSpan;
use std::cell::Cell;

thread_local! {
    static RUNS_ON_THIS_THREAD: Cell<usize> = const { Cell::new(0) };
}

/// How many times the highlighter has run on the calling thread.
///
/// Exists so a test can show that drawing a code block never highlights on the thread
/// that draws.
#[doc(hidden)]
pub fn runs_on_this_thread() -> usize {
    RUNS_ON_THIS_THREAD.with(Cell::get)
}

/// The languages a fence may name, with the words that name them.
///
/// The first column is what a fence says, lower-cased; the second is what the grammar set
/// is searched by (an extension, then a name), and the third is the grammar's own name,
/// tried when the second finds nothing.
const LANGUAGES: &[(&[&str], &str, &str)] = &[
    (&["rust", "rs"], "rs", "Rust"),
    (&["kotlin", "kt", "kts"], "kt", "Kotlin"),
    (&["swift"], "swift", "Swift"),
    (&["java"], "java", "Java"),
    (&["c", "h"], "c", "C"),
    (
        &["c++", "cpp", "cc", "cxx", "hpp", "hh", "hxx"],
        "cpp",
        "C++",
    ),
    (&["c#", "csharp", "cs"], "cs", "C#"),
    (&["go", "golang"], "go", "Go"),
    (&["python", "py", "py3", "python3"], "py", "Python"),
    (
        &["javascript", "js", "mjs", "cjs", "node", "jsx"],
        "js",
        "JavaScript",
    ),
    (&["typescript", "ts", "mts", "cts"], "ts", "TypeScript"),
    (&["tsx"], "tsx", "TypeScriptReact"),
    (&["json", "jsonc"], "json", "JSON"),
    (&["yaml", "yml"], "yaml", "YAML"),
    (&["toml"], "toml", "TOML"),
    (&["html", "htm", "xhtml"], "html", "HTML"),
    (&["css"], "css", "CSS"),
    (
        &["shell", "sh", "bash", "zsh", "console", "shellscript"],
        "sh",
        "Bourne Again Shell (bash)",
    ),
    (&["sql"], "sql", "SQL"),
    (&["markdown", "md"], "md", "Markdown"),
    (&["dockerfile", "docker"], "Dockerfile", "Dockerfile"),
    (&["ruby", "rb"], "rb", "Ruby"),
];

/// The grammar a fence's language word stands for, as the lookup key the highlighter
/// uses. Common aliases resolve to the same key as the language's full name, so `rs` and
/// `rust` are coloured identically. A word not in the table is passed on as it is, and the
/// grammar set gets a chance to know it.
pub fn canonical_language(language: &str) -> String {
    let lowered = language.trim().to_ascii_lowercase();
    LANGUAGES
        .iter()
        .find(|(names, _, _)| names.contains(&lowered.as_str()))
        .map_or(lowered, |(_, key, _)| (*key).to_owned())
}

/// The fence words the crate promises to colour, one per language.
pub fn supported_languages() -> impl Iterator<Item = &'static str> {
    LANGUAGES.iter().map(|(names, _, _)| names[0])
}

#[cfg(not(target_family = "wasm"))]
mod engine {
    use super::{CodeSpan, LANGUAGES, RUNS_ON_THIS_THREAD};
    use dioxus_compose::ColorRole;
    use std::sync::OnceLock;
    use two_face::re_exports::syntect::parsing::{
        ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet,
    };

    fn syntaxes() -> &'static SyntaxSet {
        static SET: OnceLock<SyntaxSet> = OnceLock::new();
        SET.get_or_init(two_face::syntax::extra_newlines)
    }

    /// Grammar scope prefixes and the role each is drawn in. The innermost scope that has
    /// an entry decides, and within one scope the first matching entry does, so the more
    /// specific names come first.
    const SCOPES: &[(&str, ColorRole)] = &[
        ("punctuation.definition.comment", ColorRole::SyntaxComment),
        ("comment", ColorRole::SyntaxComment),
        ("constant.character.escape", ColorRole::SyntaxEscape),
        ("punctuation.definition.string", ColorRole::SyntaxString),
        ("string", ColorRole::SyntaxString),
        ("constant.numeric", ColorRole::SyntaxNumber),
        ("support.constant", ColorRole::SyntaxConstant),
        ("constant", ColorRole::SyntaxConstant),
        ("variable.language", ColorRole::SyntaxKeyword),
        ("keyword.operator", ColorRole::SyntaxOperator),
        ("keyword", ColorRole::SyntaxKeyword),
        ("storage.type.primitive", ColorRole::SyntaxType),
        ("storage.type.numeric", ColorRole::SyntaxType),
        ("storage", ColorRole::SyntaxKeyword),
        ("support.macro", ColorRole::SyntaxMacro),
        ("entity.name.macro", ColorRole::SyntaxMacro),
        ("entity.name.function.macro", ColorRole::SyntaxMacro),
        ("support.function.macro", ColorRole::SyntaxMacro),
        ("entity.name.function", ColorRole::SyntaxFunction),
        ("support.function", ColorRole::SyntaxFunction),
        ("variable.function", ColorRole::SyntaxFunction),
        ("entity.name.tag", ColorRole::SyntaxTag),
        ("entity.other.attribute-name", ColorRole::SyntaxAttribute),
        ("support.type.property-name", ColorRole::SyntaxProperty),
        ("entity.name.type", ColorRole::SyntaxType),
        ("entity.name.class", ColorRole::SyntaxType),
        ("entity.name.struct", ColorRole::SyntaxType),
        ("entity.name.enum", ColorRole::SyntaxType),
        ("entity.name.trait", ColorRole::SyntaxType),
        ("entity.name.interface", ColorRole::SyntaxType),
        ("entity.other.inherited-class", ColorRole::SyntaxType),
        ("support.type", ColorRole::SyntaxType),
        ("support.class", ColorRole::SyntaxType),
        ("variable.other.member", ColorRole::SyntaxProperty),
        ("variable.other.property", ColorRole::SyntaxProperty),
        ("meta.property-name", ColorRole::SyntaxProperty),
        ("variable.parameter", ColorRole::SyntaxVariable),
        ("variable", ColorRole::SyntaxVariable),
        ("markup.heading", ColorRole::SyntaxKeyword),
        ("markup.inserted", ColorRole::DiffAdded),
        ("markup.deleted", ColorRole::DiffRemoved),
        ("markup.changed", ColorRole::DiffModified),
        ("markup.raw", ColorRole::SyntaxString),
        ("markup.underline.link", ColorRole::SyntaxString),
        ("punctuation", ColorRole::SyntaxPunctuation),
    ];

    fn scope_table() -> &'static [(Scope, ColorRole)] {
        static TABLE: OnceLock<Vec<(Scope, ColorRole)>> = OnceLock::new();
        TABLE.get_or_init(|| {
            SCOPES
                .iter()
                .filter_map(|(name, role)| Scope::new(name).ok().map(|scope| (scope, *role)))
                .collect()
        })
    }

    fn role_of(stack: &ScopeStack, table: &[(Scope, ColorRole)]) -> Option<ColorRole> {
        stack.as_slice().iter().rev().find_map(|scope| {
            table
                .iter()
                .find(|(prefix, _)| prefix.is_prefix_of(*scope))
                .map(|(_, role)| *role)
        })
    }

    fn syntax_for(key: &str) -> Option<&'static SyntaxReference> {
        let set = syntaxes();
        set.find_syntax_by_token(key).or_else(|| {
            LANGUAGES
                .iter()
                .find(|(_, lookup, _)| *lookup == key)
                .and_then(|(_, _, name)| set.find_syntax_by_name(name))
        })
    }

    fn push(spans: &mut Vec<CodeSpan>, start: usize, end: usize, role: Option<ColorRole>) {
        let Some(role) = role else {
            return;
        };
        if end <= start {
            return;
        }
        if let Some(last) = spans.last_mut() {
            if last.role == Some(role) && (last.start + last.len) as usize == start {
                last.len += (end - start) as u32;
                return;
            }
        }
        spans.push(CodeSpan::colored(start as u32, (end - start) as u32, role));
    }

    pub(super) fn highlight(language: &str, text: &str) -> Option<Vec<CodeSpan>> {
        RUNS_ON_THIS_THREAD.with(|runs| runs.set(runs.get() + 1));
        let key = super::canonical_language(language);
        let syntax = syntax_for(&key)?;
        let set = syntaxes();
        let table = scope_table();
        let mut state = ParseState::new(syntax);
        let mut stack = ScopeStack::new();
        let mut spans = Vec::new();
        let mut offset = 0;
        for line in text.split_inclusive('\n') {
            let Ok(operations) = state.parse_line(line, set) else {
                break;
            };
            let mut cursor = 0;
            for (at, operation) in operations {
                let at = at.min(line.len());
                if at > cursor {
                    push(
                        &mut spans,
                        offset + cursor,
                        offset + at,
                        role_of(&stack, table),
                    );
                    cursor = at;
                }
                if stack.apply(&operation).is_err() {
                    return Some(spans);
                }
            }
            // The line break itself is never coloured.
            let end = line.strip_suffix('\n').map_or(line.len(), str::len);
            if end > cursor {
                push(
                    &mut spans,
                    offset + cursor,
                    offset + end,
                    role_of(&stack, table),
                );
            }
            offset += line.len();
        }
        Some(spans)
    }
}

/// Colours `text` as the language a fence named. `None` when no grammar knows the
/// language, which callers draw as plain monospace.
///
/// Runs on the calling thread. The `Markdown` and `CodeBlock` components never call it
/// on the UI thread; they hand the work to the highlighter thread.
#[cfg(not(target_family = "wasm"))]
pub fn highlight(language: &str, text: &str) -> Option<Vec<CodeSpan>> {
    engine::highlight(language, text)
}

/// A browser build has no highlighter: there is no worker thread to run it on.
#[cfg(target_family = "wasm")]
pub fn highlight(_language: &str, _text: &str) -> Option<Vec<CodeSpan>> {
    RUNS_ON_THIS_THREAD.with(|runs| runs.set(runs.get() + 1));
    None
}

/// What a code block gets back: the key it asked with, and the runs.
pub(crate) type Delivery = Box<dyn FnOnce(u64, std::sync::Arc<Vec<CodeSpan>>) + Send>;

#[cfg(not(target_family = "wasm"))]
mod worker {
    use super::{CodeSpan, Delivery};
    use std::collections::HashMap;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::mpsc::{Sender, channel};
    use std::sync::{Arc, OnceLock};

    pub(super) struct Job {
        pub key: u64,
        pub language: String,
        pub text: Arc<str>,
        pub deliver: Delivery,
    }

    /// Results kept for blocks that come back, such as a message scrolled out of a list
    /// and back in. Bounded, because a long session sees a great deal of code.
    const CACHE_LIMIT: usize = 128;

    pub(super) fn sender() -> Option<&'static Sender<Job>> {
        static SENDER: OnceLock<Option<Sender<Job>>> = OnceLock::new();
        SENDER
            .get_or_init(|| {
                let (sender, receiver) = channel::<Job>();
                std::thread::Builder::new()
                    .name("dioxus-compose-markdown-highlight".to_owned())
                    .spawn(move || {
                        let mut cache: HashMap<u64, Arc<Vec<CodeSpan>>> = HashMap::new();
                        for job in receiver {
                            let spans = match cache.get(&job.key) {
                                Some(spans) => spans.clone(),
                                None => {
                                    // A grammar that trips the engine must not take the
                                    // thread down with it: every later block would stay
                                    // plain for the rest of the session.
                                    let result = catch_unwind(AssertUnwindSafe(|| {
                                        super::highlight(&job.language, &job.text)
                                            .unwrap_or_default()
                                    }));
                                    let spans = Arc::new(result.unwrap_or_default());
                                    if cache.len() >= CACHE_LIMIT {
                                        cache.clear();
                                    }
                                    cache.insert(job.key, spans.clone());
                                    spans
                                }
                            };
                            let deliver = job.deliver;
                            let _ = catch_unwind(AssertUnwindSafe(move || {
                                deliver(job.key, spans)
                            }));
                        }
                    })
                    .ok()
                    .map(|_| sender)
            })
            .as_ref()
    }
}

/// Asks the highlighter thread for `text`'s colours. `deliver` runs on that thread.
///
/// On a target without threads nothing is delivered and the code stays plain.
pub(crate) fn request(key: u64, language: &str, text: std::sync::Arc<str>, deliver: Delivery) {
    #[cfg(not(target_family = "wasm"))]
    {
        if let Some(sender) = worker::sender() {
            let _ = sender.send(worker::Job {
                key,
                language: language.to_owned(),
                text,
                deliver,
            });
        }
    }
    #[cfg(target_family = "wasm")]
    {
        let _ = (key, language, text, deliver);
    }
}

/// The key a result is filed under: the language and the exact text.
pub(crate) fn key_of(language: &str, text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    canonical_language(language).hash(&mut hasher);
    text.hash(&mut hasher);
    hasher.finish()
}
