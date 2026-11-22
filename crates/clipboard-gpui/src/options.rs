use anyhow::{Result, bail};
use std::path::PathBuf;

/// GUI launch mode from process environment only. Command-line argv is ignored.
pub struct Options {
    pub data_dir: Option<PathBuf>,
    pub monitor: bool,
    pub smoke_test: bool,
    pub start_hidden: bool,
    /// One-shot: wait longer for the previous instance to release its lock.
    pub wait_for_instance: bool,
}

impl Options {
    pub fn from_env() -> Result<Self> {
        let smoke_test = clipboard_platform::launch_env::smoke_test();
        let data_dir = clipboard_platform::launch_env::data_dir();
        if smoke_test && data_dir.is_none() {
            bail!("冒烟测试需要设置环境变量 ELEGANTCLIPBOARD_DATA_DIR");
        }
        if smoke_test && clipboard_platform::launch_env::start_hidden() {
            bail!("冒烟测试不能与 ELEGANTCLIPBOARD_START_HIDDEN 同时使用");
        }
        Ok(Self {
            data_dir,
            monitor: clipboard_platform::launch_env::monitor(),
            smoke_test,
            start_hidden: clipboard_platform::launch_env::start_hidden(),
            wait_for_instance: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn clear_launch_env() {
        use clipboard_platform::launch_env::{DATA_DIR, NO_MONITOR, SMOKE_TEST, START_HIDDEN};
        for name in [DATA_DIR, NO_MONITOR, SMOKE_TEST, START_HIDDEN] {
            // SAFETY: tests hold ENV_LOCK; no other threads read these keys here.
            unsafe { std::env::remove_var(name) };
        }
    }

    #[test]
    fn env_smoke_requires_data_dir_and_disables_monitor() -> Result<()> {
        let _guard = ENV_LOCK.lock().unwrap();
        clear_launch_env();
        // SAFETY: exclusive ENV_LOCK.
        unsafe {
            std::env::set_var(clipboard_platform::launch_env::SMOKE_TEST, "1");
        }
        assert!(Options::from_env().is_err());
        // SAFETY: exclusive ENV_LOCK.
        unsafe {
            std::env::set_var(clipboard_platform::launch_env::DATA_DIR, r"C:\isolated");
        }
        let options = Options::from_env()?;
        assert!(options.smoke_test);
        assert!(!options.monitor);
        assert!(!options.start_hidden);
        assert_eq!(
            options.data_dir.as_deref(),
            Some(std::path::Path::new(r"C:\isolated"))
        );
        clear_launch_env();
        Ok(())
    }

    #[test]
    fn start_hidden_from_env() -> Result<()> {
        let _guard = ENV_LOCK.lock().unwrap();
        clear_launch_env();
        // SAFETY: exclusive ENV_LOCK.
        unsafe {
            std::env::set_var(clipboard_platform::launch_env::START_HIDDEN, "1");
        }
        let options = Options::from_env()?;
        assert!(options.start_hidden);
        assert!(options.monitor);
        clear_launch_env();
        Ok(())
    }
}
