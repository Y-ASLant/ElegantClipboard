use clipboard_rs::{
	Clipboard, ClipboardContext, ClipboardHandler, ClipboardWatcher, ClipboardWatcherContext,
};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

struct CountingHandler {
	tx: mpsc::Sender<()>,
}

impl ClipboardHandler for CountingHandler {
	fn on_clipboard_change(&mut self) {
		// Ignore send errors: the receiver may already be gone on shutdown.
		let _ = self.tx.send(());
	}
}

// Native watcher tests require a desktop session and exclusive clipboard access.
#[test]
#[ignore = "requires exclusive access to the system clipboard; run with --ignored --test-threads=1"]
fn test_watch_with_short_interval_detects_change_and_shuts_down() {
	let (tx, rx) = mpsc::channel();

	// A short interval is the whole point of `new_with_interval`: detection
	// latency on the polling backends should track it rather than the 500ms
	// default. On event-driven backends the interval is accepted and ignored.
	let mut watcher =
		ClipboardWatcherContext::new_with_interval(Duration::from_millis(50)).unwrap();
	let shutdown = watcher
		.add_handler(CountingHandler { tx })
		.get_shutdown_channel();

	let watch_thread = thread::spawn(move || {
		watcher.start_watch();
	});

	// Give the watcher a moment to record the initial change count, then mutate
	// the clipboard so the handler fires.
	thread::sleep(Duration::from_millis(200));
	let ctx = ClipboardContext::new().unwrap();
	ctx.set_text("clipboard-rs watch interval test".to_string())
		.unwrap();

	// The change should be observed well within a second given the 50ms poll.
	let observed = rx.recv_timeout(Duration::from_secs(5));

	// Shutting down must unblock `start_watch` so the thread can join.
	shutdown.stop();
	watch_thread.join().expect("watch thread should join");
	observed.expect("watcher should observe the clipboard change");
}

// `new` must keep its original behavior and signature (back-compat).
#[test]
#[ignore = "requires a native desktop session; run with --ignored --test-threads=1"]
fn test_new_still_works_without_interval() {
	let _watcher: ClipboardWatcherContext<NoopHandler> = ClipboardWatcherContext::new().unwrap();
}

struct NoopHandler;
impl ClipboardHandler for NoopHandler {
	fn on_clipboard_change(&mut self) {}
}
