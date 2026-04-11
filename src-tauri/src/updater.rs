//! 基于 GitHub Release 的更新检查与下载模块
//!
//! 检查最新版本、下载 NSIS 安装包并汇报进度。
//! 编译时可通过 `UPDATER_GITHUB_TOKEN` 环境变量嵌入 API Token 以提高速率上限。

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::Emitter;
use tracing::{debug, info};

/// 下载取消标志
static DOWNLOAD_CANCELLED: AtomicBool = AtomicBool::new(false);

/// 取消正在进行的下载
pub fn cancel_download() {
    DOWNLOAD_CANCELLED.store(true, Ordering::SeqCst);
}

fn reset_cancel() {
    DOWNLOAD_CANCELLED.store(false, Ordering::SeqCst);
}

const GITHUB_API_URL: &str =
    "https://api.github.com/repos/Y-ASLant/ElegantClipboard/releases?per_page=30";

/// 编译时嵌入的可选 GitHub API Token
const GITHUB_TOKEN: Option<&str> = option_env!("UPDATER_GITHUB_TOKEN");

/// 构建带系统代理的 ureq Agent
fn build_agent() -> ureq::Agent {
    let mut builder = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_response(Some(Duration::from_secs(30)));

    // 优先环境变量，其次 Windows 系统代理
    if let Some(proxy) = ureq::Proxy::try_from_env() {
        debug!("Update proxy: using environment variable");
        builder = builder.proxy(Some(proxy));
    } else if let Some(proxy) = read_system_proxy() {
        debug!("Update proxy: using system proxy");
        builder = builder.proxy(Some(proxy));
    } else {
        debug!("Update proxy: direct connection");
    }

    builder.build().into()
}

/// 从 Windows 注册表读取系统代理设置
#[cfg(target_os = "windows")]
fn read_system_proxy() -> Option<ureq::Proxy> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let inet = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings")
        .ok()?;
    let enabled: u32 = inet.get_value("ProxyEnable").ok()?;
    if enabled == 0 {
        return None;
    }
    let server: String = inet.get_value("ProxyServer").ok()?;
    if server.is_empty() {
        return None;
    }

    // 格式可能是 "host:port" 或 "http=h:p;https=h:p;..."
    let addr = if server.contains('=') {
        // 提取 https 或 http 代理
        server
            .split(';')
            .find_map(|seg| {
                let seg = seg.trim();
                if seg.starts_with("https=") {
                    Some(seg.trim_start_matches("https=").to_string())
                } else if seg.starts_with("http=") {
                    Some(seg.trim_start_matches("http=").to_string())
                } else {
                    None
                }
            })?
    } else {
        server
    };

    let url = if addr.starts_with("http://") || addr.starts_with("https://") || addr.starts_with("socks") {
        addr
    } else {
        format!("http://{}", addr)
    };

    debug!("System proxy address: {}", url);
    ureq::Proxy::new(&url).ok()
}

#[cfg(not(target_os = "windows"))]
fn read_system_proxy() -> Option<ureq::Proxy> {
    None
}

// ── GitHub API 响应类型 ──

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    body: Option<String>,
    published_at: Option<String>,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

// ── 公共类型 ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub has_update: bool,
    pub latest_version: String,
    pub current_version: String,
    pub release_notes: String,
    pub download_url: String,
    pub file_name: String,
    pub file_size: u64,
    pub published_at: String,
}

// ── 公共 API ─────────────────────────────────────────────────────────────────────

/// 检查 GitHub 最新版本并与当前版本比较。
/// 若有多个中间版本未更新，会合并所有新版本的更新日志。
pub fn check_update() -> Result<UpdateInfo, String> {
    let current_version = env!("CARGO_PKG_VERSION");
    info!("Checking for updates (current: v{})", current_version);

    let agent = build_agent();
    let mut req = agent
        .get(GITHUB_API_URL)
        .header("Accept", "application/vnd.github.v3+json")
        .header("User-Agent", "ElegantClipboard");

    if let Some(token) = GITHUB_TOKEN
        && !token.is_empty() {
            req = req.header("Authorization", &format!("Bearer {}", token));
        }

    // GitHub 列表已按发布时间倒序
    let releases: Vec<GitHubRelease> = match req.call() {
        Ok(mut resp) => resp
            .body_mut()
            .read_json()
            .map_err(|e| format!("解析响应失败: {}", e))?,
        Err(ureq::Error::StatusCode(403)) => {
            return Err("GitHub API 请求限额已用尽，请稍后再试".into())
        }
        Err(ureq::Error::StatusCode(404)) => return Err("未找到发布版本".into()),
        Err(ureq::Error::StatusCode(code)) => return Err(format!("GitHub API 返回错误: {}", code)),
        Err(e) => return Err(format!("网络连接失败: {}", e)),
    };

    // 取所有比当前版本新的发布（列表已倒序，遇到不更新的即可停止）
    let newer_releases: Vec<&GitHubRelease> = releases
        .iter()
        .take_while(|r| is_newer(r.tag_name.trim_start_matches('v'), current_version))
        .collect();

    if newer_releases.is_empty() {
        let latest_version = releases
            .first()
            .map(|r| r.tag_name.trim_start_matches('v').to_string())
            .unwrap_or_else(|| current_version.to_string());
        info!("Update check: already at latest v{}", latest_version);
        return Ok(UpdateInfo {
            has_update: false,
            latest_version,
            current_version: current_version.to_string(),
            release_notes: String::new(),
            download_url: String::new(),
            file_name: String::new(),
            file_size: 0,
            published_at: String::new(),
        });
    }

    let latest_release = newer_releases[0];
    let latest_version = latest_release.tag_name.trim_start_matches('v').to_string();

    // 查找匹配架构的 NSIS 安装包
    let arch_suffix = match std::env::consts::ARCH {
        "aarch64" => "arm64-setup.exe",
        _ => "x64-setup.exe",
    };
    let setup_asset = latest_release
        .assets
        .iter()
        .find(|a| a.name.ends_with(arch_suffix));

    let (download_url, file_name, file_size) = setup_asset
        .map(|a| (a.browser_download_url.clone(), a.name.clone(), a.size))
        .unwrap_or_default();

    // 合并所有新版本的更新日志（最新在前）
    let release_notes = newer_releases
        .iter()
        .map(|r| {
            let ver = r.tag_name.trim_start_matches('v');
            let notes = r.body.as_deref().unwrap_or("").trim();
            if notes.is_empty() {
                format!("## v{}", ver)
            } else {
                format!("## v{}\n{}", ver, notes)
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    info!(
        "Update check: latest=v{}, skipped {} version(s)",
        latest_version,
        newer_releases.len()
    );

    Ok(UpdateInfo {
        has_update: true,
        latest_version,
        current_version: current_version.to_string(),
        release_notes,
        download_url,
        file_name,
        file_size,
        published_at: latest_release.published_at.clone().unwrap_or_default(),
    })
}

/// 从 GitHub 下载更新安装包，并向前端发射下载进度事件。
/// 返回下载文件的本地路径。
pub fn download(app: &tauri::AppHandle, url: &str, file_name: &str) -> Result<String, String> {
    info!("Downloading update: {}", file_name);
    reset_cancel();

    let agent = build_agent();
    let response = match agent
        .get(url)
        .header("User-Agent", "ElegantClipboard")
        .call()
    {
        Ok(resp) => resp,
        Err(ureq::Error::StatusCode(code)) => {
            return Err(format!("下载服务器返回错误 (HTTP {})", code))
        }
        Err(_) => return Err("网络连接失败，请检查网络后重试".into()),
    };

    let total: u64 = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let temp_dir = std::env::temp_dir().join("ElegantClipboard");
    std::fs::create_dir_all(&temp_dir).map_err(|e| format!("创建临时目录失败: {}", e))?;
    let file_path = temp_dir.join(file_name);

    let mut file = std::fs::File::create(&file_path).map_err(|e| format!("创建文件失败: {}", e))?;
    let mut body = response.into_body();
    let mut reader = body.as_reader();
    let mut buf = vec![0u8; 65536]; // 64 KB 读取缓冲
    let mut downloaded = 0u64;
    let mut last_emit = std::time::Instant::now();

    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("读取数据失败: {}", e))?;
        if n == 0 {
            break;
        }
        if DOWNLOAD_CANCELLED.load(Ordering::SeqCst) {
            drop(file);
            let _ = std::fs::remove_file(&file_path);
            return Err("下载已取消".into());
        }
        file.write_all(&buf[..n])
            .map_err(|e| format!("写入文件失败: {}", e))?;
        downloaded += n as u64;

        // 限流：约 10 次/秒发射进度事件
        if last_emit.elapsed() >= std::time::Duration::from_millis(100) || downloaded >= total {
            let _ = app.emit(
                "update-download-progress",
                serde_json::json!({
                    "downloaded": downloaded,
                    "total": total,
                }),
            );
            last_emit = std::time::Instant::now();
        }
    }

    info!("Download complete: {} bytes -> {:?}", downloaded, file_path);
    Ok(file_path.to_string_lossy().to_string())
}

/// 启动已下载的 NSIS 安装程序。
pub fn install(installer_path: &str) -> Result<(), String> {
    info!("Launching installer: {}", installer_path);

    std::process::Command::new(installer_path)
        .spawn()
        .map_err(|e| format!("启动安装程序失败: {}", e))?;

    Ok(())
}

// ── 辅助函数 ────────────────────────────────────────────────────────────────

/// 比较语义版本：若 latest 严格大于 current 则返回 true。
fn is_newer(latest: &str, current: &str) -> bool {
    let parse = |v: &str| -> Vec<u32> {
        v.trim_start_matches('v')
            .split('.')
            .filter_map(|s| s.parse().ok())
            .collect()
    };
    parse(latest) > parse(current)
}
