mod options;
#[cfg(any(windows, test))]
mod state;
#[cfg(windows)]
mod ui;

fn main() -> anyhow::Result<()> {
    let Some(options) = options::Options::parse(std::env::args_os().skip(1))? else {
        println!(
            "ElegantClipboard\n\n  --data-dir PATH   使用独立数据目录\n  --no-monitor      不监听系统剪贴板\n  --smoke-test      打开窗口后自动退出（需 --data-dir，禁用采集）\n  --help            显示帮助"
        );
        return Ok(());
    };
    #[cfg(windows)]
    return ui::run(options);
    #[cfg(not(windows))]
    {
        let _ = options;
        anyhow::bail!("当前基础版仅支持 Windows；macOS 与 Linux 后端尚未接入")
    }
}
