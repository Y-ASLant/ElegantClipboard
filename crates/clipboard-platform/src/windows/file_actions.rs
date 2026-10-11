use anyhow::{Context, Result, bail};
use clipboard_core::{History, database::ClipboardItem};
use std::path::{Path, PathBuf};

pub(super) fn plain_text_for_copy(item: &ClipboardItem) -> Result<&str> {
    if !matches!(item.content_type.as_str(), "text" | "url" | "html" | "rtf") {
        bail!("该记录不支持纯文本复制");
    }
    item.text_content
        .as_deref()
        .filter(|text| !text.is_empty())
        .context("记录没有可复制的纯文本")
}

pub(super) fn item_paths_for_action(
    item: &ClipboardItem,
    staged_dir: &Path,
) -> Result<Vec<String>> {
    let paths = match item.content_type.as_str() {
        "files" => History::file_paths_for_copy(item, staged_dir)?,
        "image" => vec![item.image_path.clone().context("图片文件路径缺失")?],
        _ => bail!("该记录不是文件或图片"),
    };
    if paths.iter().any(|path| !Path::new(path).exists()) {
        bail!("源文件、文件夹或图片已不存在");
    }
    Ok(paths)
}

pub(super) fn reveal_in_explorer(path: &Path) -> Result<()> {
    explorer_command(path)?
        .spawn()
        .context("无法启动 Windows 资源管理器")?;
    Ok(())
}

pub(super) fn explorer_command(path: &Path) -> Result<std::process::Command> {
    let mut command = std::process::Command::new(explorer_executable()?);
    command.arg("/select,").arg(path);
    Ok(command)
}

fn explorer_executable() -> Result<PathBuf> {
    let system_root = std::env::var_os("SystemRoot").context("无法确定 Windows 系统目录")?;
    let explorer = PathBuf::from(system_root).join("explorer.exe");
    if !explorer.is_file() {
        bail!("找不到 Windows 资源管理器");
    }
    Ok(explorer)
}

pub(super) fn open_data_directory(data_dir: &Path) -> Result<()> {
    data_directory_command(data_dir)?
        .spawn()
        .context("无法打开数据目录")?;
    Ok(())
}

pub(super) fn data_directory_command(data_dir: &Path) -> Result<std::process::Command> {
    if !data_dir.is_dir() {
        bail!("数据目录不存在");
    }
    let mut command = std::process::Command::new(explorer_executable()?);
    command.arg(data_dir);
    Ok(command)
}

pub(super) fn save_item_as(
    item: &ClipboardItem,
    staged_dir: &Path,
    destination: &Path,
) -> Result<u64> {
    if !destination.is_absolute() {
        bail!("另存为目标必须是绝对路径");
    }
    let destination_name = destination.file_name().context("另存为目标缺少文件名")?;
    let destination_parent = destination.parent().context("另存为目标缺少父目录")?;
    if !destination_parent.is_dir() {
        bail!("另存为目标目录不存在");
    }
    if destination.is_dir() {
        bail!("另存为目标不能是文件夹");
    }

    let paths = item_paths_for_action(item, staged_dir)?;
    let source = Path::new(&paths[0]);
    if !source.is_file() {
        bail!("文件夹不支持另存为，请先在资源管理器中复制");
    }
    let canonical_source = source.canonicalize().context("无法解析另存为源文件")?;
    let canonical_destination = if destination.exists() {
        destination
            .canonicalize()
            .context("无法解析另存为目标文件")?
    } else {
        destination_parent
            .canonicalize()
            .context("无法解析另存为目标目录")?
            .join(destination_name)
    };
    if canonical_source == canonical_destination {
        bail!("不能将文件另存为其自身");
    }
    std::fs::copy(&canonical_source, destination).with_context(|| {
        format!(
            "无法将 {} 另存为 {}",
            canonical_source.display(),
            destination.display()
        )
    })
}
