//! The editor in a window, driving itself and timing every frame.
//!
//! The page runs as an ordinary `HtmlRuntime`, wrapped in [`Probe`], which the Host drives
//! in its place. The probe times each call the Host makes into the page and, between
//! frames, plays one scenario by writing the editor's signal the way its handlers do:
//!
//! - `idle`: frames with nothing changed;
//! - `page`: a page down per frame, a hundred times, from the top;
//! - `wheel`: 12px per frame, three hundred times, from the middle of the file;
//! - `type`: five hundred keystrokes in the middle of the file, one every other frame,
//!   with a new line every fifty;
//! - `none`: nothing; the renderer's own made-up typing (`DXC_SYNTH=type`) is what is
//!   measured, through the real text field and the `input` handler.
//!
//! While a scenario runs, the probe asks for the next frame as soon as one is served, so
//! the frames come at the renderer's own pace. What it reports per frame:
//!
//! - `host`: the page's `render` (Dioxus, layout, plan, bridge), which is the Host's part
//!   of the frame and includes the renderer's measure call for text it has not measured;
//! - `interval`: from the start of this frame to the start of the next. Frames are served
//!   one after another on the UI thread, so this is the Host's part, the renderer's part
//!   (applying the batch, recomposing, drawing, presenting) and any wait for the display;
//! - for a keystroke, `key to next frame`: the key is handed to the page at the start of a
//!   frame, as a dispatched event would be, and the next frame starts once the one that
//!   shows it has been drawn, so this is the keystroke-to-frame time.
//!
//! The results go to stderr as Markdown, and to `EDITOR_SPIKE_OUT` when it is set.
//!
//! Environment: `EDITOR_SCENARIO` (above), `EDITOR_FILE` (a file to load instead of the
//! generated 100,000 lines), `EDITOR_SCROLL_MODE` (`top` or `transform`).

use std::fmt::Write as _;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use dioxus_compose::html::HtmlRuntime;
use dioxus_compose::protocol::{HostEvent, ProtocolError};
use dioxus_compose::schema::Window;
use dioxus_compose::{Batch, Runtime};
use editor_spike::{Options, ScrollMode, Stats, drive, load, set_options, set_source};

const WIDTH: f32 = editor_spike::VIEWPORT.0;
const HEIGHT: f32 = editor_spike::VIEWPORT.1;
/// Frames before a scenario starts, so the window has settled and every text it shows
/// has been measured once.
const SETTLE_FRAMES: usize = 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scenario {
    Idle,
    Page,
    Wheel,
    Type,
    None,
}

impl Scenario {
    fn from_env() -> Self {
        match std::env::var("EDITOR_SCENARIO").as_deref() {
            Ok("idle") => Scenario::Idle,
            Ok("page") => Scenario::Page,
            Ok("wheel") => Scenario::Wheel,
            Ok("type") => Scenario::Type,
            Ok("none") => Scenario::None,
            Ok(other) => panic!("EDITOR_SCENARIO={other} is not idle, page, wheel, type or none"),
            Err(_) => Scenario::Wheel,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Scenario::Idle => "idle",
            Scenario::Page => "page",
            Scenario::Wheel => "wheel",
            Scenario::Type => "type",
            Scenario::None => "none",
        }
    }

    /// How many frames the scenario plays.
    fn frames(self) -> usize {
        match self {
            Scenario::Idle => 120,
            Scenario::Page => 100,
            Scenario::Wheel => 300,
            Scenario::Type => 1000,
            Scenario::None => 0,
        }
    }
}

struct FrameRecord {
    start: Instant,
    frame_time: u64,
    host: u64,
    /// Whether a keystroke was handed to the page at the start of this frame.
    key: bool,
    /// Whether the frame belongs to the scenario, rather than settling or setting up.
    measured: bool,
}

struct Probe {
    inner: HtmlRuntime,
    scenario: Scenario,
    label: String,
    mode: ScrollMode,
    frames: Vec<FrameRecord>,
    /// Frames served since the window opened.
    served: usize,
    /// Keystrokes handed to the page so far.
    keys: usize,
    rebuild: Option<Duration>,
    /// `render` calls that answered an event rather than a frame, and how long each took.
    event_renders: Vec<u64>,
    handle_events: Vec<u64>,
    reported: bool,
}

impl Probe {
    fn new(inner: HtmlRuntime, scenario: Scenario, label: String, mode: ScrollMode) -> Self {
        Self {
            inner,
            scenario,
            label,
            mode,
            frames: Vec::with_capacity(SETTLE_FRAMES + scenario.frames() + 8),
            served: 0,
            keys: 0,
            rebuild: None,
            event_renders: Vec::new(),
            handle_events: Vec::new(),
            reported: false,
        }
    }

    fn last_frame(&self) -> usize {
        SETTLE_FRAMES + 1 + self.scenario.frames()
    }

    /// Whether the scenario still wants frames.
    fn running(&self) -> bool {
        self.scenario != Scenario::None && self.served <= self.last_frame()
    }

    /// Plays the scenario's step for the frame about to render. Returns whether a key was
    /// typed and whether the frame is one the scenario measures.
    fn step(&mut self) -> (bool, bool) {
        let frame = self.served;
        if frame < SETTLE_FRAMES || frame > self.last_frame() {
            return (false, false);
        }
        // The frame after settling sets the scenario up and is not measured.
        if frame == SETTLE_FRAMES {
            let scenario = self.scenario;
            self.inner.run_in_context(&mut || {
                drive(|editor| {
                    let middle = editor.lines.len() / 2;
                    match scenario {
                        Scenario::Page => editor.scroll_to(0.0, HEIGHT),
                        Scenario::Wheel => editor.jump_to(middle, 0, HEIGHT),
                        Scenario::Type => editor.jump_to(middle, 12, HEIGHT),
                        Scenario::Idle | Scenario::None => {}
                    }
                });
            });
            return (false, false);
        }
        let index = frame - SETTLE_FRAMES - 1;
        let scenario = self.scenario;
        let mut typed = false;
        match scenario {
            Scenario::Page => self.inner.run_in_context(&mut || {
                drive(|editor| editor.scroll_by(HEIGHT - editor_spike::LINE_HEIGHT, HEIGHT));
            }),
            Scenario::Wheel => self.inner.run_in_context(&mut || {
                drive(|editor| editor.scroll_by(12.0, HEIGHT));
            }),
            Scenario::Type if index % 2 == 0 => {
                let letter = key(self.keys);
                self.keys += 1;
                typed = true;
                self.inner.run_in_context(&mut || {
                    let mut buffer = [0u8; 4];
                    let text = letter.encode_utf8(&mut buffer);
                    drive(|editor| editor.insert(text, HEIGHT));
                });
            }
            _ => {}
        }
        (typed, true)
    }

    fn report(&mut self) {
        if self.reported {
            return;
        }
        self.reported = true;
        let mut host = Vec::new();
        let mut interval = Vec::new();
        let mut frame_time = Vec::new();
        let mut key_host = Vec::new();
        let mut key_to_frame = Vec::new();
        let mut settle_interval = Vec::new();
        let mut after_host = Vec::new();
        for pair in self.frames.windows(2) {
            let (this, next) = (&pair[0], &pair[1]);
            let gap = (next.start - this.start).as_nanos() as u64;
            if !this.measured {
                if this.host > 0 && next.start > this.start {
                    settle_interval.push(gap);
                }
                continue;
            }
            host.push(this.host);
            interval.push(gap);
            after_host.push(gap.saturating_sub(this.host));
            frame_time.push(next.frame_time.saturating_sub(this.frame_time));
            if this.key {
                key_host.push(this.host);
                key_to_frame.push(gap);
            }
        }
        let mut out = String::new();
        let _ = writeln!(
            out,
            "editor-spike window: {} | {} | scroll {} | frames served {} | rebuild {:.2} ms",
            self.scenario.name(),
            self.label,
            self.mode.name(),
            self.served,
            self.rebuild.map_or(0.0, |d| d.as_secs_f64() * 1e3)
        );
        let _ = writeln!(
            out,
            "| scenario | file | host render | frame interval | interval minus host | renderer frame clock | key: host | key to next frame |"
        );
        let _ = writeln!(out, "|---|---|---|---|---|---|---|---|");
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} | {} | {} | {} |",
            self.scenario.name(),
            self.label,
            Stats::of(&host).cell(),
            Stats::of(&interval).cell(),
            Stats::of(&after_host).cell(),
            Stats::of(&frame_time).cell(),
            Stats::of(&key_host).cell(),
            Stats::of(&key_to_frame).cell(),
        );
        let _ = writeln!(
            out,
            "settling frames: interval {} | event renders: {} | handle_event: {}",
            Stats::of(&settle_interval).cell(),
            Stats::of(&self.event_renders).cell(),
            Stats::of(&self.handle_events).cell(),
        );
        eprintln!("{out}");
        if let Ok(path) = std::env::var("EDITOR_SPIKE_OUT") {
            let _ = std::fs::write(&path, &out);
        }
    }
}

/// The letters the scenario types: words and spaces, and a new line every fifty keys.
fn key(index: usize) -> char {
    const TEXT: &[u8] = b"let value = compute(index, 42); ";
    if index % 50 == 49 {
        '\n'
    } else {
        TEXT[index % TEXT.len()] as char
    }
}

impl Runtime for Probe {
    fn batch(&self) -> &Batch {
        self.inner.batch()
    }

    fn batch_mut(&mut self) -> &mut Batch {
        self.inner.batch_mut()
    }

    fn rebuild(&mut self) {
        let started = Instant::now();
        self.inner.rebuild();
        self.rebuild = Some(started.elapsed());
    }

    fn render(&mut self, frame_time_nanos: Option<u64>) {
        let Some(frame_time) = frame_time_nanos else {
            // An event's render: the Host renders right after the handler ran.
            let started = Instant::now();
            self.inner.render(None);
            let spent = started.elapsed().as_nanos() as u64;
            self.event_renders.push(spent);
            if self.scenario == Scenario::None {
                eprintln!("editor-spike event: render {:.3} ms", spent as f64 / 1e6);
            }
            return;
        };
        let start = Instant::now();
        let (key, measured) = self.step();
        self.inner.render(Some(frame_time));
        let host = start.elapsed().as_nanos() as u64;
        self.frames.push(FrameRecord {
            start,
            frame_time,
            host,
            key,
            measured,
        });
        self.served += 1;
        if self.scenario != Scenario::None && self.served > self.last_frame() {
            self.report();
        }
    }

    fn handle_event(&mut self, event: &HostEvent<'_>) -> Result<i64, ProtocolError> {
        let started = Instant::now();
        let result = self.inner.handle_event(event);
        let spent = started.elapsed().as_nanos() as u64;
        self.handle_events.push(spent);
        // The renderer's own typing ends the process from its side, with no chance to
        // report at the end, so each event is reported as it happens.
        if self.scenario == Scenario::None {
            eprintln!(
                "editor-spike event: handle_event {:.3} ms (node {}, handler {})",
                spent as f64 / 1e6,
                event.node_id,
                event.handler_id
            );
        }
        result
    }

    fn size_token(&self, node_id: u32) -> Option<u32> {
        self.inner.size_token(node_id)
    }

    fn poll_work(&mut self, context: &mut Context<'_>) -> Poll<()> {
        let inner = self.inner.poll_work(context);
        if self.running() {
            return Poll::Ready(());
        }
        inner
    }

    fn run_in_context(&mut self, action: &mut dyn FnMut()) {
        self.inner.run_in_context(action);
    }
}

fn main() {
    let scenario = Scenario::from_env();
    let mode = match std::env::var("EDITOR_SCROLL_MODE").as_deref() {
        Ok("transform") => ScrollMode::Transform,
        _ => ScrollMode::Top,
    };
    let path = std::env::var("EDITOR_FILE").ok().filter(|path| !path.is_empty());
    let (lines, label) = load(path.as_deref(), 100_000);
    set_source(lines);
    set_options(Options {
        viewport: (WIDTH, HEIGHT),
        scroll_mode: mode,
    });
    eprintln!(
        "editor-spike window: starting {} on {label}, scroll {}",
        scenario.name(),
        mode.name()
    );

    // A run that the renderer does not end by itself (no `DXC_SYNTH=exit`) is ended here,
    // a while after the scenario, so a CI step never waits on an open window.
    if let Some(seconds) = std::env::var("EDITOR_EXIT_AFTER")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
    {
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(seconds));
            eprintln!("editor-spike window: ending the process after {seconds}s");
            std::process::exit(0);
        });
    }

    dioxus_compose::LaunchBuilder::new()
        .with_window(
            Window::new()
                .with_title("Editor spike")
                .with_size(WIDTH as u16, HEIGHT as u16),
        )
        .into_core()
        .launch_runtime(move || {
            Box::new(Probe::new(
                HtmlRuntime::with_config(editor_spike::app, editor_spike::config()),
                scenario,
                label.clone(),
                mode,
            )) as Box<dyn Runtime>
        });
}
