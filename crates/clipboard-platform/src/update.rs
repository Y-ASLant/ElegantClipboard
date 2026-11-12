//! GitHub Releases 更新检查与安装包下载（仅 Windows 安装流程）。

use anyhow::{Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

const RELEASES_API: &str =
    "https://api.github.com/repos/Y-ASLant/ElegantClipboard/releases?per_page=20";
const SETUP_SUFFIX: &str = "-setup.exe";

/// 一次可下载的更新：版本号、安装包与可选的校验和地址、发布页链接。
#[derive(Debug, Clone)]
pub struct ReleaseInfo {
    pub version: String,
    pub setup_url: String,
    pub sha256_url: Option<String>,
    pub notes_url: String,
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    html_url: String,
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .user_agent(concat!("ElegantClipboard/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// 拉取与当前版本同通道的最新 release；`None` 表示已是最新或无同通道 release。
pub fn fetch_latest(current: &str) -> Result<Option<ReleaseInfo>> {
    let body = agent()
        .get(RELEASES_API)
        .header("Accept", "application/vnd.github+json")
        .call()
        .context("无法连接 GitHub Releases")?
        .body_mut()
        .read_to_string()
        .context("无法读取 GitHub Releases 响应")?;
    let releases: Vec<GithubRelease> =
        serde_json::from_str(&body).context("无法解析 GitHub Releases 响应")?;
    let current_is_prerelease = current.contains('-');
    let best = releases
        .iter()
        .filter(|release| !release.draft && release.prerelease == current_is_prerelease)
        .filter_map(|release| {
            let version = release.tag_name.trim_start_matches('v');
            parse_version(version).map(|parsed| (parsed, release))
        })
        .max_by(|(a, _), (b, _)| a.cmp(b));
    let Some((_, release)) = best else {
        return Ok(None);
    };
    let version = release.tag_name.trim_start_matches('v');
    if !version_is_newer(version, current) {
        return Ok(None);
    }
    let Some(setup) = release
        .assets
        .iter()
        .find(|asset| asset.name.ends_with(SETUP_SUFFIX))
    else {
        return Ok(None);
    };
    let sha256_url = release
        .assets
        .iter()
        .find(|asset| asset.name == format!("{}.sha256", setup.name))
        .map(|asset| asset.browser_download_url.clone());
    Ok(Some(ReleaseInfo {
        version: version.to_owned(),
        setup_url: setup.browser_download_url.clone(),
        sha256_url,
        notes_url: release.html_url.clone(),
    }))
}

/// `X.Y.Z[-beta.N]` → 可排序元组；非 `beta.N` 后缀按 beta.0 处理。
fn parse_version(version: &str) -> Option<(u64, u64, u64, Option<u64>)> {
    let (core, pre) = version.split_once('-').unwrap_or((version, ""));
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    let beta = if pre.is_empty() {
        None
    } else {
        Some(
            pre.strip_prefix("beta.")
                .and_then(|n| n.parse().ok())
                .unwrap_or(0),
        )
    };
    Some((major, minor, patch, beta))
}

/// 语义化版本比较：`-beta.N` 后缀视为低于同号正式版。
fn version_is_newer(latest: &str, current: &str) -> bool {
    let Some(latest) = parse_version(latest) else {
        return false;
    };
    let Some(current) = parse_version(current) else {
        return true;
    };
    match (latest.0, latest.1, latest.2).cmp(&(current.0, current.1, current.2)) {
        std::cmp::Ordering::Equal => match (latest.3, current.3) {
            (None, Some(_)) => true,
            (Some(_), None) => false,
            (Some(l), Some(c)) => l > c,
            (None, None) => false,
        },
        ordering => ordering == std::cmp::Ordering::Greater,
    }
}

/// 流式下载安装包到 `dir/ElegantClipboard-setup.exe`，可选 sha256 校验；失败清理残件。
pub fn download_setup(info: &ReleaseInfo, dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(dir).context("无法创建更新目录")?;
    let agent = agent();
    let expected = match &info.sha256_url {
        Some(url) => Some(
            agent
                .get(url)
                .call()
                .context("无法下载校验文件")?
                .body_mut()
                .read_to_string()
                .context("无法读取校验文件")?
                .split_whitespace()
                .next()
                .context("校验文件为空")?
                .to_lowercase(),
        ),
        None => None,
    };
    let destination = dir.join("ElegantClipboard-setup.exe");
    let result = download_and_verify(&agent, &info.setup_url, &destination, expected.as_deref());
    if result.is_err() {
        let _ = std::fs::remove_file(&destination);
    }
    result.map(|()| destination)
}

fn download_and_verify(
    agent: &ureq::Agent,
    url: &str,
    destination: &Path,
    expected: Option<&str>,
) -> Result<()> {
    let mut response = agent
        .get(url)
        .call()
        .context("无法连接 GitHub 下载安装包")?;
    let mut reader = response.body_mut().as_reader();
    let mut file = std::fs::File::create(destination).context("无法写入安装包")?;
    let mut hasher = expected.map(|_| Sha256::new());
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer).context("下载中断")?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read]).context("写入安装包失败")?;
        if let Some(hasher) = &mut hasher {
            hasher.update(&buffer[..read]);
        }
    }
    if let (Some(hasher), Some(expected)) = (hasher, expected) {
        let actual = hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        anyhow::ensure!(actual == expected, "安装包校验失败（sha256 不匹配）");
    }
    Ok(())
}

/// 启动已下载的 NSIS 安装器（仅 Windows；per-user 安装，无需提权）。
#[cfg(windows)]
pub fn run_installer(path: &Path) -> Result<()> {
    std::process::Command::new(path)
        .spawn()
        .context("无法启动安装器")?;
    Ok(())
}

#[cfg(not(windows))]
pub fn run_installer(_path: &Path) -> Result<()> {
    Err(anyhow::anyhow!("当前平台不支持自动安装"))
}

#[cfg(test)]
mod tests {
    use super::version_is_newer;

    #[test]
    fn version_comparison_handles_beta_channel() {
        assert!(version_is_newer("2.0.0-beta.20", "2.0.0-beta.19"));
        assert!(version_is_newer("2.0.0", "2.0.0-beta.19"));
        assert!(!version_is_newer("2.0.0-beta.19", "2.0.0-beta.19"));
        assert!(!version_is_newer("2.0.0-beta.19", "2.0.0"));
        assert!(version_is_newer("2.0.1", "2.0.0"));
        assert!(!version_is_newer("garbage", "2.0.0"));
        assert!(version_is_newer("2.0.1", "garbage"));
    }
}
