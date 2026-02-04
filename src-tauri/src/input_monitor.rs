//! Global input monitoring for click-outside detection
//!
//! This module uses rdev to monitor global mouse events.
//! When a click is detected outside the main window, the window is hidden.
//! This is necessary because the window is set to non-focusable (to not steal focus),
//! which means Tauri's onFocusChanged event never fires.

use parking_lot::Mutex;
use rdev::{listen, Event, EventType};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use tauri::WebviewWindow;
use tracing::info;

/// Main window reference for click detection
static MAIN_WINDOW: Mutex<Option<WebviewWindow>> = Mutex::new(None);

/// Whether mouse monitoring is currently active
static MOUSE_MONITORING_ENABLED: AtomicBool = AtomicBool::new(false);

/// Current cursor position
static CURSOR_POSITION: Mutex<(f64, f64)> = Mutex::new((0.0, 0.0));

/// Initialize input monitor with main window reference
pub fn init(window: WebviewWindow) {
    *MAIN_WINDOW.lock() = Some(window);
}

/// Start the global input monitoring thread
pub fn start_monitoring() {
    thread::spawn(|| {
        if let Err(error) = listen(move |event| {
            handle_input_event(event);
        }) {
            eprintln!("Input monitor error: {:?}", error);
        }
    });
    info!("Input monitor started");
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

/// Get current cursor position
#[allow(dead_code)]
pub fn get_cursor_position() -> (f64, f64) {
    *CURSOR_POSITION.lock()
}

/// Handle input events
fn handle_input_event(event: Event) {
    match event.event_type {
        EventType::MouseMove { x, y } => {
            *CURSOR_POSITION.lock() = (x, y);
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
    let (cursor_x, cursor_y) = *CURSOR_POSITION.lock();
    
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
