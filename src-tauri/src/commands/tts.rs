use sha2::{Sha256, Digest};
use tracing::debug;

/// Edge TTS 朗读结果（音频 + 词边界时间戳）
#[derive(serde::Serialize, Clone)]
pub struct WordBoundary {
    /// 音频偏移（毫秒）
    pub offset_ms: f64,
    /// 持续时长（毫秒）
    pub duration_ms: f64,
    /// 该词的文本
    pub text: String,
}

#[derive(serde::Serialize)]
pub struct TtsResult {
    pub audio: String,
    pub boundaries: Vec<WordBoundary>,
}

/// Edge TTS 使用的固定 Token
const EDGE_TTS_TOKEN: &str = "6A5AA1D4EAFF4E9FB37E23D68491D6F4";

/// Chromium 版本号（需要跟随 Edge 浏览器版本更新）
const CHROMIUM_FULL_VERSION: &str = "143.0.3650.75";
const CHROMIUM_MAJOR_VERSION: &str = "143";

/// 生成 Sec-MS-GEC 安全令牌（基于当前时间 + Token 的 SHA-256）
/// 算法来源：https://github.com/rany2/edge-tts/blob/master/src/edge_tts/drm.py
fn generate_sec_ms_gec() -> String {
    // Windows FILETIME epoch offset (1601-01-01 到 1970-01-01 的秒数)
    const WIN_EPOCH: f64 = 11644473600.0;
    const S_TO_NS: f64 = 1e9;

    let unix_time = chrono::Utc::now().timestamp() as f64;
    // 转换到 Windows 纪元
    let mut ticks = unix_time + WIN_EPOCH;
    // 先向下取整到最近的 5 分钟（300 秒）
    ticks -= ticks % 300.0;
    // 再转换为 100 纳秒刻度
    ticks *= S_TO_NS / 100.0;

    let hash_input = format!("{:.0}{}", ticks, EDGE_TTS_TOKEN);
    let hash = Sha256::digest(hash_input.as_bytes());
    hash.iter().map(|b| format!("{:02X}", b)).collect()
}

/// 生成随机 MUID（用于 Cookie）
fn generate_muid() -> String {
    let bytes = uuid::Uuid::new_v4().as_bytes().to_vec();
    bytes.iter().map(|b| format!("{:02X}", b)).collect()
}

/// Edge TTS 预定义声源列表
pub const EDGE_VOICES: &[(&str, &str)] = &[
    // 美式英语
    ("en-US-AriaNeural",          "Aria（美式女声·自然）"),
    ("en-US-JennyNeural",         "Jenny（美式女声·亲切）"),
    ("en-US-GuyNeural",           "Guy（美式男声·稳重）"),
    ("en-US-ChristopherNeural",   "Christopher（美式男声·清晰）"),
    ("en-US-AnaNeural",           "Ana（美式女声·年轻）"),
    ("en-US-AndrewMultilingualNeural", "Andrew（美式男声·多语言）"),
    ("en-US-AvaMultilingualNeural",    "Ava（美式女声·多语言）"),
    ("en-US-BrianMultilingualNeural",  "Brian（美式男声·多语言）"),
    // 英式英语
    ("en-GB-SoniaNeural",         "Sonia（英式女声·优雅）"),
    ("en-GB-RyanNeural",          "Ryan（英式男声·绅士）"),
    ("en-GB-LibbyNeural",         "Libby（英式女声·温暖）"),
    ("en-GB-MaisieNeural",        "Maisie（英式女声·活泼）"),
    // 中文
    ("zh-CN-XiaoxiaoNeural",      "晓晓（中文女声·温柔）"),
    ("zh-CN-YunxiNeural",         "云希（中文男声·阳光）"),
    ("zh-CN-YunjianNeural",       "云健（中文男声·沉稳）"),
    ("zh-CN-XiaoyiNeural",        "晓伊（中文女声·活泼）"),
];

/// 解析 audio.metadata 消息中的 WordBoundary 条目
fn parse_word_boundaries(msg: &str) -> Vec<WordBoundary> {
    let json_start = match msg.find("\r\n\r\n") {
        Some(pos) => pos + 4,
        None => return vec![],
    };
    let json_str = &msg[json_start..];
    let value: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let mut boundaries = Vec::new();
    if let Some(metadata) = value.get("Metadata").and_then(|m| m.as_array()) {
        for item in metadata {
            if item.get("Type").and_then(|t| t.as_str()) != Some("WordBoundary") {
                continue;
            }
            if let Some(data) = item.get("Data") {
                let offset_ticks = data.get("Offset").and_then(|o| o.as_f64()).unwrap_or(0.0);
                let duration_ticks = data.get("Duration").and_then(|d| d.as_f64()).unwrap_or(0.0);
                let word = data.get("text")
                    .and_then(|t| t.get("Text"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();
                if !word.is_empty() {
                    boundaries.push(WordBoundary {
                        offset_ms: offset_ticks / 10_000.0,
                        duration_ms: duration_ticks / 10_000.0,
                        text: word,
                    });
                }
            }
        }
    }
    boundaries
}

/// 通过 Edge TTS WebSocket 合成音频，返回 MP3 字节和词边界
fn edge_tts_synthesize(text: &str, voice: &str, rate: &str, _proxy_mode: &str, _proxy_url: &str) -> Result<(Vec<u8>, Vec<WordBoundary>), String> {
    use tungstenite::{connect, Message};
    use tungstenite::client::IntoClientRequest;
    use tungstenite::http::HeaderValue;

    let conn_id = uuid::Uuid::new_v4().to_string().replace('-', "");
    let sec_ms_gec = generate_sec_ms_gec();
    let sec_ms_gec_version = format!("1-{}", CHROMIUM_FULL_VERSION);
    let ws_url = format!(
        "wss://speech.platform.bing.com/consumer/speech/synthesize/readaloud/edge/v1?TrustedClientToken={}&ConnectionId={}&Sec-MS-GEC={}&Sec-MS-GEC-Version={}",
        EDGE_TTS_TOKEN, conn_id, sec_ms_gec, sec_ms_gec_version
    );

    debug!("Edge TTS URL: {}", ws_url);

    let mut request = ws_url.into_client_request()
        .map_err(|e| format!("构建 WebSocket 请求失败: {}", e))?;
    let headers = request.headers_mut();
    let ua = format!(
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{}.0.0.0 Safari/537.36 Edg/{}.0.0.0",
        CHROMIUM_MAJOR_VERSION, CHROMIUM_MAJOR_VERSION
    );
    headers.insert("User-Agent", HeaderValue::from_str(&ua).unwrap());
    headers.insert("Origin", HeaderValue::from_static("chrome-extension://jdiccldimpdaibmpdkjnbmckianbfold"));
    headers.insert("Pragma", HeaderValue::from_static("no-cache"));
    headers.insert("Cache-Control", HeaderValue::from_static("no-cache"));
    headers.insert("Accept-Encoding", HeaderValue::from_static("gzip, deflate, br, zstd"));
    headers.insert("Accept-Language", HeaderValue::from_static("en-US,en;q=0.9"));
    let muid_cookie = format!("muid={};", generate_muid());
    headers.insert("Cookie", HeaderValue::from_str(&muid_cookie).unwrap());

    let (mut socket, _) = connect(request)
        .map_err(|e| format!("Edge TTS WebSocket 连接失败: {}", e))?;

    // 1. 发送配置消息
    let timestamp = chrono::Utc::now().format("%a %b %d %Y %H:%M:%S GMT+0000 (Coordinated Universal Time)").to_string();
    let config_msg = format!(
        "X-Timestamp:{}\r\nContent-Type:application/json; charset=utf-8\r\nPath:speech.config\r\n\r\n\
{{\"context\":{{\"synthesis\":{{\"audio\":{{\"metadataoptions\":{{\"sentenceBoundaryEnabled\":\"false\",\"wordBoundaryEnabled\":\"true\"}},\"outputFormat\":\"audio-24khz-48kbitrate-mono-mp3\"}}}}}}}}",
        timestamp
    );
    socket.send(Message::Text(config_msg.into())).map_err(|e| format!("发送配置失败: {}", e))?;

    // 2. 发送 SSML 消息
    let escaped_text = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;");

    let ssml = format!(
        "<speak version='1.0' xmlns='http://www.w3.org/2001/10/synthesis' xml:lang='en-US'><voice name='{}'><prosody rate='{}' pitch='+0Hz' volume='+0%'>{}</prosody></voice></speak>",
        voice, rate, escaped_text
    );

    let timestamp2 = chrono::Utc::now().format("%a %b %d %Y %H:%M:%S GMT+0000 (Coordinated Universal Time)").to_string();
    let ssml_msg = format!(
        "X-RequestId:{}\r\nContent-Type:application/ssml+xml\r\nX-Timestamp:{}Z\r\nPath:ssml\r\n\r\n{}",
        conn_id, timestamp2, ssml
    );
    socket.send(Message::Text(ssml_msg.into())).map_err(|e| format!("发送 SSML 失败: {}", e))?;

    // 3. 接收音频数据 + 词边界
    let mut audio_data = Vec::new();
    let mut boundaries: Vec<WordBoundary> = Vec::new();

    loop {
        let msg = socket.read().map_err(|e| format!("读取 WebSocket 消息失败: {}", e))?;
        match msg {
            Message::Binary(data) => {
                // 二进制消息：前 2 字节为头部长度（big-endian u16）
                if data.len() < 2 {
                    continue;
                }
                let header_len = u16::from_be_bytes([data[0], data[1]]) as usize;
                let audio_start = 2 + header_len;
                if audio_start < data.len() {
                    audio_data.extend_from_slice(&data[audio_start..]);
                }
            }
            Message::Text(text) => {
                let text_str: &str = &text;
                if text_str.contains("Path:audio.metadata") {
                    boundaries.extend(parse_word_boundaries(text_str));
                }
                if text_str.contains("Path:turn.end") {
                    break;
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    let _ = socket.close(None);

    if audio_data.is_empty() {
        return Err("Edge TTS 未返回音频数据".to_string());
    }

    debug!("Edge TTS 合成完成，音频大小: {} bytes, 词边界: {} 条", audio_data.len(), boundaries.len());
    Ok((audio_data, boundaries))
}

/// Edge TTS 合成（Tauri 命令）—— 返回 base64 编码的 MP3
#[tauri::command]
pub async fn tts_speak_edge(
    text: String,
    voice: String,
    rate: String,
    proxy_mode: String,
    proxy_url: String,
) -> Result<TtsResult, String> {
    tokio::task::spawn_blocking(move || {
        let (audio, boundaries) = edge_tts_synthesize(&text, &voice, &rate, &proxy_mode, &proxy_url)?;
        Ok(TtsResult {
            audio: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &audio),
            boundaries,
        })
    })
    .await
    .map_err(|e| format!("TTS 任务失败: {}", e))?
}

/// 获取 Edge TTS 可用声源列表（Tauri 命令）
#[tauri::command]
pub fn tts_get_edge_voices() -> Vec<(String, String)> {
    EDGE_VOICES.iter().map(|(id, label)| (id.to_string(), label.to_string())).collect()
}
