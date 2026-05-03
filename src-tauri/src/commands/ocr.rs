use std::sync::Arc;
use tauri::{Emitter, Manager};
use super::AppState;
use crate::database;

/// 暂存待显示的 OCR 文本（供新窗口挂载后主动获取）
static PENDING_OCR_TEXT: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

/// 百度 access_token 缓存: (api_key, secret_key) -> (token, 过期时间)
static BAIDU_TOKEN_CACHE: std::sync::Mutex<Option<(String, String, String, std::time::Instant)>> =
    std::sync::Mutex::new(None);

/// 获取百度 access_token（优先返回缓存）
fn get_baidu_access_token(
    client: &reqwest::blocking::Client,
    api_key: &str,
    secret_key: &str,
) -> Result<String, String> {
    // 检查缓存
    if let Some((cached_ak, cached_sk, cached_token, expires_at)) =
        BAIDU_TOKEN_CACHE.lock().unwrap().as_ref()
    {
        if cached_ak == api_key
            && cached_sk == secret_key
            && std::time::Instant::now() < *expires_at
        {
            return Ok(cached_token.clone());
        }
    }

    // 请求新 token
    let token_url = format!(
        "https://aip.baidubce.com/oauth/2.0/token?grant_type=client_credentials&client_id={}&client_secret={}",
        api_key, secret_key
    );
    let token_resp = client
        .post(&token_url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .send()
        .map_err(|e| format!("获取百度 OCR token 失败: {}", e))?;
    let token_text = token_resp
        .text()
        .map_err(|e| format!("读取 token 失败: {}", e))?;
    let token_val: serde_json::Value =
        serde_json::from_str(&token_text).map_err(|e| format!("解析 token 失败: {}", e))?;
    let access_token = token_val["access_token"]
        .as_str()
        .ok_or_else(|| format!("获取 access_token 失败: {}", token_text))?
        .to_string();

    // expires_in 通常为 2592000 (30天)，提前 1 小时过期以确保安全
    let expires_in = token_val["expires_in"].as_u64().unwrap_or(2592000);
    let expires_at =
        std::time::Instant::now() + std::time::Duration::from_secs(expires_in.saturating_sub(3600));

    // 写入缓存
    *BAIDU_TOKEN_CACHE.lock().unwrap() = Some((
        api_key.to_string(),
        secret_key.to_string(),
        access_token.clone(),
        expires_at,
    ));

    Ok(access_token)
}

/// 截取全屏，直接写出 BMP 文件字节（跳过 image crate，零编码开销）
#[cfg(target_os = "windows")]
fn capture_full_screen() -> Result<Vec<u8>, String> {
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject,
        GetDIBits, SelectObject, BITMAPINFO, BITMAPINFOHEADER,
        BI_RGB, DIB_RGB_COLORS, SRCCOPY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetDesktopWindow, GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
        SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    };

    unsafe {
        let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);

        if width <= 0 || height <= 0 {
            return Err("获取屏幕尺寸失败".to_string());
        }

        let desktop = GetDesktopWindow();
        let hdc_screen = windows::Win32::Graphics::Gdi::GetDC(Some(desktop));
        if hdc_screen.is_invalid() {
            return Err("获取屏幕 DC 失败".to_string());
        }

        let hdc_mem = CreateCompatibleDC(Some(hdc_screen));
        let hbm = CreateCompatibleBitmap(hdc_screen, width, height);
        let old_bm = SelectObject(hdc_mem, hbm.into());

        let _ = BitBlt(hdc_mem, 0, 0, width, height, Some(hdc_screen), x, y, SRCCOPY);

        SelectObject(hdc_mem, old_bm);

        // 读取位图数据（bottom-up，BMP 原生格式，不需要翻转）
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: height, // bottom-up (BMP 原生方向)
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let row_bytes = (width as usize) * 4;
        let pixel_size = row_bytes * height as usize;
        let mut pixels = vec![0u8; pixel_size];
        let lines = GetDIBits(
            hdc_mem,
            hbm,
            0,
            height as u32,
            Some(pixels.as_mut_ptr() as *mut _),
            &bmi as *const _ as *mut _,
            DIB_RGB_COLORS,
        );

        let _ = DeleteObject(hbm.into());
        let _ = DeleteDC(hdc_mem);
        windows::Win32::Graphics::Gdi::ReleaseDC(Some(desktop), hdc_screen);

        if lines == 0 {
            return Err("读取屏幕像素失败".to_string());
        }

        // 直接拼装 BMP 文件（BGRA 是 BMP 原生色序，无需转换）
        let file_header_size = 14u32;
        let info_header_size = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        let file_size = file_header_size + info_header_size + pixel_size as u32;

        let mut bmp = Vec::with_capacity(file_size as usize);
        // BITMAPFILEHEADER
        bmp.extend_from_slice(b"BM");
        bmp.extend_from_slice(&file_size.to_le_bytes());
        bmp.extend_from_slice(&0u16.to_le_bytes()); // reserved1
        bmp.extend_from_slice(&0u16.to_le_bytes()); // reserved2
        bmp.extend_from_slice(&(file_header_size + info_header_size).to_le_bytes()); // pixel offset
        // BITMAPINFOHEADER
        bmp.extend_from_slice(&info_header_size.to_le_bytes());
        bmp.extend_from_slice(&width.to_le_bytes());
        bmp.extend_from_slice(&height.to_le_bytes()); // bottom-up
        bmp.extend_from_slice(&1u16.to_le_bytes()); // planes
        bmp.extend_from_slice(&32u16.to_le_bytes()); // bpp
        bmp.extend_from_slice(&0u32.to_le_bytes()); // compression (BI_RGB)
        bmp.extend_from_slice(&(pixel_size as u32).to_le_bytes());
        bmp.extend_from_slice(&0i32.to_le_bytes()); // x ppm
        bmp.extend_from_slice(&0i32.to_le_bytes()); // y ppm
        bmp.extend_from_slice(&0u32.to_le_bytes()); // colors used
        bmp.extend_from_slice(&0u32.to_le_bytes()); // colors important
        // Pixel data
        bmp.extend_from_slice(&pixels);

        Ok(bmp)
    }
}

#[cfg(not(target_os = "windows"))]
fn capture_full_screen() -> Result<Vec<u8>, String> {
    Err("当前平台不支持截图".to_string())
}

/// 截取全屏并保存到临时文件，返回路径
#[tauri::command]
pub async fn ocr_capture_screen() -> Result<String, String> {
    let png_data = tokio::task::spawn_blocking(capture_full_screen)
        .await
        .map_err(|e| format!("截图任务失败: {}", e))??;

    let tmp_dir = std::env::temp_dir().join("ElegantClipboard");
    std::fs::create_dir_all(&tmp_dir)
        .map_err(|e| format!("创建临时目录失败: {}", e))?;
    let tmp_path = tmp_dir.join("ocr_screenshot.bmp");
    std::fs::write(&tmp_path, &png_data)
        .map_err(|e| format!("保存截图失败: {}", e))?;

    Ok(tmp_path.to_string_lossy().to_string())
}

/// 裁剪截图指定区域并返回 base64
#[tauri::command]
pub async fn ocr_crop_region(
    image_path: String,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let img = image::open(&image_path)
            .map_err(|e| format!("打开截图失败: {}", e))?;
        let cropped = img.crop_imm(x, y, width, height);
        let mut buf = std::io::Cursor::new(Vec::new());
        cropped.write_to(&mut buf, image::ImageFormat::Png)
            .map_err(|e| format!("裁剪编码失败: {}", e))?;
        Ok(base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            buf.into_inner(),
        ))
    })
    .await
    .map_err(|e| format!("裁剪任务失败: {}", e))?
}

/// 构建 HTTP 客户端（根据代理配置）
fn build_client(proxy_mode: &str, proxy_url: &str) -> Result<reqwest::blocking::Client, String> {
    let builder = reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(60))
        .danger_accept_invalid_certs(true);
    let builder = crate::proxy::apply_proxy(builder, proxy_mode, proxy_url)?;
    builder.build().map_err(|e| format!("创建 HTTP 客户端失败: {}", e))
}

/// 调用百度 OCR API 识别图片中的文字
#[tauri::command]
pub async fn ocr_recognize_baidu(
    image_base64: String,
    api_key: String,
    secret_key: String,
    proxy_mode: String,
    proxy_url: String,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let client = build_client(&proxy_mode, &proxy_url)?;

        // 1. 获取 access_token（优先返回缓存）
        let access_token = get_baidu_access_token(&client, &api_key, &secret_key)?;

        // 2. 调用通用文字识别（高精度版）
        let ocr_url = format!(
            "https://aip.baidubce.com/rest/2.0/ocr/v1/accurate_basic?access_token={}",
            access_token
        );
        let body = format!("image={}", urlencoded(&image_base64));
        let ocr_resp = client
            .post(&ocr_url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .map_err(|e| format!("百度 OCR 请求失败: {}", e))?;
        let status = ocr_resp.status();
        let ocr_text = ocr_resp.text().map_err(|e| format!("读取 OCR 结果失败: {}", e))?;
        if !status.is_success() {
            return Err(format!("百度 OCR 错误 ({}): {}", status, ocr_text));
        }
        let ocr_val: serde_json::Value = serde_json::from_str(&ocr_text)
            .map_err(|e| format!("解析 OCR 结果失败: {}", e))?;
        if let Some(err_code) = ocr_val["error_code"].as_i64() {
            let err_msg = ocr_val["error_msg"].as_str().unwrap_or("未知错误");
            return Err(format!("百度 OCR 错误 ({}): {}", err_code, err_msg));
        }
        let words_result = ocr_val["words_result"]
            .as_array()
            .ok_or_else(|| "OCR 结果格式异常".to_string())?;
        let lines: Vec<&str> = words_result
            .iter()
            .filter_map(|r| r["words"].as_str())
            .collect();
        Ok(lines.join("\n"))
    })
    .await
    .map_err(|e| format!("OCR 任务失败: {}", e))?
}

/// URL 编码
fn urlencoded(s: &str) -> String {
    let mut result = String::new();
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(byte as char);
            }
            _ => {
                result.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    result
}

/// 打开 OCR 截图选区窗口（全屏覆盖层）
/// 如果窗口已存在则复用（通过事件通知前端更新），否则首次创建。
#[tauri::command]
pub async fn open_ocr_screenshot_window(app: tauri::AppHandle, screenshot_path: String) -> Result<(), String> {
    // 复用已有窗口：先移到屏幕外（防止 WebView2 缓存帧闪烁旧选区），再发事件让前端加载新图片
    if let Some(w) = app.get_webview_window("ocr-screenshot") {
        let _ = w.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(-10000, -10000)));
        let _ = w.emit("ocr-screenshot-update", &screenshot_path);
        return Ok(());
    }

    // 首次创建
    let url = format!("/ocr-screenshot?path={}", urlencoded(&screenshot_path));
    let _window = tauri::WebviewWindowBuilder::new(
        &app,
        "ocr-screenshot",
        tauri::WebviewUrl::App(url.into()),
    )
    .title("")
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .resizable(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .focused(true)
    .visible(false)
    // 初始放到屏幕外，避免 WebView2 初始化时的白屏闪烁
    .position(-10000.0, -10000.0)
    .inner_size(1.0, 1.0)
    .build()
    .map_err(|e| format!("创建截图窗口失败: {}", e))?;

    // 前端图片加载完成后会调用 ocr_screenshot_ready 来设置正确位置并显示

    Ok(())
}

/// 前端截图加载完成后调用，将窗口移到正确位置并显示
#[tauri::command]
pub async fn ocr_screenshot_ready(app: tauri::AppHandle) -> Result<(), String> {
    let window = app.get_webview_window("ocr-screenshot")
        .ok_or("截图窗口不存在")?;

    // 获取虚拟屏幕尺寸和位置
    #[cfg(target_os = "windows")]
    let (vx, vy, vw, vh) = unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{
            GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
            SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
        };
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    #[cfg(not(target_os = "windows"))]
    let (vx, vy, vw, vh) = (0, 0, 1920, 1080);

    let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize {
        width: vw as u32,
        height: vh as u32,
    }));
    let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(vx, vy)));
    let _ = window.set_always_on_top(true);
    let _ = window.show();
    let _ = window.set_focus();

    Ok(())
}

/// 打开 OCR识别结果窗口
#[tauri::command]
pub async fn open_ocr_result_window(app: tauri::AppHandle, text: String) -> Result<(), String> {
    let label = "ocr-result";

    if let Some(window) = app.get_webview_window(label) {
        let _ = window.emit("ocr-result-update", &text);
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_always_on_top(true);
        let _ = window.set_focus();
        return Ok(());
    }

    let _window = tauri::WebviewWindowBuilder::new(
        &app,
        label,
        tauri::WebviewUrl::App("/ocr-result".into()),
    )
    .title("OCR识别结果")
    .inner_size(520.0, 420.0)
    .min_inner_size(360.0, 300.0)
    .decorations(false)
    .transparent(true)
    .shadow(true)
    .visible(false)
    .resizable(true)
    .always_on_top(true)
    .center()
    .build()
    .map_err(|e| format!("创建 OCR 结果窗口失败: {}", e))?;

    // 暂存文本，前端挂载后通过 get_pending_ocr_text 命令获取
    *PENDING_OCR_TEXT.lock().unwrap() = text;

    Ok(())
}

/// 前端挂载后调用，获取暂存的 OCR 文本
#[tauri::command]
pub async fn get_pending_ocr_text() -> Result<String, String> {
    let text = std::mem::take(&mut *PENDING_OCR_TEXT.lock().unwrap());
    Ok(text)
}

/// 注册 OCR 快捷键
pub fn register_ocr_shortcut(app: &tauri::AppHandle) {
    let state = match app.try_state::<Arc<AppState>>() {
        Some(s) => s,
        None => return,
    };
    let settings_repo = database::SettingsRepository::new(&state.db);

    let enabled = settings_repo
        .get("ocr_enabled")
        .ok()
        .flatten()
        .map(|v| v == "true")
        .unwrap_or(false);
    if !enabled {
        return;
    }

    let shortcut_str = match settings_repo.get("ocr_shortcut").ok().flatten() {
        Some(s) if !s.is_empty() => s,
        _ => return,
    };

    let registered = crate::hotkey::register(
        &shortcut_str,
        std::sync::Arc::new(|app, key_state| {
            if key_state == crate::hotkey::KeyState::Pressed {
                let app = app.clone();
                std::thread::spawn(move || {
                    trigger_ocr_capture(&app);
                });
            }
        }),
    );

    if !registered {
        tracing::warn!("OCR 快捷键格式无效: {}", shortcut_str);
    } else {
        tracing::info!("OCR 快捷键已注册: {}", shortcut_str);
    }
}

/// 注销 OCR 快捷键
pub fn unregister_ocr_shortcut(app: &tauri::AppHandle) {
    let state = match app.try_state::<Arc<AppState>>() {
        Some(s) => s,
        None => return,
    };
    let settings_repo = database::SettingsRepository::new(&state.db);

    if let Some(shortcut_str) = settings_repo.get("ocr_shortcut").ok().flatten() {
        if !shortcut_str.is_empty() {
            crate::hotkey::unregister(&shortcut_str);
        }
    }
}

/// 触发 OCR 截图流程
fn trigger_ocr_capture(app: &tauri::AppHandle) {
    // 隐藏主窗口并同步状态
    if let Some(main_win) = app.get_webview_window("main") {
        if main_win.is_visible().unwrap_or(false) {
            let _ = main_win.hide();
            crate::keyboard_hook::set_window_state(crate::keyboard_hook::WindowState::Hidden);
            crate::input_monitor::disable_mouse_monitoring();
        }
    }

    // 短暂延迟确保窗口隐藏
    std::thread::sleep(std::time::Duration::from_millis(30));

    match capture_full_screen() {
        Ok(png_data) => {
            let tmp_dir = std::env::temp_dir().join("ElegantClipboard");
            let _ = std::fs::create_dir_all(&tmp_dir);
            let tmp_path = tmp_dir.join("ocr_screenshot.bmp");
            if let Err(e) = std::fs::write(&tmp_path, &png_data) {
                tracing::error!("保存截图失败: {}", e);
                return;
            }
            let path_str = tmp_path.to_string_lossy().to_string();
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = open_ocr_screenshot_window(app, path_str).await {
                    tracing::error!("打开截图窗口失败: {}", e);
                }
            });
        }
        Err(e) => {
            tracing::error!("截图失败: {}", e);
        }
    }
}

/// 更新 OCR 快捷键（前端调用）
#[tauri::command]
pub async fn update_ocr_shortcut(
    app: tauri::AppHandle,
    new_shortcut: String,
) -> Result<(), String> {
    // 注销旧的
    unregister_ocr_shortcut(&app);

    if new_shortcut.is_empty() {
        return Ok(());
    }

    // 保存到数据库
    let state = app.state::<Arc<AppState>>();
    let settings_repo = database::SettingsRepository::new(&state.db);
    settings_repo
        .set("ocr_shortcut", &new_shortcut)
        .map_err(|e| format!("保存 OCR 快捷键失败: {}", e))?;

    // 检查 OCR 是否启用
    let enabled = settings_repo
        .get("ocr_enabled")
        .ok()
        .flatten()
        .map(|v| v == "true")
        .unwrap_or(false);
    if !enabled {
        return Ok(());
    }

    // 注册新的
    register_ocr_shortcut(&app);

    Ok(())
}

/// 切换 OCR 启用状态后重新注册/注销快捷键
#[tauri::command]
pub async fn ocr_toggle_enabled(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    if enabled {
        register_ocr_shortcut(&app);
    } else {
        unregister_ocr_shortcut(&app);
    }
    Ok(())
}
