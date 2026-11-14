//! Rune spike: a toolbar with a segmented control over a live list. A background thread sends
//! 50,000 messages at 500 a second, in batches four times a second; the list keeps the newest
//! 5,000. It follows its tail while it sits at the bottom, lets go when scrolled up, and follows
//! again once scrolled back to the end.
//!
//! `RUNE_COMMIT_BUDGET_MS=8 cargo run --release -p eh-gui --example spike_tail` prints every
//! commit over 8 ms.

use std::borrow::Cow;
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

use rune::{App, AppHandle, AppSpec, Cx, El, GrowthSpec, ScrollSpec, col, oklch, row, text};
use rune_kit::{Kit, PaletteSeed, ScrollBar, SegmentedControl, Toolbar};

const TOTAL_MESSAGES: usize = 50_000;
/// The newest rows the live list keeps; a virtual list re-keys every row whenever its row set
/// changes, so this bounds the cost of each batch (about 5 ms in a release build).
const LIVE_ROWS: usize = 5_000;
const BATCH: usize = 125;
const BATCH_EVERY: Duration = Duration::from_millis(250);
const ROW_HEIGHT: f32 = 28.0;

struct Spike {
    kit: Kit,
    section: usize,
    /// Global index of the first row in the list.
    first: usize,
    rows: usize,
}

impl Spike {
    fn new() -> Spike {
        Spike {
            kit: Kit::from_palette(PaletteSeed {
                bg: oklch(0.21, 0.004, 286.0),
                fg: oklch(0.96, 0.002, 286.0),
                accent: oklch(0.64, 0.19, 258.0),
            }),
            section: 0,
            first: 0,
            rows: 0,
        }
    }

    /// Takes every message up to global index `sent`, keeping the newest [`LIVE_ROWS`].
    fn receive_up_to(&mut self, sent: usize) {
        self.first = sent.saturating_sub(LIVE_ROWS);
        self.rows = sent - self.first;
    }
}

fn message_row(s: &Spike, i: usize) -> El {
    let k = &s.kit;
    let n = s.first + i;
    row().w_full().h(ROW_HEIGHT).px(12.0).center_y().child(
        text(format!("Message {n:>6}  partition {}", n % 4))
            .font(k.ty.ui)
            .color(k.c.text_base),
    )
}

fn view(s: &Spike, _: &mut Cx<Spike>) -> El {
    let k = &s.kit;
    let sections = SegmentedControl::new(&["Read", "Export"], Some(s.section))
        .label("Section")
        .on_pick(|s: &mut Spike, i, _| s.section = i)
        .build(k);
    let toolbar = Toolbar::new().center(sections).build(k);
    let list = col()
        .key("rows")
        .w_full()
        .growth(GrowthSpec::default())
        .virtual_rows_version(s.first)
        .virtual_rows(
            s.rows,
            ROW_HEIGHT,
            |s: &Spike, i| Cow::Owned(format!("m{}", s.first + i)),
            message_row,
        );
    let viewport = col()
        .key("list")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec { stick_bottom: true })
        .child(list)
        .scroll_bar(k);
    col()
        .key("window")
        .w_full()
        .h_full()
        .fill(k.c.bg_base)
        .child(toolbar)
        .child(viewport)
}

fn feed(handle: AppHandle<Spike>) {
    let mut sent = 0;
    while sent < TOTAL_MESSAGES {
        thread::sleep(BATCH_EVERY);
        sent = (sent + BATCH).min(TOTAL_MESSAGES);
        handle.update_if(move |s, tx| {
            tx.instant();
            s.receive_up_to(sent);
            true
        });
    }
}

fn main() -> ExitCode {
    let app = App::new(AppSpec::new(
        "eh-gui spike",
        (1280.0, 800.0),
        Spike::new,
        view,
    ));
    let handle = app.handle();
    thread::spawn(move || feed(handle));
    match app.try_run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("spike_tail: the window could not start: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime};

    fn runtime_with(sent: usize) -> Runtime<Spike> {
        let mut state = Spike::new();
        state.receive_up_to(sent);
        let mut rt = Runtime::new(state, view, (1280.0, 800.0), Env::new(true, 2.0));
        rt.frame(0.0);
        for _ in 0..10 {
            rt.frame(1.0 / 60.0);
        }
        rt
    }

    #[test]
    fn pressing_export_switches_the_section_headlessly() {
        let mut rt = runtime_with(0);

        let (x, y) = rt.find("segment1").unwrap();
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);

        assert_eq!(rt.state.section, 1);
        assert!(
            rt.elements()
                .iter()
                .any(|e| e.key.as_deref() == Some("toolbar"))
        );
    }

    #[test]
    fn a_full_live_list_mounts_only_a_window_of_rows() {
        let rt = runtime_with(TOTAL_MESSAGES);

        let mounted = rt
            .elements()
            .iter()
            .filter(|e| e.key.as_deref().is_some_and(|k| k.starts_with('m')))
            .count();
        assert!(mounted > 0 && mounted < 1_000, "mounted {mounted} rows");
    }

    #[test]
    fn the_live_list_keeps_only_the_newest_rows() {
        let mut state = Spike::new();

        state.receive_up_to(TOTAL_MESSAGES);

        assert_eq!(state.rows, LIVE_ROWS);
        assert_eq!(state.first, TOTAL_MESSAGES - LIVE_ROWS);
    }

    #[test]
    #[ignore = "timing probe, run by hand in release with --ignored --nocapture"]
    fn probe_batch_commit_cost_at_the_cap() {
        let mut rt = runtime_with(TOTAL_MESSAGES - 20 * BATCH);
        let mut costs = Vec::new();
        for step in 1..=20 {
            rt.state
                .receive_up_to(TOTAL_MESSAGES - 20 * BATCH + step * BATCH);
            rt.invalidate();
            let started = std::time::Instant::now();
            rt.frame(1.0 / 60.0);
            costs.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        costs.sort_by(f64::total_cmp);
        eprintln!(
            "batch commit at {LIVE_ROWS} rows: median {:.2} ms, worst {:.2} ms",
            costs[costs.len() / 2],
            costs[costs.len() - 1]
        );
    }
}
