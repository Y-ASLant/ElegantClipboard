use crate::config::AppConfig;
use tauri::{Emitter, Manager};

/// Monotonic sequence for text-preview updates; used to cancel stale delayed retries.
pub(crate) static TEXT_PREVIEW_UPDATE_SEQ: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

#[tauri::command]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn show_image_preview(
    app: tauri::AppHandle,
    image_path: String,
    img_width: f64,
    img_height: f64,
    offset_y: f64,
    win_x: f64,
    win_y: f64,
    win_width: f64,
    win_height: f64,
    align: Option<String>,
) -> Result<(), String> {
    let mut newly_created = false;
    let window = if let Some(w) = app.get_webview_window("image-preview") {
        w
    } else {
        newly_created = true;
        tauri::WebviewWindowBuilder::new(
            &app,
            "image-preview",
            tauri::WebviewUrl::App("/image-preview.html".into()),
        )
        .title("")
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .visible(false)
        .build()
        .map_err(|e| format!("创建预览窗口失败: {}", e))?
    };

    let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize {
        width: win_width as u32,
        height: win_height as u32,
    }));
    let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
        x: win_x as i32,
        y: win_y as i32,
    }));

    if newly_created {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }

    let _ = window.set_always_on_top(true);
    // Make transparent areas click-through so screenshot tools won't detect the window
    let _ = window.set_ignore_cursor_events(true);

    let _ = window.emit(
        "image-preview-update",
        serde_json::json!({
            "imagePath": image_path,
            "width": img_width,
            "height": img_height,
            "offsetY": offset_y,
            "align": align.as_deref().unwrap_or("left"),
        }),
    );

    let _ = window.show();
    Ok(())
}

#[tauri::command]
pub async fn hide_image_preview(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("image-preview") {
        let _ = window.hide();
        let _ = window.emit("image-preview-clear", ());
    }
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn show_text_preview(
    app: tauri::AppHandle,
    text: String,
    win_x: f64,
    win_y: f64,
    win_width: f64,
    win_height: f64,
    align: Option<String>,
    theme: Option<String>,
    sharp_corners: Option<bool>,
    window_effect: Option<String>,
) -> Result<(), String> {
    let seq = TEXT_PREVIEW_UPDATE_SEQ.fetch_add(1, std::sync::atomic::Ordering::AcqRel) + 1;
    let mut newly_created = false;
    let window = if let Some(w) = app.get_webview_window("text-preview") {
        w
    } else {
        newly_created = true;
        let w = tauri::WebviewWindowBuilder::new(
            &app,
            "text-preview",
            tauri::WebviewUrl::App("/text-preview.html".into()),
        )
        .title("")
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .visible(false)
        .build()
        .map_err(|e| format!("创建文本预览窗口失败: {}", e))?;

        // Apply system-level window effect to match main window
        apply_preview_window_effect(&w, window_effect.as_deref());

        w
    };

    let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize {
        width: win_width as u32,
        height: win_height as u32,
    }));
    let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
        x: win_x as i32,
        y: win_y as i32,
    }));

    let _ = window.set_always_on_top(true);
    // Keep text preview click-through; scrolling is driven from main window with Ctrl+Wheel.
    let _ = window.set_ignore_cursor_events(true);

    let update_payload = serde_json::json!({
        "text": text,
        "align": align.as_deref().unwrap_or("left"),
        "theme": theme.as_deref().unwrap_or("light"),
        "sharpCorners": sharp_corners.unwrap_or(false),
    });
    let _ = window.emit("text-preview-update", update_payload.clone());
    let _ = window.show();

    if newly_created {
        let window_clone = window.clone();
        tauri::async_runtime::spawn(async move {
            for delay_ms in [120_u64, 260, 420, 680] {
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                if TEXT_PREVIEW_UPDATE_SEQ.load(std::sync::atomic::Ordering::Acquire) != seq {
                    return;
                }
                let _ = window_clone.emit("text-preview-update", update_payload.clone());
            }
        });
    }

    Ok(())
}

#[tauri::command]
pub async fn hide_text_preview(app: tauri::AppHandle) {
    TEXT_PREVIEW_UPDATE_SEQ.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    if let Some(window) = app.get_webview_window("text-preview") {
        let _ = window.hide();
        let _ = window.emit("text-preview-clear", ());
    }
}

#[tauri::command]
pub async fn open_text_editor_window(app: tauri::AppHandle, id: i64) -> Result<(), String> {
    let label = format!("text-editor-{}", id);

    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }

    let window = tauri::WebviewWindowBuilder::new(
        &app,
        &label,
        tauri::WebviewUrl::App(format!("/editor?id={}", id).into()),
    )
    .title("编辑")
    .inner_size(600.0, 460.0)
    .min_inner_size(400.0, 300.0)
    .decorations(false)
    .transparent(true)
    .shadow(true)
    .visible(false)
    .resizable(true)
    .center()
    .build()
    .map_err(|e| format!("创建编辑器窗口失败: {}", e))?;

    let _ = window;
    Ok(())
}

#[tauri::command]
pub async fn open_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    crate::tray::open_settings_window(&app)
}

#[tauri::command]
pub fn is_log_to_file_enabled() -> bool {
    AppConfig::load().is_log_to_file()
}

/// Apply system-level window effect (Acrylic/Mica/Tabbed) to a preview window.
#[cfg(target_os = "windows")]
fn apply_preview_window_effect(window: &tauri::WebviewWindow, effect: Option<&str>) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongW, SetWindowLongW, SetWindowPos, GWL_EXSTYLE, WS_EX_LAYERED,
        SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    };

    let effect = match effect {
        Some(e) if e != "none" => e,
        _ => return,
    };

    let Ok(raw_hwnd) = window.hwnd() else { return };
    let hwnd = HWND(raw_hwnd.0 as *mut _);

    // Remove WS_EX_LAYERED so composition effects can render
    unsafe {
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        if (ex_style as u32) & WS_EX_LAYERED.0 != 0 {
            SetWindowLongW(hwnd, GWL_EXSTYLE, ((ex_style as u32) & !WS_EX_LAYERED.0) as i32);
            let _ = SetWindowPos(
                hwnd, None, 0, 0, 0, 0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
    }

    let result = match effect {
        "mica" => window_vibrancy::apply_mica(window, None),
        "acrylic" => window_vibrancy::apply_acrylic(window, Some((0, 0, 0, 0))),
        "tabbed" => window_vibrancy::apply_tabbed(window, None),
        _ => return,
    };

    if let Err(e) = result {
        tracing::debug!("Preview window effect '{}' failed: {}", effect, e);
    }
}

#[cfg(not(target_os = "windows"))]
fn apply_preview_window_effect(_window: &tauri::WebviewWindow, _effect: Option<&str>) {}

#[tauri::command]
pub fn set_log_to_file(enabled: bool) -> Result<(), String> {
    let mut config = AppConfig::load();
    config.log_to_file = Some(enabled);
    config.save()
}

#[tauri::command]
pub fn get_log_file_path() -> String {
    AppConfig::load().get_log_path().to_string_lossy().to_string()
}
