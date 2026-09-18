mod options;
#[cfg(windows)]
mod paste;
#[cfg(any(windows, test))]
mod state;
#[cfg(windows)]
mod tray;
#[cfg(windows)]
mod ui;
#[cfg(windows)]
mod visual;

fn main() -> anyhow::Result<()> {
    let Some(options) = options::Options::parse(std::env::args_os().skip(1))? else {
        println!(
            "ElegantClipboard\n\n  --data-dir PATH   使用独立数据目录\n  --import-db PATH  从旧版 clipboard.db 导入到空 GPUI 目录后退出\n  --no-monitor      不监听系统剪贴板\n  --smoke-test      打开窗口后自动退出（需 --data-dir，禁用采集）\n  --help            显示帮助"
        );
        return Ok(());
    };
    #[cfg(windows)]
    {
        if let Some(source) = &options.import_db {
            let (path, report) = clipboard_platform::import_legacy_data(source, options.data_dir)?;
            println!(
                "导入完成：{} 条记录，其中 {} 条默认分组文本/网址目前可在 GPUI 版查看。新数据库：{}",
                report.total_items,
                report.visible_text_items,
                path.display()
            );
            return Ok(());
        }
        ui::run(options)
    }
    #[cfg(not(windows))]
    {
        let _ = options;
        anyhow::bail!("当前基础版仅支持 Windows；macOS 与 Linux 后端尚未接入")
    }
}
