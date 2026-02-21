//! 剪贴板来源应用检测
//! 策略: GetClipboardOwner(主) + GetForegroundWindow(补充)
//! 剪贴板所有者是实际写入剪贴板的应用，对截图工具等后台写入场景更准确

use std::path::Path;
use tracing::{debug, warn};

#[derive(Debug, Clone)]
pub struct SourceAppInfo {
    pub app_name: String,
    pub exe_path: String,
    pub icon_cache_key: String,
}

/// 获取剪贴板来源应用，需在剪贴板变化回调的最开始调用
#[cfg(target_os = "windows")]
pub fn get_clipboard_source_app() -> Option<SourceAppInfo> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::DataExchange::GetClipboardOwner;
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    let self_pid = std::process::id();

    // 从 HWND 解析完整来源信息，失败或属于自身则返回 None
    unsafe fn try_resolve(hwnd: HWND, self_pid: u32) -> Option<SourceAppInfo> {
        if hwnd.0.is_null() {
            return None;
        }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 || pid == self_pid {
            return None;
        }

        let exe_path = get_exe_path_from_pid(pid)?;
        let exe_path = resolve_uwp_app(hwnd, &exe_path).unwrap_or(exe_path);
        let app_name = get_app_display_name(&exe_path);
        let icon_cache_key = compute_icon_cache_key(&exe_path);

        Some(SourceAppInfo {
            app_name,
            exe_path,
            icon_cache_key,
        })
    }

    unsafe {
        // 主策略: 剪贴板所有者（实际写入剪贴板的应用，对截图工具等更准确）
        if let Ok(owner) = GetClipboardOwner() {
            if let Some(info) = try_resolve(owner, self_pid) {
                debug!("来源(所有者): {} ({})", info.app_name, info.exe_path);
                return Some(info);
            }
        }

        // 补充策略: 前台窗口（部分应用不设置剪贴板所有者时的兜底）
        let fg = GetForegroundWindow();
        if let Some(info) = try_resolve(fg, self_pid) {
            debug!("来源(前台): {} ({})", info.app_name, info.exe_path);
            return Some(info);
        }

        debug!("无法识别剪贴板来源");
        None
    }
}

#[cfg(not(target_os = "windows"))]
pub fn get_clipboard_source_app() -> Option<SourceAppInfo> {
    None
}

/// 通过 PID 获取进程 exe 路径
#[cfg(target_os = "windows")]
unsafe fn get_exe_path_from_pid(pid: u32) -> Option<String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let mut buf = [0u16; 1024];
    let mut size = buf.len() as u32;
    let result = QueryFullProcessImageNameW(
        handle,
        PROCESS_NAME_FORMAT(0),
        windows::core::PWSTR::from_raw(buf.as_mut_ptr()),
        &mut size,
    );
    let _ = CloseHandle(handle);
    result.ok()?;

    let path = String::from_utf16_lossy(&buf[..size as usize]);
    if path.is_empty() {
        None
    } else {
        Some(path)
    }
}

/// UWP 应用通过 ApplicationFrameHost 托管，遍历子窗口找到真实进程
#[cfg(target_os = "windows")]
unsafe fn resolve_uwp_app(
    owner_hwnd: windows::Win32::Foundation::HWND,
    exe_path: &str,
) -> Option<String> {
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{EnumChildWindows, GetWindowThreadProcessId};

    let exe_name = Path::new(exe_path).file_name()?.to_str()?;
    if !exe_name.eq_ignore_ascii_case("ApplicationFrameHost.exe") {
        return None;
    }

    let mut host_pid: u32 = 0;
    GetWindowThreadProcessId(owner_hwnd, Some(&mut host_pid));

    struct CallbackData {
        host_pid: u32,
        found_path: Option<String>,
    }

    unsafe extern "system" fn enum_callback(hwnd: HWND, lparam: LPARAM) -> windows_core::BOOL {
        unsafe {
            let data = &mut *(lparam.0 as *mut CallbackData);
            let mut child_pid: u32 = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut child_pid));

            if child_pid != 0 && child_pid != data.host_pid {
                if let Some(path) = get_exe_path_from_pid(child_pid) {
                    let name = Path::new(&path).file_name().and_then(|n| n.to_str());
                    if !name.is_some_and(|n| n.eq_ignore_ascii_case("ApplicationFrameHost.exe")) {
                        data.found_path = Some(path);
                        return windows_core::BOOL::from(false);
                    }
                }
            }
            windows_core::BOOL::from(true)
        }
    }

    let mut data = CallbackData {
        host_pid,
        found_path: None,
    };
    let _ = EnumChildWindows(
        Some(owner_hwnd),
        Some(enum_callback),
        LPARAM(&mut data as *mut _ as isize),
    );
    data.found_path
}

/// 从 exe 版本信息读取 FileDescription，失败则用文件名
#[cfg(target_os = "windows")]
fn get_app_display_name(exe_path: &str) -> String {
    get_file_description(exe_path)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            Path::new(exe_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Unknown")
                .to_string()
        })
}

/// 读取 exe 版本资源中的 FileDescription
#[cfg(target_os = "windows")]
fn get_file_description(exe_path: &str) -> Option<String> {
    use std::ffi::c_void;
    use windows::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
    };

    unsafe fn query_string(buf: &[u8], sub_path: &str) -> Option<String> {
        use std::ffi::c_void;
        use windows::Win32::Storage::FileSystem::VerQueryValueW;
        let wide: Vec<u16> = sub_path.encode_utf16().collect();
        let mut ptr: *mut c_void = std::ptr::null_mut();
        let mut len: u32 = 0;
        if !VerQueryValueW(
            buf.as_ptr() as *const c_void,
            windows::core::PCWSTR::from_raw(wide.as_ptr()),
            &mut ptr,
            &mut len,
        )
        .as_bool()
            || ptr.is_null()
            || len == 0
        {
            return None;
        }
        let slice = std::slice::from_raw_parts(ptr as *const u16, len as usize);
        let end = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
        let s = String::from_utf16_lossy(&slice[..end]).trim().to_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }

    unsafe {
        let wide_path: Vec<u16> = exe_path.encode_utf16().chain(std::iter::once(0)).collect();
        let pcwstr = windows::core::PCWSTR::from_raw(wide_path.as_ptr());
        let size = GetFileVersionInfoSizeW(pcwstr, None);
        if size == 0 {
            return None;
        }

        let mut buf = vec![0u8; size as usize];
        GetFileVersionInfoW(pcwstr, None, size, buf.as_mut_ptr() as *mut c_void).ok()?;

        // 尝试从翻译表获取本地化语言
        let mut lang_ptr: *mut c_void = std::ptr::null_mut();
        let mut lang_len: u32 = 0;
        let trans: Vec<u16> = "\\VarFileInfo\\Translation\0".encode_utf16().collect();
        if VerQueryValueW(
            buf.as_ptr() as *const c_void,
            windows::core::PCWSTR::from_raw(trans.as_ptr()),
            &mut lang_ptr,
            &mut lang_len,
        )
        .as_bool()
            && !lang_ptr.is_null()
            && lang_len >= 4
        {
            let lang = *(lang_ptr as *const u16);
            let cp = *((lang_ptr as *const u16).add(1));
            let path = format!(
                "\\StringFileInfo\\{:04x}{:04x}\\FileDescription\0",
                lang, cp
            );
            if let Some(desc) = query_string(&buf, &path) {
                return Some(desc);
            }
        }

        // 回退: 英文语言环境
        query_string(&buf, "\\StringFileInfo\\040904B0\\FileDescription\0")
    }
}

fn compute_icon_cache_key(exe_path: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(exe_path.to_lowercase().as_bytes());
    hasher.finalize().to_hex()[..12].to_string()
}

/// 提取应用图标并缓存为 PNG，返回缓存文件路径
#[cfg(target_os = "windows")]
pub fn extract_and_cache_icon(exe_path: &str, icons_dir: &Path, cache_key: &str) -> Option<String> {
    let icon_path = icons_dir.join(format!("{}.png", cache_key));
    if icon_path.exists() {
        return Some(icon_path.to_string_lossy().to_string());
    }

    std::fs::create_dir_all(icons_dir).ok()?;
    let png_data = extract_icon_png(exe_path)?;
    if let Err(e) = std::fs::write(&icon_path, &png_data) {
        warn!("图标缓存失败 {}: {}", exe_path, e);
        return None;
    }
    Some(icon_path.to_string_lossy().to_string())
}

#[cfg(not(target_os = "windows"))]
pub fn extract_and_cache_icon(
    _exe_path: &str,
    _icons_dir: &Path,
    _cache_key: &str,
) -> Option<String> {
    None
}

/// 通过 SHGetFileInfoW + GDI 提取 exe 图标为 PNG
#[cfg(target_os = "windows")]
fn extract_icon_png(exe_path: &str) -> Option<Vec<u8>> {
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
        ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use windows::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};
    use windows::Win32::UI::WindowsAndMessaging::{
        DestroyIcon, DrawIconEx, GetIconInfo, DI_NORMAL, ICONINFO,
    };

    unsafe {
        let wide_path: Vec<u16> = exe_path.encode_utf16().chain(std::iter::once(0)).collect();
        let mut shfi = SHFILEINFOW::default();
        let result = SHGetFileInfoW(
            windows::core::PCWSTR::from_raw(wide_path.as_ptr()),
            windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut shfi),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        );
        if result == 0 || shfi.hIcon.0.is_null() {
            return None;
        }

        let hicon = shfi.hIcon;
        let mut icon_info = ICONINFO::default();
        if GetIconInfo(hicon, &mut icon_info).is_err() {
            let _ = DestroyIcon(hicon);
            return None;
        }
        let (color_bmp, mask_bmp) = (icon_info.hbmColor, icon_info.hbmMask);
        let (w, h): (i32, i32) = (32, 32);

        // 创建内存 DC 并绘制图标
        let screen_dc = GetDC(None);
        let mem_dc = CreateCompatibleDC(Some(screen_dc));
        let mem_bmp = CreateCompatibleBitmap(screen_dc, w, h);
        let old_bmp = SelectObject(mem_dc, mem_bmp.into());
        let _ = DrawIconEx(mem_dc, 0, 0, hicon, w, h, 0, None, DI_NORMAL);

        // 读取像素
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        let lines = GetDIBits(
            mem_dc,
            mem_bmp,
            0,
            h as u32,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );

        // 清理 GDI 资源
        SelectObject(mem_dc, old_bmp);
        let _ = DeleteObject(mem_bmp.into());
        let _ = DeleteDC(mem_dc);
        ReleaseDC(None, screen_dc);
        if !color_bmp.0.is_null() {
            let _ = DeleteObject(color_bmp.into());
        }
        if !mask_bmp.0.is_null() {
            let _ = DeleteObject(mask_bmp.into());
        }
        let _ = DestroyIcon(hicon);

        if lines == 0 {
            return None;
        }

        // BGRA → RGBA → PNG
        for chunk in pixels.chunks_exact_mut(4) {
            chunk.swap(0, 2);
        }
        let img = image::RgbaImage::from_raw(w as u32, h as u32, pixels)?;
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Png).ok()?;
        Some(buf.into_inner())
    }
}
