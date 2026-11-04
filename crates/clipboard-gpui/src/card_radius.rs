use clipboard_core::preferences::CardCornerPreference;
use gpui_kit::{App, Global, Pixels, component::ActiveTheme, px};

#[derive(Clone, Copy)]
struct CardCorner(CardCornerPreference);
impl Global for CardCorner {}

pub fn set_card_corner(preference: CardCornerPreference, cx: &mut App) {
    cx.set_global(CardCorner(preference));
}

/// 卡片和预览使用的圆角。默认是当前全局圆角的一半。
pub fn card_radius(cx: &App) -> Pixels {
    let global = cx.theme().radius_lg;
    match cx
        .try_global::<CardCorner>()
        .map_or(CardCornerPreference::default(), |value| value.0)
    {
        CardCornerPreference::Half => global * 0.5,
        CardCornerPreference::Global => global,
        CardCornerPreference::Off => px(0.),
    }
}
