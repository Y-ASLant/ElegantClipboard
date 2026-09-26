//! Best-effort source attribution at the moment the clipboard changes.
use std::{collections::HashSet, path::Path};
use windows::Win32::{
    Foundation::{CloseHandle, HWND, LPARAM},
    System::{
        DataExchange::GetClipboardOwner,
        Threading::{
            OpenProcess, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
            QueryFullProcessImageNameW,
        },
    },
    UI::WindowsAndMessaging::{
        EnumWindows, GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
    },
};
use windows::core::BOOL;

#[derive(Clone)]
pub struct SourceApp {
    pub name: String,
    pub executable: Option<String>,
}

#[derive(Clone, Debug)]
pub struct RunningApp {
    pub name: String,
    pub process: String,
    pub icon: Option<String>,
}

pub fn running_apps(icons_dir: &Path) -> Vec<RunningApp> {
    struct Context {
        found: Vec<(String, String)>,
    }

    unsafe extern "system" fn visit(window: HWND, data: LPARAM) -> BOOL {
        let context = unsafe { &mut *(data.0 as *mut Context) };
        if !unsafe { IsWindowVisible(window) }.as_bool() {
            return BOOL(1);
        }
        let mut title = [0u16; 512];
        let length = unsafe { GetWindowTextW(window, &mut title) };
        if length <= 0 {
            return BOOL(1);
        }
        let title = String::from_utf16_lossy(&title[..length as usize]);
        if title.trim().is_empty() || title == "Program Manager" {
            return BOOL(1);
        }
        if let Some(source) = process_name_for_window(window)
            && let Some(executable) = source.executable
        {
            context.found.push((title, executable));
        }
        BOOL(1)
    }

    let mut context = Context { found: Vec::new() };
    let _ = unsafe { EnumWindows(Some(visit), LPARAM((&raw mut context) as isize)) };
    context.found.sort_by_key(|(_, path)| path.to_lowercase());
    let mut seen = HashSet::new();
    context
        .found
        .into_iter()
        .filter_map(|(name, executable)| {
            let process = executable.rsplit(['\\', '/']).next()?.to_owned();
            seen.insert(process.to_lowercase()).then(|| RunningApp {
                name,
                process,
                icon: extract_and_cache_icon(&executable, icons_dir),
            })
        })
        .collect()
}

pub fn clipboard_source() -> Option<SourceApp> {
    let owner = unsafe { GetClipboardOwner() }.ok();
    owner
        .and_then(process_name_for_window)
        .or_else(|| process_name_for_window(unsafe { GetForegroundWindow() }))
}

fn process_name_for_window(window: HWND) -> Option<SourceApp> {
    if window.0.is_null() {
        return None;
    }
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(window, Some(&mut pid)) };
    if pid == 0 || pid == std::process::id() {
        return None;
    }
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut path = [0u16; 1024];
    let mut length = path.len() as u32;
    let result = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_FORMAT(0),
            windows::core::PWSTR::from_raw(path.as_mut_ptr()),
            &mut length,
        )
    };
    let _ = unsafe { CloseHandle(process) };
    result.ok()?;
    let executable = String::from_utf16_lossy(&path[..length as usize]);
    let name = display_name_from_path(&executable)?;
    Some(SourceApp {
        name,
        executable: Some(executable),
    })
}

pub fn extract_and_cache_icon(executable: &str, icons_dir: &Path) -> Option<String> {
    let key = blake3::hash(executable.to_lowercase().as_bytes());
    let path = icons_dir.join(format!("{}.png", &key.to_hex()[..12]));
    if path.is_file() {
        return Some(path.to_string_lossy().into_owned());
    }
    let png = extract_icon_png(executable)?;
    std::fs::create_dir_all(icons_dir).ok()?;
    let mut temporary = tempfile::NamedTempFile::new_in(icons_dir).ok()?;
    std::io::Write::write_all(&mut temporary, &png).ok()?;
    match temporary.persist_noclobber(&path) {
        Ok(_) => Some(path.to_string_lossy().into_owned()),
        Err(error) if path.is_file() => {
            drop(error.file);
            Some(path.to_string_lossy().into_owned())
        }
        Err(_) => None,
    }
}

fn extract_icon_png(executable: &str) -> Option<Vec<u8>> {
    use image::{ImageFormat, RgbaImage};
    use std::io::Cursor;
    use windows::Win32::{
        Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleBitmap, CreateCompatibleDC,
            DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC, SelectObject,
        },
        UI::{
            Shell::ExtractIconExW,
            WindowsAndMessaging::{
                DI_NORMAL, DestroyIcon, DrawIconEx, GetIconInfo, HICON, ICONINFO,
            },
        },
    };

    let wide: Vec<u16> = executable.encode_utf16().chain(Some(0)).collect();
    let mut icon = HICON::default();
    let count = unsafe {
        ExtractIconExW(
            windows::core::PCWSTR::from_raw(wide.as_ptr()),
            0,
            Some(&mut icon),
            None,
            1,
        )
    };
    if count == 0 || icon.0.is_null() {
        return None;
    }
    let mut icon_info = ICONINFO::default();
    if unsafe { GetIconInfo(icon, &mut icon_info) }.is_err() {
        let _ = unsafe { DestroyIcon(icon) };
        return None;
    }
    let screen = unsafe { GetDC(None) };
    let memory = unsafe { CreateCompatibleDC(Some(screen)) };
    let bitmap = unsafe { CreateCompatibleBitmap(screen, 32, 32) };
    let pixels = if screen.0.is_null() || memory.0.is_null() || bitmap.0.is_null() {
        None
    } else {
        let old = unsafe { SelectObject(memory, bitmap.into()) };
        let drawn = unsafe { DrawIconEx(memory, 0, 0, icon, 32, 32, 0, None, DI_NORMAL) };
        unsafe { SelectObject(memory, old) };
        if drawn.is_err() {
            None
        } else {
            let mut bitmap_info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: 32,
                    biHeight: -32,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut pixels = vec![0u8; 32 * 32 * 4];
            let lines = unsafe {
                GetDIBits(
                    memory,
                    bitmap,
                    0,
                    32,
                    Some(pixels.as_mut_ptr().cast()),
                    &mut bitmap_info,
                    DIB_RGB_COLORS,
                )
            };
            (lines == 32).then_some(pixels)
        }
    };
    if !bitmap.0.is_null() {
        let _ = unsafe { DeleteObject(bitmap.into()) };
    }
    if !memory.0.is_null() {
        let _ = unsafe { DeleteDC(memory) };
    }
    if !screen.0.is_null() {
        unsafe { ReleaseDC(None, screen) };
    }
    if !icon_info.hbmColor.0.is_null() {
        let _ = unsafe { DeleteObject(icon_info.hbmColor.into()) };
    }
    if !icon_info.hbmMask.0.is_null() {
        let _ = unsafe { DeleteObject(icon_info.hbmMask.into()) };
    }
    let _ = unsafe { DestroyIcon(icon) };

    let mut pixels = pixels?;
    for chunk in pixels.as_chunks_mut::<4>().0 {
        chunk.swap(0, 2);
    }
    let image = RgbaImage::from_raw(32, 32, pixels)?;
    let mut buffer = Cursor::new(Vec::new());
    image.write_to(&mut buffer, ImageFormat::Png).ok()?;
    Some(buffer.into_inner())
}

fn display_name_from_path(path: &str) -> Option<String> {
    let name = Path::new(path).file_stem()?.to_str()?.trim();
    (!name.is_empty()).then(|| name.chars().take(128).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_path_produces_a_short_display_name() {
        assert_eq!(
            display_name_from_path(r"C:\Program Files\Example\Editor.exe"),
            Some("Editor".into())
        );
        assert_eq!(display_name_from_path(""), None);
    }

    #[test]
    #[ignore = "requires the Windows shell and a system executable"]
    fn extracts_a_png_icon_into_an_isolated_cache() -> anyhow::Result<()> {
        use image::GenericImageView;

        let system_root = std::env::var("SystemRoot")?;
        let executable = Path::new(&system_root).join("System32/notepad.exe");
        anyhow::ensure!(executable.is_file(), "system executable is unavailable");
        let cache = tempfile::tempdir()?;
        let icon = extract_and_cache_icon(&executable.to_string_lossy(), cache.path())
            .ok_or_else(|| anyhow::anyhow!("icon extraction failed"))?;
        assert!(Path::new(&icon).starts_with(cache.path()));
        let image = image::open(icon)?;
        assert_eq!(image.dimensions(), (32, 32));
        assert!(image.to_rgba8().pixels().any(|pixel| pixel[3] > 0));
        Ok(())
    }
}
