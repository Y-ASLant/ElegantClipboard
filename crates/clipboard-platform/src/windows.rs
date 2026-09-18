use ::windows::Win32::System::DataExchange::GetClipboardSequenceNumber;
use anyhow::{Context, Result, anyhow, bail};
use clipboard_core::{
    History, MAX_TEXT_BYTES, PAGE_SIZE,
    database::{ClipboardItem, Database},
    preferences::{Preferences, ThemePreference},
};
use clipboard_rs::{
    Clipboard, ClipboardContext, ClipboardHandler, ClipboardWatcher, ClipboardWatcherContext,
    ContentFormat, WatcherShutdown,
};
use directories::ProjectDirs;
use std::{
    fs::{File, OpenOptions},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

pub enum Command {
    Query {
        search: String,
        favorite_only: bool,
        limit: i64,
        generation: u64,
    },
    Copy(i64),
    Preview {
        id: i64,
        generation: u64,
    },
    Delete(i64),
    TogglePin(i64),
    ToggleFavorite(i64),
    Pause(bool),
    SetTheme(ThemePreference),
    Reorder {
        from: i64,
        to: i64,
        after: bool,
        favorite_only: bool,
        generation: u64,
    },
    Capture(String),
}

pub enum Event {
    Snapshot {
        items: Vec<ClipboardItem>,
        total: i64,
        generation: u64,
    },
    Status(String),
    Reordered {
        from: i64,
        generation: u64,
        result: Result<(), String>,
    },
    Preview {
        id: i64,
        generation: u64,
        result: Result<String, String>,
    },
    Paused(bool),
    ThemeSaved(Result<ThemePreference, String>),
    Error(String),
}

#[derive(Default)]
struct CaptureState {
    paused: bool,
    ignored_sequence: u32,
    last_sequence: u32,
}

struct WatchHandler {
    clipboard: ClipboardContext,
    state: Arc<Mutex<CaptureState>>,
    commands: SyncSender<Command>,
    events: async_channel::Sender<Event>,
}

impl ClipboardHandler for WatchHandler {
    fn on_clipboard_change(&mut self) {
        let result = self.read_change();
        if let Err(error) = result {
            let _ = self.events.try_send(Event::Error(error.to_string()));
        }
    }
}

impl WatchHandler {
    fn read_change(&mut self) -> Result<()> {
        // Serialize our writes with capture so their sequence is suppressed before
        // a delayed WM_CLIPBOARDUPDATE callback can observe the new text.
        let mut state = self.state.lock().map_err(|_| anyhow!("剪贴板状态异常"))?;
        let sequence = unsafe { GetClipboardSequenceNumber() };
        if state.paused || sequence == state.ignored_sequence || sequence == state.last_sequence {
            return Ok(());
        }
        if !self.clipboard.has(ContentFormat::Text) {
            state.last_sequence = sequence;
            return Ok(());
        }
        let text = self
            .clipboard
            .get_text()
            .map_err(|error| anyhow!("读取剪贴板失败：{error}"))?;
        if text.len() > MAX_TEXT_BYTES {
            state.last_sequence = sequence;
            bail!("文本超过 1 MiB，未保存");
        }
        // Do not silently replace different rapid copy operations with the last one.
        self.commands
            .try_send(Command::Capture(text))
            .map_err(|_| anyhow!("采集队列已满或已停止，本次复制未保存"))?;
        state.last_sequence = sequence;
        Ok(())
    }
}

pub struct Service {
    commands: SyncSender<Command>,
    stop: Arc<AtomicBool>,
    shutdown: Option<WatcherShutdown>,
    watcher: Option<JoinHandle<()>>,
    worker: Option<JoinHandle<()>>,
    events: async_channel::Sender<Event>,
    _instance_lock: File,
    pub data_dir: PathBuf,
    pub initial_theme: ThemePreference,
}

impl Service {
    pub fn start(
        data_dir: Option<PathBuf>,
        monitor: bool,
    ) -> Result<(Self, async_channel::Receiver<Event>)> {
        let data_dir = match data_dir {
            Some(path) => path,
            None => ProjectDirs::from("com", "ASLant", "ElegantClipboard-GPUI")
                .context("无法确定用户数据目录")?
                .data_local_dir()
                .to_owned(),
        };
        std::fs::create_dir_all(&data_dir).context("无法创建数据目录")?;
        let instance_lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(data_dir.join("instance.lock"))?;
        instance_lock
            .try_lock()
            .context("该数据目录已由另一个基础版实例使用")?;
        // Startup happens before entering the GUI event loop; subsequent DB work is
        // exclusively owned by the worker thread.
        let db = Database::new(data_dir.join("clipboard.db"))?;
        let history = History::new(&db);
        let preferences = Preferences::new(&db);
        let initial_theme = preferences.theme()?;
        let writer =
            ClipboardContext::new().map_err(|error| anyhow!("初始化剪贴板失败：{error}"))?;
        let (commands, incoming) = mpsc::sync_channel(64);
        let (events, outgoing) = async_channel::bounded(64);
        let state = Arc::new(Mutex::new(CaptureState::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let mut service = Self {
            commands: commands.clone(),
            stop: stop.clone(),
            shutdown: None,
            watcher: None,
            worker: None,
            events: events.clone(),
            _instance_lock: instance_lock,
            initial_theme,
            data_dir,
        };
        let worker_state = state.clone();
        let worker_events = events.clone();
        service.worker = Some(thread::Builder::new().name("history-worker".into()).spawn(
            move || {
                let mut worker = Worker {
                    history,
                    preferences,
                    clipboard: writer,
                    state: worker_state,
                    search: String::new(),
                    favorite_only: false,
                    limit: PAGE_SIZE,
                    generation: 0,
                    events: worker_events,
                };
                if let Err(error) = worker.snapshot() {
                    let _ = worker.events.try_send(Event::Error(error.to_string()));
                }
                while !stop.load(Ordering::Acquire) {
                    match incoming.recv_timeout(Duration::from_millis(250)) {
                        Ok(command) => {
                            if let Err(error) = worker.handle(command) {
                                let _ = worker.events.try_send(Event::Error(error.to_string()));
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            },
        )?);
        if monitor {
            let clipboard =
                ClipboardContext::new().map_err(|error| anyhow!("初始化监听失败：{error}"))?;
            let mut watcher = ClipboardWatcherContext::new()
                .map_err(|error| anyhow!("初始化监听失败：{error}"))?;
            watcher.add_handler(WatchHandler {
                clipboard,
                state,
                commands,
                events: events.clone(),
            });
            service.shutdown = Some(watcher.get_shutdown_channel());
            let stop = service.stop.clone();
            service.watcher = Some(
                thread::Builder::new()
                    .name("clipboard-watcher".into())
                    .spawn(move || {
                        watcher.start_watch();
                        if !stop.load(Ordering::Acquire) {
                            let _ = events
                                .try_send(Event::Error("剪贴板监听已停止，请重启应用".into()));
                        }
                    })?,
            );
        }
        Ok((service, outgoing))
    }

    pub fn send(&self, command: Command) -> Result<()> {
        self.commands
            .try_send(command)
            .map_err(|_| anyhow!("后台任务繁忙或已停止，请稍后重试"))
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        // Release a worker waiting for a slow/closed UI before joining it.
        self.events.close();
        if let Some(shutdown) = self.shutdown.take() {
            shutdown.stop();
        }
        if let Some(watcher) = self.watcher.take() {
            let _ = watcher.join();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct Worker {
    history: History,
    preferences: Preferences,
    clipboard: ClipboardContext,
    state: Arc<Mutex<CaptureState>>,
    search: String,
    favorite_only: bool,
    limit: i64,
    generation: u64,
    events: async_channel::Sender<Event>,
}

impl Worker {
    fn snapshot(&self) -> Result<()> {
        self.events
            .send_blocking(Event::Snapshot {
                items: self
                    .history
                    .list(&self.search, self.limit, self.favorite_only)?,
                total: self.history.count(&self.search, self.favorite_only)?,
                generation: self.generation,
            })
            .map_err(|_| anyhow!("窗口已关闭"))
    }

    fn handle(&mut self, command: Command) -> Result<()> {
        match command {
            Command::Query {
                search,
                favorite_only,
                limit,
                generation,
            } => {
                self.search = search;
                self.favorite_only = favorite_only;
                self.limit = limit;
                self.generation = generation;
            }
            Command::Capture(text) => {
                self.history.capture(&text)?;
            }
            Command::Copy(id) => {
                let text = self.history.text(id)?;
                let mut state = self.state.lock().map_err(|_| anyhow!("剪贴板状态异常"))?;
                self.clipboard
                    .set_text(text)
                    .map_err(|error| anyhow!("复制失败：{error}"))?;
                state.ignored_sequence = unsafe { GetClipboardSequenceNumber() };
                let _ = self.events.try_send(Event::Status(
                    "已复制，可切换到目标应用按 Ctrl+V 粘贴".into(),
                ));
                return Ok(());
            }
            Command::Preview { id, generation } => {
                self.events
                    .send_blocking(Event::Preview {
                        id,
                        generation,
                        result: self.history.text(id).map_err(|error| error.to_string()),
                    })
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::Delete(id) => self.history.delete(id)?,
            Command::ToggleFavorite(id) => {
                self.history.toggle_favorite(id)?;
            }
            Command::TogglePin(id) => {
                self.history.toggle_pin(id)?;
            }
            Command::Reorder {
                from,
                to,
                after,
                favorite_only,
                generation,
            } => {
                let result = if generation != self.generation || favorite_only != self.favorite_only
                {
                    Err(anyhow!("列表已切换，请重新拖动"))
                } else {
                    self.history.reorder(from, to, after, favorite_only)
                };
                if result.is_ok() {
                    self.snapshot()?;
                }
                self.events
                    .send_blocking(Event::Reordered {
                        from,
                        generation,
                        result: result.map_err(|error| error.to_string()),
                    })
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::SetTheme(theme) => {
                let result = self
                    .preferences
                    .set_theme(theme)
                    .map(|_| theme)
                    .map_err(|error| error.to_string());
                self.events
                    .send_blocking(Event::ThemeSaved(result))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::Pause(paused) => {
                let mut state = self.state.lock().map_err(|_| anyhow!("剪贴板状态异常"))?;
                state.paused = paused;
                state.last_sequence = unsafe { GetClipboardSequenceNumber() };
                drop(state);
                self.events
                    .send_blocking(Event::Paused(paused))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
        }
        self.snapshot()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn next_snapshot(
        events: &async_channel::Receiver<Event>,
        generation: u64,
    ) -> Vec<ClipboardItem> {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            match events.try_recv() {
                Ok(Event::Snapshot {
                    items,
                    generation: actual,
                    ..
                }) if actual == generation => return items,
                Ok(Event::Error(message)) => panic!("{message}"),
                _ => {}
            }
            assert!(
                std::time::Instant::now() < deadline,
                "worker did not respond"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn worker_serializes_capture_search_and_delete_without_touching_clipboard() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        next_snapshot(&events, 0);
        service.send(Command::Capture("中文%_ test".into()))?;
        service.send(Command::Capture("another record".into()))?;
        service.send(Command::Query {
            search: "%_".into(),
            favorite_only: false,
            limit: PAGE_SIZE,
            generation: 1,
        })?;
        let rows = next_snapshot(&events, 1);
        assert_eq!(rows.len(), 1);
        service.send(Command::Delete(rows[0].id))?;
        service.send(Command::Query {
            search: "".into(),
            favorite_only: false,
            limit: PAGE_SIZE,
            generation: 2,
        })?;
        assert_eq!(next_snapshot(&events, 2).len(), 1);
        drop(service);
        let (reopened, events) = Service::start(Some(directory.path().to_owned()), false)?;
        assert_eq!(next_snapshot(&events, 0).len(), 1);
        drop(reopened);
        Ok(())
    }

    #[test]
    fn reorder_acknowledges_success_and_rejects_stale_view() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        next_snapshot(&events, 0);
        service.send(Command::Capture("first".into()))?;
        let first = next_snapshot(&events, 0)[0].id;
        service.send(Command::Capture("second".into()))?;
        let second = next_snapshot(&events, 0)[0].id;
        for generation in [0, 99] {
            service.send(Command::Reorder {
                from: first,
                to: second,
                after: false,
                favorite_only: false,
                generation,
            })?;
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                match events.try_recv() {
                    Ok(Event::Reordered {
                        from,
                        generation: actual,
                        result,
                    }) => {
                        assert_eq!((from, actual), (first, generation));
                        assert_eq!(result.is_ok(), generation == 0);
                        break;
                    }
                    Ok(Event::Error(message)) => panic!("{message}"),
                    _ => {}
                }
                assert!(std::time::Instant::now() < deadline);
                thread::sleep(Duration::from_millis(10));
            }
        }
        drop(service);
        let (_service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        assert_eq!(next_snapshot(&events, 0)[0].id, first);
        Ok(())
    }

    #[test]
    fn theme_acknowledgement_is_persisted_before_reopening() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        assert_eq!(service.initial_theme, ThemePreference::System);
        next_snapshot(&events, 0);
        for theme in [
            ThemePreference::Light,
            ThemePreference::System,
            ThemePreference::Dark,
        ] {
            service.send(Command::SetTheme(theme))?;
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                match events.try_recv() {
                    Ok(Event::ThemeSaved(result)) => {
                        assert_eq!(result.unwrap(), theme);
                        break;
                    }
                    Ok(Event::Error(message)) => panic!("{message}"),
                    _ => {}
                }
                assert!(std::time::Instant::now() < deadline, "theme timed out");
                thread::sleep(Duration::from_millis(10));
            }
        }
        drop(service);
        let (reopened, events) = Service::start(Some(directory.path().to_owned()), false)?;
        assert_eq!(reopened.initial_theme, ThemePreference::Dark);
        assert!(next_snapshot(&events, 0).is_empty());
        Ok(())
    }

    #[test]
    fn worker_keeps_favorite_filter_during_capture_and_toggle() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        next_snapshot(&events, 0);
        service.send(Command::Capture("favorite %_".into()))?;
        let id = next_snapshot(&events, 0)[0].id;
        service.send(Command::ToggleFavorite(id))?;
        assert!(next_snapshot(&events, 0)[0].is_favorite);
        service.send(Command::Query {
            search: "%_".into(),
            favorite_only: true,
            limit: PAGE_SIZE,
            generation: 1,
        })?;
        assert_eq!(next_snapshot(&events, 1).len(), 1);
        service.send(Command::Capture("ordinary %_".into()))?;
        assert_eq!(next_snapshot(&events, 1)[0].id, id);
        service.send(Command::ToggleFavorite(id))?;
        assert!(next_snapshot(&events, 1).is_empty());
        service.send(Command::Query {
            search: "%_".into(),
            favorite_only: false,
            limit: PAGE_SIZE,
            generation: 2,
        })?;
        assert_eq!(next_snapshot(&events, 2).len(), 2);
        Ok(())
    }

    #[test]
    fn instance_lock_and_shutdown_work_even_with_a_full_event_queue() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (service, _events) = Service::start(Some(directory.path().to_owned()), false)?;
        assert!(Service::start(Some(directory.path().to_owned()), false).is_err());
        for generation in 1..=160 {
            // Deliberately create UI backpressure. A full command queue is expected.
            let _ = service.send(Command::Query {
                search: "".into(),
                favorite_only: false,
                limit: PAGE_SIZE,
                generation,
            });
            thread::sleep(Duration::from_millis(1));
        }
        drop(service);
        let (service, _) = Service::start(Some(directory.path().to_owned()), false)?;
        drop(service);
        Ok(())
    }

    #[test]
    fn preview_returns_full_text_and_reports_deleted_records() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        next_snapshot(&events, 0);
        let text = format!("中文😀\r\n{}\n末尾", "long text ".repeat(600));
        service.send(Command::Capture(text.clone()))?;
        let rows = next_snapshot(&events, 0);
        let id = rows[0].id;
        assert!(rows[0].text_content.is_none());
        for generation in 1..=2 {
            if generation == 2 {
                service.send(Command::Delete(id))?;
                assert!(next_snapshot(&events, 0).is_empty());
            }
            service.send(Command::Preview { id, generation })?;
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                match events.try_recv() {
                    Ok(Event::Preview {
                        id: actual,
                        generation: request,
                        result,
                    }) => {
                        assert_eq!((actual, request), (id, generation));
                        if generation == 1 {
                            assert_eq!(result.unwrap(), text);
                        } else {
                            assert!(result.is_err());
                        }
                        break;
                    }
                    Ok(Event::Error(message)) => panic!("{message}"),
                    _ => {}
                }
                assert!(std::time::Instant::now() < deadline, "preview timed out");
                thread::sleep(Duration::from_millis(10));
            }
        }
        Ok(())
    }

    #[test]
    fn pause_acknowledgement_is_ordered_and_does_not_change_history() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        next_snapshot(&events, 0);
        for paused in [true, false] {
            service.send(Command::Pause(paused))?;
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                match events.try_recv() {
                    Ok(Event::Paused(actual)) => {
                        assert_eq!(actual, paused);
                        break;
                    }
                    Ok(Event::Error(message)) => panic!("{message}"),
                    _ => {}
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "pause was not acknowledged"
                );
                thread::sleep(Duration::from_millis(10));
            }
        }
        service.send(Command::Query {
            search: String::new(),
            favorite_only: false,
            limit: PAGE_SIZE,
            generation: 1,
        })?;
        assert!(next_snapshot(&events, 1).is_empty());
        Ok(())
    }
}
