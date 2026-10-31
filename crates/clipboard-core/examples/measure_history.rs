//! Isolated, reproducible history-query baseline; never opens the system clipboard.
use anyhow::{Context, Result, ensure};
use clipboard_core::{
    ContentCategory, History, MAX_TEXT_BYTES,
    database::Database,
    preferences::{DedupStrategy, Preferences},
};
use serde_json::json;
use std::{hint::black_box, path::PathBuf, time::Instant};

const ROWS: usize = 10_000;
const SAMPLES: usize = 30;

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let destination = args.next().map(PathBuf::from);
    ensure!(
        args.next().is_none(),
        "usage: measure_history [new-data-directory]"
    );
    let temporary = if destination.is_none() {
        Some(tempfile::tempdir()?)
    } else {
        None
    };
    let directory = match destination {
        Some(path) => {
            // Refuse existing paths, including a user's existing clipboard database.
            std::fs::create_dir(&path).context("measurement directory must not already exist")?;
            path
        }
        None => temporary
            .as_ref()
            .context("temporary directory missing")?
            .path()
            .to_owned(),
    };
    let database = Database::new(directory.join("clipboard.db"))?;
    let history = History::new(&database);
    let preferences = Preferences::new(&database);
    preferences.set_dedup_strategy(DedupStrategy::AlwaysNew)?;
    preferences.set_hotkey(clipboard_core::preferences::HotkeyPreference::Disabled)?;
    preferences.set_onboarding_completed()?;

    let seed_started = Instant::now();
    for index in 0..ROWS {
        let mut text = format!("Audit sample {index:05}: deterministic clipboard dataset. ");
        if index % 10 == 0 {
            text.push_str("审计标记 ");
        }
        if index % 100 == 0 {
            text.push_str("audit-common ");
        }
        let bytes = if index % 1000 == 0 {
            MAX_TEXT_BYTES
        } else {
            256
        };
        text.extend(std::iter::repeat_n('x', bytes - text.len()));
        history
            .capture(&text)?
            .context("synthetic capture was unexpectedly rejected")?;
    }
    preferences.set_dedup_strategy(DedupStrategy::MoveToTop)?;
    let seed_ms = seed_started.elapsed().as_secs_f64() * 1000.0;
    ensure!(
        history.count("", false)? == ROWS as i64,
        "seed history count mismatch"
    );

    let mut results = Vec::new();
    for (label, search, expected) in [
        ("first_page", "", ROWS),
        ("ascii_search", "audit-common", ROWS / 100),
        ("unicode_search", "审计标记", ROWS / 10),
        ("missing_search", "not-present-in-this-dataset", 0),
    ] {
        for _ in 0..3 {
            black_box(query(&history, search, expected)?);
        }
        let mut durations = Vec::with_capacity(SAMPLES);
        for _ in 0..SAMPLES {
            let started = Instant::now();
            black_box(query(&history, search, expected)?);
            durations.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        durations.sort_by(f64::total_cmp);
        results.push(json!({
            "case": label,
            "matches": expected,
            "samples": SAMPLES,
            "p50_ms": durations[(SAMPLES * 50).div_ceil(100) - 1],
            "p95_ms": durations[(SAMPLES * 95).div_ceil(100) - 1],
            "max_ms": durations[SAMPLES - 1],
        }));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "rows": ROWS,
            "one_mib_text_rows": ROWS / 1000,
            "seed_ms": seed_ms,
            "directory": directory,
            "query_scope": "warm core list (100 rows) plus count; excludes UI debounce, worker scheduling and rendering",
            "results": results,
        }))?
    );
    drop(history);
    drop(preferences);
    drop(database);
    drop(temporary);
    Ok(())
}

fn query(history: &History, search: &str, expected: usize) -> Result<usize> {
    let items = history.list_filtered_in_group(search, 100, false, None, ContentCategory::All)?;
    let count = history.count_filtered_in_group(search, false, None, ContentCategory::All)?;
    ensure!(
        count == expected as i64,
        "query {search:?}: expected {expected}, got {count}"
    );
    ensure!(
        items.len() == expected.min(100),
        "query page count mismatch"
    );
    Ok(items
        .iter()
        .map(|item| item.preview.as_ref().map_or(0, String::len))
        .sum())
}
