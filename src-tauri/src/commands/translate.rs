use std::sync::Arc;
use tauri::{Emitter, Manager, State};

use super::AppState;
use crate::database;

/// 通用 HTTP 响应处理：检查状态码、读取文本、解析 JSON。
fn parse_response(
    resp: reqwest::blocking::Response,
    provider: &str,
) -> Result<serde_json::Value, String> {
    let status = resp.status();
    let text = resp.text().map_err(|e| format!("读取响应失败: {e}"))?;
    if !status.is_success() {
        return Err(format!("{provider}错误 ({status}): {text}"));
    }
    serde_json::from_str(&text).map_err(|e| format!("解析响应失败: {e}"))
}

/// 构建 HTTP 客户端（根据代理配置）
fn build_client(proxy_mode: &str, proxy_url: &str) -> Result<reqwest::blocking::Client, String> {
    let builder = reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(30));

    let builder = crate::proxy::apply_proxy(builder, proxy_mode, proxy_url)?;

    builder
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))
}

/// 微软翻译（通过 Edge 免费接口，无需 API Key）
fn translate_microsoft(
    client: &reqwest::blocking::Client,
    text: &str,
    from: &str,
    to: &str,
) -> Result<String, String> {
    let token = client
        .get("https://edge.microsoft.com/translate/auth")
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 Edg/131.0.0.0")
        .send()
        .map_err(|e| format!("获取微软翻译 token 失败: {e}"))?
        .text()
        .map_err(|e| format!("读取 token 失败: {e}"))?;

    if token.is_empty() || token.len() < 20 {
        return Err(format!("获取微软翻译 token 异常: {token}"));
    }

    let from_param = if from == "auto" { "" } else { from };
    let url = format!(
        "https://api-edge.cognitive.microsofttranslator.com/translate?api-version=3.0&to={to}{from_part}",
        to = to,
        from_part = if from_param.is_empty() {
            String::new()
        } else {
            format!("&from={from_param}")
        },
    );
    let body = serde_json::json!([{ "Text": text }]);
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 Edg/131.0.0.0")
        .json(&body)
        .send()
        .map_err(|e| format!("微软翻译请求失败: {e}"))?;
    let arr = parse_response(resp, "微软翻译")?;
    arr[0]["translations"][0]["text"]
        .as_str()
        .map(std::string::ToString::to_string)
        .ok_or_else(|| "翻译结果格式异常".to_string())
}

/// DeepLX 翻译（自定义接口地址）
fn translate_deeplx(
    client: &reqwest::blocking::Client,
    text: &str,
    from: &str,
    to: &str,
    endpoint: &str,
) -> Result<String, String> {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() {
        return Err("请在设置中填写 DeepLX 请求地址".to_string());
    }
    let source_lang = if from == "auto" { "" } else { from };
    let body = serde_json::json!({
        "text": text,
        "source_lang": source_lang,
        "target_lang": to,
    });
    let resp = client
        .post(endpoint)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .map_err(|e| format!("DeepLX 请求失败: {e}"))?;
    let val = parse_response(resp, "DeepLX")?;
    if let Some(data) = val["data"].as_str()
        && !data.is_empty()
    {
        return Ok(data.to_string());
    }
    if let Some(alternatives) = val["alternatives"].as_array()
        && let Some(first) = alternatives.first().and_then(|v| v.as_str())
    {
        return Ok(first.to_string());
    }
    Err(format!("DeepLX 翻译结果异常: {val}"))
}

/// 谷歌翻译（免费接口）
fn translate_google_free(
    client: &reqwest::blocking::Client,
    text: &str,
    from: &str,
    to: &str,
) -> Result<String, String> {
    let sl = if from == "auto" { "auto" } else { from };
    let url = format!(
        "https://translate.googleapis.com/translate_a/single?client=gtx&sl={sl}&tl={to}&dt=t&q={q}",
        sl = sl,
        to = to,
        q = urlencoding::encode(text),
    );
    let resp = client
        .get(&url)
        .send()
        .map_err(|e| format!("谷歌翻译请求失败: {e}"))?;
    let val = parse_response(resp, "谷歌翻译")?;
    let mut result = String::new();
    if let Some(sentences) = val[0].as_array() {
        for sentence in sentences {
            if let Some(t) = sentence[0].as_str() {
                result.push_str(t);
            }
        }
    }
    if result.is_empty() {
        Err("翻译结果为空".to_string())
    } else {
        Ok(result)
    }
}

/// 谷歌翻译（API Key 版）
fn translate_google_api(
    client: &reqwest::blocking::Client,
    text: &str,
    from: &str,
    to: &str,
    api_key: &str,
) -> Result<String, String> {
    if api_key.is_empty() {
        return Err("请在设置中填写 Google API Key".to_string());
    }
    let source = if from == "auto" { "" } else { from };
    let url = format!("https://translation.googleapis.com/language/translate/v2?key={api_key}");
    let mut body = serde_json::json!({ "q": text, "target": to, "format": "text" });
    if !source.is_empty() {
        body["source"] = serde_json::json!(source);
    }
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .map_err(|e| format!("Google API 请求失败: {e}"))?;
    let val = parse_response(resp, "Google API")?;
    val["data"]["translations"][0]["translatedText"]
        .as_str()
        .map(std::string::ToString::to_string)
        .ok_or_else(|| "翻译结果格式异常".to_string())
}

/// 百度翻译
fn translate_baidu(
    client: &reqwest::blocking::Client,
    text: &str,
    from: &str,
    to: &str,
    app_id: &str,
    secret_key: &str,
) -> Result<String, String> {
    if app_id.is_empty() || secret_key.is_empty() {
        return Err("请在设置中填写百度翻译 APP ID 和密钥".to_string());
    }
    fn map_lang(lang: &str) -> &str {
        match lang {
            "auto" => "auto",
            "zh" => "zh",
            "en" => "en",
            "ja" => "jp",
            "ko" => "kor",
            "fr" => "fra",
            "de" => "de",
            "es" => "spa",
            "pt" => "pt",
            "ru" => "ru",
            "ar" => "ara",
            "it" => "it",
            "th" => "th",
            "vi" => "vie",
            other => other,
        }
    }
    let from_baidu = map_lang(from);
    let to_baidu = map_lang(to);
    let salt = chrono::Utc::now().timestamp_millis().to_string();
    let sign_str = format!("{app_id}{text}{salt}{secret_key}");
    let sign = format!("{:x}", md5::compute(sign_str.as_bytes()));
    let params = [
        ("q", text),
        ("from", from_baidu),
        ("to", to_baidu),
        ("appid", app_id),
        ("salt", &salt),
        ("sign", &sign),
    ];
    let resp = client
        .post("https://fanyi-api.baidu.com/api/trans/vip/translate")
        .form(&params)
        .send()
        .map_err(|e| format!("百度翻译请求失败: {e}"))?;
    let val = parse_response(resp, "百度翻译")?;
    if let Some(err_code) = val["error_code"].as_str() {
        let err_msg = val["error_msg"].as_str().unwrap_or("未知错误");
        return Err(format!("百度翻译错误 ({err_code}): {err_msg}"));
    }
    let results = val["trans_result"]
        .as_array()
        .ok_or_else(|| "翻译结果格式异常".to_string())?;
    let translated: Vec<&str> = results.iter().filter_map(|r| r["dst"].as_str()).collect();
    if translated.is_empty() {
        Err("翻译结果为空".to_string())
    } else {
        Ok(translated.join("\n"))
    }
}

/// OpenAI / AI 翻译
fn translate_openai(
    client: &reqwest::blocking::Client,
    text: &str,
    from: &str,
    to: &str,
    endpoint: &str,
    api_key: &str,
    model: &str,
) -> Result<String, String> {
    if api_key.is_empty() {
        return Err("请在设置中填写 API Key".to_string());
    }
    let base = if endpoint.is_empty() {
        "https://api.openai.com/v1"
    } else {
        endpoint.trim_end_matches('/')
    };
    let url = format!("{base}/chat/completions");
    let model_id = if model.is_empty() {
        "gpt-4o-mini"
    } else {
        model
    };
    let from_desc = if from == "auto" {
        "auto-detected language"
    } else {
        from
    };
    let system_prompt = format!(
        "You are a professional translator. Translate the following text from {from_desc} to {to}. Only output the translation, no explanations.",
    );
    let body = serde_json::json!({
        "model": model_id,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": text },
        ],
        "temperature": 0.3,
    });
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {api_key}"))
        .json(&body)
        .send()
        .map_err(|e| format!("AI 翻译请求失败: {e}"))?;
    let val = parse_response(resp, "AI 翻译")?;
    val["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.trim().to_string())
        .ok_or_else(|| "翻译结果格式异常".to_string())
}

/// 翻译文本（Tauri 命令）
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn translate_text(
    text: String,
    from: String,
    to: String,
    provider: String,
    proxy_mode: String,
    proxy_url: String,
    deeplx_endpoint: Option<String>,
    google_api_key: Option<String>,
    baidu_app_id: Option<String>,
    baidu_secret_key: Option<String>,
    openai_endpoint: Option<String>,
    openai_api_key: Option<String>,
    openai_model: Option<String>,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let client = build_client(&proxy_mode, &proxy_url)?;
        match provider.as_str() {
            "microsoft" => translate_microsoft(&client, &text, &from, &to),
            "deeplx" => translate_deeplx(
                &client,
                &text,
                &from,
                &to,
                &deeplx_endpoint.unwrap_or_default(),
            ),
            "google_free" => translate_google_free(&client, &text, &from, &to),
            "google_api" => translate_google_api(
                &client,
                &text,
                &from,
                &to,
                &google_api_key.unwrap_or_default(),
            ),
            "baidu" => translate_baidu(
                &client,
                &text,
                &from,
                &to,
                &baidu_app_id.unwrap_or_default(),
                &baidu_secret_key.unwrap_or_default(),
            ),
            "openai" => translate_openai(
                &client,
                &text,
                &from,
                &to,
                &openai_endpoint.unwrap_or_default(),
                &openai_api_key.unwrap_or_default(),
                &openai_model.unwrap_or_default(),
            ),
            other => Err(format!("不支持的翻译提供者: {other}")),
        }
    })
    .await
    .map_err(|e| format!("翻译任务失败: {e}"))?
}

/// 将文本写入系统剪贴板
#[tauri::command]
pub async fn write_text_to_clipboard(
    state: State<'_, Arc<AppState>>,
    text: String,
    record: Option<bool>,
) -> Result<(), String> {
    let write_fn = || {
        let mut clipboard =
            arboard::Clipboard::new().map_err(|e| format!("无法访问剪贴板: {e}"))?;
        clipboard
            .set_text(&text)
            .map_err(|e| format!("写入剪贴板失败: {e}"))?;
        Ok(())
    };
    if record.unwrap_or(false) {
        write_fn()
    } else {
        super::with_paused_monitor(&state, write_fn)
    }
}

// ============ 翻译选中文字功能 ============

static PENDING_TRANSLATE_TEXT: parking_lot::Mutex<String> = parking_lot::Mutex::new(String::new());

/// 获取系统当前选中的文字（通过模拟 Ctrl+C 读取剪贴板）
/// 剪贴板备份：保存文本、HTML 和图片，确保恢复时不丢失富文本/图片内容
enum ClipboardBackup {
    Empty,
    Text(String),
    Html { html: String, text: Option<String> },
    Image(arboard::ImageData<'static>),
}

fn backup_clipboard() -> ClipboardBackup {
    let Ok(mut cb) = arboard::Clipboard::new() else {
        return ClipboardBackup::Empty;
    };
    // 优先备份 HTML（包含伴生纯文本）
    if let Ok(html) = cb.get().html()
        && !html.is_empty()
    {
        let text = cb.get_text().ok();
        return ClipboardBackup::Html { html, text };
    }
    // 其次备份纯文本
    if let Ok(text) = cb.get_text()
        && !text.is_empty()
    {
        return ClipboardBackup::Text(text);
    }
    // 最后备份图片
    if let Ok(img) = cb.get_image() {
        return ClipboardBackup::Image(img);
    }
    ClipboardBackup::Empty
}

fn restore_clipboard(backup: &ClipboardBackup) {
    let Ok(mut cb) = arboard::Clipboard::new() else {
        return;
    };
    match backup {
        ClipboardBackup::Empty => {}
        ClipboardBackup::Text(text) => {
            let _ = cb.set_text(text);
        }
        ClipboardBackup::Html { html, text } => {
            let _ = cb.set_html(html.as_str(), text.as_deref());
        }
        ClipboardBackup::Image(img) => {
            let _ = cb.set_image(img.clone());
        }
    }
}

/// 获取系统当前选中的文字（通过模拟 Ctrl+C 读取剪贴板）
fn get_selected_text_from_system(state: &Arc<AppState>) -> Result<String, String> {
    // 备份剪贴板完整内容（在暂停监控前完成，避免影响监控状态）
    let backup = backup_clipboard();

    // 使用 with_paused_monitor 确保异常安全：无论中间是否出错都会恢复监控
    super::with_paused_monitor(state, || -> Result<String, String> {
        #[cfg(target_os = "windows")]
        let seq_before =
            unsafe { windows::Win32::System::DataExchange::GetClipboardSequenceNumber() };

        super::clipboard::simulate_copy()?;

        // 轮询剪贴板序列号，最多等待 600ms
        #[cfg(target_os = "windows")]
        let clipboard_changed = {
            let mut changed = false;
            for _ in 0..30 {
                std::thread::sleep(std::time::Duration::from_millis(20));
                let seq_after =
                    unsafe { windows::Win32::System::DataExchange::GetClipboardSequenceNumber() };
                if seq_after != seq_before {
                    changed = true;
                    break;
                }
            }
            changed
        };

        // 序列号未变时仍尝试读取剪贴板（某些应用 Ctrl+C 不更新序列号，或选中内容与剪贴板相同）
        let text = arboard::Clipboard::new()
            .ok()
            .and_then(|mut cb| cb.get_text().ok())
            .unwrap_or_default();

        if !clipboard_changed && text.is_empty() {
            return Ok(String::new());
        }

        // 恢复剪贴板原始内容（文本/HTML/图片）
        restore_clipboard(&backup);

        Ok(text)
    })
}

/// 前端挂载后调用，获取暂存的待翻译文本
#[tauri::command]
pub async fn get_pending_translate_text() -> Result<String, String> {
    let text = std::mem::take(&mut *PENDING_TRANSLATE_TEXT.lock());
    Ok(text)
}

/// 打开翻译选中文字结果窗口
#[tauri::command]
pub async fn open_translate_result_window(
    app: tauri::AppHandle,
    text: String,
) -> Result<(), String> {
    let label = "translate-result";

    if let Some(window) = app.get_webview_window(label) {
        let _ = window.emit("translate-result-update", &text);
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_always_on_top(true);
        let _ = window.set_focus();
        crate::input_monitor::translate_window_shown();
        return Ok(());
    }

    *PENDING_TRANSLATE_TEXT.lock() = text;

    let window = tauri::WebviewWindowBuilder::new(
        &app,
        label,
        tauri::WebviewUrl::App("/translate-result".into()),
    )
    .title("翻译选中文字")
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
    .map_err(|e| format!("创建翻译结果窗口失败: {e}"))?;

    crate::input_monitor::setup_translate_window(&window);
    crate::input_monitor::translate_window_shown();

    Ok(())
}

#[tauri::command]
pub fn set_translate_window_pinned(pinned: bool) {
    crate::input_monitor::set_translate_window_pinned(pinned);
}

#[tauri::command]
pub fn is_translate_window_pinned() -> bool {
    crate::input_monitor::is_translate_window_pinned()
}

/// 串行化划词翻译，避免并发操作剪贴板
static TRANSLATE_SELECTION_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

/// 注册翻译选中文字快捷键
pub fn register_translate_selection_shortcut(app: &tauri::AppHandle) {
    let Some(state) = app.try_state::<Arc<AppState>>() else {
        return;
    };
    let settings_repo = database::SettingsRepository::new(&state.db);

    let enabled = settings_repo
        .get("translate_selection_enabled")
        .ok()
        .flatten()
        .is_some_and(|v| v == "true");
    if !enabled {
        return;
    }

    let shortcut_str = match settings_repo
        .get("translate_selection_shortcut")
        .ok()
        .flatten()
    {
        Some(s) if !s.is_empty() => s,
        _ => return,
    };

    let registered = crate::hotkey::register(
        &shortcut_str,
        std::sync::Arc::new(|app, key_state| {
            // 松键后触发：避免快捷键修饰键仍按住时模拟 Ctrl+C 失败
            if key_state == crate::hotkey::KeyState::Released {
                let app = app.clone();
                std::thread::spawn(move || {
                    let _guard = TRANSLATE_SELECTION_LOCK.lock();
                    trigger_translate_selection(&app);
                });
            }
        }),
    );

    if !registered {
        tracing::warn!("翻译选中文字快捷键格式无效: {}", shortcut_str);
    }
}

/// 注销翻译选中文字快捷键
pub fn unregister_translate_selection_shortcut(app: &tauri::AppHandle) {
    let Some(state) = app.try_state::<Arc<AppState>>() else {
        return;
    };
    let settings_repo = database::SettingsRepository::new(&state.db);
    if let Some(shortcut_str) = settings_repo
        .get("translate_selection_shortcut")
        .ok()
        .flatten()
        && !shortcut_str.is_empty()
    {
        crate::hotkey::unregister(&shortcut_str);
    }
}

fn trigger_translate_selection(app: &tauri::AppHandle) {
    let Some(state) = app.try_state::<Arc<AppState>>() else {
        return;
    };
    match get_selected_text_from_system(&state) {
        Ok(text) if !text.trim().is_empty() => {
            let app = app.clone();
            if let Err(err) = crate::main_thread::run_on_ui_thread(&app.clone(), move || {
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = open_translate_result_window(app, text).await {
                        tracing::error!("打开翻译结果窗口失败: {}", e);
                    }
                });
            }) {
                tracing::error!("翻译窗口调度到主线程失败: {}", err);
            }
        }
        Ok(_) => {
            use tauri_plugin_notification::NotificationExt;
            let _ = app
                .notification()
                .builder()
                .title("翻译选中文字")
                .body("未检测到选中的文字")
                .show();
        }
        Err(e) => tracing::error!("获取选中文字失败: {}", e),
    }
}

/// 更新翻译选中文字快捷键（前端调用）
#[tauri::command]
pub async fn update_translate_selection_shortcut(
    app: tauri::AppHandle,
    new_shortcut: String,
) -> Result<(), String> {
    unregister_translate_selection_shortcut(&app);

    if new_shortcut.is_empty() {
        return Ok(());
    }

    if !crate::shortcut_has_modifier(&new_shortcut) {
        return Err("快捷键至少包含一个修饰键 (Ctrl/Alt/Win)".to_string());
    }

    let state = app.state::<Arc<AppState>>();
    let settings_repo = database::SettingsRepository::new(&state.db);
    settings_repo
        .set("translate_selection_shortcut", &new_shortcut)
        .map_err(|e| format!("保存快捷键失败: {e}"))?;

    let enabled = settings_repo
        .get("translate_selection_enabled")
        .ok()
        .flatten()
        .is_some_and(|v| v == "true");
    if enabled {
        register_translate_selection_shortcut(&app);
    }

    Ok(())
}
