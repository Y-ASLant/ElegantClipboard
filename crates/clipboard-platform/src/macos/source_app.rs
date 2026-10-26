//! Best-effort attribution: NSPasteboard does not expose its owner, so use NSWorkspace's frontmost app.
use super::cocoa;
use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
use objc2_foundation::{NSData, NSString};
use std::{collections::HashSet, io::Write, path::Path};

pub use crate::source_app_types::{RunningApp, SourceApp};

fn source(application: &AnyObject) -> Option<SourceApp> {
    let pid: i32 = unsafe { msg_send![application, processIdentifier] };
    if pid <= 0 || pid as u32 == std::process::id() {
        return None;
    }
    let name: Option<Retained<NSString>> = unsafe { msg_send![application, localizedName] };
    let executable: Option<Retained<AnyObject>> = unsafe { msg_send![application, executableURL] };
    let executable = executable.and_then(|url| {
        let path: Option<Retained<NSString>> = unsafe { msg_send![&url, path] };
        path.map(|path| path.to_string())
    });
    Some(SourceApp {
        name: name?.to_string(),
        executable,
    })
}

pub fn clipboard_source() -> Option<SourceApp> {
    objc2::rc::autoreleasepool(|_| {
        let application: Option<Retained<AnyObject>> =
            unsafe { msg_send![&cocoa::workspace(), frontmostApplication] };
        let application = application?;
        source(&application)
    })
}

pub fn running_apps(icons_dir: &Path) -> Vec<RunningApp> {
    objc2::rc::autoreleasepool(|_| {
        let applications: Retained<AnyObject> =
            unsafe { msg_send![&cocoa::workspace(), runningApplications] };
        let count: usize = unsafe { msg_send![&applications, count] };
        let mut seen = HashSet::new();
        let mut result = Vec::new();
        for index in 0..count {
            let application: Retained<AnyObject> =
                unsafe { msg_send![&applications, objectAtIndex: index] };
            let policy: isize = unsafe { msg_send![&application, activationPolicy] };
            if policy != 0 {
                continue;
            }
            let Some(source) = source(&application) else {
                continue;
            };
            let Some(executable) = source.executable else {
                continue;
            };
            let Some(process) = Path::new(&executable)
                .file_name()
                .and_then(|name| name.to_str())
            else {
                continue;
            };
            let process = process.to_owned();
            if !seen.insert(process.clone()) {
                continue;
            }
            result.push(RunningApp {
                name: source.name,
                process,
                icon: extract_and_cache_icon(&executable, icons_dir),
            });
        }
        result.sort_by(|left, right| left.name.cmp(&right.name));
        result
    })
}

pub fn extract_and_cache_icon(executable: &str, icons_dir: &Path) -> Option<String> {
    let hash = clipboard_core::backup::hash_token(executable.as_bytes());
    let path = icons_dir.join(format!("{}.png", &hash[..12]));
    if path.is_file() {
        return Some(path.to_string_lossy().into_owned());
    }
    objc2::rc::autoreleasepool(|_| {
        // Workspace resolves an app bundle's icon, not the generic Mach-O executable icon.
        let bundle = Path::new(executable)
            .ancestors()
            .find(|path| path.extension().is_some_and(|extension| extension == "app"))
            .unwrap_or_else(|| Path::new(executable));
        let file = NSString::from_str(bundle.to_str()?);
        let image: Option<Retained<AnyObject>> =
            unsafe { msg_send![&cocoa::workspace(), iconForFile: &*file] };
        let tiff: Option<Retained<NSData>> = unsafe { msg_send![&image?, TIFFRepresentation] };
        let bitmap: Option<Retained<AnyObject>> =
            unsafe { msg_send![class!(NSBitmapImageRep), imageRepWithData: &*tiff?] };
        let properties: Retained<AnyObject> =
            unsafe { msg_send![class!(NSDictionary), dictionary] };
        let png: Option<Retained<NSData>> = unsafe {
            msg_send![&bitmap?, representationUsingType: 4usize, properties: &*properties]
        };
        let png = png?;
        std::fs::create_dir_all(icons_dir).ok()?;
        let mut temporary = tempfile::NamedTempFile::new_in(icons_dir).ok()?;
        // The freshly encoded immutable NSData is retained locally and never mutated.
        temporary
            .write_all(unsafe { png.as_bytes_unchecked() })
            .ok()?;
        match temporary.persist_noclobber(&path) {
            Ok(_) => Some(path.to_string_lossy().into_owned()),
            Err(_) if path.is_file() => Some(path.to_string_lossy().into_owned()),
            Err(_) => None,
        }
    })
}
