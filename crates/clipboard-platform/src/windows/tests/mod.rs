use super::{Command, Event, Service};
use anyhow::{Result, bail};
use clipboard_core::{
    ContentCategory, History, PAGE_SIZE,
    database::{ClipboardItem, Database},
};
use std::{thread, time::Duration};

mod capture;
mod clipboard;
mod data;
mod delivery;
mod groups;
mod history;
mod lifecycle;
mod preferences;
mod preview;

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
        if let Ok(event) = events.try_recv()
            && let Some(value) = match_event(event)
        {
            return value;
        }
        assert!(std::time::Instant::now() < deadline, "{timeout_message}");
        thread::sleep(Duration::from_millis(10));
    }
}
