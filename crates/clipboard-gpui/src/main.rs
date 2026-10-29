#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod capabilities;
mod design_system;
mod diagnostics;
mod links_core;
#[cfg(windows)]
#[path = "windows/mod.rs"]
mod native;
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "unix/mod.rs"]
mod native;
mod options;
mod popup_geometry;
mod sound_core;
mod state;
mod tray;
mod ui;
mod visual;
mod window_effect;

fn main() -> anyhow::Result<()> {
    let _diagnostics = diagnostics::initialize();
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        pid = std::process::id(),
        "application starting"
    );
    let result = run();
    match &result {
        Ok(()) => tracing::info!("application shutdown completed"),
        Err(error) => {
            tracing::error!(error = %format_args!("{error:#}"), "application shutdown failed")
        }
    }
    result
}

fn run() -> anyhow::Result<()> {
    let Some(options) = options::Options::parse(std::env::args_os().skip(1))? else {
        println!(
            "ElegantClipboard\n\n  --data-dir PATH              使用独立数据目录\n  --import-db PATH             从旧版 clipboard.db 导入到空 GPUI 目录后退出\n  --import-backup PATH         将 GPUI ZIP 备份恢复到指定空数据目录后退出\n  --import-legacy-backup PATH  将旧版 ZIP 备份导入到指定空数据目录后退出\n  --no-monitor                 不监听系统剪贴板\n  --start-hidden               启动后隐藏到托盘（托盘不可用时显示窗口）\n  --smoke-test                 打开窗口后自动退出（需 --data-dir，禁用采集）\n  --help                       显示帮助"
        );
        return Ok(());
    };
    #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
    {
        if let Some(source) = &options.import_legacy_backup {
            let data_dir = options.data_dir.clone().expect("参数已校验");
            let (path, report) = clipboard_platform::import_legacy_backup_data(source, data_dir)?;
            println!(
                "旧版备份导入完成：{} 条记录，{} 张图片、{} 个图标；{} 条图片引用未包含在备份中。新数据库：{}",
                report.items.total_items,
                report.images,
                report.icons,
                report.unbundled_images,
                path.display()
            );
            return Ok(());
        }
        if let Some(source) = &options.import_backup {
            let data_dir = options.data_dir.clone().expect("参数已校验");
            let (path, report) = clipboard_platform::restore_backup_data(source, data_dir)?;
            println!(
                "恢复完成：{} 条记录、{} 张图片、{} 个图标；{} 个源附件未包含在备份中。新数据库：{}",
                report.total_items,
                report.restored_images,
                report.restored_icons,
                report.missing_images + report.missing_icons,
                path.display()
            );
            return Ok(());
        }
        if let Some(source) = &options.import_db {
            let (path, report) = clipboard_platform::import_legacy_data(source, options.data_dir)?;
            println!(
                "导入完成：{} 条记录。新数据库：{}",
                report.total_items,
                path.display()
            );
            return Ok(());
        }
        ui::run(options)
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        let _ = options;
        anyhow::bail!("This operating system is not supported")
    }
}
