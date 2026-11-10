use super::settings_common::{
    choice_tabs, command_checkbox, command_checkbox_help, owner_state, pref_checkbox,
    settings_description, settings_field_help, settings_inline, settings_stack,
};
use super::*;
use gpui_kit::component::chart::BarChart;

impl SettingsWindowView {
    pub(super) fn settings_transient_filter_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_state) = owner_state(&self.owner, cx) else {
            return div().into_any_element();
        };
        let language = owner_state.language;
        settings_stack()
            .gap_2()
            .child(command_checkbox(
                cx,
                "transient-filter",
                tr(
                    language,
                    "过滤瞬时复制内容（推荐开启）",
                    "Filter transient copies (recommended)",
                ),
                owner_state.transient_filter.value,
                owner_state.transient_filter.pending,
                &self.owner,
                Command::SetTransientFilter,
                |owner| owner.transient_filter.pending = true,
            ))
            .into_any_element()
    }

    /// 去重策略与粘贴后恢复剪贴板（决策 0003）。
    pub(super) fn settings_dedup_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_state) = owner_state(&self.owner, cx) else {
            return div().into_any_element();
        };
        let language = owner_state.language;
        let strategy = owner_state.dedup_strategy.value;
        let strategy_pending = owner_state.dedup_strategy.pending;
        settings_stack()
            .child(
                settings_field_help(
                    tr(language, "相同内容再次复制时", "When the same content is copied again"),
                    tr(
                        language,
                        "「各存一条」保留每次复制的独立时间线；「移到列表顶部」合并为同一条记录。",
                        "\"Keep separate entries\" preserves each copy on the timeline; \"Move to top\" merges them into one entry.",
                    ),
                    "dedup-strategy-help",
                )
                .child(choice_tabs(
                    "dedup-strategy-tabs",
                    &self.owner,
                    [DedupStrategy::MoveToTop, DedupStrategy::AlwaysNew],
                    [
                        tr(language, "移到列表顶部", "Move to top"),
                        tr(language, "各存一条", "Keep separate entries"),
                    ],
                    strategy,
                    strategy_pending,
                    |state| state.dedup_strategy.value,
                    |owner, value, cx| {
                        if owner.send(Command::SetDedupStrategy(value), cx) {
                            owner.dedup_strategy.pending = true;
                            cx.notify();
                        }
                    },
                    cx,
                )),
            )
            .child(
                command_checkbox_help(
                    cx,
                    "restore-clipboard",
                    tr(
                        language,
                        "粘贴后恢复原剪贴板",
                        "Restore clipboard after pasting",
                    ),
                    owner_state.restore_clipboard.value,
                    owner_state.restore_clipboard.pending,
                    &self.owner,
                    Command::SetRestoreClipboard,
                    |owner| owner.restore_clipboard.pending = true,
                    Some(tr(
                        language,
                        "开启后，受控粘贴完成时会把粘贴前的剪贴板内容写回。写入与目标应用读取存在时序竞争，个别应用可能需要再次手动粘贴；默认关闭。",
                        "When enabled, the clipboard content captured before a controlled paste is written back afterwards. Writing races with the target app reading it; some apps may need a manual paste. Off by default.",
                    )),
                ),
            )
            .into_any_element()
    }

    pub(super) fn settings_monitor_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_state) = owner_state(&self.owner, cx) else {
            return div().into_any_element();
        };
        let language = owner_state.language;
        let monitor_types = owner_state.monitor_types.value;
        let monitor_types_pending = owner_state.monitor_types.pending;

        type MonitorChoice<'a> = (
            &'a str,
            &'a str,
            bool,
            fn(&mut MonitorTypesPreference, bool),
        );
        let choices: [MonitorChoice<'_>; 6] = [
            (
                "monitor-text",
                tr(language, "文本", "Text"),
                monitor_types.text,
                |next, checked| next.text = checked,
            ),
            (
                "monitor-url",
                tr(language, "网址", "URL"),
                monitor_types.url,
                |next, checked| next.url = checked,
            ),
            (
                "monitor-html",
                "HTML",
                monitor_types.html,
                |next, checked| next.html = checked,
            ),
            ("monitor-rtf", "RTF", monitor_types.rtf, |next, checked| {
                next.rtf = checked
            }),
            (
                "monitor-image",
                tr(language, "图片", "Images"),
                monitor_types.image,
                |next, checked| next.image = checked,
            ),
            (
                "monitor-files",
                tr(language, "文件", "Files"),
                monitor_types.files,
                |next, checked| next.files = checked,
            ),
        ];
        settings_inline()
            .children(
                choices
                    .into_iter()
                    .map(|(id, label, selected, set_checked)| {
                        let mut next = monitor_types;
                        set_checked(&mut next, !selected);
                        pref_checkbox(
                            cx,
                            id,
                            label,
                            selected,
                            monitor_types_pending || !next.valid(),
                            &self.owner,
                            |owner| owner.monitor_types.value,
                            set_checked,
                            ClipboardView::save_monitor_types,
                        )
                        .min_w_0()
                    }),
            )
            .into_any_element()
    }

    pub(super) fn settings_daily_counts_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner) = owner_state(&self.owner, cx) else {
            return div().into_any_element();
        };
        let Some(counts) = &owner.daily_counts else {
            return settings_description(
                tr(
                    owner.language,
                    "尚未获取每日统计",
                    "Daily counts are not available",
                ),
                cx,
            )
            .into_any_element();
        };
        let today = chrono::Local::now().date_naive();
        let data = (0..7)
            .rev()
            .map(|offset| {
                let day = (today - chrono::Duration::days(offset)).to_string();
                let count = counts
                    .iter()
                    .find(|(date, _)| date == &day)
                    .map_or(0, |(_, count)| *count);
                (day, count)
            })
            .collect::<Vec<_>>();
        if data.iter().all(|(_, count)| *count == 0) {
            return settings_description(
                tr(
                    owner.language,
                    "最近 7 天没有历史记录",
                    "No items in the past 7 days",
                ),
                cx,
            )
            .into_any_element();
        }
        div()
            .w_full()
            .min_w_0()
            .h(px(visual::DAILY_CHART_HEIGHT))
            .child(
                BarChart::new(data)
                    .id("daily-clipboard-counts")
                    .name(tr(owner.language, "历史条数", "Items"))
                    .band(|(date, _)| date[5..].to_string())
                    .value(|(_, count)| *count as f64)
                    .label(|(_, count)| count.to_string())
                    .grid(false),
            )
            .into_any_element()
    }
}
