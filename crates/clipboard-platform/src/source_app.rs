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

pub use crate::source_app_types::{RunningApp, SourceApp};

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
    context
        .found
        .sort_by_cached_key(|(_, path)| path.to_lowercase());
    let mut seen = HashSet::new();
    context
        .found
        .into_iter()
        .filter_map(|(name, executable)| {
            let process = executable.rsplit(['\\', '/']).next()?;
            if !seen.insert(process.to_lowercase()) {
                return None;
            }
            Some(RunningApp {
                name,
                process: process.to_owned(),
                icon: extract_and_cache_icon(&executable, icons_dir),
            })
        })
        .collect()
}

pub fn clipboard_source() -> Option<SourceApp> {
    let owner = unsafe { GetClipboardOwner() }.ok();
    let foreground = unsafe { GetForegroundWindow() };
    let mut source = owner
        .and_then(process_name_for_window)
        .or_else(|| process_name_for_window(foreground))?;
    // The owner is often a hidden helper window; the document is named in the
    // foreground title, which only counts when it belongs to the same program.
    if process_name_for_window(foreground)
        .is_some_and(|front| front.executable == source.executable)
    {
        source.detail = window_title(foreground)
            .as_deref()
            .and_then(document_from_title);
    }
    Some(source)
}

fn window_title(window: HWND) -> Option<String> {
    let mut title = [0u16; 512];
    let length = unsafe { GetWindowTextW(window, &mut title) };
    (length > 0).then(|| String::from_utf16_lossy(&title[..length as usize]))
}

/// Leading part of a "document - application" title; a bare title names no document.
fn document_from_title(title: &str) -> Option<String> {
    let end = [" - ", " — ", " – "]
        .iter()
        .filter_map(|separator| title.find(separator))
        .min()?;
    let document = title[..end].trim().trim_start_matches(['*', '●']).trim();
    (!document.is_empty()).then(|| document.to_owned())
}

/// `SourceURL` from the header of a CF_HTML payload, as browsers write it.
pub(crate) fn source_url_from_html(data: &[u8]) -> Option<String> {
    let head = String::from_utf8_lossy(&data[..data.len().min(4096)]);
    head.split('<')
        .next()?
        .lines()
        .find_map(|line| line.strip_prefix("SourceURL:"))
        .map(str::trim)
        .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
        .map(str::to_owned)
}

/// Short process/app name for a paste target `(hwnd, pid)`, best-effort.
pub fn label_for_target((hwnd, _pid): (isize, u32)) -> Option<String> {
    process_name_for_window(HWND(hwnd as *mut _)).map(|source| source.name)
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
        detail: None,
    })
}

pub fn extract_and_cache_icon(executable: &str, icons_dir: &Path) -> Option<String> {
    let hash = clipboard_core::backup::hash_token(executable.to_lowercase().as_bytes());
    let stem = &hash[..12];
    let path = icons_dir.join(format!("{stem}.png"));
    // g2 = alpha-preserving DIB; old cache used screen bitmaps (black fringes).
    let generation = icons_dir.join(format!("{stem}.g2"));
    if path.is_file() && generation.is_file() {
        return Some(path.to_string_lossy().into_owned());
    }
    let png = extract_icon_png(executable)?;
    std::fs::create_dir_all(icons_dir).ok()?;
    let mut temporary = tempfile::NamedTempFile::new_in(icons_dir).ok()?;
    std::io::Write::write_all(&mut temporary, &png).ok()?;
    temporary.persist(&path).ok()?;
    let _ = std::fs::write(&generation, b"2");
    Some(path.to_string_lossy().into_owned())
}

const ICON_PX: i32 = 32;
const ICON_BYTES: usize = (ICON_PX * ICON_PX * 4) as usize;

fn extract_icon_png(executable: &str) -> Option<Vec<u8>> {
    use image::{ImageFormat, RgbaImage};
    use std::io::Cursor;

    let icon = load_icon(executable)?;
    // Prefer a zeroed 32-bit DIB so DrawIconEx can keep real alpha.
    // Screen-compatible bitmaps drop alpha and leave black fringes.
    let mut pixels = draw_icon(icon, [0, 0, 0, 0])?;
    if !pixels.as_chunks::<4>().0.iter().any(|px| px[3] != 0) {
        let on_black = draw_icon(icon, [0, 0, 0, 255])?;
        let on_white = draw_icon(icon, [255, 255, 255, 255])?;
        for ((out, black), white) in pixels
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(on_black.as_chunks::<4>().0)
            .zip(on_white.as_chunks::<4>().0)
        {
            let alpha = 255u8.saturating_sub(
                ((u16::from(white[0].saturating_sub(black[0]))
                    + u16::from(white[1].saturating_sub(black[1]))
                    + u16::from(white[2].saturating_sub(black[2])))
                    / 3) as u8,
            );
            if alpha == 0 {
                out.copy_from_slice(&[0, 0, 0, 0]);
            } else {
                out[0] = black[0];
                out[1] = black[1];
                out[2] = black[2];
                out[3] = alpha;
            }
        }
    }
    let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::DestroyIcon(icon) };

    for chunk in pixels.as_chunks_mut::<4>().0 {
        chunk.swap(0, 2);
    }
    let image = RgbaImage::from_raw(ICON_PX as u32, ICON_PX as u32, pixels)?;
    let mut buffer = Cursor::new(Vec::new());
    image.write_to(&mut buffer, ImageFormat::Png).ok()?;
    Some(buffer.into_inner())
}

fn draw_icon(
    icon: windows::Win32::UI::WindowsAndMessaging::HICON,
    fill: [u8; 4],
) -> Option<Vec<u8>> {
    use windows::Win32::{
        Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection,
            DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
        },
        UI::WindowsAndMessaging::{DI_NORMAL, DrawIconEx},
    };

    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: ICON_PX,
            biHeight: -ICON_PX,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let screen = unsafe { GetDC(None) };
    let memory = unsafe { CreateCompatibleDC(Some(screen)) };
    let mut bits = std::ptr::null_mut();
    let bitmap =
        unsafe { CreateDIBSection(Some(memory), &info, DIB_RGB_COLORS, &mut bits, None, 0) }
            .ok()
            .filter(|bitmap| !bitmap.0.is_null() && !bits.is_null());
    let pixels = match bitmap {
        Some(bitmap) if !screen.0.is_null() && !memory.0.is_null() => {
            let slice = unsafe { std::slice::from_raw_parts_mut(bits.cast::<u8>(), ICON_BYTES) };
            for px in slice.as_chunks_mut::<4>().0 {
                px.copy_from_slice(&fill);
            }
            let old = unsafe { SelectObject(memory, bitmap.into()) };
            let drawn =
                unsafe { DrawIconEx(memory, 0, 0, icon, ICON_PX, ICON_PX, 0, None, DI_NORMAL) };
            unsafe { SelectObject(memory, old) };
            let out = drawn.is_ok().then(|| slice.to_vec());
            let _ = unsafe { DeleteObject(bitmap.into()) };
            out
        }
        _ => None,
    };
    if !memory.0.is_null() {
        let _ = unsafe { DeleteDC(memory) };
    }
    if !screen.0.is_null() {
        unsafe { ReleaseDC(None, screen) };
    }
    pixels
}

fn load_icon(executable: &str) -> Option<windows::Win32::UI::WindowsAndMessaging::HICON> {
    use windows::Win32::UI::{Shell::ExtractIconExW, WindowsAndMessaging::HICON};

    let normalized = executable.replace('/', "\\");
    let wide = crate::instance::to_wide(&normalized);
    let path = windows::core::PCWSTR::from_raw(wide.as_ptr());
    let mut icon = HICON::default();
    let count = unsafe { ExtractIconExW(path, 0, Some(&mut icon), None, 1) };
    if count != 0 && !icon.0.is_null() {
        return Some(icon);
    }
    shell_icon(path)
}

fn shell_icon(
    path: windows::core::PCWSTR,
) -> Option<windows::Win32::UI::WindowsAndMessaging::HICON> {
    use windows::Win32::{
        Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES,
        System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize},
        UI::Shell::{SHFILEINFOW, SHGFI_ICON, SHGetFileInfoW},
    };

    let init = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let mut info = SHFILEINFOW::default();
    let found = unsafe {
        SHGetFileInfoW(
            path,
            FILE_FLAGS_AND_ATTRIBUTES::default(),
            Some(&mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON,
        )
    };
    let icon = (found != 0 && !info.hIcon.0.is_null()).then_some(info.hIcon);
    if init.is_ok() {
        unsafe { CoUninitialize() };
    }
    icon
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
    fn window_title_names_a_document_only_before_an_application_suffix() {
        assert_eq!(
            document_from_title("*备忘.txt - 记事本"),
            Some("备忘.txt".into())
        );
        assert_eq!(
            document_from_title("main.rs - project - Editor"),
            Some("main.rs".into())
        );
        assert_eq!(document_from_title("Documents"), None);
        assert_eq!(document_from_title(" - Editor"), None);
    }

    #[test]
    fn html_header_yields_only_a_web_source_url() {
        let html = b"Version:0.9\r\nStartHTML:0000000105\r\nSourceURL:https://example.com/a?b=1\r\n<html>SourceURL:https://wrong.example</html>";
        assert_eq!(
            source_url_from_html(html),
            Some("https://example.com/a?b=1".into())
        );
        assert_eq!(
            source_url_from_html(b"Version:0.9\r\nSourceURL:about:blank\r\n<html>"),
            None
        );
        assert_eq!(source_url_from_html(b"Version:0.9\r\n<html></html>"), None);
    }

    #[test]
    #[ignore = "requires the Windows shell and a system executable"]
    fn extracts_a_png_icon_into_an_isolated_cache() -> anyhow::Result<()> {
        use image::GenericImageView;

        let system_root = std::env::var("SystemRoot")?;
        let root = Path::new(&system_root);
        let mut executables = vec![
            root.join("System32/notepad.exe"),
            root.join("System32/ApplicationFrameHost.exe"),
        ];
        if let Ok(entries) = std::fs::read_dir(root.join("SystemApps")) {
            executables.extend(entries.flatten().filter_map(|entry| {
                let candidate = entry.path().join("TextInputHost.exe");
                candidate.is_file().then_some(candidate)
            }));
        }
        anyhow::ensure!(
            executables.iter().any(|path| path
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case("TextInputHost.exe"))),
            "TextInputHost.exe is unavailable"
        );
        for executable in executables {
            anyhow::ensure!(executable.is_file(), "{}", executable.display());
            let cache = tempfile::tempdir()?;
            let icon = extract_and_cache_icon(&executable.to_string_lossy(), cache.path())
                .ok_or_else(|| {
                    anyhow::anyhow!("icon extraction failed for {}", executable.display())
                })?;
            assert!(Path::new(&icon).starts_with(cache.path()));
            let image = image::open(&icon)?;
            assert_eq!(image.dimensions(), (32, 32), "{}", executable.display());
            assert!(
                image.to_rgba8().pixels().any(|pixel| pixel[3] > 0),
                "{}",
                executable.display()
            );
        }
        Ok(())
    }
}
