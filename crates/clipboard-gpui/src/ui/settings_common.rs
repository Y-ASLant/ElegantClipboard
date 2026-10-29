use super::*;
use gpui_kit::component::checkbox::Checkbox;

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

pub(super) fn settings_field(label: &'static str) -> Div {
    settings_stack()
        .max_w(px(visual::CHOICE_MAX_WIDTH))
        .gap(px(visual::SPACE_2))
        .child(div().font_medium().child(label))
}

pub(super) fn settings_description(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .w_full()
        .min_w_0()
        .text_size(px(visual::TEXT_SMALL))
        .font_normal()
        .line_height(relative(1.5))
        .text_color(cx.theme().muted_foreground)
        .whitespace_normal()
        .child(text.into())
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
) -> TabBar
where
    T: Copy + PartialEq + 'static,
{
    let owner = owner.clone();
    TabBar::new(id)
        .w_full()
        .min_w_0()
        .max_w(px(visual::CHOICE_MAX_WIDTH))
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
                .disabled(disabled)
                .flex_1()
        }))
}

/// 行为开关复选框的统一构建：命令发送成功后执行 `on_sent`（置 pending
/// 标记，必要时做额外清理）并刷新界面。
pub(super) fn command_checkbox(
    id: &'static str,
    label: &'static str,
    checked: bool,
    disabled: bool,
    owner: &WeakEntity<ClipboardView>,
    command: fn(bool) -> Command,
    on_sent: fn(&mut ClipboardView),
) -> Checkbox {
    let owner = owner.clone();
    Checkbox::new(id)
        .w_full()
        .min_w_0()
        .min_h(px(visual::CONTROL_HEIGHT))
        .py(px(visual::SPACE_1))
        .text_size(px(visual::TEXT_BODY))
        .whitespace_normal()
        .label(label)
        .checked(checked)
        .disabled(disabled)
        .on_change(move |checked, _, cx| {
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
