use anyhow::{Result, bail};
use windows::{
    Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
    core::{PCWSTR, w},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectLink {
    Author,
    Repository,
    Issues,
}

impl ProjectLink {
    fn url(self) -> PCWSTR {
        match self {
            Self::Author => w!("https://github.com/Y-ASLant"),
            Self::Repository => w!("https://github.com/Y-ASLant/ElegantClipboard"),
            Self::Issues => w!("https://github.com/Y-ASLant/ElegantClipboard/issues"),
        }
    }
}

pub fn open(link: ProjectLink) -> Result<()> {
    // Only URLs compiled into this module can reach ShellExecuteW.
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            link.url(),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if (result.0 as isize) <= 32 {
        bail!("无法打开浏览器（ShellExecuteW: {}）", result.0 as isize);
    }
    Ok(())
}
