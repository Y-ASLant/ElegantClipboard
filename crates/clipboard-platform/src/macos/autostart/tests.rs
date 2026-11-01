use super::*;

fn executable(version: &str) -> PathBuf {
    std::env::temp_dir()
        .join(version)
        .join("elegant-clipboard-gpui")
}

#[test]
fn duplicate_arguments_or_misleading_nested_keys_are_not_owned_entries() {
    let temporary = tempfile::tempdir().unwrap();
    let data_dir = temporary.path().join("data");
    let label = label(&data_dir).unwrap();
    let content = entry(&executable("old"), &data_dir).unwrap();
    let duplicate = content.replace("</dict>", "<key>ProgramArguments</key><array/></dict>");
    assert!(plist::parse(&duplicate, &label, &data_dir).is_none());
    let nested = content
        .replace(
            "<key>ProgramArguments</key>",
            "<key>Other</key><dict><key>ProgramArguments</key>",
        )
        .replace("</array>", "</array></dict>");
    assert!(plist::parse(&nested, &label, &data_dir).is_none());
}
