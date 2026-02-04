//! Window positioning utilities
//!
//! Provides functions to position the window at the cursor location
//! with smart boundary detection to keep the window within screen bounds.

use tauri::{PhysicalPosition, PhysicalSize, WebviewWindow};
use tracing::debug;

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::POINT;
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

/// Get current cursor position
#[cfg(target_os = "windows")]
pub fn get_cursor_position() -> (i32, i32) {
    let mut point = POINT { x: 0, y: 0 };
    unsafe {
        if GetCursorPos(&mut point).is_ok() {
            return (point.x, point.y);
        }
    }
    // Fallback to input_monitor's cached position
    let (x, y) = crate::input_monitor::get_cursor_position();
    (x as i32, y as i32)
}

#[cfg(not(target_os = "windows"))]
pub fn get_cursor_position() -> (i32, i32) {
    let (x, y) = crate::input_monitor::get_cursor_position();
    (x as i32, y as i32)
}

/// Position window at cursor with smart boundary detection
pub fn position_at_cursor(window: &WebviewWindow) -> Result<(), String> {
    let (cursor_x, cursor_y) = get_cursor_position();
    
    // Get window size
    let window_size = window.outer_size().map_err(|e| e.to_string())?;
    
    // Get monitor at cursor position
    let monitor = get_monitor_at_cursor(window, cursor_x, cursor_y)?;
    
    // Calculate best position
    let position = calculate_best_position(
        cursor_x,
        cursor_y,
        window_size,
        &monitor,
    );
    
    window.set_position(position).map_err(|e| e.to_string())?;
    debug!("Window positioned at ({}, {})", position.x, position.y);
    
    Ok(())
}

/// Monitor info for positioning calculations
struct MonitorInfo {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

/// Get monitor containing the cursor position
fn get_monitor_at_cursor(window: &WebviewWindow, cursor_x: i32, cursor_y: i32) -> Result<MonitorInfo, String> {
    // Try to get all monitors
    if let Ok(monitors) = window.available_monitors() {
        for m in monitors {
            let pos = m.position();
            let size = m.size();
            let mx = pos.x;
            let my = pos.y;
            let mw = size.width as i32;
            let mh = size.height as i32;
            
            if cursor_x >= mx && cursor_x < mx + mw && 
               cursor_y >= my && cursor_y < my + mh {
                return Ok(MonitorInfo {
                    x: mx,
                    y: my,
                    width: mw,
                    height: mh,
                });
            }
        }
    }
    
    // Fallback to primary monitor
    if let Ok(Some(monitor)) = window.primary_monitor() {
        let pos = monitor.position();
        let size = monitor.size();
        return Ok(MonitorInfo {
            x: pos.x,
            y: pos.y,
            width: size.width as i32,
            height: size.height as i32,
        });
    }
    
    // Ultimate fallback
    Ok(MonitorInfo {
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
    })
}

/// Calculate optimal window position near cursor
fn calculate_best_position(
    cursor_x: i32,
    cursor_y: i32,
    window_size: PhysicalSize<u32>,
    monitor: &MonitorInfo,
) -> PhysicalPosition<i32> {
    const MARGIN: i32 = 12; // Gap between cursor and window
    
    let w = window_size.width as i32;
    let h = window_size.height as i32;
    
    // Default position: bottom-right of cursor
    let mut x = cursor_x + MARGIN;
    let mut y = cursor_y + MARGIN;
    
    // If window exceeds right boundary, move to left of cursor
    if x + w > monitor.x + monitor.width {
        x = cursor_x - w - MARGIN;
    }
    
    // If window exceeds bottom boundary, move above cursor
    if y + h > monitor.y + monitor.height {
        y = cursor_y - h - MARGIN;
    }
    
    // Ensure window stays within monitor bounds
    x = x.max(monitor.x).min(monitor.x + monitor.width - w);
    y = y.max(monitor.y).min(monitor.y + monitor.height - h);
    
    PhysicalPosition::new(x, y)
}
