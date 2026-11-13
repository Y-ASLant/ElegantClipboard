use super::settings_common::{
    choice_tabs, owner_state, pref_checkbox, pref_switch, settings_field, settings_inline,
    settings_stack,
};
use super::*;

impl SettingsWindowView {
    pub(super) fn settings_todo_tab_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_state) = owner_state(&self.owner, cx) else {
            return div().into_any_element();
        };
        let language = owner_state.language;
        let preference = owner_state.todo_preference.value;
        let pending = owner_state.todo_preference.pending;
        settings_stack()
            .child(pref_switch(
                cx,
                "todo-enabled",
                tr(language, "显示待办", "Show to-dos"),
                preference.enabled,
                pending,
                &self.owner,
                |owner| owner.todo_preference.value,
                |next, checked| next.enabled = checked,
                ClipboardView::save_todo_preference,
            ))
            .child(
                settings_field(tr(language, "标签位置", "Tab position")).child(choice_tabs(
                    "todo-tab-position",
                    &self.owner,
                    [TodoTabPosition::Last, TodoTabPosition::AfterFavorites],
                    [
                        tr(language, "末尾", "Last"),
                        tr(language, "收藏之后", "After favorites"),
                    ],
                    preference.tab_position,
                    pending || !preference.enabled,
                    |owner| owner.todo_preference.value.tab_position,
                    |owner, tab_position, cx| {
                        owner.save_todo_preference(
                            TodoPreference {
                                tab_position,
                                ..owner.todo_preference.value
                            },
                            cx,
                        );
                    },
                    cx,
                )),
            )
            .into_any_element()
    }

    pub(super) fn settings_todo_list_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_state) = owner_state(&self.owner, cx) else {
            return div().into_any_element();
        };
        let language = owner_state.language;
        let preference = owner_state.todo_preference.value;
        let pending = owner_state.todo_preference.pending;
        settings_stack()
            .child(
                settings_field(tr(language, "卡片密度", "Card density")).child(choice_tabs(
                    "todo-density",
                    &self.owner,
                    [
                        TodoDensity::FollowDisplay,
                        TodoDensity::Compact,
                        TodoDensity::Standard,
                        TodoDensity::Spacious,
                    ],
                    [
                        tr(language, "跟随显示", "Match display"),
                        tr(language, "紧凑", "Compact"),
                        tr(language, "标准", "Standard"),
                        tr(language, "宽松", "Spacious"),
                    ],
                    preference.density,
                    pending,
                    |owner| owner.todo_preference.value.density,
                    |owner, density, cx| {
                        owner.save_todo_preference(
                            TodoPreference {
                                density,
                                ..owner.todo_preference.value
                            },
                            cx,
                        );
                    },
                    cx,
                )),
            )
            .child(
                settings_inline().children(
                    [
                        (
                            "todo-show-created",
                            tr(language, "显示创建时间", "Show created time"),
                            preference.show_created,
                            (|next: &mut TodoPreference, checked| next.show_created = checked)
                                as fn(&mut TodoPreference, bool),
                        ),
                        (
                            "todo-show-edited",
                            tr(language, "显示最后编辑时间", "Show last edited time"),
                            preference.show_edited,
                            |next: &mut TodoPreference, checked| next.show_edited = checked,
                        ),
                        (
                            "todo-show-completed",
                            tr(language, "显示已完成", "Show completed"),
                            preference.show_completed,
                            |next: &mut TodoPreference, checked| next.show_completed = checked,
                        ),
                    ]
                    .into_iter()
                    .map(|(id, label, selected, set_checked)| {
                        pref_checkbox(
                            cx,
                            id,
                            label,
                            selected,
                            pending,
                            &self.owner,
                            |owner| owner.todo_preference.value,
                            set_checked,
                            ClipboardView::save_todo_preference,
                        )
                    }),
                ),
            )
            .into_any_element()
    }

    pub(super) fn settings_todo_toast_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_state) = owner_state(&self.owner, cx) else {
            return div().into_any_element();
        };
        let language = owner_state.language;
        let enabled = owner_state.todo_preference.value.toast;
        let pending = owner_state.todo_preference.pending;
        settings_inline()
            .child(pref_switch(
                cx,
                "todo-toast",
                tr(language, "启用", "Enable"),
                enabled,
                pending,
                &self.owner,
                |owner| owner.todo_preference.value,
                |next, checked| next.toast = checked,
                ClipboardView::save_todo_preference,
            ))
            .into_any_element()
    }
}
