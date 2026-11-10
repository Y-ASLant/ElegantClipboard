use super::settings_common::{
    SectionDescription, label_with_help, section_description_layout, settings_description,
    settings_stack,
};
use super::*;
use gpui_kit::component::{
    group_box::{GroupBox, GroupBoxVariant, GroupBoxVariants},
    setting::{SelectIndex, SettingGroup, SettingItem, SettingPage, Settings},
};

struct Section {
    page: SettingsPage,
    chinese: &'static str,
    english: &'static str,
    description_chinese: &'static str,
    description_english: &'static str,
    content: fn(&SettingsWindowView, &mut Context<SettingsWindowView>) -> AnyElement,
}

const SECTIONS: &[Section] = &[
    Section {
        page: SettingsPage::General,
        chinese: "窗口位置",
        english: "Window position",
        description_chinese: "",
        description_english: "",
        content: SettingsWindowView::settings_positioning_content,
    },
    Section {
        page: SettingsPage::General,
        chinese: "窗口行为",
        english: "Window behavior",
        description_chinese: "",
        description_english: "",
        content: SettingsWindowView::settings_behavior_content,
    },
    Section {
        page: SettingsPage::General,
        chinese: "开机启动",
        english: "Startup",
        description_chinese: "",
        description_english: "",
        content: SettingsWindowView::settings_startup_content,
    },
    Section {
        page: SettingsPage::General,
        chinese: "诊断",
        english: "Diagnostics",
        description_chinese: "记录应用运行与粘贴过程日志，便于排查问题",
        description_english: "Log app and paste activity to help diagnose issues",
        content: SettingsWindowView::settings_diagnostics_content,
    },
    Section {
        page: SettingsPage::Display,
        chinese: "列表显示",
        english: "List display",
        description_chinese: "",
        description_english: "",
        content: SettingsWindowView::settings_display_content,
    },
    Section {
        page: SettingsPage::Todos,
        chinese: "列表",
        english: "List",
        description_chinese: "卡片外观与历史列表相同",
        description_english: "Same card appearance as history",
        content: SettingsWindowView::settings_todo_list_content,
    },
    Section {
        page: SettingsPage::Todos,
        chinese: "提醒",
        english: "Reminders",
        description_chinese: "开启后，添加或完成待办会在屏幕右下角弹出，3 秒后消失，也可点关闭。不进入主窗口。",
        description_english: "After adding or completing a to-do, a toast appears at the bottom-right of the screen. It closes after 3 seconds or when dismissed, and does not appear inside the main window.",
        content: SettingsWindowView::settings_todo_toast_content,
    },
    Section {
        page: SettingsPage::Display,
        chinese: "悬停预览",
        english: "Hover preview",
        description_chinese: "",
        description_english: "",
        content: SettingsWindowView::settings_hover_content,
    },
    Section {
        page: SettingsPage::Theme,
        chinese: "外观与语言",
        english: "Appearance & language",
        description_chinese: "",
        description_english: "",
        content: SettingsWindowView::settings_appearance_content,
    },
    Section {
        page: SettingsPage::Data,
        chinese: "监听内容类型",
        english: "Capture types",
        description_chinese: "选择保存到历史的内容类型；至少保留一种",
        description_english: "Choose which content types enter history; keep at least one",
        content: SettingsWindowView::settings_monitor_content,
    },
    Section {
        page: SettingsPage::Data,
        chinese: "采集格式",
        english: "Capture formats",
        description_chinese: "关闭后只读取名单中的格式",
        description_english: "Read only listed formats when off",
        content: SettingsWindowView::settings_capture_formats_content,
    },
    Section {
        page: SettingsPage::Data,
        chinese: "瞬时内容过滤",
        english: "Transient filtering",
        description_chinese: "复制后 300ms 内被撤销或覆盖的内容（划词工具临时复制、表格单元格碎片）不再进入历史。复制后立即从本应用粘贴不受影响。",
        description_english: "Content undone or replaced within 300ms of copying (selection-tool flashes, spreadsheet fragments) no longer enters history. Pasting from this app right after copying is unaffected.",
        content: SettingsWindowView::settings_transient_filter_content,
    },
    Section {
        page: SettingsPage::Data,
        chinese: "去重与剪贴板",
        english: "Deduplication",
        description_chinese: "",
        description_english: "",
        content: SettingsWindowView::settings_dedup_content,
    },
    Section {
        page: SettingsPage::Data,
        chinese: "保留策略",
        english: "Retention",
        description_chinese: "每个分组独立限制，置顶和收藏不计入数量且不会自动删除。0 表示关闭对应限制。",
        description_english: "Limits apply per group. Pinned and favorite items are excluded and protected. 0 disables a limit.",
        content: SettingsWindowView::settings_retention_content,
    },
    Section {
        page: SettingsPage::Data,
        chinese: "每日历史条数",
        english: "Daily history",
        description_chinese: "最近 7 天每天保存的剪贴板记录数（含今天）",
        description_english: "Retained clipboard items per day for the past 7 days, including today",
        content: SettingsWindowView::settings_daily_counts_content,
    },
    Section {
        page: SettingsPage::Data,
        chinese: "本地数据",
        english: "Storage",
        description_chinese: "",
        description_english: "",
        content: SettingsWindowView::settings_storage_content,
    },
    Section {
        page: SettingsPage::Data,
        chinese: "导入与恢复",
        english: "Import & restore",
        description_chinese: "",
        description_english: "",
        content: SettingsWindowView::settings_import_content,
    },
    Section {
        page: SettingsPage::Data,
        chinese: "隐私清理",
        english: "Privacy",
        description_chinese: "",
        description_english: "",
        content: SettingsWindowView::settings_privacy_content,
    },
    Section {
        page: SettingsPage::AppFilter,
        chinese: "来源应用过滤",
        english: "Source app filter",
        description_chinese: "",
        description_english: "",
        content: SettingsWindowView::settings_app_filter_content,
    },
    Section {
        page: SettingsPage::Audio,
        chinese: "复制音效",
        english: "Copy sound",
        description_chinese: "复制历史记录时播放反馈音",
        description_english: "Play a sound when copying a history item",
        content: copy_audio_content,
    },
    Section {
        page: SettingsPage::Audio,
        chinese: "粘贴音效",
        english: "Paste sound",
        description_chinese: "向原窗口发送粘贴快捷键时播放反馈音",
        description_english: "Play a sound when sending paste to the previous window",
        content: paste_audio_content,
    },
    Section {
        page: SettingsPage::Shortcuts,
        chinese: "唤出快捷键",
        english: "Shortcut",
        description_chinese: "",
        description_english: "",
        content: SettingsWindowView::settings_shortcut_content,
    },
    Section {
        page: SettingsPage::Shortcuts,
        chinese: "快速粘贴",
        english: "Quick paste",
        description_chinese: "默认关闭；启用后可从其他应用直接粘贴当前分组排在前面的记录。数字键自动支持数字小键盘。",
        description_english: "Off by default; enable to paste top items in the selected group from another app. Digit keys also work on the numpad.",
        content: SettingsWindowView::settings_quick_paste_content,
    },
    Section {
        page: SettingsPage::About,
        chinese: "",
        english: "",
        description_chinese: "",
        description_english: "",
        content: SettingsWindowView::settings_about_content,
    },
];

fn copy_audio_content(
    view: &SettingsWindowView,
    cx: &mut Context<SettingsWindowView>,
) -> AnyElement {
    view.settings_audio_content(SoundChannel::Copy, cx)
}

fn paste_audio_content(
    view: &SettingsWindowView,
    cx: &mut Context<SettingsWindowView>,
) -> AnyElement {
    view.settings_audio_content(SoundChannel::Paste, cx)
}

fn settings_title_bar(language: LanguagePreference, cx: &App) -> TitleBar {
    TitleBar::new().bg(cx.theme().sidebar).child(
        div()
            .text_size(cx.theme().font_size)
            .font_family(cx.theme().font_family.clone())
            .font_medium()
            .child(tr(language, "设置", "Settings")),
    )
}

impl Render for SettingsWindowView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(owner) = self.owner.upgrade() else {
            return window_shell(cx)
                .child(settings_title_bar(self.language, cx))
                .child(
                    div().p_3().child(
                        Alert::error(
                            "settings-owner-unavailable",
                            tr(
                                self.language,
                                "主窗口已关闭，无法继续更改设置。请关闭此窗口并重新启动应用。",
                                "The main window is no longer available. Close this window and restart the app to change settings.",
                            ),
                        )
                        .small()
                        .min_w_0()
                        .whitespace_normal(),
                    ),
                );
        };
        let (language, preview_lines, display_pending) = {
            let owner = owner.read(cx);
            (
                owner.language,
                owner.display.card_max_lines,
                owner.display_pending,
            )
        };
        let sync_preview_lines = preview_lines != self.preview_lines_last_value
            || (self.preview_lines_last_pending && !display_pending);
        self.preview_lines_last_value = preview_lines;
        self.preview_lines_last_pending = display_pending;
        if sync_preview_lines {
            let confirmed = preview_lines.to_string();
            if self.preview_lines_input.read(cx).value() != confirmed {
                self.preview_lines_input
                    .update(cx, |input, cx| input.set_value(confirmed, window, cx));
            }
        }
        let (opacity, opacity_pending, delay_ms, delay_pending, zoom_step, zoom_pending) = {
            let owner = owner.read(cx);
            (
                i32::from(owner.window_effect.opacity),
                owner.window_effect_pending,
                i32::from(owner.hover_preference.delay_ms),
                owner.hover_preference_pending,
                i32::from(owner.hover_preference.zoom_step),
                owner.hover_preference_pending,
            )
        };
        self.window_opacity
            .sync(opacity, opacity_pending, window, cx);
        self.hover_delay.sync(delay_ms, delay_pending, window, cx);
        self.hover_zoom.sync(zoom_step, zoom_pending, window, cx);
        let (ui_size, card_size, preview_size, fonts_pending) = {
            let owner = owner.read(cx);
            (
                i32::from(owner.fonts.value.ui_font_size),
                i32::from(owner.fonts.value.card_font_size),
                i32::from(owner.fonts.value.preview_font_size),
                owner.fonts.pending,
            )
        };
        self.ui_font_size.sync(ui_size, fonts_pending, window, cx);
        self.card_font_size
            .sync(card_size, fonts_pending, window, cx);
        self.preview_font_size
            .sync(preview_size, fonts_pending, window, cx);
        let locale = match language {
            LanguagePreference::Chinese => "zh-CN",
            LanguagePreference::English => "en",
        };
        if &*gpui_kit::component::locale() != locale {
            gpui_kit::component::set_locale(locale);
        }
        window.set_window_title(tr(language, "设置", "Settings"));
        let settings = cx.entity().downgrade();
        let pages = SettingsPage::ALL.map(|page| {
            let groups = SECTIONS
                .iter()
                .enumerate()
                .filter(|(_, section)| section.page == page)
                .map(|(section_ix, section)| {
                    let title = tr(language, section.chinese, section.english);
                    let description = tr(
                        language,
                        section.description_chinese,
                        section.description_english,
                    );
                    let settings = settings.clone();
                    let group = SettingGroup::new().variant(GroupBoxVariant::Normal);
                    group
                        .text_size(px(visual::TEXT_BODY))
                        .font_semibold()
                        .py(px(visual::SPACE_2))
                        .gap(px(visual::SPACE_2))
                        .item(
                            SettingItem::render(move |_, _, cx| {
                                let content = settings
                                    .update(cx, |view, cx| {
                                        if view.page != page {
                                            view.page = page;
                                            view.shortcut_recording = false;
                                            view.shortcut_editing = None;
                                            view.shortcut_capture_error = None;
                                        }
                                        (section.content)(view, cx)
                                    })
                                    .unwrap_or_else(|_| div().into_any_element());
                                let layout = section_description_layout(description);
                                let body = if layout == SectionDescription::Body {
                                    settings_stack()
                                        .child(settings_description(description, cx))
                                        .child(content)
                                        .into_any_element()
                                } else {
                                    content
                                };
                                // 页面内容直接渲染，切换页面不做淡入动效。
                                div()
                                    .w_full()
                                    .min_w_0()
                                    .child(
                                        GroupBox::new()
                                            .id(("settings-card", section_ix))
                                            .outline()
                                            .when(!title.is_empty(), |card| {
                                                card.title(
                                                    if layout == SectionDescription::Tooltip {
                                                        label_with_help(
                                                            title,
                                                            description,
                                                            ("section-help", section_ix),
                                                        )
                                                    } else {
                                                        div().child(title)
                                                    },
                                                )
                                            })
                                            .content_style(
                                                StyleRefinement::default()
                                                    .bg(cx.theme().group_box)
                                                    .border_color(
                                                        crate::design_system::container_border(cx),
                                                    )
                                                    .min_w_0(),
                                            )
                                            .child(body),
                                    )
                                    .into_any_element()
                            })
                            .keywords(
                                [page.label(language), title, description]
                                    .into_iter()
                                    .filter(|k| !k.is_empty())
                                    .collect::<Vec<_>>(),
                            ),
                        )
                });
            SettingPage::new(page.label(language))
                .icon(page.icon())
                .header_style(
                    &StyleRefinement::default()
                        .px(px(visual::SPACE_4))
                        .py(px(visual::SPACE_3))
                        .text_size(px(visual::TEXT_TITLE))
                        .font_semibold(),
                )
                .resettable(false)
                .groups(groups)
        });

        window_shell(cx)
            .child(settings_title_bar(language, cx))
            .child(
                div().flex_1().min_h_0().min_w_0().child(
                    Settings::new(if language == LanguagePreference::Chinese {
                        "clipboard-settings-zh"
                    } else {
                        "clipboard-settings-en"
                    })
                    .default_selected_index(SelectIndex {
                        page_ix: self.page as usize,
                        group_ix: None,
                    })
                    .sidebar_width(px(200.))
                    .sidebar_size_range(px(184.)..px(224.))
                    .sidebar_style(&StyleRefinement::default().bg(cx.theme().sidebar))
                    .with_group_variant(GroupBoxVariant::Outline)
                    .pages(pages),
                ),
            )
    }
}
