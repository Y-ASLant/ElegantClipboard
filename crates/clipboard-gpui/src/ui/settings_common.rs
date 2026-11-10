use super::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::tooltip::Tooltip;

pub(super) fn settings_stack() -> Div {
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(visual::SPACE_4))
        .text_size(px(visual::TEXT_BODY))
        .font_normal()
        .line_height(relative(1.5))
        .whitespace_normal()
}

/// CJK help copy has no spaces; without a max width the tooltip stays one line and is clipped.
const HELP_TOOLTIP_MAX_WIDTH: f32 = 320.;

/// Height, padding and label size for every settings checkbox and switch.
pub(super) fn settings_toggle<T: Styled>(cx: &App, control: T) -> T {
    control
        .min_h(px(visual::CONTROL_HEIGHT))
        .py(px(visual::TOGGLE_PADDING_Y))
        .text_size(px(visual::TEXT_BODY))
        .font_family(crate::design_system::ui_font_family(cx))
        .font_normal()
        .line_height(relative(1.5))
        .whitespace_normal()
}

/// Horizontal layout for groups of **homogeneous** controls (typically checkboxes
/// or identical components) that benefit from two-column presentation.
/// Do not use for mixing controls with different intrinsic heights
/// (e.g. TabBar + NumberInput). Use `settings_stack` + `settings_field` instead.
pub(super) fn settings_columns() -> Div {
    settings_stack().flex_row().flex_wrap().items_start()
}

/// Use only as a direct child of `settings_columns` for same-height siblings.
pub(super) fn settings_column() -> Div {
    settings_stack().flex_basis(px(280.)).flex_grow(1.)
}

/// Shortcut badge + Kit `Button` actions on one baseline (see `Kbd`, `Button::small`).
pub(super) fn settings_shortcut_row() -> Div {
    h_flex()
        .w_full()
        .min_w_0()
        .flex_wrap()
        .items_center()
        .gap(px(visual::SPACE_2))
        .text_size(px(visual::TEXT_BODY))
        .font_normal()
        .line_height(relative(1.))
}

/// Natural-width choices and actions; never stretch individual checkboxes.
pub(super) fn settings_inline() -> Div {
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_x(px(visual::SPACE_4))
        .gap_y(px(visual::SPACE_2))
        .text_size(px(visual::TEXT_BODY))
        .font_normal()
        .line_height(relative(1.5))
}

pub(super) fn settings_field(label: &'static str) -> Div {
    settings_stack()
        .gap(px(visual::SPACE_2))
        .child(settings_label(label))
}

fn settings_label(label: impl Into<SharedString>) -> Div {
    div().font_normal().child(label.into())
}

pub(super) fn settings_column_field(label: &'static str) -> Div {
    settings_field(label).flex_basis(px(280.)).flex_grow(1.)
}

/// 字段/分区标题旁的「说明」入口：悬停图标展示解释文本，不再常驻占行。
pub(super) fn help_tooltip(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Button {
    let text: SharedString = text.into();
    let mut button = Button::new(id)
        .ghost()
        .xsmall()
        .icon(IconName::Info)
        .flex_none()
        .accessibility_label(text.clone());
    // Kit `Button::tooltip(str)` lays out as a single nowrap flex row, so long
    // CJK help is clipped by the window. GPUI's tooltip builder can wrap.
    button.interactivity().hoverable_tooltip(move |window, cx| {
        Tooltip::new(text.clone())
            .flex()
            .flex_col()
            .max_w(px(HELP_TOOLTIP_MAX_WIDTH))
            .min_w(px(0.))
            .whitespace_normal()
            .line_height(relative(1.5))
            .build(window, cx)
    });
    button
}

/// 「标签 + 说明图标」行，供字段标签使用。
/// 不单独设行高：与 `settings_label` 同用 1.5 行高，保证同排字段的控件顶缘对齐。
fn label_with_help(
    label: &'static str,
    help: impl Into<SharedString>,
    id: impl Into<ElementId>,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(visual::SPACE_1))
        .child(settings_label(label))
        .child(help_tooltip(id, help))
}

/// 带说明图标的字段标签行，替代 `settings_field` + 尾部描述行的组合。
pub(super) fn settings_field_help(
    label: &'static str,
    help: impl Into<SharedString>,
    id: impl Into<ElementId>,
) -> Div {
    settings_stack()
        .gap(px(visual::SPACE_2))
        .child(label_with_help(label, help, id))
}

/// `settings_column_field` 的带说明版本，与 `settings_field_help` 配套。
pub(super) fn settings_column_field_help(
    label: &'static str,
    help: impl Into<SharedString>,
    id: impl Into<ElementId>,
) -> Div {
    settings_field_help(label, help, id)
        .flex_basis(px(280.))
        .flex_grow(1.)
}

pub(super) fn settings_inset_list(cx: &App) -> Div {
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .rounded(cx.theme().radius)
        .border(px(visual::BORDER_WIDTH))
        .border_color(crate::design_system::container_border(cx))
        .overflow_hidden()
}

pub(super) fn settings_inset_list_row(cx: &App, last: bool) -> Div {
    let mut row = div()
        .w_full()
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(visual::SPACE_3))
        .px(px(visual::SPACE_3))
        .py(px(visual::SPACE_2));
    if !last {
        row = row.border_b_1().border_color(cx.theme().border);
    }
    row
}

pub(super) const SETTINGS_INSET_LIST_MAX_HEIGHT: f32 = 288.;

pub(super) fn settings_description(text: impl Into<SharedString>, cx: &App) -> Div {
    settings_description_sized(text, visual::TEXT_SMALL, cx)
}

/// `settings_description` 的正文（TEXT_BODY）字号版本，用于与操作按钮同行的说明。
pub(super) fn settings_description_body(text: impl Into<SharedString>, cx: &App) -> Div {
    settings_description_sized(text, visual::TEXT_BODY, cx)
}

fn settings_description_sized(text: impl Into<SharedString>, size: f32, cx: &App) -> Div {
    div()
        .w_full()
        .min_w_0()
        .text_size(px(size))
        .font_normal()
        .line_height(relative(1.5))
        .text_color(cx.theme().muted_foreground)
        .whitespace_normal()
        .child(text.into())
}

/// 偏好布尔开关的 `on_change` 闭包：`read` 取当前偏好副本，`set` 改写一个字段，`save` 提交。
pub(super) fn pref_change<P: Clone + 'static>(
    owner: &WeakEntity<ClipboardView>,
    read: fn(&ClipboardView) -> P,
    set: impl Fn(&mut P, bool) + 'static,
    save: fn(&mut ClipboardView, P, &mut Context<ClipboardView>),
) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
    let owner = owner.clone();
    move |checked, _, cx| {
        let _ = owner.update(cx, |owner, cx| {
            let mut next = read(owner);
            set(&mut next, *checked);
            save(owner, next, cx);
        });
    }
}

/// 偏好布尔 Checkbox 的统一构建；disabled 与每行条件逻辑留在调用点。
#[allow(clippy::too_many_arguments)]
pub(super) fn pref_checkbox<P: Clone + 'static>(
    cx: &App,
    id: impl Into<ElementId>,
    label: &'static str,
    checked: bool,
    disabled: bool,
    owner: &WeakEntity<ClipboardView>,
    read: fn(&ClipboardView) -> P,
    set: impl Fn(&mut P, bool) + 'static,
    save: fn(&mut ClipboardView, P, &mut Context<ClipboardView>),
) -> Checkbox {
    settings_toggle(cx, Checkbox::new(id))
        .label(label)
        .checked(checked)
        .disabled(disabled)
        .on_change(pref_change(owner, read, set, save))
}

/// `pref_checkbox` 的 `Switch` 版本。
#[allow(clippy::too_many_arguments)]
pub(super) fn pref_switch<P: Clone + 'static>(
    cx: &App,
    id: impl Into<ElementId>,
    label: &'static str,
    checked: bool,
    disabled: bool,
    owner: &WeakEntity<ClipboardView>,
    read: fn(&ClipboardView) -> P,
    set: impl Fn(&mut P, bool) + 'static,
    save: fn(&mut ClipboardView, P, &mut Context<ClipboardView>),
) -> Switch {
    settings_toggle(cx, Switch::new(id))
        .label(label)
        .checked(checked)
        .disabled(disabled)
        .on_change(pref_change(owner, read, set, save))
}

/// 设置各 section 共用的开场：升级 owner 弱引用并读取主窗口状态。
/// 返回 `None` 表示主窗口实体已释放，调用方应给出各自的占位元素。
pub(super) fn owner_state<'a>(
    owner: &WeakEntity<ClipboardView>,
    cx: &'a App,
) -> Option<&'a ClipboardView> {
    Some(owner.upgrade()?.read(cx))
}

/// 同上，但只需要界面语言时的简写。
pub(super) fn owner_language(
    owner: &WeakEntity<ClipboardView>,
    cx: &App,
) -> Option<LanguagePreference> {
    owner_state(owner, cx).map(|state| state.language)
}

/// 枚举选择 TabBar 的统一构建。`choices` 与 `labels` 等长且顺序一致；
/// 点击时重新读取 `current_of`，值确实变化后才调用 `apply` 保存。
#[allow(clippy::too_many_arguments)]
pub(super) fn choice_tabs<T, const N: usize>(
    id: &'static str,
    owner: &WeakEntity<ClipboardView>,
    choices: [T; N],
    labels: [&'static str; N],
    current: T,
    disabled: bool,
    current_of: impl Fn(&ClipboardView) -> T + 'static,
    apply: impl Fn(&mut ClipboardView, T, &mut Context<ClipboardView>) + 'static,
    cx: &App,
) -> TabBar
where
    T: Copy + PartialEq + 'static,
{
    let font_family = crate::design_system::ui_font_family(cx);
    let owner = owner.clone();
    TabBar::new(id)
        .w_full()
        .min_w_0()
        .large()
        .h(px(visual::CONTROL_HEIGHT_LARGE))
        .segmented()
        .last_empty_space(div().w_0())
        .selected_index(
            choices
                .iter()
                .position(|&choice| choice == current)
                .unwrap_or(0),
        )
        .on_click(move |index: &usize, _, cx| {
            if disabled {
                return;
            }
            let Some(&value) = choices.get(*index) else {
                return;
            };
            let _ = owner.update(cx, |owner, cx| {
                if current_of(owner) != value {
                    apply(owner, value, cx);
                }
            });
        })
        .children(labels.map(|label| {
            Tab::new()
                .label(label)
                .text_size(px(visual::TEXT_BODY))
                .font_family(font_family.clone())
                .aria_label(label)
                .disabled(disabled)
                .flex_1()
        }))
}

/// 行为开关复选框的统一构建：命令发送成功后执行 `on_sent`（置 pending
/// 标记，必要时做额外清理）并刷新界面。
#[allow(clippy::too_many_arguments)]
pub(super) fn command_checkbox(
    cx: &App,
    id: &'static str,
    label: &'static str,
    checked: bool,
    disabled: bool,
    owner: &WeakEntity<ClipboardView>,
    command: fn(bool) -> Command,
    on_sent: fn(&mut ClipboardView),
) -> Checkbox {
    command_checkbox_help(
        cx,
        id,
        label,
        checked,
        disabled,
        owner,
        command,
        on_sent,
        None::<&'static str>,
    )
}

/// `command_checkbox` 的带悬停说明版本；`tooltip` 传入 `Some` 时挂到复选框上。
#[allow(clippy::too_many_arguments)]
pub(super) fn command_checkbox_help(
    cx: &App,
    id: &'static str,
    label: &'static str,
    checked: bool,
    disabled: bool,
    owner: &WeakEntity<ClipboardView>,
    command: fn(bool) -> Command,
    on_sent: fn(&mut ClipboardView),
    tooltip: Option<impl Into<SharedString>>,
) -> Checkbox {
    let owner = owner.clone();
    let checkbox = settings_toggle(cx, Checkbox::new(id))
        .min_w_0()
        .label(label)
        .checked(checked)
        .disabled(disabled);
    let checkbox = match tooltip {
        Some(tooltip) => checkbox.tooltip(tooltip.into()),
        None => checkbox,
    };
    checkbox.on_change(move |checked, _, cx| {
        if disabled {
            return;
        }
        let _ = owner.update(cx, |owner, cx| {
            if owner.send(command(*checked), cx) {
                on_sent(owner);
                cx.notify();
            }
        });
    })
}
