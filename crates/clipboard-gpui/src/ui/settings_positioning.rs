use super::settings_common::{owner_state, settings_stack};
use super::*;

impl SettingsWindowView {
    pub(super) fn settings_positioning_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(state) = owner_state(&self.owner, cx) else {
            return div().into_any_element();
        };
        let language = state.language;
        let window_position = state.window_position.value;
        let reason = crate::capabilities::positioning_unavailable_reason(language);
        let pending = state.window_position.pending;
        let choices = [
            WindowPositionPreference::FollowCursor,
            WindowPositionPreference::ScreenCenter,
            WindowPositionPreference::FixedPosition,
        ];
        let labels = [
            tr(language, "跟随光标", "Follow cursor"),
            tr(language, "当前屏幕居中", "Center on screen"),
            tr(language, "保持位置", "Keep position"),
        ];
        let owner = self.owner.clone();
        settings_stack()
            .child(
                TabBar::new("settings-window-position-tabs")
                    .large()
                    .h(px(visual::CONTROL_HEIGHT_LARGE))
                    .segmented()
                    .w_full()
                    .min_w_0()
                    .last_empty_space(div().w_0())
                    .selected_index(
                        choices
                            .iter()
                            .position(|choice| *choice == window_position)
                            .unwrap_or(0),
                    )
                    .children(choices.iter().zip(labels).map(|(choice, label)| {
                        Tab::new()
                            .label(label)
                            .text_size(px(visual::TEXT_BODY))
                            .flex_1()
                            .disabled(
                                pending
                                    || (reason.is_some()
                                        && *choice != WindowPositionPreference::FixedPosition),
                            )
                    }))
                    .on_click(move |index: &usize, _, cx| {
                        let Some(&preference) = choices.get(*index) else {
                            return;
                        };
                        if pending
                            || (reason.is_some()
                                && preference != WindowPositionPreference::FixedPosition)
                        {
                            return;
                        }
                        let _ = owner.update(cx, |owner, cx| {
                            if owner.send(Command::SetWindowPosition(preference), cx) {
                                owner.window_position.pending = true;
                                cx.notify();
                            }
                        });
                    }),
            )
            .when_some(reason, |panel, reason| {
                panel.child(Alert::warning("position-capability", reason).small())
            })
            .into_any_element()
    }
}
