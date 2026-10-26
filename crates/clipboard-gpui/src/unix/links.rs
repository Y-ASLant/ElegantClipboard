use anyhow::{Context, Result, bail};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectLink {
    Author,
    Repository,
    Issues,
}

pub fn open(link: ProjectLink) -> Result<()> {
    let url = match link {
        ProjectLink::Author => "https://github.com/Y-ASLant",
        ProjectLink::Repository => "https://github.com/Y-ASLant/ElegantClipboard",
        ProjectLink::Issues => "https://github.com/Y-ASLant/ElegantClipboard/issues",
    };
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
