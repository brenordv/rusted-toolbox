//! Rune spike: a toolbar with a segmented control over a virtual list that a background
//! thread grows to 50,000 rows at 500 rows a second. The list follows its tail while it sits
//! at the bottom and lets go when scrolled up.
//!
//! `RUNE_COMMIT_BUDGET_MS=8 cargo run -p eh-gui --example spike_tail` prints every commit over
//! 8 ms.

use std::borrow::Cow;
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

use rune::{App, AppHandle, AppSpec, Cx, El, GrowthSpec, ScrollSpec, col, oklch, row, text};
use rune_kit::{Kit, PaletteSeed, SegmentedControl, Toolbar};

const TOTAL_ROWS: usize = 50_000;
const ROWS_PER_TICK: usize = 50;
const TICK: Duration = Duration::from_millis(100);
const ROW_HEIGHT: f32 = 28.0;

struct Spike {
    kit: Kit,
    section: usize,
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
            rows: 0,
        }
    }
}

fn message_row(s: &Spike, i: usize) -> El {
    let k = &s.kit;
    row().w_full().h(ROW_HEIGHT).px(12.0).center_y().child(
        text(format!("Message {i:>6}  partition {}", i % 4))
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
        .virtual_rows_version(0u8)
        .virtual_rows(
            s.rows,
            ROW_HEIGHT,
            |_: &Spike, i| Cow::Owned(format!("m{i}")),
            message_row,
        );
    let viewport = col()
        .key("list")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec { stick_bottom: true })
        .child(list);
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
    while sent < TOTAL_ROWS {
        thread::sleep(TICK);
        sent = (sent + ROWS_PER_TICK).min(TOTAL_ROWS);
        handle.update_if(move |s, _| {
            s.rows = sent;
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

    #[test]
    fn pressing_export_switches_the_section_headlessly() {
        let mut rt = Runtime::new(Spike::new(), view, (1280.0, 800.0), Env::new(true, 2.0));
        rt.frame(0.0);

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
    fn a_long_list_mounts_only_a_window_of_rows() {
        let mut state = Spike::new();
        state.rows = TOTAL_ROWS;
        let mut rt = Runtime::new(state, view, (1280.0, 800.0), Env::new(true, 2.0));

        rt.frame(0.0);
        for _ in 0..10 {
            rt.frame(1.0 / 60.0);
        }

        let mounted = rt
            .elements()
            .iter()
            .filter(|e| e.key.as_deref().is_some_and(|k| k.starts_with('m')))
            .count();
        assert!(mounted > 0 && mounted < 1_000, "mounted {mounted} rows");
    }
}
