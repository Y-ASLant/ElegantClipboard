use super::settings_common::{settings_section_title, settings_stack};
use super::*;
use gpui_kit::component::{
    group_box::{GroupBox, GroupBoxVariant, GroupBoxVariants},
    setting::{SelectIndex, SettingGroup, SettingItem, SettingPage, Settings},
};

impl SettingsPage {
    pub(super) const ALL: [Self; 9] = [
        Self::General,
        Self::Display,
        Self::Todos,
        Self::Theme,
        Self::Data,
        Self::AppFilter,
        Self::Audio,
        Self::Shortcuts,
        Self::About,
    ];

    pub(super) fn label(self, language: LanguagePreference) -> &'static str {
        match self {
            Self::General => tr(language, "常规", "General"),
            Self::Display => tr(language, "显示", "Display"),
            Self::Todos => tr(language, "备忘录", "Memos"),
            Self::Theme => tr(language, "外观", "Appearance"),
            Self::Data => tr(language, "数据", "Data"),
            Self::AppFilter => tr(language, "应用过滤", "App filter"),
            Self::Audio => tr(language, "音效", "Audio"),
            Self::Shortcuts => tr(language, "快捷键", "Shortcuts"),
            Self::About => tr(language, "关于", "About"),
        }
    }

    pub(super) fn icon(self) -> IconName {
        match self {
            Self::General => IconName::Settings,
            Self::Display => IconName::Eye,
            Self::Todos => IconName::CircleCheck,
            Self::Theme => IconName::Palette,
            Self::Data => IconName::HardDrive,
            Self::AppFilter => IconName::Search,
            Self::Audio => IconName::Bell,
            Self::Shortcuts => IconName::SquareTerminal,
            Self::About => IconName::Info,
        }
    }
}

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
        chinese: "主页入口",
        english: "Home tab",
        description_chinese: "隐藏备忘录入口不会删除已有内容。",
        description_english: "Hiding the memo tab does not delete existing memos.",
        content: SettingsWindowView::settings_todo_tab_content,
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
        description_chinese: "开启后，添加备忘录或完成待办会在屏幕右下角弹出，3 秒后消失，也可点关闭。不进入主窗口。",
        description_english: "After adding a memo or completing a to-do, a toast appears at the bottom-right of the screen. It closes after 3 seconds or when dismissed, and does not appear inside the main window.",
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
        description_chinese: "",
        description_english: "",
        content: SettingsWindowView::settings_capture_formats_content,
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
        description_chinese: "导入/恢复写入当前保存位置（可先在「本地数据」更改位置），会替换该处历史并重启。旧版 ZIP 只导入历史与媒体；GPUI 备份含设置。「重置应用」恢复默认设置，并将保存位置迁回默认目录；不删历史。",
        description_english: "Import/restore writes into the current save location (change it under Local data first if needed), replaces history there, and restarts. Legacy ZIP: history and media only. GPUI backup includes settings. Reset app restores default settings and moves the save location back to the default folder; history is kept.",
        content: SettingsWindowView::settings_import_content,
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
                owner.display.value.card_max_lines,
                owner.display.pending,
            )
        };
        self.preview_lines
            .sync(i32::from(preview_lines), display_pending, window, cx);
        let (opacity, opacity_pending, delay_ms, delay_pending, zoom_step, zoom_pending) = {
            let owner = owner.read(cx);
            (
                i32::from(owner.window_effect.value.opacity),
                owner.window_effect.pending,
                i32::from(owner.hover_preference.value.delay_ms),
                owner.hover_preference.pending,
                i32::from(owner.hover_preference.value.zoom_step),
                owner.hover_preference.pending,
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
                    // title 留给侧栏；视觉标题画在 GroupBox 上，便于标题后挂 ⓘ。
                    let group = SettingGroup::new().variant(GroupBoxVariant::Normal);
                    let group = if title.is_empty() {
                        group
                    } else {
                        group.title(title)
                    };
                    group
                        .text_size(px(0.))
                        .line_height(relative(0.))
                        .py(px(0.))
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
                                let help = (!description.is_empty()).then(|| description.into());
                                let heading = (!title.is_empty()).then(|| {
                                    settings_section_title(
                                        title,
                                        help,
                                        ("settings-section-help", section_ix),
                                        cx,
                                    )
                                });
                                // 页面内容直接渲染，切换页面不做淡入动效。
                                let base_style = StyleRefinement::default()
                                    .bg(cx.theme().group_box)
                                    .border_color(crate::design_system::container_border(cx))
                                    .min_w_0();
                                let card_style = if page == SettingsPage::About {
                                    // 关于页无分组标题，底边距减半。
                                    base_style
                                        .pt(px(visual::SPACE_4))
                                        .px(px(visual::SPACE_4))
                                        .pb(px(visual::SPACE_2))
                                } else {
                                    base_style
                                };
                                let card = GroupBox::new()
                                    .id(("settings-card", section_ix))
                                    .outline()
                                    .content_style(card_style)
                                    .when_some(heading, |box_, title_row| box_.title(title_row))
                                    .child(settings_stack().child(content));
                                div().w_full().min_w_0().child(card).into_any_element()
                            })
                            .keywords({
                                let mut keys = vec![page.label(language), title, description];
                                if section.chinese == "本地数据" {
                                    keys.extend([
                                        "删除全部历史",
                                        "Delete all history",
                                        "隐私",
                                        "Privacy",
                                    ]);
                                }
                                keys.into_iter()
                                    .filter(|k| !k.is_empty())
                                    .collect::<Vec<_>>()
                            }),
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
