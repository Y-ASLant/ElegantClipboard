//! Chakra 风格的原生主题：语义颜色与动效统一投影到 GPUI Kit。

use clipboard_core::preferences::{CornerRadiusPreference, WindowEffect};
use gpui_kit::{
    App, Global, Hsla, Pixels,
    base::{Easing, Spring},
    component::{ActiveTheme, Theme, ThemeColor},
    px, rgb,
};
use std::time::Duration;

#[cfg(windows)]
pub const UI_FONT_FAMILY: &str = "Microsoft YaHei UI";
#[cfg(not(windows))]
pub const UI_FONT_FAMILY: &str = ".SystemUIFont";

#[derive(Clone, Copy)]
struct CornerRadius(CornerRadiusPreference);
impl Global for CornerRadius {}

#[derive(Clone, Copy)]
struct ThemeCornerDefaults {
    radius: Pixels,
    large: Pixels,
}
impl Global for ThemeCornerDefaults {}

pub fn set_corner_radius(preference: CornerRadiusPreference, cx: &mut App) {
    cx.set_global(CornerRadius(preference));
    apply(cx);
}
pub const FAST: Duration = Duration::from_millis(150);
pub const NORMAL: Duration = Duration::from_millis(200);
pub const SLOW: Duration = Duration::from_millis(300);
pub const PREVIEW_REVEAL: Duration = Duration::from_millis(190);

/// 在明暗主题切换后应用；Theme::update 同步控件、语义 token 与底层绘制。
pub fn apply(cx: &mut App) {
    #[cfg(windows)]
    match crate::native::input::client_area_animations_enabled() {
        Ok(enabled) => cx.set_reduce_motion(!enabled),
        Err(error) => tracing::warn!(%error, "failed to read system animation preference"),
    }
    let dark = cx.theme().is_dark();
    // Capture Kit's unmodified defaults once, before applying any saved preference.
    // Later System selections must not reuse the last user-selected pixel radius.
    if !cx.has_global::<ThemeCornerDefaults>() {
        cx.set_global(ThemeCornerDefaults {
            radius: cx.theme().radius,
            large: cx.theme().radius_lg,
        });
    }
    let defaults = *cx.global::<ThemeCornerDefaults>();
    let configuration = if dark {
        &cx.theme().dark_theme
    } else {
        &cx.theme().light_theme
    };
    let (radius, large) = match cx
        .try_global::<CornerRadius>()
        .map_or_else(CornerRadiusPreference::default, |value| value.0)
    {
        CornerRadiusPreference::System => (
            configuration
                .radius
                .map_or(defaults.radius, |value| px(value as f32)),
            configuration
                .radius_lg
                .map_or(defaults.large, |value| px(value as f32)),
        ),
        CornerRadiusPreference::Fixed(value) => (px(f32::from(value)), px(f32::from(value))),
    };
    let material = crate::window_effect::active(cx);
    Theme::update(cx, |theme| {
        apply_colors(&mut theme.colors, dark, material.effect);
        // Kit's segmented selection always paints a raised shadow beneath its
        // background token. Keep that small elevated fill solid so its own
        // shadow cannot bleed through; inputs still use the translucent color.
        theme.tokens.background = theme.colors.background.into();
        if material.effect != WindowEffect::Opaque {
            theme.tokens.background.background =
                color(if dark { 0x414752 } else { 0xf2f3f5 }).into();
        }
        theme.font_family = UI_FONT_FAMILY.into();
        theme.font_size = px(16.);
        theme.radius = radius;
        theme.radius_lg = large;
        theme.shadow = false;
        theme.focus_ring = false;
        theme.motion.duration_fast = FAST;
        theme.motion.duration_normal = NORMAL;
        theme.motion.duration_slow = SLOW;
        theme.motion.easing_enter =
            Easing::cubic_bezier(0., 0., 0.58, 1.).expect("valid Chakra ease-out curve");
        theme.motion.easing_exit =
            Easing::cubic_bezier(0.42, 0., 1., 1.).expect("valid Chakra ease-in curve");
        theme.motion.easing_move =
            Easing::cubic_bezier(0.42, 0., 0.58, 1.).expect("valid Chakra ease-in-out curve");
        theme.motion.spring_control = Spring::new(NORMAL);
        theme.motion.spring_move = Spring::new(SLOW).with_damping(1.).with_epsilon(0.1);
    });
    apply_scrollbar(cx);
}

fn apply_scrollbar(cx: &mut App) {
    // Refine the official Base scrollbar after Kit projects its colors and
    // system visibility mode. The 12px hit rail fits the history card gutter.
    let theme = gpui_kit::base::Theme::global_mut(cx);
    let scrollbar = std::mem::take(&mut theme.scrollbar);
    let rail_width = px(crate::visual::SCROLLBAR_RAIL_WIDTH);
    let styles = scrollbar
        .styles()
        .clone()
        .track(|style| style.width(rail_width))
        .track_hover(|style| style.width(rail_width))
        .track_active(|style| {
            style
                .width(rail_width)
                .border_color(gpui_kit::transparent_black())
        })
        .thumb(|style| {
            style
                .width(px(3.))
                .inset(px(2.))
                .radius(px(4.))
                .min_length(px(24.))
        })
        .thumb_hover(|style| style.width(px(6.)).inset(px(2.)).radius(px(4.)))
        .thumb_active(|style| style.width(px(8.)).inset(px(2.)).radius(px(4.)));
    theme.scrollbar = scrollbar.with_styles(styles).with_motion(
        gpui_kit::base::ScrollbarMotion::default()
            .with_idle(Duration::from_millis(900))
            .with_enter(FAST)
            .with_exit(NORMAL)
            .with_expand(Duration::from_millis(180)),
    );
}

fn color(value: u32) -> Hsla {
    rgb(value).into()
}

/// The canvas is separate from the opaque surface used by menus and dialogs.
pub fn canvas_background(cx: &App) -> Hsla {
    crate::window_effect::shell_background(
        color(if cx.theme().is_dark() {
            0x15171b
        } else {
            0xffffff
        }),
        crate::window_effect::active(cx),
    )
}

pub fn container_border(cx: &App) -> Hsla {
    if crate::window_effect::active(cx).effect == WindowEffect::Opaque {
        cx.theme().border
    } else {
        cx.theme().border.opacity(0.)
    }
}

fn apply_colors(colors: &mut ThemeColor, dark: bool, effect: WindowEffect) {
    let surface = color(if dark { 0x22252b } else { 0xffffff });
    let foreground = color(if dark { 0xf1f3f5 } else { 0x18181b });
    let subtle = color(if dark { 0x1b1e23 } else { 0xfafafa });
    let muted = color(if dark { 0x2d3138 } else { 0xf4f4f5 });
    let border = color(if dark { 0x3a4049 } else { 0xe4e4e7 });
    let emphasized = color(if dark { 0x515b69 } else { 0xd4d4d8 });
    let secondary_text = color(if dark { 0xb5becb } else { 0x52525b });
    let has_material = effect != WindowEffect::Opaque;
    // Containers expose the single shell tint. Control contrast is independent
    // of shell opacity: using the remaining alpha made 85% controls disappear.
    // Detached overlays keep a separate opaque surface.
    let content = if has_material {
        surface.opacity(0.)
    } else {
        surface
    };
    let control_strength = 0.60;
    let tint = |opaque: Hsla, ink: Hsla, coverage: f32| {
        if has_material {
            ink.opacity(control_strength * coverage)
        } else {
            opaque
        }
    };
    let ink = color(if dark { 0xffffff } else { 0x000000 });
    let control = tint(surface, surface, 0.20);
    let control_muted = tint(muted, ink, 0.16);
    let control_hover = tint(muted, ink, 0.28);
    let control_pressed = tint(border, ink, 0.42);
    let control_selected = tint(surface, color(0xffffff), 0.55);
    let content_hover = tint(subtle, ink, 0.28);
    let content_border = if has_material {
        border.opacity(0.)
    } else {
        border
    };
    // `colors.border` drives both the canvas-shared separators (sidebar edge,
    // page-header rule, window frame, resize handle) and control outlines.
    // Solid paint makes those separators read as hard bars cut across the
    // system backdrop. Dropping alpha keeps them tonal while control outlines
    // stay legible; controls needing a hard edge use `emphasized`/`input`.
    let hairline = if has_material {
        border.opacity(0.55)
    } else {
        border
    };
    let primary = color(0x2563eb);
    let primary_text = color(if dark { 0x60a5fa } else { 0x2563eb });
    let blue_subtle = color(if dark { 0x14204a } else { 0xeff6ff });
    let white = color(0xffffff);
    let selection_surface = tint(blue_subtle, primary, 0.45);

    // Kit inputs (including Settings' internal search) use this surface.
    // Dialogs, menus, notifications and passive previews use popover instead.
    colors.background = control;
    colors.foreground = foreground;
    colors.border = hairline;
    colors.input = if has_material && dark {
        // Kit derives dark input fill from 70% of its input-border color.
        // Keep its fill and outline stable when only shell opacity changes.
        foreground.opacity(control_strength * 0.40)
    } else {
        emphasized
    };
    colors.accent = selection_surface;
    colors.accent_foreground = primary_text;
    colors.primary = primary;
    colors.primary_foreground = white;
    colors.primary_hover = color(0x1d4ed8);
    colors.primary_active = color(0x173da6);
    colors.button_primary = colors.primary;
    colors.button_primary_foreground = white;
    colors.button_primary_hover = colors.primary_hover;
    colors.button_primary_active = colors.primary_active;
    colors.secondary = control_muted;
    colors.secondary_foreground = foreground;
    colors.secondary_hover = tint(border, ink, 0.28);
    colors.secondary_active = tint(emphasized, ink, 0.42);
    colors.button_secondary = colors.secondary;
    colors.button_secondary_foreground = foreground;
    colors.button_secondary_hover = colors.secondary_hover;
    colors.button_secondary_active = colors.secondary_active;
    colors.button = control;
    colors.button_foreground = foreground;
    colors.button_hover = control_hover;
    colors.button_active = control_pressed;
    colors.muted = control_muted;
    colors.muted_foreground = secondary_text;
    colors.ring = color(0x3b82f6);
    colors.caret = primary_text;
    colors.selection = color(0x3b82f6).opacity(if dark { 0.35 } else { 0.2 });
    colors.group_box = content;
    colors.group_box_foreground = foreground;
    colors.accordion = content;
    colors.popover = surface;
    colors.popover_foreground = foreground;
    colors.sidebar = if has_material {
        surface.opacity(0.)
    } else {
        subtle
    };
    colors.sidebar_foreground = secondary_text;
    colors.sidebar_border = content_border;
    colors.sidebar_accent = selection_surface;
    colors.sidebar_accent_foreground = primary_text;
    colors.sidebar_primary = primary;
    colors.sidebar_primary_foreground = white;
    colors.tab = control;
    colors.tab_foreground = secondary_text;
    colors.tab_active = control_selected;
    colors.tab_active_foreground = primary_text;
    // Disabled segmented tabs use this token for their outer background.
    // Keep it clear so they do not cover the shared track and selection indicator.
    colors.tab_bar = surface.opacity(0.);
    colors.tab_bar_segmented = control_muted;
    colors.list = content;
    colors.list_head = tint(subtle, ink, 0.16);
    colors.list_even = content;
    colors.list_hover = content_hover;
    colors.list_active = selection_surface;
    colors.list_active_border = primary;
    colors.table = content;
    colors.table_head = tint(subtle, ink, 0.16);
    colors.table_head_foreground = secondary_text;
    colors.table_even = tint(subtle, ink, 0.08);
    colors.table_hover = control_hover;
    colors.table_active = selection_surface;
    colors.table_active_border = primary;
    colors.table_row_border = content_border;
    colors.table_foot = tint(subtle, ink, 0.16);
    colors.table_foot_foreground = secondary_text;
    colors.title_bar = if has_material {
        surface.opacity(0.)
    } else {
        subtle
    };
    colors.title_bar_border = content_border;
    colors.status_bar = if has_material {
        surface.opacity(0.)
    } else {
        subtle
    };
    colors.status_bar_border = content_border;
    colors.scrollbar = surface.opacity(0.);
    colors.scrollbar_thumb = secondary_text.opacity(0.45);
    colors.scrollbar_thumb_hover = secondary_text;
    colors.switch = tint(emphasized, ink, 0.55);
    colors.switch_thumb = white;
    colors.slider_bar = control_muted;
    colors.slider_thumb = primary;
    colors.progress_bar = primary;
    colors.drag_border = primary;
    colors.drop_target = selection_surface;
    colors.link = primary_text;
    colors.link_hover = color(if dark { 0xa3cfff } else { 0x173da6 });
    colors.link_active = primary_text;
    colors.description_list_label = tint(subtle, ink, 0.16);
    colors.description_list_label_foreground = secondary_text;
    colors.skeleton = control_muted;
    colors.overlay = color(0x09090b).opacity(if dark { 0.7 } else { 0.45 });
    colors.window_border = border;

    colors.danger = color(if dark { 0xf87171 } else { 0xdc2626 });
    colors.danger_foreground = white;
    colors.danger_hover = color(0xb91c1c);
    colors.danger_active = color(0x991b1b);
    colors.button_danger = color(0xdc2626);
    colors.button_danger_foreground = white;
    colors.button_danger_hover = colors.danger_hover;
    colors.button_danger_active = colors.danger_active;
    colors.success = color(if dark { 0x4ade80 } else { 0x16a34a });
    colors.success_foreground = white;
    colors.success_hover = color(0x15803d);
    colors.success_active = color(0x166534);
    colors.button_success = color(0x16a34a);
    colors.button_success_foreground = white;
    colors.button_success_hover = colors.success_hover;
    colors.button_success_active = colors.success_active;
    colors.warning = color(if dark { 0xfbbf24 } else { 0xb45309 });
    colors.warning_foreground = white;
    colors.warning_hover = color(0x92400e);
    colors.warning_active = color(0x78350f);
    colors.button_warning = color(0xb45309);
    colors.button_warning_foreground = white;
    colors.button_warning_hover = colors.warning_hover;
    colors.button_warning_active = colors.warning_active;
    colors.info = primary_text;
    colors.info_foreground = white;
    colors.info_hover = colors.primary_hover;
    colors.info_active = colors.primary_active;
    colors.button_info = primary;
    colors.button_info_foreground = white;
    colors.button_info_hover = colors.primary_hover;
    colors.button_info_active = colors.primary_active;
    colors.blue = primary_text;
    colors.blue_light = selection_surface;
    colors.red = colors.danger;
    colors.red_light = tint(
        color(if dark { 0x300c0c } else { 0xfef2f2 }),
        colors.danger,
        0.35,
    );
    colors.green = colors.success;
    colors.green_light = tint(
        color(if dark { 0x052e16 } else { 0xf0fdf4 }),
        colors.success,
        0.35,
    );
    colors.yellow = colors.warning;
    colors.yellow_light = tint(
        color(if dark { 0x2d1800 } else { 0xfffbeb }),
        colors.warning,
        0.35,
    );
}
