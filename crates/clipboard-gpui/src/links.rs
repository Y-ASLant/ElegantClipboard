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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn about_links_are_fixed_https_destinations() {
        for link in [
            ProjectLink::Author,
            ProjectLink::Repository,
            ProjectLink::Issues,
        ] {
            let mut index = 0;
            let mut units = Vec::new();
            unsafe {
                while *link.url().0.add(index) != 0 {
                    units.push(*link.url().0.add(index));
                    index += 1;
                }
            }
            let url = String::from_utf16(&units).unwrap();
            assert!(url.starts_with("https://github.com/Y-ASLant"));
        }
    }
}
