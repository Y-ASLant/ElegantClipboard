use super::settings_common::{settings_description, settings_stack};
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
        description_chinese: "选择每次唤出历史窗口时的位置",
        description_english: "Choose where the history window opens",
        content: SettingsWindowView::settings_positioning_content,
    },
    Section {
        page: SettingsPage::General,
        chinese: "窗口行为",
        english: "Window behavior",
        description_chinese: "控制窗口大小记忆及搜索和隐藏行为",
        description_english: "Control size memory, search and hide behavior",
        content: SettingsWindowView::settings_behavior_content,
    },
    Section {
        page: SettingsPage::General,
        chinese: "开机启动",
        english: "Startup",
        description_chinese: "登录系统后自动启动",
        description_english: "Launch automatically after signing in",
        content: SettingsWindowView::settings_startup_content,
    },
    Section {
        page: SettingsPage::Display,
        chinese: "列表显示",
        english: "List display",
        description_chinese: "调整分类栏和卡片间距",
        description_english: "Adjust filters and card spacing",
        content: SettingsWindowView::settings_display_content,
    },
    Section {
        page: SettingsPage::Display,
        chinese: "悬停预览",
        english: "Hover preview",
        description_chinese: "分别控制内容类型、信息标签、延时、位置和图片缩放",
        description_english: "Choose content types, detail chips, delay, position and image zoom",
        content: SettingsWindowView::settings_hover_content,
    },
    Section {
        page: SettingsPage::Theme,
        chinese: "外观与语言",
        english: "Appearance & language",
        description_chinese: "选择界面语言、明暗主题和主题色",
        description_english: "Choose the interface language, theme, and theme color",
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
        chinese: "瞬时内容过滤",
        english: "Transient filtering",
        description_chinese: "过滤 300ms 内被撤销或覆盖的瞬时复制",
        description_english: "Filter copies undone or replaced within 300ms",
        content: SettingsWindowView::settings_transient_filter_content,
    },
    Section {
        page: SettingsPage::Data,
        chinese: "去重与剪贴板",
        english: "Deduplication",
        description_chinese: "选择相同内容的去重策略，以及粘贴后是否恢复原剪贴板",
        description_english: "Choose how duplicate copies are merged and whether pasting restores the clipboard",
        content: SettingsWindowView::settings_dedup_content,
    },
    Section {
        page: SettingsPage::Data,
        chinese: "保留策略",
        english: "Retention",
        description_chinese: "按分组限制普通历史数量和天数；收藏与置顶保留",
        description_english: "Limit ordinary history by group, count and age; keep favorites and pinned items",
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
        description_chinese: "查看和管理历史记录与图片的存储空间",
        description_english: "View and manage storage for history and images",
        content: SettingsWindowView::settings_storage_content,
    },
    Section {
        page: SettingsPage::Data,
        chinese: "导入与恢复",
        english: "Import & restore",
        description_chinese: "将旧版数据或备份恢复到独立空目录",
        description_english: "Restore legacy data or a backup to a separate empty folder",
        content: SettingsWindowView::settings_import_content,
    },
    Section {
        page: SettingsPage::Data,
        chinese: "隐私清理",
        english: "Privacy",
        description_chinese: "永久删除全部剪贴板历史",
        description_english: "Permanently delete all clipboard history",
        content: SettingsWindowView::settings_privacy_content,
    },
    Section {
        page: SettingsPage::AppFilter,
        chinese: "来源应用过滤",
        english: "Source app filter",
        description_chinese: "按来源应用决定是否保存新复制的内容",
        description_english: "Choose which source apps can add new history",
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
        description_chinese: "用于打开剪贴板窗口的全局快捷键",
        description_english: "Global shortcut used to open the clipboard",
        content: SettingsWindowView::settings_shortcut_content,
    },
    Section {
        page: SettingsPage::Shortcuts,
        chinese: "快速粘贴",
        english: "Quick paste",
        description_chinese: "默认关闭；启用后可从其他应用直接粘贴当前分组排在前面的记录",
        description_english: "Off by default; enable to paste top items in the selected group from another app",
        content: SettingsWindowView::settings_quick_paste_content,
    },
    Section {
        page: SettingsPage::About,
        chinese: "应用简介",
        english: "Overview",
        description_chinese: "跨平台剪贴板管理器",
        description_english: "Cross-platform clipboard manager",
        content: SettingsWindowView::settings_about_intro_content,
    },
    Section {
        page: SettingsPage::About,
        chinese: "作者与项目",
        english: "Author & project",
        description_chinese: "在浏览器中打开项目主页或提交问题",
        description_english: "Open the project page or report an issue in your browser",
        content: SettingsWindowView::settings_about_details_content,
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
            .text_size(px(visual::TEXT_BODY))
            .font_semibold()
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
                    SettingGroup::new()
                        .title(title)
                        .variant(GroupBoxVariant::Normal)
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
                                visual::reveal(
                                    div().w_full().min_w_0().child(
                                        GroupBox::new()
                                            .id(("settings-card", section_ix))
                                            .outline()
                                            .content_style(
                                                StyleRefinement::default()
                                                    .bg(cx.theme().group_box)
                                                    .border_color(
                                                        crate::design_system::container_border(cx),
                                                    )
                                                    .min_w_0(),
                                            )
                                            .child(
                                                settings_stack()
                                                    .child(settings_description(description, cx))
                                                    .child(content),
                                            ),
                                    ),
                                    ("settings-section-reveal", section_ix),
                                    cx,
                                )
                            })
                            .keywords([
                                page.label(language),
                                title,
                                description,
                            ]),
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
                div()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .text_size(px(visual::TEXT_BODY))
                    .child(
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
