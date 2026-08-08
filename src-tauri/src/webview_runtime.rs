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
const RECOVERY_WINDOW_SECS: u64 = 10 * 60;
const MAX_RECOVERY_ATTEMPTS: u8 = 2;
const RENDER_UNRESPONSIVE_WINDOW_SECS: u64 = 30;
const MAX_RENDER_UNRESPONSIVE_EVENTS: u8 = 2;

static LOADED_VERSION: LazyLock<parking_lot::RwLock<Option<String>>> =
    LazyLock::new(|| parking_lot::RwLock::new(None));
static NATIVE_EVENTS_REGISTERED: AtomicBool = AtomicBool::new(false);
static INTENTIONAL_EXIT: AtomicBool = AtomicBool::new(false);
// 0 = none, 1 = restart when safe, 2 = restart immediately, 3 = recovery fuse open.
static RESTART_LEVEL: AtomicU8 = AtomicU8::new(0);
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
                    true,
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

pub(crate) fn initialize(app: &tauri::AppHandle, main_window: &tauri::WebviewWindow) {
    if let Ok(version) = tauri::webview_version() {
        let version = version.to_string();
        *LOADED_VERSION.write() = Some(version.clone());
        tracing::info!(loaded_version = %version, "WebView runtime supervisor initialized");
    }

    register_native_events(main_window);
    start_version_poll(app.clone());
}

pub(crate) fn ensure_runtime_current(app: &tauri::AppHandle) -> Result<(), String> {
    match RESTART_LEVEL.load(Ordering::Acquire) {
        1 | 2 => {
            return Err("WebView2 正在恢复，ElegantClipboard 即将重启".to_string());
        }
        3 => {
            return Err("WebView2 自动恢复已停止，请手动重启 ElegantClipboard".to_string());
        }
        _ => {}
    }
    let available = tauri::webview_version()
        .map_err(|error| format!("查询 WebView2 版本失败: {error}"))?
        .to_string();
    let loaded = LOADED_VERSION.read().clone();

    if runtime_version_changed(loaded.as_deref(), &available) {
        tracing::warn!(
            loaded_version = ?loaded,
            available_version = %available,
            "A newer WebView2 runtime is available; dynamic window creation is suspended"
        );
        schedule_restart(app, "runtime_version_changed", false);
        return Err("WebView2 运行时已更新，ElegantClipboard 正在重启".to_string());
    }

    Ok(())
}

fn runtime_version_changed(loaded: Option<&str>, available: &str) -> bool {
    loaded.is_some_and(|loaded| loaded != available)
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
        schedule_restart(
            app,
            &format!("window_operation_failed:{label}:{operation}"),
            true,
        );
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

fn start_version_poll(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(VERSION_POLL_INTERVAL);
            if INTENTIONAL_EXIT.load(Ordering::Acquire) {
                return;
            }
            if ensure_runtime_current(&app).is_err() {
                return;
            }
        }
    });
}

fn schedule_restart(app: &tauri::AppHandle, reason: &str, immediate: bool) {
    if INTENTIONAL_EXIT.load(Ordering::Acquire) {
        return;
    }

    let requested_level = if immediate { 2 } else { 1 };
    let previous = RESTART_LEVEL.fetch_max(requested_level, Ordering::AcqRel);
    tracing::warn!(
        reason,
        immediate,
        previous_level = previous,
        "WebView recovery restart requested"
    );
    if previous != 0 {
        return;
    }

    notify_restart(app, immediate);
    let app = app.clone();
    let reason = reason.to_string();
    std::thread::spawn(move || {
        if !immediate {
            loop {
                if RESTART_LEVEL.load(Ordering::Acquire) >= 2 || safe_to_restart(&app) {
                    break;
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        } else {
            std::thread::sleep(Duration::from_millis(250));
        }

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

fn safe_to_restart(app: &tauri::AppHandle) -> bool {
    app.webview_windows().iter().all(|(label, window)| {
        !requires_user_attention(label) || !window.is_visible().unwrap_or(false)
    })
}

fn requires_user_attention(label: &str) -> bool {
    label == "settings" || label == "translate-result" || label.starts_with("text-editor-")
}

fn notify_restart(app: &tauri::AppHandle, immediate: bool) {
    use tauri_plugin_notification::NotificationExt;

    let body = if immediate {
        "WebView2 连接已失效，程序将自动重启恢复"
    } else {
        "WebView2 已更新；关闭设置、编辑器或翻译窗口后将自动重启"
    };
    let _ = app
        .notification()
        .builder()
        .title("ElegantClipboard 正在恢复")
        .body(body)
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
    let path = recovery_marker_path();
    let now = epoch_secs();
    let current = std::fs::read_to_string(&path)
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
        && let Err(error) = std::fs::write(&path, raw)
    {
        tracing::warn!(%error, "Failed to persist WebView recovery marker");
    }
    true
}

fn next_recovery_marker(current: Option<RecoveryMarker>, now: u64) -> Option<RecoveryMarker> {
    let mut marker = current
        .filter(|marker| {
            now.saturating_sub(marker.first_attempt_epoch_secs) <= RECOVERY_WINDOW_SECS
        })
        .unwrap_or(RecoveryMarker {
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
fn register_environment_handlers(
    app: &tauri::AppHandle,
    environment: webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Environment,
) {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Environment5;
    use webview2_com::{
        BrowserProcessExitedEventHandler, NewBrowserVersionAvailableEventHandler, take_pwstr,
    };
    use windows_core::{Interface, PWSTR};

    let mut raw_version = PWSTR::null();
    if unsafe { environment.BrowserVersionString(&mut raw_version) }.is_ok() {
        let version = take_pwstr(raw_version);
        *LOADED_VERSION.write() = Some(version.clone());
        tracing::info!(loaded_version = %version, "Registered WebView2 environment");
    }

    let update_app = app.clone();
    let update_handler = NewBrowserVersionAvailableEventHandler::create(Box::new(move |_, _| {
        schedule_restart(&update_app, "new_browser_version_available", false);
        Ok(())
    }));
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
                        schedule_restart(&exit_app, "browser_process_exited_failed", true);
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
                true,
            );
        } else if is_renderer_unresponsive(kind.0) {
            if record_renderer_unresponsive(&failure_label) {
                schedule_restart(
                    &failure_app,
                    &format!("renderer_unresponsive:{}", failure_label),
                    true,
                );
            } else {
                tracing::warn!(label = %failure_label, "Reloading unresponsive WebView2 renderer");
                if let Err(error) = unsafe { reload_webview.Reload() } {
                    tracing::error!(label = %failure_label, %error, "Failed to reload unresponsive WebView2 renderer");
                    schedule_restart(
                        &failure_app,
                        &format!("renderer_reload_failed:{}", failure_label),
                        true,
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

    #[test]
    fn runtime_version_change_requires_restart() {
        assert!(!runtime_version_changed(None, "151.0"));
        assert!(!runtime_version_changed(Some("151.0"), "151.0"));
        assert!(runtime_version_changed(Some("150.0"), "151.0"));
    }

    #[test]
    fn recovery_attempts_reset_after_window_and_then_fuse() {
        let now = RECOVERY_WINDOW_SECS + 100;
        let expired = RecoveryMarker {
            first_attempt_epoch_secs: 1,
            attempts: MAX_RECOVERY_ATTEMPTS,
        };
        let reset = next_recovery_marker(Some(expired), now).unwrap();
        assert_eq!(reset.attempts, 1);
        assert_eq!(reset.first_attempt_epoch_secs, now);

        let exhausted = RecoveryMarker {
            first_attempt_epoch_secs: now,
            attempts: MAX_RECOVERY_ATTEMPTS,
        };
        assert!(next_recovery_marker(Some(exhausted), now).is_none());
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
