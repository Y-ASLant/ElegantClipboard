//! 系统字体枚举与偏好解析（对齐旧版 main 分支的继承关系）。

use clipboard_core::preferences::FontPreference;
use fontdb::{Database, ID};
use gpui_kit::SharedString;
use std::sync::LazyLock;

#[cfg(windows)]
pub const DEFAULT_UI_FONT_FAMILY: &str = "Microsoft YaHei UI";
#[cfg(not(windows))]
pub const DEFAULT_UI_FONT_FAMILY: &str = ".SystemUIFont";

static SYSTEM_FONT_FAMILIES: LazyLock<Vec<SharedString>> = LazyLock::new(load_system_font_families);

pub fn system_font_families() -> &'static [SharedString] {
    SYSTEM_FONT_FAMILIES.as_slice()
}

pub fn resolve_ui_family(preference: &FontPreference) -> SharedString {
    family_or_default(&preference.ui_font, DEFAULT_UI_FONT_FAMILY)
}

pub fn resolve_card_family(preference: &FontPreference) -> SharedString {
    if preference.card_font.is_empty() {
        return resolve_ui_family(preference);
    }
    family_or_default(&preference.card_font, DEFAULT_UI_FONT_FAMILY)
}

pub fn resolve_preview_family(preference: &FontPreference) -> SharedString {
    if preference.preview_font.is_empty() {
        return resolve_card_family(preference);
    }
    family_or_default(&preference.preview_font, DEFAULT_UI_FONT_FAMILY)
}

fn family_or_default(custom: &str, fallback: &str) -> SharedString {
    let trimmed = custom.trim();
    if trimmed.is_empty() {
        fallback.into()
    } else {
        trimmed.into()
    }
}

fn load_system_font_families() -> Vec<SharedString> {
    let mut database = Database::new();
    database.load_system_fonts();
    let mut families = database
        .faces()
        .flat_map(|face| face.families.iter().map(|(name, _)| name.as_str()))
        .filter(|name| !is_decorative_font(name))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    families.sort_by_key(|name| name.to_ascii_lowercase());
    families.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    families.into_iter().map(SharedString::from).collect()
}

fn is_decorative_font(name: &str) -> bool {
    const SKIP_PREFIXES: &[&str] = &[
        "Webdings",
        "Wingdings",
        "MT Extra",
        "Symbol",
        "Bookshelf Symbol",
        "MS Outlook",
        "High Tower Text",
        "Pristina",
        "Jokerman",
        "Vivaldi",
        "Kristen IT",
        "French Script",
        "Playbill",
        "Mistral",
        "Papyrus",
    ];
    SKIP_PREFIXES
        .iter()
        .any(|prefix| name.eq_ignore_ascii_case(prefix) || name.starts_with(prefix))
}

#[allow(dead_code)]
pub fn has_system_family(name: &str) -> bool {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return true;
    }
    let mut database = Database::new();
    database.load_system_fonts();
    database
        .faces()
        .any(|face| face_has_family(face.id, &database, trimmed))
}

fn face_has_family(face: ID, database: &Database, family: &str) -> bool {
    database
        .face(face)
        .into_iter()
        .flat_map(|face| face.families.iter())
        .any(|(name, _)| name.eq_ignore_ascii_case(family))
}
