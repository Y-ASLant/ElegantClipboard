use super::{
    Command, Event,
    capture::{CaptureState, WatchHandler},
    data::{lock_instance, resolve_data_dir},
    queue::{CaptureFeedback, CommandQueue},
    worker::Worker,
};
use crate::instance::InstanceSignal;
use ::windows::Win32::System::DataExchange::GetClipboardSequenceNumber;
use anyhow::{Result, anyhow};
use clipboard_core::{
    ContentCategory, History, PAGE_SIZE,
    database::Database,
    preferences::{
        AppFilterPreference, AudioPreference, DisplayPreference, HotkeyPreference,
        HoverPreviewPreference, LanguagePreference, MonitorTypesPreference, PasteKeyPreference,
        PasteShortcutConfig, Preferences, ThemePreference, WindowEffectPreference,
        WindowPositionPreference, WindowSizePreference,
    },
};
use clipboard_rs::{ClipboardContext, ClipboardWatcher, ClipboardWatcherContext, WatcherShutdown};
use std::{
    fs::File,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

pub struct Service {
    commands: Arc<CommandQueue>,
    stop: Arc<AtomicBool>,
    shutdown: Option<WatcherShutdown>,
    watcher: Option<JoinHandle<()>>,
    worker: Option<JoinHandle<()>>,
    pub(super) instance_signal: Option<InstanceSignal>,
    pub(super) events: async_channel::Sender<Event>,
    _instance_lock: File,
    pub data_dir: PathBuf,
    pub initial_theme: ThemePreference,
    pub initial_language: LanguagePreference,
    pub initial_hotkey: HotkeyPreference,
    pub initial_paused: bool,
    pub initial_window_size: Option<WindowSizePreference>,
    pub initial_persist_window_size: bool,
    pub initial_auto_reset_state: bool,
    pub initial_search_auto_focus: bool,
    pub initial_search_auto_clear: bool,
    pub initial_skip_clear_confirm: bool,
    pub initial_paste_close_window: bool,
    pub initial_paste_key: PasteKeyPreference,
    pub initial_paste_move_to_top: bool,
    pub initial_quick_paste_enabled: bool,
    pub initial_paste_shortcuts: PasteShortcutConfig,
    pub initial_window_position: WindowPositionPreference,
    pub initial_hover_preview: HoverPreviewPreference,
    pub initial_display: DisplayPreference,
    pub initial_audio: AudioPreference,
    pub initial_retention: clipboard_core::retention::RetentionPolicy,
    pub initial_retention_saved: bool,
    pub initial_stage_files: bool,
    pub initial_transient_filter: bool,
    pub initial_monitor_types: MonitorTypesPreference,
    pub initial_app_filter: AppFilterPreference,
    pub initial_window_effect: WindowEffectPreference,
    pub initial_onboarding_completed: bool,
}

impl Service {
    pub fn start(
        data_dir: Option<PathBuf>,
        monitor: bool,
    ) -> Result<(Self, async_channel::Receiver<Event>)> {
        let data_dir = resolve_data_dir(data_dir)?;
        let instance_lock = lock_instance(&data_dir)?;
        // Startup happens before entering the GUI event loop; subsequent DB work is
        // exclusively owned by the worker thread.
        let db = Database::new(data_dir.join("clipboard.db"))?;
        let history = History::new(&db);
        let preferences = Preferences::new(&db);
        let initial_theme = preferences.theme()?;
        let initial_language = preferences.language()?;
        let initial_hotkey = preferences.hotkey()?;
        let initial_paused = preferences.capture_paused()?;
        let initial_window_size = preferences.window_size()?;
        let initial_persist_window_size = preferences.persist_window_size()?;
        let initial_auto_reset_state = preferences.auto_reset_state()?;
        let initial_search_auto_focus = preferences.search_auto_focus()?;
        let initial_search_auto_clear = preferences.search_auto_clear()?;
        let initial_skip_clear_confirm = preferences.skip_clear_confirm()?;
        let initial_paste_close_window = preferences.paste_close_window()?;
        let initial_paste_key = preferences.paste_key()?;
        let initial_paste_move_to_top = preferences.paste_move_to_top()?;
        let initial_quick_paste_enabled = preferences.quick_paste_enabled()?;
        let initial_paste_shortcuts = preferences.paste_shortcuts()?;
        let initial_window_position = preferences.window_position()?;
        let initial_hover_preview = preferences.hover_preview()?;
        let initial_display = preferences.display()?;
        let initial_audio = preferences.audio()?;
        let initial_retention = preferences.retention()?;
        let initial_retention_saved = preferences.retention_saved()?;
        let initial_stage_files = preferences.stage_files()?;
        let initial_transient_filter = preferences.transient_filter()?;
        let initial_monitor_types = preferences.monitor_types()?;
        let initial_app_filter = preferences.app_filter()?;
        let initial_window_effect = preferences.window_effect()?;
        let initial_onboarding_completed = if preferences.onboarding_completed()? {
            true
        } else if history.count("", false)? > 0
            || history.groups()?.iter().any(|group| group.item_count > 0)
        {
            // Existing GPUI data predates the introduction of onboarding.
            preferences.set_onboarding_completed()?;
            true
        } else {
            false
        };
        let writer =
            ClipboardContext::new().map_err(|error| anyhow!("初始化剪贴板失败：{error}"))?;
        let commands = Arc::new(CommandQueue::default());
        let incoming = commands.clone();
        let capture_feedback = Arc::new(CaptureFeedback::default());
        let (events, outgoing) = async_channel::bounded(64);
        let state = Arc::new(Mutex::new(CaptureState {
            paused: initial_paused,
            ..Default::default()
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let mut service = Self {
            commands: commands.clone(),
            stop: stop.clone(),
            shutdown: None,
            watcher: None,
            worker: None,
            instance_signal: None,
            events: events.clone(),
            _instance_lock: instance_lock,
            initial_theme,
            initial_language,
            initial_hotkey,
            initial_paused,
            initial_window_size,
            initial_persist_window_size,
            initial_auto_reset_state,
            initial_search_auto_focus,
            initial_search_auto_clear,
            initial_skip_clear_confirm,
            initial_paste_close_window,
            initial_paste_key,
            initial_paste_move_to_top,
            initial_quick_paste_enabled,
            initial_paste_shortcuts,
            initial_window_position,
            initial_hover_preview,
            initial_display,
            initial_audio,
            initial_retention,
            initial_retention_saved,
            initial_stage_files,
            initial_transient_filter,
            initial_monitor_types,
            initial_app_filter: initial_app_filter.clone(),
            initial_window_effect,
            initial_onboarding_completed,
            data_dir,
        };
        let worker_state = state.clone();
        let worker_events = events.clone();
        let worker_feedback = capture_feedback.clone();
        let images_dir = service.data_dir.join("images");
        let icons_dir = service.data_dir.join("icons");
        let staged_dir = service.data_dir.join("staged");
        let worker_data_dir = service.data_dir.clone();
        service.worker = Some(thread::Builder::new().name("history-worker".into()).spawn(
            move || {
                let mut worker = Worker {
                    history,
                    preferences,
                    clipboard: writer,
                    images_dir,
                    staged_dir,
                    icons_dir,
                    data_dir: worker_data_dir,
                    state: worker_state,
                    monitor_types: initial_monitor_types,
                    app_filter: initial_app_filter,
                    transient: super::worker::transient::TransientWindow::new(
                        initial_transient_filter,
                    ),
                    search: String::new(),
                    favorite_only: false,
                    category: ContentCategory::All,
                    group_id: None,
                    limit: PAGE_SIZE,
                    generation: 0,
                    snapshot_pending: false,
                    events: worker_events,
                };
                if let Err(error) = worker.maintain_retention() {
                    worker_feedback.error(format!("执行保留策略失败：{error}"));
                }
                // 启动清扫：上次运行遗留的 blob pending 临时文件与图片临时文件。
                if let Err(error) = worker.sweep_temp_media() {
                    worker_feedback.error(format!("清理临时文件失败：{error}"));
                }
                let mut retention_checked = std::time::Instant::now();
                if let Err(error) = worker.send_groups().and_then(|_| worker.snapshot()) {
                    let _ = worker.events.send_blocking(Event::Error(error.to_string()));
                }
                loop {
                    if !stop.load(Ordering::Acquire) {
                        if retention_checked.elapsed() >= Duration::from_secs(300) {
                            retention_checked = std::time::Instant::now();
                            if let Err(error) = worker.maintain_retention() {
                                worker_feedback.error(format!("执行保留策略失败：{error}"));
                            }
                        }
                        worker_feedback.flush(&worker.events);
                        if let Err(error) = worker.flush_capture_snapshot() {
                            worker_feedback.error(error.to_string());
                        }
                    }
                    let timeout = worker
                        .transient
                        .next_deadline()
                        .map(|deadline| {
                            let now = std::time::Instant::now();
                            deadline
                                .saturating_duration_since(now)
                                .min(Duration::from_millis(250))
                        })
                        .unwrap_or(Duration::from_millis(250));
                    match incoming.recv_timeout(timeout) {
                        Ok((command, _permit)) => {
                            let observed_capture =
                                matches!(command, Command::ObservedCapture { .. });
                            if stop.load(Ordering::Acquire)
                                && !matches!(
                                    command,
                                    Command::ObservedCapture { .. }
                                        | Command::SetMonitorTypes(_)
                                        | Command::SetAppFilter(_)
                                )
                            {
                                continue;
                            }
                            // 真实 watcher 的采集先进 300ms 瞬时确认窗口。
                            if let Command::ObservedCapture {
                                content,
                                source,
                                extra,
                                deferred: true,
                            } = command
                            {
                                for flushed in
                                    worker.transient.push(super::worker::transient::new_pending(
                                        last_seen_sequence(&worker.state),
                                        content,
                                        extra,
                                        source,
                                        std::time::Instant::now(),
                                    ))
                                {
                                    if let Err(error) = worker.handle_observed_capture(
                                        flushed.content,
                                        flushed.extra,
                                        flushed.source,
                                    ) {
                                        worker_feedback.error(error.to_string());
                                    }
                                }
                                continue;
                            }
                            let failure_kind = command.failure_kind();
                            if let Err(error) = worker.handle(command) {
                                match failure_kind {
                                    None if observed_capture => {
                                        worker_feedback.error(error.to_string());
                                    }
                                    None => {
                                        let _ = worker.events.send_blocking(
                                            Event::BackgroundError(error.to_string()),
                                        );
                                    }
                                    Some(kind) => {
                                        let _ = worker.events.send_blocking(Event::CommandFailed {
                                            kind,
                                            message: error.to_string(),
                                        });
                                    }
                                }
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                    // 瞬时窗口到期确认：在 CaptureState 临界区内重读当前剪贴板。
                    if !worker.transient.is_empty() {
                        deliver_transient_due(&mut worker, &worker_feedback);
                    }
                }
                // 退出时丢弃未确认的瞬时条目（<300ms 窗口内，重启后无损失）。
                worker.transient.drain();
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
                feedback: capture_feedback.clone(),
            });
            service.shutdown = Some(watcher.get_shutdown_channel());
            let stop = service.stop.clone();
            let watcher_events = events.clone();
            service.watcher = Some(
                thread::Builder::new()
                    .name("clipboard-watcher".into())
                    .spawn(move || {
                        watcher.start_watch();
                        if !stop.load(Ordering::Acquire) {
                            capture_feedback.error("剪贴板监听已停止，请重启应用".into());
                            capture_feedback.flush(&watcher_events);
                        }
                    })?,
            );
        }
        match InstanceSignal::start(&service.data_dir, events.clone()) {
            Ok(signal) => service.instance_signal = Some(signal),
            Err(error) => {
                let _ = events.try_send(Event::BackgroundError(format!(
                    "重复启动唤出不可用：{error}"
                )));
            }
        }
        Ok((service, outgoing))
    }

    pub fn send(&self, command: Command) -> Result<()> {
        self.commands.try_send(command)
    }
}

/// watcher 入队时的序列号；瞬时窗口用它比对确认时的当前序列号。
fn last_seen_sequence(state: &Mutex<CaptureState>) -> u32 {
    state.lock().map(|state| state.last_sequence).unwrap_or(0)
}

/// 逐条确认到期的瞬时条目：在 CaptureState 临界区内重读当前剪贴板，
/// 与 watcher 的稳定读互斥（模式同 handle_copy 的写回锁）。
fn deliver_transient_due(worker: &mut Worker, feedback: &CaptureFeedback) {
    let mut deliver = Vec::new();
    {
        let Ok(state) = crate::windows::capture::lock_capture_state(&worker.state) else {
            return;
        };
        let ignored_sequence = state.ignored_sequence;
        let mut reread = || {
            let sequence = unsafe { GetClipboardSequenceNumber() };
            let content = super::read_formats::read_current_content(&worker.clipboard)?;
            Ok((sequence, content))
        };
        for (entry, outcome) in
            worker
                .transient
                .poll_due(std::time::Instant::now(), ignored_sequence, &mut reread)
        {
            if matches!(outcome, super::worker::transient::PollOutcome::Deliver) {
                deliver.push(entry);
            }
        }
        drop(state);
    }
    for entry in deliver {
        if let Err(error) = worker.handle_observed_capture(entry.content, entry.extra, entry.source)
        {
            feedback.error(error.to_string());
        } else {
            worker.snapshot_pending = true;
        }
    }
    // 确认入库后与直通路径一致地推送列表快照。
    if worker.snapshot_pending
        && let Err(error) = worker.flush_capture_snapshot()
    {
        feedback.error(error.to_string());
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.commands.close();
        // Release blocked replies before draining accepted captures and joining producers.
        self.events.close();
        self.instance_signal = None;
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
