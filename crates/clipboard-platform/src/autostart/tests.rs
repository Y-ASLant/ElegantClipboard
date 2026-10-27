use super::*;

fn value(exe: &Path, data_dir: &Path) -> registry::Value {
    registry::Value {
        kind: REG_SZ,
        bytes: to_wide_bytes(&command(exe, data_dir)),
    }
}

#[test]
fn previous_executable_and_verbatim_data_path_still_identify_owned_startup() {
    let old = Path::new(r"C:\Users\用户\AppData\Roaming\ElegantClipboard-GPUI");
    let saved = value(Path::new(r"D:\old release\elegant-clipboard-gpui.exe"), old);
    assert!(belongs_to(
        &saved,
        Path::new(r"\\?\C:\Users\用户\AppData\Roaming\ElegantClipboard-GPUI")
    ));
    assert!(!belongs_to(
        &saved,
        Path::new(r"C:\Users\用户\AppData\Roaming\ElegantClipboard")
    ));
    let other = value(Path::new(r"D:\another-app.exe"), old);
    assert!(!belongs_to(&other, old));
}

#[test]
fn startup_identity_rejects_other_arguments_and_corrupt_values() {
    let data_dir = Path::new(r"D:\history");
    let mut saved = value(Path::new(r"D:\elegant-clipboard-gpui.exe"), data_dir);
    saved.bytes = to_wide_bytes(&format!("{} --reset", saved.string().unwrap()));
    assert!(!belongs_to(&saved, data_dir));
    saved.bytes = to_wide_bytes(
        "\"D:\\elegant-clipboard-gpui.exe\" --start-hidden --data-dir \"D:\\history\"\0garbage",
    );
    assert!(!belongs_to(&saved, data_dir));
    saved.bytes.pop();
    assert!(!belongs_to(&saved, data_dir));
}

#[test]
fn missing_old_directory_can_still_resolve_both_compatible_registry_names() {
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("removed GPUI profile");
    let names = names(&missing).unwrap();
    let lower_case = Path::new(&missing.to_string_lossy().to_lowercase()).to_path_buf();
    assert_eq!(names, super::names(&lower_case).unwrap());
    assert_eq!(names[0].rsplit('-').next(), names[1].rsplit('-').next());
    assert_ne!(names[0], names[1]);
    assert!(!missing.exists());
}
