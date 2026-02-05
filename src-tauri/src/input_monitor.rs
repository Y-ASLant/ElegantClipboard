//! Global input monitoring for click-outside detection
//!
//! This module uses rdev to monitor global mouse events.
//! When a click is detected outside the main window, the window is hidden.
//! This is necessary because the window is set to non-focusable (to not steal focus),
//! which means Tauri's onFocusChanged event never fires.

use parking_lot::Mutex;
use rdev::{listen, Event, EventType};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::thread::{self, JoinHandle};
use tauri::WebviewWindow;
use tracing::{info, warn};

/// Main window reference for click detection
static MAIN_WINDOW: Mutex<Option<WebviewWindow>> = Mutex::new(None);

/// Whether mouse monitoring is currently active
static MOUSE_MONITORING_ENABLED: AtomicBool = AtomicBool::new(false);

/// Whether the window is pinned (won't hide on click outside)
static WINDOW_PINNED: AtomicBool = AtomicBool::new(false);

/// Whether the monitor thread is running
static MONITOR_RUNNING: AtomicBool = AtomicBool::new(false);

/// Current cursor position (stored as i64 to use atomics, multiply by 100 for precision)
/// This avoids lock contention on high-frequency mouse move events
static CURSOR_X: AtomicI64 = AtomicI64::new(0);
static CURSOR_Y: AtomicI64 = AtomicI64::new(0);

/// Thread handle for cleanup
static THREAD_HANDLE: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

/// Initialize input monitor with main window reference
pub fn init(window: WebviewWindow) {
    *MAIN_WINDOW.lock() = Some(window);
}

/// Start the global input monitoring thread
pub fn start_monitoring() {
    // Prevent multiple starts
    if MONITOR_RUNNING.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        warn!("Input monitor already running");
        return;
    }

    let handle = thread::spawn(|| {
        if let Err(error) = listen(move |event| {
            handle_input_event(event);
        }) {
            eprintln!("Input monitor error: {:?}", error);
        }
        MONITOR_RUNNING.store(false, Ordering::SeqCst);
    });
    
    *THREAD_HANDLE.lock() = Some(handle);
    info!("Input monitor started");
}

/// Stop the input monitor (note: rdev::listen cannot be gracefully stopped)
#[allow(dead_code)]
pub fn stop_monitoring() {
    MONITOR_RUNNING.store(false, Ordering::SeqCst);
    // Note: rdev::listen runs in a blocking loop and cannot be interrupted gracefully
    // The thread will only stop when the application exits
}

/// Enable mouse click monitoring (call when window becomes visible)
pub fn enable_mouse_monitoring() {
    MOUSE_MONITORING_ENABLED.store(true, Ordering::Relaxed);
}

/// Disable mouse click monitoring (call when window is hidden)
pub fn disable_mouse_monitoring() {
    MOUSE_MONITORING_ENABLED.store(false, Ordering::Relaxed);
}

/// Check if mouse monitoring is enabled
#[allow(dead_code)]
pub fn is_mouse_monitoring_enabled() -> bool {
    MOUSE_MONITORING_ENABLED.load(Ordering::Relaxed)
}

/// Set window pinned state (when pinned, window won't hide on click outside)
pub fn set_window_pinned(pinned: bool) {
    WINDOW_PINNED.store(pinned, Ordering::Relaxed);
}

/// Check if window is pinned
pub fn is_window_pinned() -> bool {
    WINDOW_PINNED.load(Ordering::Relaxed)
}

/// Get current cursor position
#[allow(dead_code)]
pub fn get_cursor_position() -> (f64, f64) {
    let x = CURSOR_X.load(Ordering::Relaxed) as f64;
    let y = CURSOR_Y.load(Ordering::Relaxed) as f64;
    (x, y)
}

/// Handle input events with throttling for mouse moves
fn handle_input_event(event: Event) {
    match event.event_type {
        EventType::MouseMove { x, y } => {
            // Only track mouse position when monitoring is enabled
            // This significantly reduces CPU usage when window is hidden
            if MOUSE_MONITORING_ENABLED.load(Ordering::Relaxed) {
                // Use atomic store - no lock needed
                CURSOR_X.store(x as i64, Ordering::Relaxed);
                CURSOR_Y.store(y as i64, Ordering::Relaxed);
            }
        }
        EventType::ButtonPress(button) => {
            // Only handle left and right clicks
            if matches!(button, rdev::Button::Left | rdev::Button::Right) {
                handle_click_outside();
            }
        }
        _ => {}
    }
}

/// Check if cursor is outside the window bounds
fn is_mouse_outside_window(window: &WebviewWindow) -> bool {
    // Get cursor position from atomics - no lock needed
    let cursor_x = CURSOR_X.load(Ordering::Relaxed) as f64;
    let cursor_y = CURSOR_Y.load(Ordering::Relaxed) as f64;
    
    // Get window position and size
    let position = match window.outer_position() {
        Ok(pos) => pos,
        Err(_) => return false,
    };
    
    let size = match window.outer_size() {
        Ok(s) => s,
        Err(_) => return false,
    };
    
    let win_x = position.x as f64;
    let win_y = position.y as f64;
    let win_width = size.width as f64;
    let win_height = size.height as f64;
    
    cursor_x < win_x || cursor_x > win_x + win_width
        || cursor_y < win_y || cursor_y > win_y + win_height
}

/// Handle click outside event - hide window if click is outside
fn handle_click_outside() {
    // Only process if monitoring is enabled
    if !MOUSE_MONITORING_ENABLED.load(Ordering::Relaxed) {
        return;
    }
    
    // Don't hide if window is pinned
    if WINDOW_PINNED.load(Ordering::Relaxed) {
        return;
    }
    
    if let Some(window) = MAIN_WINDOW.lock().as_ref() {
        // Check if window is visible and click is outside
        if window.is_visible().unwrap_or(false) && is_mouse_outside_window(window) {
            let _ = window.hide();
            // Update window state
            crate::keyboard_hook::set_window_state(crate::keyboard_hook::WindowState::Hidden);
            // Disable monitoring since window is now hidden
            disable_mouse_monitoring();
        }
    }
}
