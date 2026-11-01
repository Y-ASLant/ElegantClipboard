use super::*;

fn executable(version: &str) -> PathBuf {
    std::env::temp_dir()
        .join(version)
        .join("elegant-clipboard-gpui")
}

#[test]
fn duplicate_exec_or_unexpanded_field_code_is_not_an_owned_entry() {
    let temporary = tempfile::tempdir().unwrap();
    let data_dir = temporary.path().join("data");
    let content = entry(&executable("old"), &data_dir).unwrap();
    assert!(desktop::parse(&(content.clone() + "Exec=other\n"), &data_dir).is_none());
    assert!(
        desktop::parse(
            &content.replace(" --start-hidden", " %u --start-hidden"),
            &data_dir
        )
        .is_none()
    );
}
