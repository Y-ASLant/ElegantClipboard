use super::*;
use gpui_kit::component::checkbox::Checkbox;

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
        .segmented()
        .selected_index(
            choices
                .iter()
                .position(|&choice| choice == current)
                .unwrap_or(0),
        )
        .on_click(move |index: &usize, _, cx| {
            let Some(&value) = choices.get(*index) else {
                return;
            };
            let _ = owner.update(cx, |owner, cx| {
                if current_of(owner) != value {
                    apply(owner, value, cx);
                }
            });
        })
        .children(labels.map(|label| Tab::new().label(label).disabled(disabled).flex_1()))
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
        .w(px(350.))
        .text_sm()
        .label(label)
        .checked(checked)
        .disabled(disabled)
        .on_change(move |checked, _, cx| {
            let _ = owner.update(cx, |owner, cx| {
                if owner.send(command(*checked), cx) {
                    on_sent(owner);
                    cx.notify();
                }
            });
        })
}
