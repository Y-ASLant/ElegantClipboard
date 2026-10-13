//! Windows 系统背板：亚克力（Acrylic）与云母（Mica）。
//!
//! 让背板真正透出来需要四件事同时成立，缺任何一件都表现为「设置了却没效果」：
//!
//! 1. **去掉系统 caption**。带 `WS_CAPTION`（`WS_BORDER | WS_DLGFRAME`）时 DWM 把
//!    系统背板只画在标题栏那一条上，客户区始终不透。GPUI 的 `appears_transparent`
//!    只影响它自己的 `WM_NCCALCSIZE`/`WM_NCHITTEST`，并不会改窗口样式。
//! 2. **设置 DWM 属性**。`DWMWA_SYSTEMBACKDROP_TYPE` 选背板种类，
//!    `DWMWA_USE_HOSTBACKDROPBRUSH` 是 Win11 22H2+ 让背板铺进客户区的开关。
//! 3. **让客户区透出**。`Window::set_background_appearance` 非 `Opaque` 时 GPUI 才把
//!    交换链清成透明色。
//! 4. **根 div 不能有不透明背景**。见 [`AcrylicRootPlugin`]——这是最隐蔽的一条。
//!
//! 材质只作用于窗口外壳，不修改组件主题。控件与浮层保留原有底色，
//! 避免半透明表面透出自身阴影；装饰性容器可局部透明以露出同一层外壳。
//!
//! 生效值通过 Global 广播：三处窗口（主窗口、设置窗口、悬停预览）在创建时套用，
//! 偏好保存成功后由全局刷新推送给所有已打开窗口。

use clipboard_core::preferences::{WindowEffect, WindowEffectPreference};
use clipboard_platform::window_effect::{WindowEffectBackend, WindowEffectCapabilities};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DWM_SYSTEMBACKDROP_TYPE, DWM_WINDOW_CORNER_PREFERENCE, DWMSBT_MAINWINDOW, DWMSBT_NONE,
    DWMSBT_TABBEDWINDOW, DWMSBT_TRANSIENTWINDOW, DWMWA_BORDER_COLOR, DWMWA_COLOR_DEFAULT,
    DWMWA_COLOR_NONE, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_HOSTBACKDROPBRUSH,
    DWMWA_VISIBLE_FRAME_BORDER_THICKNESS, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DEFAULT,
    DWMWCP_ROUND, DWMWINDOWATTRIBUTE, DwmSetWindowAttribute,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_STYLE, GetClientRect, GetWindowLongW, PostMessageW, SIZE_RESTORED, SWP_FRAMECHANGED,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetWindowLongW, SetWindowPos,
    WINDOW_STYLE, WM_SIZE, WS_BORDER, WS_DLGFRAME,
};
use windows::core::BOOL;

use gpui_kit::base::{Root, RootPlugin};
use gpui_kit::{
    App, Context, Div, Global, Hsla, IntoElement, Render, Stateful, StyleRefinement, Window,
    WindowBackgroundAppearance, div, prelude::*, transparent_black,
};

mod frameless;

use frameless::set_frameless;

#[derive(Clone, Copy, Default)]
pub(crate) struct WindowEffectSetting(pub(crate) WindowEffectPreference);

impl Global for WindowEffectSetting {}

/// 只广播本机实际生效的效果；请求值继续保存在视图和 SQLite 中。
pub(crate) fn record(preference: WindowEffectPreference, cx: &mut App) {
    let effect = WindowEffectCapabilities::current()
        .resolve(preference.effect)
        .effect();
    cx.set_global(WindowEffectSetting(WindowEffectPreference {
        effect,
        ..preference
    }));
}

pub(crate) fn active(cx: &App) -> WindowEffectPreference {
    cx.try_global::<WindowEffectSetting>()
        .map_or_else(WindowEffectPreference::default, |setting| setting.0)
}

/// 在应用初始化之后注册，晚于 gpui-component 的 `WindowState`。
///
/// GPUI 的直接混合状态是 `SrcBlendAlpha = ONE, DestBlendAlpha = ONE`，即 **alpha
/// 加法累积**：任何一层不透明 quad 一旦落笔，整个客户区的 alpha 就被钉死成 1，
/// 之后无论叠多少半透明层都救不回来。gpui-component 的 `WindowState::style` 恰恰
/// 给根 div 刷了一层不透明 `theme().tokens.background`，表现就是「边框透出背板、
/// 内容区纯黑」——DComp 只看到 alpha=1。
///
/// 根插件按注册顺序依次 `refine`，后注册者覆盖先注册者，所以这里在效果开启时把根背景
/// 改写为**全透明**：GPUI 的 `paint_quad` 会直接跳过透明背景，alpha 不再被钉死。
struct AcrylicRootPlugin;

impl Render for AcrylicRootPlugin {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

impl RootPlugin for AcrylicRootPlugin {
    fn style(&self, surface: &mut Stateful<Div>, _: &mut Window, cx: &mut App) {
        if active(cx).effect == WindowEffect::None {
            return;
        }
        surface
            .style()
            .refine(&StyleRefinement::default().bg(transparent_black()));
    }
}

/// 注册根插件。必须在 `gpui_kit::init` 之后调用才有覆盖 `WindowState` 的资格，
/// 且只影响之后创建的窗口。
pub(crate) fn register_root_plugin(cx: &mut App) {
    Root::register_plugin::<AcrylicRootPlugin>(cx, |_, _| AcrylicRootPlugin);
}

/// 首次开窗或有效材质变化时调用；遮罩强度不参与原生窗口配置。
pub(crate) fn apply(effect: WindowEffect, window: &mut Window) {
    let capabilities = WindowEffectCapabilities::current();
    let backend = capabilities.resolve(effect);
    window.set_background_appearance(appearance(backend));
    let Some(hwnd) = window_handle(window) else {
        return;
    };
    let enabled = backend != WindowEffectBackend::Opaque;
    set_window_style(hwnd, enabled);
    if capabilities.supports_system_backdrop() {
        set_dwm_attributes(hwnd, backend.effect());
    }
    // 无边框要放在最后：它靠 SWP_FRAMECHANGED 让新客户区生效，
    // 且依赖前面已经装好的窗口样式。
    set_frameless(enabled, hwnd);
    // 子类装好之后**必须再刷新一次**非客户区。`set_window_style` 里那次刷新发生在子类
    // 安装之前，那时客户区还是「窗口减边框」的旧尺寸；只有子类就位后重算，
    // `WM_NCCALCSIZE` 才会把客户区还给整个窗口。少了这一步，第一次开启效果时
    // 客户区尺寸是错的（右下留灰块），而手动拖一下窗口又会顺带修好它——
    // 这就是「只有第一次出现、拖过之后再也不复现」的原因。
    refresh_frame(hwnd);
    // 上面改过窗口样式/边框，客户区尺寸可能跟着变了，必须补一条 WM_SIZE 告诉 GPUI，
    // 否则它会一直用旧尺寸布局。
    notify_client_size(hwnd);
}

/// 让 GPUI 按**当前**客户区尺寸重新布局。
///
/// GPUI 只在收到 `WM_SIZE` 时更新布局尺寸——`gpui-fast-windows` 的 `handle_size_msg`
/// 直接取 `lparam` 里的宽高，并不会自己去 `GetClientRect`。而套用窗口效果会改窗口样式
/// 与 DWM 背板属性（去掉 `WS_BORDER`/`WS_DLGFRAME`、加系统背板与圆角），客户区随之变大；
/// 不补这条消息，内容就停在旧尺寸，右/下露出一条**没被绘制**的区域——背板从那里透出来，
/// 看起来就是窗口右下多了一条灰色带（悬停预览窗最明显）。手动拖一下窗口会恢复正常，
/// 因为拖动会真的产生 `WM_SIZE`。
///
/// **必须用 `PostMessageW`，不能用 `SendMessageW`**：`SendMessageW` 是同步派发，而所有
/// 调用点都在「正在更新某个窗口」的过程中（开窗 builder、偏好保存回调）。`gpui-fast`
/// 处理 `WM_SIZE` 时是通过 `handle.update` 回调把新尺寸写回窗口的（`Window::bounds_changed`
/// 里更新 `viewport_size`），而窗口此刻正被借用，那个回调只会静默失败（源码里是
/// `.log_err()`），尺寸照旧不变。投递到消息队列则会等本次窗口更新结束后再派发，
/// 回调能成功执行——这正是「拖一下窗口就好」所走的路径。
fn notify_client_size(hwnd: HWND) {
    let mut rect = RECT::default();
    if unsafe { GetClientRect(hwnd, &mut rect) }.is_err() {
        return;
    }
    let width = (rect.right - rect.left).max(1) as u32 & 0xFFFF;
    let height = (rect.bottom - rect.top).max(1) as u32 & 0xFFFF;
    // WM_SIZE 的 lparam 低位是宽、高位是高，都按 16 位打包。
    unsafe {
        let _ = PostMessageW(
            Some(hwnd),
            WM_SIZE,
            WPARAM(SIZE_RESTORED as usize),
            LPARAM(((height << 16) | width) as isize),
        );
    }
}

fn appearance(backend: WindowEffectBackend) -> WindowBackgroundAppearance {
    match backend {
        WindowEffectBackend::Opaque => WindowBackgroundAppearance::Opaque,
        WindowEffectBackend::LegacyAcrylic => WindowBackgroundAppearance::Blurred,
        // Clear any WCA accent before installing the native DWM Acrylic backdrop.
        WindowEffectBackend::Acrylic => WindowBackgroundAppearance::Transparent,
        WindowEffectBackend::Mica => WindowBackgroundAppearance::MicaBackdrop,
        WindowEffectBackend::MicaAlt => WindowBackgroundAppearance::MicaAltBackdrop,
    }
}

fn backdrop(effect: WindowEffect) -> DWM_SYSTEMBACKDROP_TYPE {
    match effect {
        WindowEffect::None => DWMSBT_NONE,
        WindowEffect::Acrylic => DWMSBT_TRANSIENTWINDOW,
        WindowEffect::Mica => DWMSBT_MAINWINDOW,
        WindowEffect::MicaAlt => DWMSBT_TABBEDWINDOW,
    }
}

/// 外壳只绘制一层主题染色；强度不分摊给控件，也不改变 DWM 的模糊半径。
pub(crate) fn shell_background(base: Hsla, preference: WindowEffectPreference) -> Hsla {
    if preference.effect == WindowEffect::None {
        return base;
    }
    Hsla {
        a: f32::from(preference.opacity) / 100.0,
        ..base
    }
}

/// 开启效果时去掉 caption 让 DWM 把背板铺进客户区；关闭时还原成启用前的样式。
/// 必须记住原始样式再还原，而不能无条件 `| WS_CAPTION`——悬停预览是 `WindowKind::PopUp`，
/// GPUI 给它的是空样式（`WINDOW_STYLE(0x0)`），本身没有 caption，
/// 那样反而会给它凭空加上一个标题栏。
fn set_window_style(hwnd: HWND, transparent: bool) {
    let current = WINDOW_STYLE(unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as _);
    let original = if transparent {
        // 第一次改动前先记住原样式，供关闭效果时精确还原。
        ORIGINAL_STYLES.with(|styles| {
            styles
                .borrow_mut()
                .entry(hwnd.0 as isize)
                .or_insert(current.0)
                .to_owned()
        })
    } else {
        ORIGINAL_STYLES
            .with(|styles| styles.borrow_mut().remove(&(hwnd.0 as isize)))
            .unwrap_or(current.0)
    };
    let updated = if transparent {
        current & !(WS_BORDER | WS_DLGFRAME)
    } else {
        WINDOW_STYLE(original)
    };
    if updated != current {
        unsafe { SetWindowLongW(hwnd, GWL_STYLE, updated.0 as i32) };
    }
    refresh_frame(hwnd);
}

thread_local! {
    /// HWND -> 启用效果前的窗口样式。
    static ORIGINAL_STYLES: std::cell::RefCell<std::collections::HashMap<isize, u32>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// 通知 DWM 重新计算非客户区；样式与背板属性都改完之后必须调用。
fn refresh_frame(hwnd: HWND) {
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

/// 设置背板种类、宿主刷，以及去掉背板外围那圈系统边框。
///
/// 三组属性缺一不可：
/// - `DWMWA_SYSTEMBACKDROP_TYPE` / `DWMWA_USE_HOSTBACKDROPBRUSH`：选背板并铺进客户区。
/// - `DWMWA_VISIBLE_FRAME_BORDER_THICKNESS` / `DWMWA_BORDER_COLOR`：DWM 会在背板外围补一条
///   1px 的「可见边框」，在亚克力/云母上被背板衬得格外扎眼——表现为窗口外侧套了一圈多余的
///   细框（悬停预览这种小窗口尤其明显）。置厚度 0、颜色 `NONE` 才真正干净。
/// - `DWMWA_WINDOW_CORNER_PREFERENCE`：无 caption 的窗口默认不圆角，而预览窗内容是圆角卡片，
///   不显式指定就会出现「圆角内容 + 直角窗口」的错位，四角露出背板。统一走 Win11 圆角。
///
/// 关闭效果时把三者还原成系统默认，避免留下启用的痕迹。
fn set_dwm_attributes(hwnd: HWND, effect: WindowEffect) {
    let enabled = effect != WindowEffect::None;
    let use_host_backdrop_brush = BOOL::from(enabled);
    let backdrop_type = backdrop(effect);
    let border_color: u32 = if enabled {
        DWMWA_COLOR_NONE
    } else {
        DWMWA_COLOR_DEFAULT
    };
    let border_thickness: u32 = u32::from(!enabled);
    let corner: DWM_WINDOW_CORNER_PREFERENCE = if enabled {
        DWMWCP_ROUND
    } else {
        DWMWCP_DEFAULT
    };
    // Safety: 取值都是本函数构造的局部量，按引用传入并在调用期间存活。
    unsafe {
        let _ = set_attribute(hwnd, DWMWA_USE_HOSTBACKDROPBRUSH, &use_host_backdrop_brush);
        let _ = set_attribute(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, &backdrop_type);
        let _ = set_attribute(hwnd, DWMWA_BORDER_COLOR, &border_color);
        let _ = set_attribute(
            hwnd,
            DWMWA_VISIBLE_FRAME_BORDER_THICKNESS,
            &border_thickness,
        );
        let _ = set_attribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &corner);
    }
}

fn window_handle(window: &Window) -> Option<HWND> {
    // `Window::window_handle` 是 GPUI 自己的同名方法，必须限定 trait 才能拿到原生句柄。
    let handle = HasWindowHandle::window_handle(window).ok()?;
    match handle.as_raw() {
        RawWindowHandle::Win32(handle) => {
            Some(HWND(isize::from(handle.hwnd) as *mut core::ffi::c_void))
        }
        _ => None,
    }
}

/// # Safety
///
/// `value` 在调用期间必须保持存活，且属性参数要求的布局与 `T` 一致。
unsafe fn set_attribute<T>(
    hwnd: HWND,
    attribute: DWMWINDOWATTRIBUTE,
    value: &T,
) -> windows::core::Result<()> {
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            attribute,
            (value as *const T).cast(),
            std::mem::size_of::<T>() as u32,
        )
    }
}
