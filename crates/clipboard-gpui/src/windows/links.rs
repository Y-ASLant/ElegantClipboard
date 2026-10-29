use crate::links_core::ProjectLink;
use ::windows::{
    Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
    core::{HSTRING, PCWSTR, w},
};
use anyhow::{Result, bail};

pub fn open(link: ProjectLink) -> Result<()> {
    // The URL's only source is the closed ProjectLink enum (links_core), so
    // ShellExecuteW never sees an arbitrary runtime string.
    let url = HSTRING::from(link.url());
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(url.as_ptr()),
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
