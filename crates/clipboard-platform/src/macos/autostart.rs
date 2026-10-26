//! Per-profile LaunchAgent, loaded by launchd at the next graphical login.
use anyhow::{Context, Result};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

fn configuration(data_dir: &Path) -> Result<(PathBuf, String)> {
    let data_dir = data_dir.canonicalize().context("无法解析数据目录")?;
    let executable = std::env::current_exe()?;
    let hash = clipboard_core::backup::hash_token(data_dir.as_os_str().as_encoded_bytes());
    let label = format!("app.elegantclipboard.{}", &hash[..16]);
    let home = std::env::var_os("HOME").context("macOS HOME 环境变量缺失")?;
    let path = PathBuf::from(home)
        .join("Library/LaunchAgents")
        .join(format!("{label}.plist"));
    let executable = xml(executable.to_str().context("程序路径不是有效 UTF-8")?);
    let data_dir = xml(data_dir.to_str().context("数据目录不是有效 UTF-8")?);
    let contents = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\"><dict>\n\
         <key>Label</key><string>{label}</string>\n\
         <key>ProgramArguments</key><array><string>{executable}</string>\
         <string>--start-hidden</string><string>--data-dir</string><string>{data_dir}</string></array>\n\
         <key>RunAtLoad</key><true/>\n\
         <key>LimitLoadToSessionType</key><string>Aqua</string>\n\
         </dict></plist>\n"
    );
    Ok((path, contents))
}

fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub fn enabled(data_dir: &Path) -> Result<bool> {
    let (path, expected) = configuration(data_dir)?;
    match std::fs::read_to_string(path) {
        Ok(value) => Ok(value == expected),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).context("无法读取 macOS 登录启动设置"),
    }
}

pub fn set_enabled(data_dir: &Path, enabled: bool) -> Result<()> {
    let (path, contents) = configuration(data_dir)?;
    if enabled {
        let directory = path.parent().context("LaunchAgents 路径无效")?;
        std::fs::create_dir_all(directory)?;
        let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
        temporary.write_all(contents.as_bytes())?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(&path)
            .context("无法保存 macOS 登录启动设置")?;
    } else {
        match std::fs::remove_file(path) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error).context("无法关闭 macOS 登录启动"),
        }
    }
    Ok(())
}
