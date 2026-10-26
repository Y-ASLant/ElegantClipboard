use super::x11::X11;
use std::{collections::HashSet, path::Path};
use x11rb::protocol::{
    res::{ClientIdMask, ClientIdSpec, ConnectionExt as _},
    xproto::ConnectionExt,
};

pub use crate::source_app_types::{RunningApp, SourceApp};

fn executable(x: &X11, window: u32) -> Option<String> {
    let pid = x.pid(window).ok()?;
    if pid == std::process::id() {
        return None;
    }
    // EWMH PIDs are meaningful locally only; never attribute a remote X client
    // to an unrelated local process with the same numeric identifier.
    let host = x
        .connection
        .get_property(
            false,
            window,
            x.atom("WM_CLIENT_MACHINE").ok()?,
            0u32,
            0,
            256,
        )
        .ok()?
        .reply()
        .ok()?
        .value;
    let local = std::fs::read_to_string("/proc/sys/kernel/hostname").ok()?;
    let host = std::str::from_utf8(&host).ok()?.trim_end_matches('\0');
    if host != local.trim() {
        return None;
    }
    std::fs::read_link(format!("/proc/{pid}/exe"))
        .ok()?
        .to_str()
        .map(str::to_owned)
}

fn source(x: &X11, window: u32) -> Option<SourceApp> {
    let executable = executable(x, window)?;
    let name = Path::new(&executable)
        .file_stem()?
        .to_string_lossy()
        .into_owned();
    Some(SourceApp {
        name,
        executable: Some(executable),
    })
}

pub fn clipboard_source() -> Option<SourceApp> {
    let x = X11::connect().ok()?;
    let selection = x.atom("CLIPBOARD").ok()?;
    let owner = x
        .connection
        .get_selection_owner(selection)
        .ok()?
        .reply()
        .ok()?
        .owner;
    if owner == 0 {
        return None;
    }
    let attribution = clipboard_owner_source(&x, owner);
    let current = x
        .connection
        .get_selection_owner(selection)
        .ok()?
        .reply()
        .ok()?
        .owner;
    // Shared capture also brackets content and attribution with the XFixes epoch,
    // which detects same-owner replacement and owner A -> B -> A transitions.
    (current == owner).then_some(attribution).flatten()
}

fn clipboard_owner_source(x: &X11, owner: u32) -> Option<SourceApp> {
    let pid = x
        .connection
        .res_query_version(1, 2)
        .ok()
        .and_then(|reply| reply.reply().ok())
        .filter(|version| (version.server_major, version.server_minor) >= (1, 2))
        .and_then(|_| {
            x.connection
                .res_query_client_ids(&[ClientIdSpec {
                    client: owner,
                    mask: ClientIdMask::LOCAL_CLIENT_PID,
                }])
                .ok()?
                .reply()
                .ok()
        })
        .and_then(|reply| {
            reply.ids.into_iter().find_map(|value| {
                (u32::from(value.spec.mask) & u32::from(ClientIdMask::LOCAL_CLIENT_PID) != 0)
                    .then(|| value.value.first().copied())
                    .flatten()
            })
        });
    if let Some(pid) = pid {
        if pid == 0 || pid == std::process::id() {
            return None;
        }
        let executable = std::fs::read_link(format!("/proc/{pid}/exe")).ok()?;
        let name = executable.file_stem()?.to_string_lossy().into_owned();
        return Some(SourceApp {
            name,
            executable: executable.to_str().map(str::to_owned),
        });
    }
    source(x, owner)
}

pub fn running_apps(icons_dir: &Path) -> Vec<RunningApp> {
    let Ok(x) = X11::connect() else {
        return Vec::new();
    };
    let Ok(windows) = x.property(x.root, "_NET_CLIENT_LIST") else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    let mut apps = Vec::new();
    for window in windows {
        let Some(source) = source(&x, window) else {
            continue;
        };
        let Some(process) = source.executable else {
            continue;
        };
        if seen.insert(process.clone()) {
            apps.push(RunningApp {
                name: source.name,
                icon: cache_icon(&x, window, &process, icons_dir),
                process,
            });
        }
    }
    apps.sort_by(|left, right| left.name.cmp(&right.name));
    apps
}

pub fn extract_and_cache_icon(executable: &str, icons_dir: &Path) -> Option<String> {
    let x = X11::connect().ok()?;
    x.property(x.root, "_NET_CLIENT_LIST")
        .ok()?
        .into_iter()
        .find(|window| self::executable(&x, *window).as_deref() == Some(executable))
        .and_then(|window| cache_icon(&x, window, executable, icons_dir))
}

fn cache_icon(x: &X11, window: u32, executable: &str, directory: &Path) -> Option<String> {
    let hash = clipboard_core::backup::hash_token(executable.as_bytes());
    let path = directory.join(format!("{hash}.png"));
    if path.is_file() {
        return Some(path.to_string_lossy().into_owned());
    }
    let pixels = x.property(window, "_NET_WM_ICON").ok()?;
    let mut offset = 0;
    let mut best = None;
    while offset + 2 <= pixels.len() {
        let width = pixels[offset];
        let height = pixels[offset + 1];
        let count = width.checked_mul(height)? as usize;
        offset += 2;
        let end = offset.checked_add(count)?;
        let data = pixels.get(offset..end)?;
        if (1..=256).contains(&width)
            && (1..=256).contains(&height)
            && best.as_ref().is_none_or(|(w, h, _)| width * height > w * h)
        {
            best = Some((width, height, data));
        }
        offset = end;
    }
    let (width, height, pixels) = best?;
    let bytes: Vec<u8> = pixels
        .iter()
        .flat_map(|argb| {
            [
                (*argb >> 16) as u8,
                (*argb >> 8) as u8,
                *argb as u8,
                (*argb >> 24) as u8,
            ]
        })
        .collect();
    let image = image::RgbaImage::from_raw(width, height, bytes)?;
    std::fs::create_dir_all(directory).ok()?;
    image.save(&path).ok()?;
    Some(path.to_string_lossy().into_owned())
}
