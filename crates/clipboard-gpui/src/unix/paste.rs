use anyhow::{Result, bail};
use clipboard_core::preferences::PasteKeyPreference;
use clipboard_platform::{
    clipboard_sequence,
    desktop,
    paste_delivery::{
        paste_shortcut_trigger_held, PasteWaitPolicy, PASTE_INJECT_WAIT, ReadyResult,
    },
};
use gpui_kit::Window;
use std::time::{Duration, Instant};

pub const POLL_INTERVAL: Duration = Duration::from_millis(20);

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
    wait: PasteWaitPolicy,
    started: Instant,
}

impl PendingPaste {
    pub fn begin(
        target: (isize, u32),
        sequence: u32,
        paste_key: PasteKeyPreference,
        wait: PasteWaitPolicy,
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
            wait,
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
        let trigger_held = self
            .wait
            .trigger_shortcut
            .as_deref()
            .is_some_and(paste_shortcut_trigger_held);
        match self.wait.ready_to_inject(
            desktop::modifiers_pressed(),
            trigger_held,
            self.started.elapsed(),
        ) {
            ReadyResult::TimedOut => {
                bail!("等待修饰键释放超时，已取消自动粘贴；请松开按键后手动粘贴");
            }
            ReadyResult::Waiting => return Ok(false),
            ReadyResult::Ready => {}
        }
        let allow_modifiers = matches!(
            (&self.wait.mode, self.wait.trigger_shortcut.as_ref()),
            (clipboard_core::preferences::PasteDeliveryMode::Rapid, Some(_))
        );
        desktop::send_paste(self.paste_key, allow_modifiers)?;
        Ok(true)
    }
}
