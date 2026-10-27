use anyhow::{Result, bail};
use clipboard_core::preferences::PasteKeyPreference;
use clipboard_platform::{clipboard_sequence, desktop};
use gpui_kit::Window;
use std::time::{Duration, Instant};

pub const POLL_INTERVAL: Duration = Duration::from_millis(20);
const MODIFIER_WAIT: Duration = Duration::from_secs(2);

pub fn is_external_target(_: &Window, target: (isize, u32)) -> bool {
    target.0 != 0
        && desktop::unavailable_reason().is_none()
        && target.1 != std::process::id()
        && desktop::target_alive(target)
}

pub struct PendingPaste {
    target: (isize, u32),
    sequence: u32,
    paste_key: PasteKeyPreference,
    started: Instant,
}

impl PendingPaste {
    pub fn begin(
        target: (isize, u32),
        sequence: u32,
        paste_key: PasteKeyPreference,
    ) -> Result<Self> {
        if target.0 == 0 || target.1 == std::process::id() || !desktop::target_alive(target) {
            bail!("目标窗口已关闭，内容已复制，请手动粘贴");
        }
        if !desktop::foreground_target()
            .is_some_and(|current| current == target || current.1 == std::process::id())
        {
            bail!("前台窗口已变化，已取消自动粘贴，请检查后手动粘贴");
        }
        if sequence == 0 || clipboard_sequence()? != sequence {
            bail!("剪贴板内容已变化，已取消自动粘贴，请检查后手动粘贴");
        }
        desktop::activate_target(target)?;
        Ok(Self {
            target,
            sequence,
            paste_key,
            started: Instant::now(),
        })
    }

    pub fn try_send(&self) -> Result<bool> {
        if !desktop::target_alive(self.target) {
            bail!("目标窗口已关闭，内容已复制，请手动粘贴");
        }
        if desktop::foreground_target() != Some(self.target) {
            bail!("前台窗口已变化，已取消自动粘贴，请检查后手动粘贴");
        }
        if clipboard_sequence()? != self.sequence {
            bail!("剪贴板内容已变化，已取消自动粘贴，请检查后手动粘贴");
        }
        if self.started.elapsed() >= MODIFIER_WAIT {
            bail!("等待修饰键释放超时，已取消自动粘贴；请松开按键后手动粘贴");
        }
        if desktop::modifiers_pressed() {
            return Ok(false);
        }
        desktop::send_paste(self.paste_key)?;
        Ok(true)
    }
}
