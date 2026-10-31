use super::*;
use crate::source_app;
use std::time::{Duration, Instant};

fn text_capture(value: &str) -> CapturedClipboard {
    CapturedClipboard::Text(value.to_owned())
}

fn entry(sequence: u32, value: &str, now: Instant) -> PendingCapture {
    new_pending(
        sequence,
        text_capture(value),
        Vec::new(),
        vec!["original/native".into()],
        Some(source_app::SourceApp {
            name: "Test".into(),
            executable: None,
        }),
        now,
    )
}

fn delivered(outcomes: &[(PendingCapture, PollOutcome)]) -> usize {
    outcomes
        .iter()
        .filter(|(_, outcome)| matches!(outcome, PollOutcome::Deliver))
        .count()
}

#[test]
fn disabled_window_delivers_immediately() {
    let mut window = TransientWindow::new(false);
    let now = Instant::now();
    let immediate = window.push(entry(1, "text", now));
    assert_eq!(immediate.len(), 1);
    assert!(window.is_empty());
}

#[test]
fn due_entry_with_unchanged_sequence_is_delivered() {
    let mut window = TransientWindow::new(true);
    let now = Instant::now();
    assert!(window.push(entry(7, "stable", now)).is_empty());
    let results = window.poll_due(now + TRANSIENT_WINDOW, 0, &mut || {
        Ok((7, Some(text_capture("stable")), Vec::new()))
    });
    assert_eq!(delivered(&results), 1);
    assert_eq!(results[0].0.formats, ["original/native"]);
}

#[test]
fn unchanged_sequence_adopts_format_names_from_the_confirmation_read() {
    let mut window = TransientWindow::new(true);
    let now = Instant::now();
    let mut pending = entry(7, "stable", now);
    pending.formats.clear();
    assert!(window.push(pending).is_empty());
    let results = window.poll_due(now + TRANSIENT_WINDOW, 0, &mut || {
        Ok((7, Some(text_capture("stable")), vec!["CF_HDROP".into()]))
    });
    assert_eq!(delivered(&results), 1);
    assert_eq!(results[0].0.formats, ["CF_HDROP"]);
}

#[test]
fn replaced_by_empty_content_is_discarded() {
    let mut window = TransientWindow::new(true);
    let now = Instant::now();
    window.push(entry(7, "flash", now));
    // 序列号变化且当前剪贴板为空（划词工具还原）。
    let results = window.poll_due(now + TRANSIENT_WINDOW, 0, &mut || Ok((8, None, Vec::new())));
    assert_eq!(delivered(&results), 0);
}

#[test]
fn replaced_by_different_content_is_discarded() {
    let mut window = TransientWindow::new(true);
    let now = Instant::now();
    window.push(entry(7, "old", now));
    let results = window.poll_due(now + TRANSIENT_WINDOW, 0, &mut || {
        Ok((8, Some(text_capture("new content")), Vec::new()))
    });
    assert_eq!(delivered(&results), 0);
}

#[test]
fn replaced_by_identical_content_uses_latest_formats_even_when_empty() {
    for formats in [vec!["replacement/native".into()], Vec::new()] {
        let mut window = TransientWindow::new(true);
        let now = Instant::now();
        window.push(entry(7, "same", now));
        let results = window.poll_due(now + TRANSIENT_WINDOW, 0, &mut || {
            Ok((8, Some(text_capture("same")), formats.clone()))
        });
        assert_eq!(delivered(&results), 1);
        assert_eq!(results[0].0.formats, formats);
    }
}

#[test]
fn replaced_by_own_paste_is_still_delivered() {
    let mut window = TransientWindow::new(true);
    let now = Instant::now();
    window.push(entry(7, "user copy", now));
    // 序列号 9 是自身写回（ignored_sequence=9）。
    let results = window.poll_due(now + TRANSIENT_WINDOW, 9, &mut || {
        Ok((
            9,
            Some(text_capture("pasted item")),
            vec!["own/paste".into()],
        ))
    });
    assert_eq!(delivered(&results), 1);
    assert_eq!(results[0].0.formats, ["original/native"]);
}

#[test]
fn repeated_read_failures_eventually_discard() {
    let mut window = TransientWindow::new(true);
    let now = Instant::now();
    window.push(entry(7, "flaky", now));
    for round in 0..3u64 {
        let results = window.poll_due(
            now + TRANSIENT_WINDOW + Duration::from_millis(250 * round),
            0,
            &mut || Err(anyhow::anyhow!("clipboard busy")),
        );
        if round < 2 {
            assert!(results.is_empty(), "前两轮应顺延重试");
        } else {
            assert_eq!(delivered(&results), 0, "第三次失败后放弃");
        }
    }
    assert!(window.is_empty());
}

#[test]
fn overflow_flushes_oldest_entry() {
    let mut window = TransientWindow::new(true);
    let now = Instant::now();
    for sequence in 1..=MAX_PENDING as u32 {
        assert!(window.push(entry(sequence, "fill", now)).is_empty());
    }
    let flushed = window.push(entry(99, "newest", now));
    assert_eq!(flushed.len(), 1, "最旧条目应被挤出立即入库");
    assert_eq!(flushed[0].sequence, 1);
    assert_eq!(flushed[0].formats, ["original/native"]);
    assert_eq!(window.next_deadline().unwrap(), now + TRANSIENT_WINDOW);
}

#[test]
fn queued_captures_keep_their_own_sequence_and_only_the_final_content_survives() {
    let now = Instant::now();
    let mut window = TransientWindow::new(true);
    window.push(entry(101, "brief", now));
    window.push(entry(102, "final", now + Duration::from_millis(100)));
    let outcomes = window.poll_due(
        now + TRANSIENT_WINDOW + Duration::from_millis(100),
        0,
        &mut || Ok((102, Some(text_capture("final")), Vec::new())),
    );
    assert_eq!(outcomes.len(), 2);
    assert!(matches!(outcomes[0].1, PollOutcome::Discard));
    assert!(matches!(outcomes[1].1, PollOutcome::Deliver));
    assert_eq!(outcomes[1].0.sequence, 102);
    assert_eq!(outcomes[1].0.content, text_capture("final"));
}

#[test]
fn drain_clears_all_pending() {
    let mut window = TransientWindow::new(true);
    let now = Instant::now();
    window.push(entry(1, "a", now));
    window.push(entry(2, "b", now));
    window.drain();
    assert!(window.is_empty());
}
