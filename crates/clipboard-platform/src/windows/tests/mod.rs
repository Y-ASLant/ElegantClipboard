use super::{Command, Event, Service};
use anyhow::{Result, bail};
use clipboard_core::{
    ContentCategory, History, PAGE_SIZE,
    database::{ClipboardItem, Database},
};
use std::{thread, time::Duration};

mod appearance;
mod capture;
mod clipboard;
mod data;
mod delivery;
mod groups;
mod history;
mod lifecycle;
mod preferences;
mod preview;
mod search;

fn next_snapshot(events: &async_channel::Receiver<Event>, generation: u64) -> Vec<ClipboardItem> {
    wait_for(
        events,
        Duration::from_secs(5),
        "worker did not respond",
        |event| match event {
            Event::Snapshot {
                items,
                generation: actual,
                ..
            } if actual == generation => Some(items),
            Event::Error(message) => panic!("{message}"),
            Event::CommandFailed { message, .. } | Event::BackgroundError(message) => {
                panic!("{message}")
            }
            _ => None,
        },
    )
}

/// Returns the very next event without skipping unexpected replies.
fn next_event(events: &async_channel::Receiver<Event>) -> Event {
    wait_for(
        events,
        Duration::from_secs(5),
        "worker did not send the next event",
        Some,
    )
}

/// Polls the event stream until `match_event` returns `Some`, discarding
/// unrelated events. Panics with `timeout_message` when nothing matches.
fn wait_for<T>(
    events: &async_channel::Receiver<Event>,
    timeout: Duration,
    timeout_message: &str,
    mut match_event: impl FnMut(Event) -> Option<T>,
) -> T {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match events.try_recv() {
            Ok(event) => {
                if let Some(value) = match_event(event) {
                    return value;
                }
            }
            Err(async_channel::TryRecvError::Closed) => {
                panic!("event stream closed: {timeout_message}");
            }
            Err(async_channel::TryRecvError::Empty) => {}
        }
        assert!(std::time::Instant::now() < deadline, "{timeout_message}");
        thread::sleep(Duration::from_millis(10));
    }
}
