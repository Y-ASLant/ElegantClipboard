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

/// 构建带系统代理的 reqwest 客户端
fn build_client() -> reqwest::blocking::Client {
    let mut builder = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(30));

    // 复用 proxy.rs 的系统代理检测逻辑
    if let Some(sys_proxy) = crate::proxy::get_windows_system_proxy() {
        debug!("Update proxy: using system proxy ({})", sys_proxy);
        if let Ok(proxy) = reqwest::Proxy::all(&sys_proxy) {
            builder = builder.proxy(proxy);
        }
    } else {
        debug!("Update proxy: direct connection");
    }

    builder
        .build()
        .expect("Failed to build reqwest client for updater")
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

fn fetch_releases() -> Result<Vec<GitHubRelease>, String> {
    let client = build_client();
    let mut req = client
        .get(GITHUB_API_URL)
        .header("Accept", "application/vnd.github.v3+json")
        .header("User-Agent", "ElegantClipboard");

    if let Some(token) = GITHUB_TOKEN
        && !token.is_empty()
    {
        req = req.header("Authorization", &format!("Bearer {token}"));
    }

    let resp = req.send().map_err(|e| {
        if e.is_timeout() {
            "网络连接超时，请检查网络后重试".into()
        } else if e.is_connect() {
            "网络连接失败，请检查网络后重试".into()
        } else {
            format!("网络请求失败: {e}")
        }
    })?;

    let status = resp.status();
    if status == reqwest::StatusCode::FORBIDDEN {
        return Err("GitHub API 请求限额已用尽，请稍后再试".into());
    }
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err("未找到发布版本".into());
    }
    if status.is_client_error() || status.is_server_error() {
        return Err(format!("GitHub API 返回错误: {status}"));
    }

    resp.json::<Vec<GitHubRelease>>()
        .map_err(|e| format!("解析响应失败: {e}"))
}

/// 检查 GitHub 最新版本并与当前版本比较。
/// 若有多个中间版本未更新，会合并所有新版本的更新日志。
pub fn check_update() -> Result<UpdateInfo, String> {
    let current_version = env!("CARGO_PKG_VERSION");
    info!("Checking for updates (current: v{})", current_version);

    let releases = fetch_releases()?;

    // 取所有比当前版本新的发布（列表已倒序，遇到不更新的即可停止）
    let newer_releases: Vec<&GitHubRelease> = releases
        .iter()
        .take_while(|r| is_newer(r.tag_name.trim_start_matches('v'), current_version))
        .collect();

    if newer_releases.is_empty() {
        let latest_version = releases.first().map_or_else(
            || current_version.to_string(),
            |r| r.tag_name.trim_start_matches('v').to_string(),
        );
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

    let setup_asset =
        setup_asset.ok_or_else(|| format!("未找到适用于当前架构({arch_suffix})的安装包"))?;
    let (download_url, file_name, file_size) = (
        setup_asset.browser_download_url.clone(),
        setup_asset.name.clone(),
        setup_asset.size,
    );

    // 合并所有新版本的更新日志（最新在前）
    let release_notes = newer_releases
        .iter()
        .map(|r| {
            let ver = r.tag_name.trim_start_matches('v');
            let notes = r.body.as_deref().unwrap_or("").trim();
            if notes.is_empty() {
                format!("## v{ver}")
            } else {
                format!("## v{ver}\n{notes}")
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

    let client = build_client();
    let resp = client
        .get(url)
        .header("User-Agent", "ElegantClipboard")
        .send()
        .map_err(|e| {
            if e.is_timeout() {
                "网络连接超时，请检查网络后重试".into()
            } else {
                format!("网络连接失败: {e}")
            }
        })?;

    let status = resp.status();
    if status.is_client_error() || status.is_server_error() {
        return Err(format!("下载服务器返回错误 (HTTP {status})"));
    }

    let total: u64 = resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let temp_dir = std::env::temp_dir().join("ElegantClipboard");
    std::fs::create_dir_all(&temp_dir).map_err(|e| format!("创建临时目录失败: {e}"))?;
    let file_path = temp_dir.join(file_name);

    let mut file = std::fs::File::create(&file_path).map_err(|e| format!("创建文件失败: {e}"))?;
    let mut reader = std::io::BufReader::new(resp);
    let mut buf = vec![0u8; 65536]; // 64 KB 读取缓冲
    let mut downloaded = 0u64;
    let mut last_emit = std::time::Instant::now();

    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("读取数据失败: {e}"))?;
        if n == 0 {
            break;
        }
        if DOWNLOAD_CANCELLED.load(Ordering::SeqCst) {
            drop(file);
            let _ = std::fs::remove_file(&file_path);
            return Err("下载已取消".into());
        }
        file.write_all(&buf[..n])
            .map_err(|e| format!("写入文件失败: {e}"))?;
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

/// 启动已下载的 NSIS 安装程序（应用内更新场景）。
///
/// 参数说明：
/// - `/P`：passive 模式，仅显示进度 UI，跳过欢迎页/许可页/完成页，无需用户点击下一步。
/// - `/R`：安装成功后自动重启已安装的应用，实现"点更新 → 自动换版启动"。
///
/// 完全静默可改用 `/S`；此处选 `/P` 是为了让用户能看到更新进度反馈。
pub fn install(installer_path: &str) -> Result<(), String> {
    info!(
        "Launching installer (passive + restart): {}",
        installer_path
    );

    std::process::Command::new(installer_path)
        .args(["/P", "/R"])
        .spawn()
        .map_err(|e| format!("启动安装程序失败: {e}"))?;

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
