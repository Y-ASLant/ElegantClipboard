use super::*;
use gpui_kit::base::Popover;

/// 触发器包装：Popover 展开时会对 trigger 调 `Selectable::open(true)`，
/// `Button` 把它画成选中深色。这里截住 open 状态，不再向 Button 传递，
/// 展开期间触发器保持常态。
#[derive(IntoElement)]
struct GroupSelectTrigger {
    button: Button,
    open: bool,
}

impl GroupSelectTrigger {
    fn new(button: Button) -> Self {
        Self {
            button,
            open: false,
        }
    }
}

impl Selectable for GroupSelectTrigger {
    fn selected(mut self, selected: bool) -> Self {
        self.button = self.button.selected(selected);
        self
    }

    fn is_selected(&self) -> bool {
        self.button.is_selected()
    }

    fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    fn is_open(&self) -> bool {
        self.open
    }
}

impl RenderOnce for GroupSelectTrigger {
    fn render(self, window: &mut Window, cx: &mut gpui_kit::App) -> impl IntoElement {
        RenderOnce::render(self.button, window, cx)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GroupChoice {
    Default,
    Existing(i64),
    Create,
    Edit,
    MoveUp,
    MoveDown,
}

#[derive(Clone)]
pub(super) struct GroupOption {
    pub choice: GroupChoice,
    pub title: SharedString,
    /// 操作项的行首图标；分组项为 None。
    pub icon: Option<gpui_kit::assets::IconName>,
}

pub(super) fn group_options(
    groups: &[Group],
    language: LanguagePreference,
    selected: Option<i64>,
) -> Vec<GroupOption> {
    let (default, create, edit, move_up, move_down) = match language {
        LanguagePreference::Chinese => ("默认分组", "新建分组", "编辑分组", "上移分组", "下移分组"),
        LanguagePreference::English => (
            "Default group",
            "New group",
            "Edit group",
            "Move group up",
            "Move group down",
        ),
    };

    // Group snapshots are already ordered by the repository.
    let selected_index = selected.and_then(|id| groups.iter().position(|group| group.id == id));
    let mut options = Vec::with_capacity(groups.len() + 5);
    options.push(GroupOption {
        choice: GroupChoice::Default,
        title: default.into(),
        icon: None,
    });
    options.extend(groups.iter().map(|group| GroupOption {
        choice: GroupChoice::Existing(group.id),
        title: group.name.clone().into(),
        icon: None,
    }));
    options.push(GroupOption {
        choice: GroupChoice::Create,
        title: create.into(),
        icon: Some(gpui_kit::assets::IconName::Plus),
    });
    if let Some(index) = selected_index {
        options.push(GroupOption {
            choice: GroupChoice::Edit,
            title: edit.into(),
            icon: Some(gpui_kit::assets::IconName::Settings),
        });
        if index > 0 {
            options.push(GroupOption {
                choice: GroupChoice::MoveUp,
                title: move_up.into(),
                icon: Some(gpui_kit::assets::IconName::ArrowUp),
            });
        }
        if index + 1 < groups.len() {
            options.push(GroupOption {
                choice: GroupChoice::MoveDown,
                title: move_down.into(),
                icon: Some(gpui_kit::assets::IconName::ArrowDown),
            });
        }
    }
    options
}

impl ClipboardView {
    pub(super) fn render_group_select(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected = self.history.group_id();
        let title = self
            .groups
            .iter()
            .find(|group| Some(group.id) == selected)
            .map(|group| SharedString::from(group.name.clone()))
            .unwrap_or_else(|| tr(self.language, "默认分组", "Default group").into());
        let owner = cx.entity().downgrade();
        let selected_focus = window
            .use_keyed_state("selected-group-focus", cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();
        let scroll = window
            .use_keyed_state("group-menu-scroll", cx, |_, _| ScrollHandle::new())
            .read(cx)
            .clone();
        let open_scroll = scroll.clone();
        let selected_row = self
            .groups
            .iter()
            .position(|group| Some(group.id) == selected)
            .map_or(0, |index| index + 1);
        Popover::new("group-menu")
            .anchor(Anchor::TopRight)
            .offset(px(4.))
            .track_focus(&selected_focus)
            .on_open_change(move |open, _, _| {
                if *open {
                    open_scroll.scroll_to_item(selected_row * 2);
                }
            })
            .trigger(GroupSelectTrigger::new(
                Button::new("group-select")
                    .outline()
                    .w_full()
                    .h(px(visual::CONTROL_HEIGHT))
                    .text_size(px(visual::TEXT_BODY))
                    .label(title)
                    .dropdown_caret(true)
                    .disabled(
                        self.todos_open
                            || self.group_save_pending
                            || self.group_delete_pending
                            || self.clear_pending
                            || self.group_reorder_pending,
                    ),
            ))
            .content(move |_, window, cx| {
                let Some(owner) = owner.upgrade() else {
                    return div().into_any_element();
                };
                let view = owner.read(cx);
                let selected = view
                    .history
                    .group_id()
                    .map_or(GroupChoice::Default, GroupChoice::Existing);
                let options = group_options(&view.groups, view.language, view.history.group_id());
                let focus_handles: Vec<_> = options
                    .iter()
                    .enumerate()
                    .map(|(index, option)| {
                        if option.choice == selected {
                            selected_focus.clone()
                        } else {
                            window
                                .use_keyed_state(("group-option-focus", index), cx, |_, cx| {
                                    cx.focus_handle()
                                })
                                .read(cx)
                                .clone()
                        }
                    })
                    .collect();
                let mut rows = div()
                    .id("group-menu-items")
                    .flex()
                    .flex_col()
                    .w_full()
                    .max_h(px(290.))
                    .track_scroll(&scroll)
                    .overflow_y_scroll();
                for (index, option) in options.into_iter().enumerate() {
                    if index > 0 {
                        rows = rows.child(div().flex_none().h(px(2.)));
                    }
                    let owner = owner.downgrade();
                    let choice = option.choice;
                    rows = rows.child(
                        Button::new(("group-option", index))
                            .ghost()
                            .flex_none()
                            .w_full()
                            .h(px(visual::CONTROL_HEIGHT_COMPACT))
                            .justify_start()
                            .px(px(6.))
                            .track_focus(&focus_handles[index])
                            .accessibility_label(option.title.clone())
                            .selected(choice == selected)
                            .text_color(cx.theme().foreground)
                            .when(choice == selected, |button| button.bg(cx.theme().accent))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .w_full()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_1()
                                            .min_w_0()
                                            .items_center()
                                            .gap(px(6.))
                                            .when_some(option.icon, |row, icon| {
                                                row.child(
                                                    Icon::new(icon)
                                                        .xsmall()
                                                        .text_color(cx.theme().muted_foreground),
                                                )
                                            })
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .truncate()
                                                    .child(option.title.clone()),
                                            ),
                                    )
                                    .when(choice == selected, |row| {
                                        row.child(Icon::new(IconName::Check).xsmall())
                                    }),
                            )
                            .on_click(cx.listener(move |popover, _, window, cx| {
                                popover.dismiss(window, cx);
                                let _ = owner.update(cx, |view, cx| {
                                    view.choose_group(choice, window, cx);
                                });
                            })),
                    );
                }
                let scroll = scroll.clone();
                let list = rows.on_key_down(move |event, window, cx| {
                    let current = focus_handles
                        .iter()
                        .position(|handle| handle.is_focused(window))
                        .unwrap_or(0);
                    let next = match event.keystroke.key.as_str() {
                        "down" => (current + 1) % focus_handles.len(),
                        "up" => (current + focus_handles.len() - 1) % focus_handles.len(),
                        "home" => 0,
                        "end" => focus_handles.len() - 1,
                        _ => return,
                    };
                    focus_handles[next].focus(window, cx);
                    // Each preceding option contributes one row and one spacing element.
                    scroll.scroll_to_item(next * 2);
                    window.refresh();
                    cx.stop_propagation();
                });
                v_flex()
                    .occlude()
                    .popover_style(cx)
                    .p(px(4.))
                    // 菜单外缘与触发器槽位同宽，同一 token。
                    .w(px(visual::GROUP_SELECT_WIDTH))
                    .child(list)
                    .into_any_element()
            })
    }
}
