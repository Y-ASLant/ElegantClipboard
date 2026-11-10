use super::settings_common::{
    choice_tabs, owner_state, settings_field, settings_inline, settings_stack, settings_toggle,
};
use super::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::switch::Switch;

impl SettingsWindowView {
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
                        let owner = self.owner.clone();
                        settings_toggle(cx, Checkbox::new(id))
                            .label(label)
                            .checked(selected)
                            .disabled(pending)
                            .on_change(move |checked, _, cx| {
                                let _ = owner.update(cx, |owner, cx| {
                                    let mut next = owner.todo_preference.value;
                                    set_checked(&mut next, *checked);
                                    owner.save_todo_preference(next, cx);
                                });
                            })
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
        let owner = self.owner.clone();
        settings_inline()
            .child(
                settings_toggle(cx, Switch::new("todo-toast"))
                    .label(tr(language, "启用", "Enable"))
                    .checked(enabled)
                    .disabled(pending)
                    .on_change(move |checked, _, cx| {
                        let _ = owner.update(cx, |owner, cx| {
                            owner.save_todo_preference(
                                TodoPreference {
                                    toast: *checked,
                                    ..owner.todo_preference.value
                                },
                                cx,
                            );
                        });
                    }),
            )
            .into_any_element()
    }
}
