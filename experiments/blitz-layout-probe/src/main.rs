//! Lays the VS Code workbench fixtures out with blitz-dom, headless, and writes every
//! element's box and every text run's position in the same shape `capture-vscode.mjs` read
//! out of the running VS Code, so `scripts/compare.mjs` can put the two side by side.
//!
//! usage (from this directory):
//!   cargo run --release -- <code-oss checkout> <codicon.ttf>
//!
//! Reads fixtures/{activitybar,sidebar,tabs,text}.html and captured/meta.json.
//! Writes captured/<name>.blitz.json, captured/blitz-css-errors.json and
//! captured/blitz-meta.json.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};

use blitz_dom::{BaseDocument, DocumentConfig, Node, net::Resource};
use blitz_html::HtmlDocument;
use blitz_traits::{
    net::{BoxedHandler, Bytes, NetProvider, Request, SharedCallback, Url},
    shell::{ColorScheme, Viewport},
};
use parley::PositionedLayoutItem;
use serde_json::{Value, json};

type Queue = Arc<Mutex<Vec<Result<Resource, Option<String>>>>>;

/// Serves the fixtures' stylesheets and fonts from disk.
///
/// `file:` URLs are read as they are. The stylesheet the window built at run time refers to
/// fonts by `vscode-file://vscode-app/<application>/Contents/Resources/app/...`; those are
/// mapped onto the Code-OSS checkout (`extensions/...` stays `extensions/...`, `out/...`
/// becomes `src/...`). `codicon.ttf` is not in the Code-OSS tree (the build copies it out of
/// the `@vscode/codicons` npm package), so it is served from the path given on the command
/// line.
struct LocalFiles {
    code_oss: PathBuf,
    codicon: PathBuf,
    queue: Queue,
    log: Arc<Mutex<Vec<String>>>,
}

impl LocalFiles {
    fn resolve(&self, url: &Url) -> Option<PathBuf> {
        let path = match url.scheme() {
            "file" => url.to_file_path().ok()?,
            "vscode-file" => {
                let decoded = percent_decode(url.path());
                let (_, rest) = decoded.split_once("/Contents/Resources/app/")?;
                if let Some(out) = rest.strip_prefix("out/") {
                    self.code_oss.join("src").join(out)
                } else {
                    self.code_oss.join(rest)
                }
            }
            _ => return None,
        };
        if path.exists() {
            return Some(path);
        }
        if path.file_name().is_some_and(|n| n == "codicon.ttf") {
            return Some(self.codicon.clone());
        }
        Some(path)
    }
}

impl NetProvider<Resource> for LocalFiles {
    fn fetch(&self, doc_id: usize, request: Request, handler: BoxedHandler<Resource>) {
        let url = request.url.clone();
        let Some(path) = self.resolve(&url) else {
            self.log.lock().unwrap().push(format!("unsupported url: {url}"));
            return;
        };
        match fs::read(&path) {
            Ok(bytes) => {
                self.log
                    .lock()
                    .unwrap()
                    .push(format!("served {} ({} bytes) for {url}", path.display(), bytes.len()));
                let queue = self.queue.clone();
                let callback: SharedCallback<Resource> =
                    Arc::new(move |_doc: usize, result: Result<Resource, Option<String>>| {
                        queue.lock().unwrap().push(result);
                    });
                handler.bytes(doc_id, Bytes::from(bytes), callback);
            }
            Err(e) => {
                self.log
                    .lock()
                    .unwrap()
                    .push(format!("missing {} for {url}: {e}", path.display()));
            }
        }
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[derive(Clone, Copy, Debug, Default)]
struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl Rect {
    fn union(self, o: Rect) -> Rect {
        let x0 = self.x.min(o.x);
        let y0 = self.y.min(o.y);
        let x1 = (self.x + self.w).max(o.x + o.w);
        let y1 = (self.y + self.h).max(o.y + o.h);
        Rect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
    }
    fn json(&self) -> Value {
        json!({ "x": self.x, "y": self.y, "w": self.w, "h": self.h })
    }
}

fn attr<'a>(node: &'a Node, name: &str) -> Option<&'a str> {
    node.element_data()?
        .attrs()
        .iter()
        .find(|a| &*a.name.local == name)
        .map(|a| a.value.as_str())
}

/// Position of a node's border box in document coordinates, from the unrounded layout
/// (Chromium reports fractional positions, so the rounded one would add up to half a pixel
/// of error that is not blitz-dom's).
fn abs_unrounded(doc: &BaseDocument, id: usize) -> (f32, f32) {
    let (mut x, mut y) = (0.0f32, 0.0f32);
    let mut cur = Some(id);
    while let Some(i) = cur {
        let n = doc.get_node(i).unwrap();
        x += n.unrounded_layout.location.x - n.scroll_offset.x as f32;
        y += n.unrounded_layout.location.y - n.scroll_offset.y as f32;
        cur = n.layout_parent.get();
    }
    (x, y)
}

fn abs_rounded(doc: &BaseDocument, id: usize) -> (f32, f32) {
    let p = doc.get_node(id).unwrap().absolute_position(0.0, 0.0);
    (p.x, p.y)
}

/// One glyph run (or inline box) placed in document coordinates, with the node that owns
/// it: the element whose style span it was shaped in, or the inline box itself.
struct Placed {
    owner: usize,
    root: usize,
    line: usize,
    rect: Rect,
    is_box: bool,
}

struct InlineInfo {
    placed: Vec<Placed>,
    /// Elements that live inside an inline formatting context without a box of their own
    /// (an ordinary `<span>`): Chromium gives them the union of their fragments.
    spans: HashSet<usize>,
    /// Families, sizes and variation coordinates of the fonts the runs were shaped with.
    fonts: BTreeMap<String, usize>,
}

fn collect_inline(doc: &BaseDocument) -> InlineInfo {
    let mut info = InlineInfo { placed: Vec::new(), spans: HashSet::new(), fonts: BTreeMap::new() };
    let mut font_names: HashMap<(u64, u32), String> = HashMap::new();
    for (root_id, root) in doc.tree().iter() {
        let Some(el) = root.element_data() else { continue };
        let Some(text_layout) = el.inline_layout_data.as_ref() else { continue };
        let layout = &text_layout.layout;
        let scale = layout.scale();
        let (rx, ry) = abs_unrounded(doc, root_id);
        let l = &root.unrounded_layout;
        let ox = rx + l.padding.left + l.border.left;
        let oy = ry + l.padding.top + l.border.top;

        let boxes: HashSet<usize> = layout.inline_boxes().iter().map(|b| b.id as usize).collect();
        mark_spans(doc, root_id, &boxes, &mut info.spans);

        for (li, line) in layout.lines().enumerate() {
            for item in line.items() {
                match item {
                    PositionedLayoutItem::GlyphRun(gr) => {
                        let run = gr.run();
                        let m = run.metrics();
                        let rect = Rect {
                            x: ox + gr.offset() / scale,
                            y: oy + (gr.baseline() - m.ascent) / scale,
                            w: gr.advance() / scale,
                            h: (m.ascent + m.descent) / scale,
                        };
                        info.placed.push(Placed { owner: gr.style().brush.id, root: root_id, line: li, rect, is_box: false });
                        let font = run.font();
                        let key = (font.data.id(), font.index);
                        let name = font_names
                            .entry(key)
                            .or_insert_with(|| font_name(font.data.data(), font.index).unwrap_or_else(|| "?".into()))
                            .clone();
                        let desc = format!("{name} @ {}px coords {:?}", run.font_size() / scale, run.normalized_coords());
                        *info.fonts.entry(desc).or_default() += 1;
                    }
                    PositionedLayoutItem::InlineBox(b) => {
                        let id = b.id as usize;
                        let (bx, by) = abs_unrounded(doc, id);
                        let n = doc.get_node(id).unwrap();
                        let rect = Rect { x: bx, y: by, w: n.unrounded_layout.size.width, h: n.unrounded_layout.size.height };
                        info.placed.push(Placed { owner: id, root: root_id, line: li, rect, is_box: true });
                    }
                }
            }
        }
    }
    info
}

/// Marks every element inside the inline formatting context of `root` that is neither an
/// inline box nor the root of a context of its own.
fn mark_spans(doc: &BaseDocument, id: usize, boxes: &HashSet<usize>, spans: &mut HashSet<usize>) {
    let node = doc.get_node(id).unwrap();
    let kids = node.before.into_iter().chain(node.children.iter().copied()).chain(node.after);
    for c in kids {
        let child = doc.get_node(c).unwrap();
        if child.element_data().is_none() || boxes.contains(&c) {
            continue;
        }
        if child.element_data().is_some_and(|e| e.inline_layout_data.is_some()) {
            continue;
        }
        spans.insert(c);
        mark_spans(doc, c, boxes, spans);
    }
}

/// Reads a font's family and PostScript names from its `name` table.
fn font_name(data: &[u8], index: u32) -> Option<String> {
    let rd16 = |o: usize| -> Option<usize> { Some(u16::from_be_bytes([*data.get(o)?, *data.get(o + 1)?]) as usize) };
    let rd32 = |o: usize| -> Option<usize> {
        Some(u32::from_be_bytes([*data.get(o)?, *data.get(o + 1)?, *data.get(o + 2)?, *data.get(o + 3)?]) as usize)
    };
    let mut base = 0;
    if data.get(0..4)? == b"ttcf" {
        base = rd32(12 + 4 * index as usize)?;
    }
    let tables = rd16(base + 4)?;
    let mut name_off = None;
    for i in 0..tables {
        let rec = base + 12 + 16 * i;
        if data.get(rec..rec + 4)? == b"name" {
            name_off = Some(rd32(rec + 8)?);
        }
    }
    let off = name_off?;
    let count = rd16(off + 2)?;
    let strings = off + rd16(off + 4)?;
    let mut family = None;
    let mut postscript = None;
    for i in 0..count {
        let r = off + 6 + 12 * i;
        let (platform, name_id, len, so) = (rd16(r)?, rd16(r + 6)?, rd16(r + 8)?, rd16(r + 10)?);
        let raw = data.get(strings + so..strings + so + len)?;
        let s = match platform {
            0 | 3 => String::from_utf16_lossy(&raw.chunks(2).filter(|c| c.len() == 2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect::<Vec<_>>()),
            _ => raw.iter().map(|&b| b as char).collect(),
        };
        match name_id {
            1 if family.is_none() || platform == 3 => family = Some(s),
            6 if postscript.is_none() || platform == 3 => postscript = Some(s),
            _ => {}
        }
    }
    Some(format!("{} / {}", family.unwrap_or_default(), postscript.unwrap_or_default()))
}

struct Loaded {
    doc: HtmlDocument,
    parse_ms: f64,
    resolve_ms: f64,
    errors: Vec<String>,
}

fn load(fixture: &Path, viewport: &Viewport, provider: &Arc<LocalFiles>) -> Loaded {
    let html = fs::read_to_string(fixture).unwrap_or_else(|e| panic!("cannot read {}: {e}", fixture.display()));
    let base = Url::from_file_path(fixture.canonicalize().unwrap()).unwrap().to_string();
    let net: Arc<dyn NetProvider<Resource>> = provider.clone();
    let config = DocumentConfig {
        viewport: Some(viewport.clone()),
        base_url: Some(base),
        net_provider: Some(net),
        ..Default::default()
    };
    let t0 = Instant::now();
    let mut doc = HtmlDocument::from_html(&html, config);
    // The provider answers synchronously, so every stylesheet and font is already queued;
    // loading a stylesheet can queue its fonts, so drain until nothing is left.
    let mut errors = Vec::new();
    loop {
        let batch: Vec<_> = std::mem::take(&mut *provider.queue.lock().unwrap());
        if batch.is_empty() {
            break;
        }
        for r in batch {
            match r {
                Ok(res) => doc.load_resource(res),
                Err(e) => errors.push(format!("{e:?}")),
            }
        }
    }
    let parse_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let t1 = Instant::now();
    doc.resolve(0.0);
    let resolve_ms = t1.elapsed().as_secs_f64() * 1000.0;
    Loaded { doc, parse_ms, resolve_ms, errors }
}

fn region(doc: &BaseDocument) -> Value {
    let info = collect_inline(doc);

    // Union of fragments for every span, walking from each fragment's owner up to its root.
    let mut span_rects: HashMap<usize, Rect> = HashMap::new();
    for p in &info.placed {
        let mut cur = if p.is_box { doc.get_node(p.owner).and_then(|n| n.parent) } else { Some(p.owner) };
        while let Some(id) = cur {
            if id == p.root {
                break;
            }
            if info.spans.contains(&id) {
                span_rects.entry(id).and_modify(|r| *r = r.union(p.rect)).or_insert(p.rect);
            }
            cur = doc.get_node(id).and_then(|n| n.parent);
        }
    }

    // Text runs per owner element and line, merged into one rect per line.
    let mut text_lines: BTreeMap<usize, BTreeMap<(usize, usize), Rect>> = BTreeMap::new();
    for p in info.placed.iter().filter(|p| !p.is_box) {
        text_lines
            .entry(p.owner)
            .or_default()
            .entry((p.root, p.line))
            .and_modify(|r| *r = r.union(p.rect))
            .or_insert(p.rect);
    }

    let mut elements = Vec::new();
    let mut texts = Vec::new();
    for (id, node) in doc.tree().iter() {
        let Some(pid) = attr(node, "data-probe-id").and_then(|v| v.parse::<usize>().ok()) else { continue };
        let (rect, rounded, kind) = if info.spans.contains(&id) {
            let r = span_rects.get(&id).copied().unwrap_or_default();
            (r, r, "inline")
        } else {
            let (x, y) = abs_unrounded(doc, id);
            let (rx, ry) = abs_rounded(doc, id);
            let s = node.unrounded_layout.size;
            let fs = node.final_layout.size;
            (Rect { x, y, w: s.width, h: s.height }, Rect { x: rx, y: ry, w: fs.width, h: fs.height }, "box")
        };
        elements.push(json!({
            "id": pid, "tag": node.element_data().map(|e| e.name.local.to_string()),
            "x": rect.x, "y": rect.y, "w": rect.w, "h": rect.h,
            "rounded": rounded.json(), "kind": kind,
        }));
        // Every non-blank text child, positioned by the runs shaped in this element's span.
        for &c in &node.children {
            let Some(t) = doc.get_node(c).and_then(|n| n.text_data()) else { continue };
            if t.content.trim().is_empty() {
                continue;
            }
            let rects: Vec<Value> = text_lines
                .get(&id)
                .map(|lines| lines.values().map(|r| r.json()).collect())
                .unwrap_or_default();
            texts.push(json!({ "parent": pid, "text": t.content, "rects": rects }));
        }
    }
    json!({ "elements": elements, "texts": texts, "fonts": info.fonts })
}

fn text_cases(doc: &BaseDocument) -> Value {
    let mut results = Vec::new();
    for (_, node) in doc.tree().iter() {
        let Some(case) = attr(node, "data-case") else { continue };
        let size = node.unrounded_layout.size;
        let Some(tl) = node.element_data().and_then(|e| e.inline_layout_data.as_ref()) else {
            results.push(json!({ "id": case, "error": "no inline layout", "box": { "w": size.width, "h": size.height }, "lines": [], "chars": [] }));
            continue;
        };
        let layout = &tl.layout;
        let scale = layout.scale();
        // Byte offset of each cluster's start, its x and advance, and its line.
        let mut clusters: Vec<(std::ops::Range<usize>, f32, f32, usize)> = Vec::new();
        let mut lines = Vec::new();
        let mut line_tops = Vec::new();
        let mut fonts: BTreeMap<String, usize> = BTreeMap::new();
        for (li, line) in layout.lines().enumerate() {
            let m = line.metrics();
            line_tops.push((m.min_coord / scale, (m.max_coord - m.min_coord) / scale));
            lines.push(json!({
                "x": m.offset / scale, "y": m.min_coord / scale,
                "w": (m.advance - m.trailing_whitespace) / scale, "h": (m.max_coord - m.min_coord) / scale,
                "advanceWithTrailingSpace": m.advance / scale, "lineHeight": m.line_height / scale,
                "ascent": m.ascent / scale, "descent": m.descent / scale,
            }));
            for item in line.items() {
                if let PositionedLayoutItem::GlyphRun(gr) = item {
                    let run = gr.run();
                    *fonts
                        .entry(format!(
                            "{} @ {}px coords {:?}",
                            font_name(run.font().data.data(), run.font().index).unwrap_or_else(|| "?".into()),
                            run.font_size() / scale,
                            run.normalized_coords()
                        ))
                        .or_default() += 1;
                    let mut x = gr.offset();
                    for c in run.clusters() {
                        clusters.push((c.text_range(), x / scale, c.advance() / scale, li));
                        x += c.advance();
                    }
                }
            }
        }
        // One rect per character, in the same order Chromium's were taken (by code point).
        // A cluster that covers several characters (a ligature) is split evenly, which is
        // also how Chromium places carets inside a ligature.
        let mut chars = Vec::new();
        for (bi, _) in tl.text.char_indices() {
            let found = clusters.iter().find(|(r, ..)| r.contains(&bi));
            match found {
                Some((r, x, w, li)) => {
                    let in_cluster: Vec<usize> = tl.text[r.clone()].char_indices().map(|(o, _)| o + r.start).collect();
                    let k = in_cluster.iter().position(|&o| o == bi).unwrap_or(0);
                    let n = in_cluster.len().max(1) as f32;
                    let (top, h) = line_tops[*li];
                    chars.push(json!({ "x": x + w * k as f32 / n, "y": top, "w": w / n, "h": h, "line": li }));
                }
                None => chars.push(Value::Null),
            }
        }
        results.push(json!({
            "id": case, "box": { "w": size.width, "h": size.height }, "lines": lines, "chars": chars,
            "text": tl.text, "fonts": fonts,
        }));
    }
    json!({ "results": results })
}

mod css_errors {
    //! Parses the stylesheets the way blitz-dom does, but with an error reporter, so the
    //! declarations and rules stylo drops are counted rather than silently ignored.

    use std::{collections::BTreeMap, sync::Mutex};

    use blitz_traits::net::Url;
    use cssparser::SourceLocation;
    use serde_json::{Value, json};
    use style::{
        context::QuirksMode,
        error_reporting::{ContextualParseError, ParseErrorReporter},
        media_queries::MediaList,
        servo_arc::Arc as ServoArc,
        shared_lock::SharedRwLock,
        stylesheets::{AllowImportRules, Origin, Stylesheet, UrlExtraData},
    };

    #[derive(Default)]
    struct Collector {
        by_key: Mutex<BTreeMap<(String, String), (usize, String)>>,
    }

    impl ParseErrorReporter for Collector {
        fn report_error(&self, _url: &UrlExtraData, location: SourceLocation, error: ContextualParseError) {
            let (kind, key) = match &error {
                ContextualParseError::UnsupportedPropertyDeclaration(decl, _, _) => {
                    ("property", decl.split(':').next().unwrap_or("").trim().to_string())
                }
                ContextualParseError::InvalidRule(rule, _) | ContextualParseError::UnsupportedRule(rule, _) => {
                    ("rule", rule.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(120).collect())
                }
                ContextualParseError::UnsupportedValue(v, _) => ("value", v.chars().take(80).collect()),
                other => ("other", other.to_string().chars().take(120).collect()),
            };
            let sample = format!("line {}: {}", location.line + 1, error.to_string().chars().take(200).collect::<String>());
            let mut map = self.by_key.lock().unwrap();
            let e = map.entry((kind.to_string(), key)).or_insert((0, sample));
            e.0 += 1;
        }
    }

    pub fn report(css: &str, url: Url) -> Value {
        let collector = Collector::default();
        let lock = SharedRwLock::new();
        let _sheet = Stylesheet::from_str(
            css,
            UrlExtraData::from(url),
            Origin::Author,
            ServoArc::new(lock.wrap(MediaList::empty())),
            lock.clone(),
            None,
            Some(&collector),
            QuirksMode::NoQuirks,
            AllowImportRules::Yes,
        );
        let map = collector.by_key.into_inner().unwrap();
        let mut rows: Vec<_> = map.into_iter().collect();
        rows.sort_by(|a, b| b.1.0.cmp(&a.1.0));
        let total: usize = rows.iter().map(|r| r.1.0).sum();
        json!({
            "dropped": total,
            "byKind": rows.iter().fold(BTreeMap::<String, usize>::new(), |mut m, r| { *m.entry(r.0.0.clone()).or_default() += r.1.0; m }),
            "entries": rows.iter().map(|((kind, key), (n, sample))| json!({ "kind": kind, "key": key, "count": n, "sample": sample })).collect::<Vec<_>>(),
        })
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: blitz-layout-probe <code-oss checkout> <codicon.ttf>");
        std::process::exit(2);
    }
    let code_oss = PathBuf::from(&args[1]);
    let codicon = PathBuf::from(&args[2]);
    let dir = std::env::current_dir().unwrap();
    let captured = dir.join("captured");
    let fixtures = dir.join("fixtures");

    let meta: Value = serde_json::from_str(&fs::read_to_string(captured.join("meta.json")).unwrap()).unwrap();
    let vw = meta["viewport"]["width"].as_f64().unwrap() as f32;
    let vh = meta["viewport"]["height"].as_f64().unwrap() as f32;
    let dpr = meta["viewport"]["dpr"].as_f64().unwrap() as f32;
    // The viewport takes physical pixels and a scale, as a window would hand them over.
    let viewport = Viewport::new((vw * dpr) as u32, (vh * dpr) as u32, dpr, ColorScheme::Dark);

    let provider = Arc::new(LocalFiles {
        code_oss: code_oss.clone(),
        codicon,
        queue: Arc::new(Mutex::new(Vec::new())),
        log: Arc::new(Mutex::new(Vec::new())),
    });

    let mut timings = serde_json::Map::new();
    let mut fonts = serde_json::Map::new();
    for name in ["activitybar", "sidebar", "tabs", "text"] {
        let loaded = load(&fixtures.join(format!("{name}.html")), &viewport, &provider);
        let out = if name == "text" { text_cases(&loaded.doc) } else { region(&loaded.doc) };
        if let Some(f) = out.get("fonts") {
            fonts.insert(name.into(), f.clone());
        }
        let n = out.get("elements").or_else(|| out.get("results")).and_then(|v| v.as_array()).map_or(0, |a| a.len());
        fs::write(captured.join(format!("{name}.blitz.json")), serde_json::to_string_pretty(&out).unwrap()).unwrap();
        println!(
            "{name}: {n} records, parse and load {:.1} ms, resolve {:.1} ms, {} resource errors",
            loaded.parse_ms,
            loaded.resolve_ms,
            loaded.errors.len()
        );
        timings.insert(name.into(), json!({ "parseAndLoadMs": loaded.parse_ms, "resolveMs": loaded.resolve_ms, "resourceErrors": loaded.errors }));
    }

    // After the first document exists, stylo's layout preferences (flexbox, grid) are on,
    // so the parse below accepts what blitz-dom's own parse accepts.
    let css_path = dir.join("../../.scratch/blitz-layout-probe/out/workbench.css");
    let workbench_css = fs::read_to_string(&css_path).unwrap_or_else(|e| panic!("cannot read {}: {e}", css_path.display()));
    let runtime_css = fs::read_to_string(captured.join("runtime.css")).unwrap();
    let errors = json!({
        "workbench.css": css_errors::report(&workbench_css, Url::from_file_path(css_path.canonicalize().unwrap()).unwrap()),
        "runtime.css": css_errors::report(&runtime_css, Url::from_file_path(captured.join("runtime.css").canonicalize().unwrap()).unwrap()),
    });
    fs::write(captured.join("blitz-css-errors.json"), serde_json::to_string_pretty(&errors).unwrap()).unwrap();
    println!(
        "css dropped by stylo: workbench.css {}, runtime.css {}",
        errors["workbench.css"]["dropped"], errors["runtime.css"]["dropped"]
    );

    let log = provider.log.lock().unwrap().clone();
    fs::write(
        captured.join("blitz-meta.json"),
        serde_json::to_string_pretty(&json!({
            "viewport": { "width": vw, "height": vh, "dpr": dpr },
            "timings": timings,
            "fonts": fonts,
            "resources": log,
            "codeOss": code_oss.display().to_string(),
        }))
        .unwrap(),
    )
    .unwrap();
    for (k, v) in &fonts {
        println!("fonts in {k}: {v}");
    }
}
