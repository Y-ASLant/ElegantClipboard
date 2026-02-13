//! GitHub-based update checker and downloader
//!
//! Uses GitHub Releases API to check for new versions, download installers with
//! progress reporting, and launch the NSIS setup executable.
//!
//! An optional API token can be embedded at build time to increase rate limits
//! (60 → 5000 requests/hour). Set the `UPDATER_GITHUB_TOKEN` environment variable
//! during `cargo build`.

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use tauri::Emitter;
use tracing::info;

const GITHUB_API_URL: &str =
    "https://api.github.com/repos/Y-ASLant/ElegantClipboard/releases/latest";

/// Optional GitHub API token, embedded at compile time.
const GITHUB_TOKEN: Option<&str> = option_env!("UPDATER_GITHUB_TOKEN");

// ── GitHub API response types ──────────────────────────────────────────

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

// ── Public types ───────────────────────────────────────────────────────

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

// ── Public API ─────────────────────────────────────────────────────────

/// Check GitHub for the latest release and compare with current version.
pub fn check_update() -> Result<UpdateInfo, String> {
    let current_version = env!("CARGO_PKG_VERSION");
    info!("Checking for updates (current: v{})", current_version);

    let mut req = ureq::get(GITHUB_API_URL)
        .header("Accept", "application/vnd.github.v3+json")
        .header("User-Agent", "ElegantClipboard");

    if let Some(token) = GITHUB_TOKEN {
        if !token.is_empty() {
            req = req.header("Authorization", &format!("Bearer {}", token));
        }
    }

    let release: GitHubRelease = match req.call() {
        Ok(mut resp) => resp
            .body_mut()
            .read_json()
            .map_err(|e| format!("解析响应失败: {}", e))?,
        Err(ureq::Error::StatusCode(403)) => {
            return Err("GitHub API 请求限额已用尽，请稍后再试".into())
        }
        Err(ureq::Error::StatusCode(404)) => return Err("未找到发布版本".into()),
        Err(ureq::Error::StatusCode(code)) => {
            return Err(format!("GitHub API 返回错误: {}", code))
        }
        Err(e) => return Err(format!("网络连接失败: {}", e)),
    };

    let latest_version = release.tag_name.trim_start_matches('v').to_string();
    let has_update = is_newer(&latest_version, current_version);

    // Find NSIS setup executable in release assets
    let setup_asset = release
        .assets
        .iter()
        .find(|a| a.name.ends_with("-setup.exe"));

    let (download_url, file_name, file_size) = match setup_asset {
        Some(a) => (a.browser_download_url.clone(), a.name.clone(), a.size),
        None => (String::new(), String::new(), 0),
    };

    info!(
        "Update check: latest=v{}, has_update={}",
        latest_version, has_update
    );

    Ok(UpdateInfo {
        has_update,
        latest_version,
        current_version: current_version.to_string(),
        release_notes: release.body.unwrap_or_default(),
        download_url,
        file_name,
        file_size,
        published_at: release.published_at.unwrap_or_default(),
    })
}

/// Download an update installer from GitHub with progress reporting.
/// Progress events (`update-download-progress`) are emitted to the frontend.
/// Returns the local path to the downloaded file.
pub fn download(app: &tauri::AppHandle, url: &str, file_name: &str) -> Result<String, String> {
    info!("Downloading update: {}", file_name);

    let response = match ureq::get(url)
        .header("User-Agent", "ElegantClipboard")
        .call()
    {
        Ok(resp) => resp,
        Err(ureq::Error::StatusCode(code)) => {
            return Err(format!("下载服务器返回错误 (HTTP {})", code))
        }
        Err(_) => {
            return Err("网络连接失败，请检查网络后重试".into())
        }
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

    let mut file =
        std::fs::File::create(&file_path).map_err(|e| format!("创建文件失败: {}", e))?;
    let mut body = response.into_body();
    let mut reader = body.as_reader();
    let mut buf = vec![0u8; 65536]; // 64 KB chunks
    let mut downloaded = 0u64;
    let mut last_emit = std::time::Instant::now();

    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("读取数据失败: {}", e))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .map_err(|e| format!("写入文件失败: {}", e))?;
        downloaded += n as u64;

        // Throttle progress events to ~10/sec
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

/// Launch the downloaded NSIS installer executable.
pub fn install(installer_path: &str) -> Result<(), String> {
    info!("Launching installer: {}", installer_path);

    std::process::Command::new(installer_path)
        .spawn()
        .map_err(|e| format!("启动安装程序失败: {}", e))?;

    Ok(())
}

// ── Helpers ────────────────────────────────────────────────────────────

/// Compare semver strings: returns `true` if `latest` is strictly newer than `current`.
fn is_newer(latest: &str, current: &str) -> bool {
    let parse = |v: &str| -> Vec<u32> {
        v.trim_start_matches('v')
            .split('.')
            .filter_map(|s| s.parse().ok())
            .collect()
    };
    parse(latest) > parse(current)
}
