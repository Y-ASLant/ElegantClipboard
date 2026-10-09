use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fmt::Display,
    sync::{
        Arc, LazyLock,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{Manager, WindowEvent, webview::PageLoadEvent};
use tokio::sync::Notify;

const WINDOW_READY_TIMEOUT: Duration = Duration::from_secs(30);
const VERSION_POLL_INTERVAL: Duration = Duration::from_secs(10 * 60);
const MAX_RECOVERY_ATTEMPTS: u8 = 2;
const RENDER_UNRESPONSIVE_WINDOW_SECS: u64 = 30;
const MAX_RENDER_UNRESPONSIVE_EVENTS: u8 = 2;

static LOADED_VERSION: LazyLock<parking_lot::RwLock<Option<String>>> =
    LazyLock::new(|| parking_lot::RwLock::new(None));
static NATIVE_EVENTS_REGISTERED: AtomicBool = AtomicBool::new(false);
static INTENTIONAL_EXIT: AtomicBool = AtomicBool::new(false);
// 0 = 未安排恢复，2 = 立即重启，3 = 恢复熔断。
static RESTART_LEVEL: AtomicU8 = AtomicU8::new(0);
static RECOVERY_MARKER_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());
static MANAGED_WINDOWS: LazyLock<parking_lot::Mutex<HashMap<String, Arc<WindowReadiness>>>> =
    LazyLock::new(|| parking_lot::Mutex::new(HashMap::new()));
static UNRESPONSIVE_RENDERERS: LazyLock<parking_lot::Mutex<HashMap<String, UnresponsiveMarker>>> =
    LazyLock::new(|| parking_lot::Mutex::new(HashMap::new()));

#[derive(Debug, Serialize, Deserialize)]
struct RecoveryMarker {
    first_attempt_epoch_secs: u64,
    attempts: u8,
}

#[derive(Clone, Debug)]
struct UnresponsiveMarker {
    first_event_epoch_secs: u64,
    events: u8,
}

struct WindowReadiness {
    native_loaded: AtomicBool,
    frontend_ready: AtomicBool,
    cancelled: AtomicBool,
    process_handler_registered: AtomicBool,
    lifecycle_handler_registered: AtomicBool,
    ready: Notify,
}

impl WindowReadiness {
    fn new() -> Self {
        Self {
            native_loaded: AtomicBool::new(false),
            frontend_ready: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            process_handler_registered: AtomicBool::new(false),
            lifecycle_handler_registered: AtomicBool::new(false),
            ready: Notify::new(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct WindowCreationGuard {
    app: tauri::AppHandle,
    label: String,
    readiness: Arc<WindowReadiness>,
    owns_readiness: bool,
}

impl WindowCreationGuard {
    pub(crate) fn start(app: &tauri::AppHandle, label: impl Into<String>) -> Self {
        let label = label.into();
        let (readiness, owns_readiness) = {
            let mut windows = MANAGED_WINDOWS.lock();
            if let Some(readiness) = windows.get(&label) {
                tracing::warn!(label, "A managed WebView window is already being created");
                (readiness.clone(), false)
            } else {
                let readiness = Arc::new(WindowReadiness::new());
                windows.insert(label.clone(), readiness.clone());
                (readiness, true)
            }
        };
        let guard = Self {
            app: app.clone(),
            label,
            readiness,
            owns_readiness,
        };

        if guard.owns_readiness {
            let watchdog = guard.clone();
            std::thread::spawn(move || {
                std::thread::sleep(WINDOW_READY_TIMEOUT);
                if watchdog.readiness.frontend_ready.load(Ordering::Acquire)
                    || watchdog.readiness.cancelled.load(Ordering::Acquire)
                {
                    return;
                }
                tracing::error!(
                    label = %watchdog.label,
                    timeout_ms = WINDOW_READY_TIMEOUT.as_millis(),
                    "WebView window creation timed out before frontend readiness"
                );
                watchdog.readiness.ready.notify_waiters();
                schedule_restart(
                    &watchdog.app,
                    &format!("window_ready_timeout:{}", watchdog.label),
                );
            });
        }

        guard
    }

    pub(crate) fn cancel(&self) {
        if self.owns_readiness {
            cancel_window_readiness(&self.label, &self.readiness);
        }
    }

    pub(crate) fn on_page_load(
        &self,
        window: &tauri::WebviewWindow,
        payload: &tauri::webview::PageLoadPayload<'_>,
    ) {
        if !matches!(payload.event(), PageLoadEvent::Finished) {
            return;
        }

        self.readiness.native_loaded.store(true, Ordering::Release);
        clear_unresponsive_renderer(&self.label);
        tracing::info!(label = %self.label, url = %payload.url(), "WebView window page loaded");

        if !self
            .readiness
            .process_handler_registered
            .swap(true, Ordering::AcqRel)
        {
            register_process_failed_handler(window, self.label.clone());
        }

        if !self
            .readiness
            .lifecycle_handler_registered
            .swap(true, Ordering::AcqRel)
        {
            let label = self.label.clone();
            let readiness = self.readiness.clone();
            window.on_window_event(move |event| {
                if matches!(event, WindowEvent::Destroyed) {
                    cancel_window_readiness(&label, &readiness);
                }
            });
        }
    }
}

fn tracked_window_readiness(label: &str) -> Option<Arc<WindowReadiness>> {
    MANAGED_WINDOWS.lock().get(label).cloned()
}

fn cancel_window_readiness(label: &str, readiness: &Arc<WindowReadiness>) {
    readiness.cancelled.store(true, Ordering::Release);
    readiness.ready.notify_waiters();
    let mut windows = MANAGED_WINDOWS.lock();
    if windows
        .get(label)
        .is_some_and(|current| Arc::ptr_eq(current, readiness))
    {
        windows.remove(label);
    }
}

pub(crate) fn initialize(_app: &tauri::AppHandle, main_window: &tauri::WebviewWindow) {
    if let Ok(version) = tauri::webview_version() {
        let version = version.to_string();
        *LOADED_VERSION.write() = Some(version.clone());
        tracing::info!(loaded_version = %version, "WebView runtime supervisor initialized");
    }

    register_native_events(main_window);
    start_version_poll();
}

pub(crate) fn ensure_runtime_current(_app: &tauri::AppHandle) -> Result<(), String> {
    ensure_recovery_idle(&RESTART_LEVEL)
}

fn ensure_recovery_idle(restart_level: &AtomicU8) -> Result<(), String> {
    match restart_level.load(Ordering::Acquire) {
        1 | 2 => Err("WebView2 正在恢复，ElegantClipboard 即将重启".to_string()),
        3 => Err("WebView2 自动恢复已停止，请手动重启 ElegantClipboard".to_string()),
        _ => Ok(()),
    }
}

fn check_runtime_current() -> Result<bool, String> {
    ensure_recovery_idle(&RESTART_LEVEL)?;
    let available = tauri::webview_version()
        .map(|version| version.to_string())
        .map_err(|error| format!("查询 WebView2 版本失败: {error}"));
    let loaded = LOADED_VERSION.read().clone();
    Ok(check_runtime_versions(loaded.as_deref(), available))
}

fn check_runtime_versions(loaded: Option<&str>, available: Result<String, String>) -> bool {
    let available = match available {
        Ok(available) => available,
        Err(error) => {
            tracing::warn!(%error, "Failed to query available WebView2 runtime version");
            return false;
        }
    };
    let changed = runtime_version_changed(loaded, &available);
    if changed == Some(true) {
        tracing::info!(
            loaded_version = ?loaded,
            available_version = %available,
            "A different WebView2 runtime version is available; keeping the current environment until application restart"
        );
    }
    // 版本差异是更新过渡状态，不能据此判定当前环境失效。
    changed.is_some()
}

#[cfg(target_os = "windows")]
fn runtime_version_changed(loaded: Option<&str>, available: &str) -> Option<bool> {
    use webview2_com::Microsoft::Web::WebView2::Win32::CompareBrowserVersions;
    use windows_core::HSTRING;

    let loaded = HSTRING::from(loaded?);
    let available = HSTRING::from(available);
    let mut comparison = 0;
    match unsafe { CompareBrowserVersions(&loaded, &available, &mut comparison) } {
        Ok(()) => Some(comparison != 0),
        Err(error) => {
            tracing::warn!(%error, "Failed to compare WebView2 runtime versions");
            None
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn runtime_version_changed(loaded: Option<&str>, available: &str) -> Option<bool> {
    loaded.map(|loaded| loaded != available)
}

#[tauri::command]
pub(crate) fn managed_window_ready(window: tauri::WebviewWindow) -> Result<bool, String> {
    let label = window.label().to_string();
    let Some(readiness) = tracked_window_readiness(&label) else {
        return Err(format!("窗口 {label} 未处于受监管的创建状态"));
    };
    if readiness.cancelled.load(Ordering::Acquire) {
        return Err(format!("窗口 {label} 的创建已取消"));
    }
    if !readiness.native_loaded.load(Ordering::Acquire) {
        tracing::debug!(
            label,
            "Frontend reported ready before native page-load callback"
        );
    }

    let first_ready = !readiness.frontend_ready.swap(true, Ordering::AcqRel);
    if first_ready {
        tracing::info!(label, "WebView window frontend ready");
        readiness.ready.notify_waiters();
    }
    Ok(first_ready)
}

pub(crate) async fn wait_for_window_ready(label: &str) -> Result<(), String> {
    let Some(readiness) = tracked_window_readiness(label) else {
        return Ok(());
    };
    if readiness.frontend_ready.load(Ordering::Acquire) {
        return Ok(());
    }
    if readiness.cancelled.load(Ordering::Acquire) {
        return Err(format!("窗口 {label} 的创建已取消"));
    }

    let notified = readiness.ready.notified();
    if readiness.frontend_ready.load(Ordering::Acquire) {
        return Ok(());
    }
    if readiness.cancelled.load(Ordering::Acquire) {
        return Err(format!("窗口 {label} 的创建已取消"));
    }
    let notified = tokio::time::timeout(WINDOW_READY_TIMEOUT, notified).await;

    if readiness.frontend_ready.load(Ordering::Acquire) {
        return Ok(());
    }
    if readiness.cancelled.load(Ordering::Acquire) {
        return Err(format!("窗口 {label} 的创建已取消"));
    }
    if notified.is_err() {
        return Err(format!("窗口 {label} 在前端就绪前超时"));
    }
    Err(format!("窗口 {label} 未能完成前端就绪"))
}

pub(crate) fn window_operation_error(
    app: &tauri::AppHandle,
    label: &str,
    operation: &str,
    error: impl Display,
) -> String {
    let detail = error.to_string();
    if is_webview_connection_failure(&detail) {
        tracing::error!(label, operation, %detail, "WebView window operation lost its runtime connection");
        schedule_restart(app, &format!("window_operation_failed:{label}:{operation}"));
    }
    format!("{operation}: {detail}")
}

fn is_webview_connection_failure(message: &str) -> bool {
    message.contains("0x80010108")
        || message.contains("RPC_E_DISCONNECTED")
        || message.contains("disconnected from its clients")
        || message.contains("已与其客户端断开连接")
}

pub(crate) fn mark_intentional_exit() {
    INTENTIONAL_EXIT.store(true, Ordering::Release);
}

fn start_version_poll() {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(VERSION_POLL_INTERVAL);
            if INTENTIONAL_EXIT.load(Ordering::Acquire) {
                return;
            }
            let version_checked = match check_runtime_current() {
                Ok(checked) => checked,
                Err(_) => return,
            };
            clear_recovery_marker(&recovery_marker_path(), version_checked, &RESTART_LEVEL);
        }
    });
}

fn schedule_restart(app: &tauri::AppHandle, reason: &str) {
    if INTENTIONAL_EXIT.load(Ordering::Acquire) {
        return;
    }

    let previous = RESTART_LEVEL.fetch_max(2, Ordering::AcqRel);
    tracing::warn!(
        reason,
        previous_level = previous,
        "WebView recovery restart requested"
    );
    if previous != 0 {
        return;
    }

    notify_restart(app);
    let app = app.clone();
    let reason = reason.to_string();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(250));

        if !record_recovery_attempt() {
            RESTART_LEVEL.store(3, Ordering::Release);
            tracing::error!(reason, "WebView recovery restart fuse opened");
            notify_recovery_fuse(&app);
            return;
        }

        tracing::warn!(reason, "Restarting application to recover WebView2");
        mark_intentional_exit();
        crate::admin_launch::perform_restart(&app);
    });
}

fn notify_restart(app: &tauri::AppHandle) {
    use tauri_plugin_notification::NotificationExt;

    let _ = app
        .notification()
        .builder()
        .title("ElegantClipboard 正在恢复")
        .body("WebView2 连接已失效，程序将自动重启恢复")
        .show();
}

fn notify_recovery_fuse(app: &tauri::AppHandle) {
    use tauri_plugin_notification::NotificationExt;

    let _ = app
        .notification()
        .builder()
        .title("ElegantClipboard WebView2 恢复失败")
        .body("已停止自动重启以避免循环，请手动重启或修复 WebView2 Runtime")
        .show();
}

fn recovery_marker_path() -> std::path::PathBuf {
    crate::config::AppConfig::load()
        .get_data_dir()
        .join("webview-recovery.json")
}

fn record_recovery_attempt() -> bool {
    record_recovery_attempt_at(&recovery_marker_path(), epoch_secs())
}

fn record_recovery_attempt_at(path: &std::path::Path, now: u64) -> bool {
    let _marker_lock = RECOVERY_MARKER_LOCK.lock();
    let current = std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<RecoveryMarker>(&raw).ok());
    let Some(marker) = next_recovery_marker(current, now) else {
        return false;
    };

    if let Some(parent) = path.parent()
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        tracing::warn!(%error, "Failed to create WebView recovery marker directory");
        return true;
    }
    if let Ok(raw) = serde_json::to_string(&marker)
        && let Err(error) = std::fs::write(path, raw)
    {
        tracing::warn!(%error, "Failed to persist WebView recovery marker");
    }
    true
}

fn clear_recovery_marker(path: &std::path::Path, version_checked: bool, restart_level: &AtomicU8) {
    let _marker_lock = RECOVERY_MARKER_LOCK.lock();
    if !version_checked || restart_level.load(Ordering::Acquire) != 0 {
        return;
    }
    if let Err(error) = std::fs::remove_file(path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(%error, "Failed to clear WebView recovery marker");
    }
}

fn next_recovery_marker(current: Option<RecoveryMarker>, now: u64) -> Option<RecoveryMarker> {
    // 计数跨启动保留，只在定时运行时检查通过后清除。
    let mut marker = current.unwrap_or(RecoveryMarker {
        first_attempt_epoch_secs: now,
        attempts: 0,
    });
    if marker.attempts >= MAX_RECOVERY_ATTEMPTS {
        return None;
    }
    marker.attempts += 1;
    Some(marker)
}

fn epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn record_renderer_unresponsive(label: &str) -> bool {
    let now = epoch_secs();
    let mut renderers = UNRESPONSIVE_RENDERERS.lock();
    let marker = next_unresponsive_marker(renderers.remove(label), now);
    let restart_required = marker.events >= MAX_RENDER_UNRESPONSIVE_EVENTS;
    renderers.insert(label.to_string(), marker);
    restart_required
}

fn next_unresponsive_marker(current: Option<UnresponsiveMarker>, now: u64) -> UnresponsiveMarker {
    let mut marker = current
        .filter(|marker| {
            now.saturating_sub(marker.first_event_epoch_secs) <= RENDER_UNRESPONSIVE_WINDOW_SECS
        })
        .unwrap_or(UnresponsiveMarker {
            first_event_epoch_secs: now,
            events: 0,
        });
    marker.events += 1;
    marker
}

fn clear_unresponsive_renderer(label: &str) {
    UNRESPONSIVE_RENDERERS.lock().remove(label);
}

#[cfg(target_os = "windows")]
fn register_native_events(main_window: &tauri::WebviewWindow) {
    if NATIVE_EVENTS_REGISTERED.swap(true, Ordering::AcqRel) {
        return;
    }

    let app = main_window.app_handle().clone();
    if let Err(error) = main_window.with_webview(move |platform| {
        register_environment_handlers(&app, platform.environment());
        register_process_failed_handler_inner(&app, "main".to_string(), platform.controller());
    }) {
        NATIVE_EVENTS_REGISTERED.store(false, Ordering::Release);
        tracing::error!(%error, "Failed to access native WebView2 handles");
    }
}

#[cfg(not(target_os = "windows"))]
fn register_native_events(_main_window: &tauri::WebviewWindow) {}

#[cfg(target_os = "windows")]
fn new_browser_version_available_handler()
-> webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2NewBrowserVersionAvailableEventHandler{
    webview2_com::NewBrowserVersionAvailableEventHandler::create(Box::new(|_, _| {
        tracing::info!(
            "A new WebView2 runtime version is available; keeping the current environment until application restart"
        );
        Ok(())
    }))
}

#[cfg(target_os = "windows")]
fn register_environment_handlers(
    app: &tauri::AppHandle,
    environment: webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Environment,
) {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Environment5;
    use webview2_com::{BrowserProcessExitedEventHandler, take_pwstr};
    use windows_core::{Interface, PWSTR};

    let mut raw_version = PWSTR::null();
    if unsafe { environment.BrowserVersionString(&mut raw_version) }.is_ok() {
        let version = take_pwstr(raw_version);
        *LOADED_VERSION.write() = Some(version.clone());
        tracing::info!(loaded_version = %version, "Registered WebView2 environment");
    }

    let update_handler = new_browser_version_available_handler();
    let mut update_token = 0;
    if let Err(error) =
        unsafe { environment.add_NewBrowserVersionAvailable(&update_handler, &mut update_token) }
    {
        tracing::error!(%error, "Failed to register NewBrowserVersionAvailable handler");
    }

    match environment.cast::<ICoreWebView2Environment5>() {
        Ok(environment5) => {
            let exit_app = app.clone();
            let exit_handler =
                BrowserProcessExitedEventHandler::create(Box::new(move |_, args| {
                    if INTENTIONAL_EXIT.load(Ordering::Acquire) {
                        return Ok(());
                    }
                    let Some(args) = args else {
                        tracing::warn!("WebView2 browser process exited without event arguments");
                        return Ok(());
                    };
                    let mut kind = Default::default();
                    if let Err(error) = unsafe { args.BrowserProcessExitKind(&mut kind) } {
                        tracing::warn!(%error, "Failed to inspect WebView2 browser process exit");
                        return Ok(());
                    }

                    if is_failed_browser_process_exit_kind(kind.0) {
                        schedule_restart(&exit_app, "browser_process_exited_failed");
                    } else {
                        tracing::debug!(
                            exit_kind = kind.0,
                            "WebView2 browser process exited normally"
                        );
                    }
                    Ok(())
                }));
            let mut exit_token = 0;
            if let Err(error) =
                unsafe { environment5.add_BrowserProcessExited(&exit_handler, &mut exit_token) }
            {
                tracing::error!(%error, "Failed to register BrowserProcessExited handler");
            }
        }
        Err(error) => {
            tracing::warn!(%error, "WebView2 environment does not expose BrowserProcessExited");
        }
    }
}

#[cfg(target_os = "windows")]
fn register_process_failed_handler(window: &tauri::WebviewWindow, label: String) {
    let app = window.app_handle().clone();
    if let Err(error) = window.with_webview(move |platform| {
        register_process_failed_handler_inner(&app, label, platform.controller());
    }) {
        tracing::error!(%error, "Failed to access WebView2 controller for process monitoring");
    }
}

#[cfg(not(target_os = "windows"))]
fn register_process_failed_handler(_window: &tauri::WebviewWindow, _label: String) {}

#[cfg(target_os = "windows")]
fn register_process_failed_handler_inner(
    app: &tauri::AppHandle,
    label: String,
    controller: webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Controller,
) {
    use webview2_com::ProcessFailedEventHandler;

    let Ok(webview) = (unsafe { controller.CoreWebView2() }) else {
        tracing::error!(label, "Failed to get CoreWebView2 for process monitoring");
        return;
    };

    let failure_app = app.clone();
    let failure_label = label.clone();
    let reload_webview = webview.clone();
    let handler = ProcessFailedEventHandler::create(Box::new(move |_, args| {
        let Some(args) = args else {
            tracing::warn!(label = %failure_label, "WebView2 process failure missing event arguments");
            return Ok(());
        };
        let mut kind = Default::default();
        unsafe { args.ProcessFailedKind(&mut kind)? };
        tracing::error!(label = %failure_label, failure_kind = kind.0, "WebView2 process failed");

        if requires_application_restart_for_failure_kind(kind.0) {
            schedule_restart(
                &failure_app,
                &format!("process_failed:{}:{}", failure_label, kind.0),
            );
        } else if is_renderer_unresponsive(kind.0) {
            if record_renderer_unresponsive(&failure_label) {
                schedule_restart(
                    &failure_app,
                    &format!("renderer_unresponsive:{}", failure_label),
                );
            } else {
                tracing::warn!(label = %failure_label, "Reloading unresponsive WebView2 renderer");
                if let Err(error) = unsafe { reload_webview.Reload() } {
                    tracing::error!(label = %failure_label, %error, "Failed to reload unresponsive WebView2 renderer");
                    schedule_restart(
                        &failure_app,
                        &format!("renderer_reload_failed:{}", failure_label),
                    );
                }
            }
        }
        Ok(())
    }));
    let mut token = 0;
    if let Err(error) = unsafe { webview.add_ProcessFailed(&handler, &mut token) } {
        tracing::error!(label, %error, "Failed to register ProcessFailed handler");
    }
}

fn is_failed_browser_process_exit_kind(kind: i32) -> bool {
    kind == 1
}

fn requires_application_restart_for_failure_kind(kind: i32) -> bool {
    // COREWEBVIEW2_PROCESS_FAILED_KIND: browser=0, renderer=1, frame renderer=3.
    matches!(kind, 0 | 1 | 3)
}

fn is_renderer_unresponsive(kind: i32) -> bool {
    // COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE.
    kind == 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "windows")]
    #[test]
    fn runtime_update_event_does_not_schedule_recovery() {
        let handler = new_browser_version_available_handler();
        for _ in 0..3 {
            unsafe {
                handler
                    .Invoke(
                        None::<&webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Environment>,
                        None::<&windows_core::IUnknown>,
                    )
                    .unwrap();
            }
            assert_eq!(RESTART_LEVEL.load(Ordering::Acquire), 0);
            assert!(!INTENTIONAL_EXIT.load(Ordering::Acquire));
        }
    }

    #[test]
    fn runtime_update_does_not_require_recovery() {
        for (loaded, available) in [
            ("151.0.4129.107", "152.0.4191.53"),
            ("152.0.4191.53", "151.0.4129.107"),
            ("152.0.4191.9", "152.0.4191.19"),
        ] {
            assert!(check_runtime_versions(
                Some(loaded),
                Ok(available.to_string())
            ));
            assert!(ensure_recovery_idle(&AtomicU8::new(0)).is_ok());
        }
    }

    #[test]
    fn unavailable_version_query_does_not_block_window_creation() {
        assert!(!check_runtime_versions(
            Some("151.0.4129.107"),
            Err("version query failed".to_string())
        ));
        assert!(ensure_recovery_idle(&AtomicU8::new(0)).is_ok());
    }

    #[test]
    fn runtime_update_poll_clears_previous_recovery_attempts() {
        let directory =
            std::env::temp_dir().join(format!("ec_webview_recovery_{}", uuid::Uuid::new_v4()));
        let path = directory.join("webview-recovery.json");
        assert!(record_recovery_attempt_at(&path, 1_000));
        assert!(record_recovery_attempt_at(&path, 1_001));
        let checked =
            check_runtime_versions(Some("151.0.4129.107"), Ok("152.0.4191.53".to_string()));
        clear_recovery_marker(&path, checked, &AtomicU8::new(0));
        assert!(!path.exists());
        for _ in 0..3 {
            assert!(check_runtime_versions(
                Some("151.0.4129.107"),
                Ok("152.0.4191.53".to_string())
            ));
            assert!(!path.exists());
        }
        let recovered_at = 1_000 + VERSION_POLL_INTERVAL.as_secs();
        assert!(record_recovery_attempt_at(&path, recovered_at));
        let marker: RecoveryMarker =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(marker.attempts, 1);
        assert_eq!(marker.first_attempt_epoch_secs, recovered_at);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn runtime_version_comparison_detects_numeric_changes() {
        assert_eq!(runtime_version_changed(None, "152.0.4191.19"), None);
        assert_eq!(
            runtime_version_changed(Some("152.0.4191.19"), "152.0.4191.19"),
            Some(false)
        );
        assert_eq!(
            runtime_version_changed(Some("151.0.4191.19 beta"), "152.0.4191.19"),
            Some(true)
        );
        assert_eq!(
            runtime_version_changed(Some("152.0.4191.9"), "152.0.4191.19"),
            Some(true)
        );
        assert_eq!(
            runtime_version_changed(Some("152.0.4191.19"), "152.0.4191.9"),
            Some(true)
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn runtime_version_comparison_ignores_channel_suffixes() {
        for channel in ["beta", "dev", "canary"] {
            let version = format!("152.0.4191.19 {channel}");
            assert_eq!(
                runtime_version_changed(Some(&version), "152.0.4191.19"),
                Some(false)
            );
            assert_eq!(
                runtime_version_changed(Some("152.0.4191.19"), &version),
                Some(false)
            );
        }
        assert_eq!(
            runtime_version_changed(Some("152.0.4191.19 beta"), "152.0.4191.19 dev"),
            Some(false)
        );
    }

    #[test]
    fn recovery_attempts_do_not_expire_between_version_polls() {
        let directory =
            std::env::temp_dir().join(format!("ec_webview_recovery_{}", uuid::Uuid::new_v4()));
        let path = directory.join("webview-recovery.json");
        let started = 1_000;
        let interval = VERSION_POLL_INTERVAL.as_secs() + 5;
        assert!(record_recovery_attempt_at(&path, started));
        assert!(record_recovery_attempt_at(&path, started + interval));
        let persisted = std::fs::read_to_string(&path).unwrap();
        let marker: RecoveryMarker = serde_json::from_str(&persisted).unwrap();
        assert_eq!(marker.attempts, MAX_RECOVERY_ATTEMPTS);
        assert_eq!(marker.first_attempt_epoch_secs, started);
        assert!(!record_recovery_attempt_at(&path, started + 2 * interval));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), persisted);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn recovery_attempts_fuse_after_two_restarts() {
        let now = 1_000;
        let first = next_recovery_marker(None, now).unwrap();
        assert_eq!(first.attempts, 1);
        let second = next_recovery_marker(Some(first), now + 1).unwrap();
        assert_eq!(second.attempts, MAX_RECOVERY_ATTEMPTS);
        assert!(next_recovery_marker(Some(second), now + 2).is_none());
    }

    #[test]
    fn confirmed_current_runtime_poll_clears_recovery_attempts() {
        let directory =
            std::env::temp_dir().join(format!("ec_webview_recovery_{}", uuid::Uuid::new_v4()));
        let path = directory.join("webview-recovery.json");
        let now = 1_000;
        assert!(record_recovery_attempt_at(&path, now));
        assert!(record_recovery_attempt_at(&path, now + 1));
        let restart_level = AtomicU8::new(0);
        let runtime_current =
            check_runtime_versions(Some("152.0.4191.19"), Ok("152.0.4191.19".to_string()));

        clear_recovery_marker(&path, runtime_current, &restart_level);
        assert!(!path.exists());
        clear_recovery_marker(&path, runtime_current, &restart_level);
        let recovered_at = now + VERSION_POLL_INTERVAL.as_secs();
        assert!(record_recovery_attempt_at(&path, recovered_at));
        let reset: RecoveryMarker =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(reset.attempts, 1);
        assert_eq!(reset.first_attempt_epoch_secs, recovered_at);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn unconfirmed_runtime_poll_preserves_recovery_attempts() {
        let directory =
            std::env::temp_dir().join(format!("ec_webview_recovery_{}", uuid::Uuid::new_v4()));
        let path = directory.join("webview-recovery.json");
        assert!(record_recovery_attempt_at(&path, 1_000));
        let persisted = std::fs::read_to_string(&path).unwrap();
        let restart_level = AtomicU8::new(0);
        for (loaded, available) in [
            (None, Ok("152.0.4191.19".to_string())),
            (
                Some("151.0.4129.107"),
                Err("version query failed".to_string()),
            ),
        ] {
            let version_checked = check_runtime_versions(loaded, available);
            assert!(!version_checked);
            clear_recovery_marker(&path, version_checked, &restart_level);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), persisted);
        }
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn failed_version_comparison_preserves_recovery_attempts() {
        let directory =
            std::env::temp_dir().join(format!("ec_webview_recovery_{}", uuid::Uuid::new_v4()));
        let path = directory.join("webview-recovery.json");
        assert!(record_recovery_attempt_at(&path, 1_000));
        let persisted = std::fs::read_to_string(&path).unwrap();
        let restart_level = AtomicU8::new(0);
        for (loaded, available) in [("invalid", "152.0.4191.19"), ("152.0.4191.19", "invalid")] {
            let changed = runtime_version_changed(Some(loaded), available);
            assert_eq!(changed, None);
            let version_checked = check_runtime_versions(Some(loaded), Ok(available.to_string()));
            assert!(!version_checked);
            clear_recovery_marker(&path, version_checked, &restart_level);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), persisted);
        }
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn pending_recovery_still_blocks_window_creation() {
        assert!(ensure_recovery_idle(&AtomicU8::new(0)).is_ok());
        for level in [1, 2, 3] {
            assert!(ensure_recovery_idle(&AtomicU8::new(level)).is_err());
        }
    }

    #[test]
    fn runtime_poll_preserves_attempts_when_recovery_is_pending() {
        let directory =
            std::env::temp_dir().join(format!("ec_webview_recovery_{}", uuid::Uuid::new_v4()));
        let path = directory.join("webview-recovery.json");
        assert!(record_recovery_attempt_at(&path, 1_000));
        let persisted = std::fs::read_to_string(&path).unwrap();
        for level in [1, 2, 3] {
            clear_recovery_marker(&path, true, &AtomicU8::new(level));
            assert_eq!(std::fs::read_to_string(&path).unwrap(), persisted);
        }
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn concurrent_runtime_poll_preserves_recovery_attempts() {
        let directory =
            std::env::temp_dir().join(format!("ec_webview_recovery_{}", uuid::Uuid::new_v4()));
        let path = directory.join("webview-recovery.json");
        let restart_level = AtomicU8::new(0);
        assert!(record_recovery_attempt_at(&path, 1_000));
        let marker_lock = RECOVERY_MARKER_LOCK.lock();
        std::thread::scope(|scope| {
            let (started, waiting) = std::sync::mpsc::channel();
            let poll_path = &path;
            let poll_restart_level = &restart_level;
            let poll = scope.spawn(move || {
                started.send(()).unwrap();
                clear_recovery_marker(poll_path, true, poll_restart_level);
            });
            waiting.recv().unwrap();
            restart_level.store(1, Ordering::Release);
            drop(marker_lock);
            assert!(record_recovery_attempt_at(&path, 1_001));
            poll.join().unwrap();
        });
        let marker: RecoveryMarker =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(marker.attempts, MAX_RECOVERY_ATTEMPTS);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn process_failure_recovery_escalates_by_kind() {
        assert!(requires_application_restart_for_failure_kind(0));
        assert!(requires_application_restart_for_failure_kind(1));
        assert!(!requires_application_restart_for_failure_kind(2));
        assert!(requires_application_restart_for_failure_kind(3));
        assert!(is_renderer_unresponsive(2));
        assert!(!is_renderer_unresponsive(1));
    }

    #[test]
    fn only_failed_browser_exit_requests_recovery() {
        assert!(is_failed_browser_process_exit_kind(1));
        assert!(!is_failed_browser_process_exit_kind(0));
        assert!(!is_failed_browser_process_exit_kind(2));
    }

    #[test]
    fn renderer_unresponsive_events_reset_after_recovery_window() {
        let expired = UnresponsiveMarker {
            first_event_epoch_secs: 1,
            events: MAX_RENDER_UNRESPONSIVE_EVENTS,
        };
        let reset = next_unresponsive_marker(Some(expired), RENDER_UNRESPONSIVE_WINDOW_SECS + 100);
        assert_eq!(reset.events, 1);

        let next = next_unresponsive_marker(Some(reset), RENDER_UNRESPONSIVE_WINDOW_SECS + 100);
        assert_eq!(next.events, 2);
    }

    #[test]
    fn disconnected_window_errors_are_detected() {
        assert!(is_webview_connection_failure("HRESULT 0x80010108"));
        assert!(is_webview_connection_failure("已与其客户端断开连接"));
        assert!(!is_webview_connection_failure("the window is hidden"));
    }
}
