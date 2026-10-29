use crate::links_core::ProjectLink;
use anyhow::{Context, Result, bail};

pub fn open(link: ProjectLink) -> Result<()> {
    let url = link.url();
    let launcher = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let status = std::process::Command::new(launcher)
        .arg(url)
        .status()
        .context("无法启动系统浏览器")?;
    if !status.success() {
        bail!("无法打开浏览器：{launcher} {status}");
    }
    Ok(())
}
