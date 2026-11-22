//! The stats strip: the session's counts and rates, refreshed once a second.

use crate::design::parts::{self, Metric};
use crate::design::tokens::{SPACE_LG, SPACE_MD, SPACE_SM};
use crate::read::ShownStats;
use crate::state::AppState;
use chrono::{DateTime, TimeDelta, Utc};
use common_utils::string_utils::format_thousands;
use rune::{El, row};
use rune_kit::{Kit, Placement, Popover};
use shared_eventhub_core::stats::StatsSnapshot;

/// The partitions popover's scroll viewport.
pub const PARTITIONS_SCROLL_KEY: &str = "partitions-popover-scroll";

pub fn strip(s: &AppState, k: &Kit, layer: &mut Vec<El>) -> El {
    let fallback;
    let (stats, at, quiet) = match &s.read.shown {
        Some(ShownStats { stats, at, quiet }) => (stats, *at, *quiet),
        None => {
            fallback = StatsSnapshot::default();
            (&fallback, Utc::now(), false)
        }
    };
    let tiles = vec![
        received(s, k, stats, layer),
        tile(k, "stat-rate", "Rate", rate(stats.current_rate), 1),
        tile(k, "stat-stored", "Stored", count(stats.stored), 2),
        tile(
            k,
            "stat-duplicated",
            "Duplicated",
            count(stats.duplicated),
            3,
        ),
        tile(
            k,
            "stat-filtered",
            "Filtered out",
            count(stats.filtered_out),
            4,
        ),
        tile(k, "stat-peak", "Peak rate", rate(stats.peak_rate), 5),
        tile(
            k,
            "stat-last",
            "Last message",
            last_message(stats.last_message_at, at, quiet),
            6,
        ),
        tile(k, "stat-elapsed", "Elapsed", elapsed(stats.elapsed), 7),
    ];
    let mut strip = row()
        .key("stats-strip")
        .w_full()
        .flex_wrap()
        .gap(SPACE_SM)
        .px(SPACE_LG)
        .py(SPACE_MD)
        .children(tiles);
    if stats.ui_dropped > 0 {
        strip = strip.child(
            tile(
                k,
                "stat-ui-dropped",
                "UI dropped",
                Metric {
                    warning: true,
                    ..count(stats.ui_dropped)
                },
                8,
            )
            .help(
                "The list skipped some messages under load. The archive and the counts include \
                 them.",
            ),
        );
    }
    strip
}

fn tile(k: &Kit, key: &str, label: &str, value: Metric, order: usize) -> El {
    parts::stat_tile(k, key, label, value, order).grow(1.0)
}

/// The Received tile; pressing it shows the per-partition counts.
fn received(s: &AppState, k: &Kit, stats: &StatsSnapshot, layer: &mut Vec<El>) -> El {
    let face = parts::stat_tile(k, "stat-received", "Received", count(stats.received), 0)
        .role("button")
        .label(format!("Received {}", format_thousands(stats.received)))
        .on_press(|s: &mut AppState| s.read.partitions_open = !s.read.partitions_open);
    let rows = stats
        .per_partition
        .iter()
        .map(|(partition, progress)| {
            parts::detail_row(
                k,
                &format!("Partition {partition}"),
                &partition_line(progress.received, progress.last_received_at),
            )
        })
        .collect();
    Popover::new("partitions-popover")
        .trigger(face)
        .title("Received per partition")
        .padding(0.0)
        .child(parts::popover_body(k, PARTITIONS_SCROLL_KEY, rows))
        .open(s.read.partitions_open)
        .placement(Placement::Bottom)
        .on_close(|s: &mut AppState| s.read.partitions_open = false)
        .build(k, layer)
        .grow(1.0)
}

/// A partition's received count and last arrival: `12,345,678 · last 18:31:02 UTC`, or
/// `0 · nothing yet` before its first message. Short enough for a 296 pt row in any 12 pt mono.
fn partition_line(received: u64, last: Option<DateTime<Utc>>) -> String {
    let received = format_thousands(received);
    match last {
        Some(at) => format!("{received} · last {}", at.format("%H:%M:%S UTC")),
        None => format!("{received} · nothing yet"),
    }
}

fn count(value: u64) -> Metric {
    Metric {
        text: format_thousands(value),
        number: value as f64,
        warning: false,
    }
}

/// `312/s`, with one decimal under 10.
pub(crate) fn rate(per_second: f64) -> Metric {
    let text = if per_second < 10.0 {
        format!("{per_second:.1}/s")
    } else {
        format!("{}/s", format_thousands(per_second.round() as u64))
    };
    Metric {
        text,
        number: per_second,
        warning: false,
    }
}

/// How long ago the last message arrived, as of the tick at `now`: warning-colored while the
/// hub is quiet.
pub(crate) fn last_message(at: Option<DateTime<Utc>>, now: DateTime<Utc>, quiet: bool) -> Metric {
    let Some(at) = at else {
        return Metric {
            text: "None yet".to_owned(),
            number: 0.0,
            warning: quiet,
        };
    };
    let ago = (now - at).max(TimeDelta::zero());
    Metric {
        text: format!("{} ago", short_duration(ago.num_seconds())),
        number: ago.num_seconds() as f64,
        warning: quiet,
    }
}

fn elapsed(elapsed: std::time::Duration) -> Metric {
    let seconds = i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX);
    Metric {
        text: short_duration(seconds),
        number: seconds as f64,
        warning: false,
    }
}

/// `45s`, `2m 14s`, `3h 05m`.
pub(crate) fn short_duration(seconds: i64) -> String {
    let seconds = seconds.max(0);
    match (seconds / 3_600, (seconds % 3_600) / 60, seconds % 60) {
        (0, 0, s) => format!("{s}s"),
        (0, m, s) => format!("{m}m {s:02}s"),
        (h, m, _) => format!("{h}h {m:02}m"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[rstest::rstest]
    #[case::received(Some((18, 31, 2)), "12,345,678 · last 18:31:02 UTC")]
    #[case::nothing_yet(None, "12,345,678 · nothing yet")]
    fn a_partition_line_is_the_count_then_the_last_arrival(
        #[case] last: Option<(u32, u32, u32)>,
        #[case] expected: &str,
    ) {
        let last =
            last.map(|(h, m, s)| Utc.with_ymd_and_hms(2026, 1, 1, h, m, s).single().unwrap());

        let line = partition_line(12_345_678, last);

        assert_eq!(line, expected);
    }
}
