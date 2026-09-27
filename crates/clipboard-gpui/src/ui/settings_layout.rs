use super::*;
use gpui_kit::component::{
    group_box::GroupBoxVariant,
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
        description_chinese: "登录 Windows 后自动启动",
        description_english: "Launch automatically after signing in to Windows",
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
        description_chinese: "分别控制内容类型、延时、位置和图片缩放",
        description_english: "Choose content types, delay, position and image zoom",
        content: SettingsWindowView::settings_hover_content,
    },
    Section {
        page: SettingsPage::Theme,
        chinese: "外观与语言",
        english: "Appearance & language",
        description_chinese: "选择界面语言和明暗主题",
        description_english: "Choose the interface language and theme",
        content: SettingsWindowView::settings_appearance_content,
    },
    Section {
        page: SettingsPage::Data,
        chinese: "监听内容类型",
        english: "Capture content types",
        description_chinese: "选择保存到历史的内容类型；至少保留一种",
        description_english: "Choose which content types enter history; keep at least one",
        content: SettingsWindowView::settings_monitor_content,
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
        description_chinese: "数据库、受管图片和暂存文件",
        description_english: "Database, managed images and staged files",
        content: SettingsWindowView::settings_storage_content,
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
        chinese: "关于 ElegantClipboard",
        english: "About ElegantClipboard",
        description_chinese: "Windows 原生 GPUI 剪贴板管理器",
        description_english: "Native GPUI clipboard manager for Windows",
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
    view.settings_audio_content(true, cx)
}

fn paste_audio_content(
    view: &SettingsWindowView,
    cx: &mut Context<SettingsWindowView>,
) -> AnyElement {
    view.settings_audio_content(false, cx)
}

impl Render for SettingsWindowView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(owner) = self.owner.upgrade() else {
            return div()
                .size_full()
                .child("The main window is no longer available");
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
                .filter(|section| section.page == page)
                .map(|section| {
                    let title = tr(language, section.chinese, section.english);
                    let description = tr(
                        language,
                        section.description_chinese,
                        section.description_english,
                    );
                    let settings = settings.clone();
                    SettingGroup::new()
                        .title(title)
                        .description(description)
                        .item(
                            SettingItem::render(move |_, _, cx| {
                                div().w_full().min_w_0().child(
                                    settings
                                        .update(cx, |view, cx| {
                                            if view.page != page {
                                                view.page = page;
                                                view.shortcut_recording = false;
                                                view.shortcut_editing = None;
                                                view.shortcut_capture_error = None;
                                            }
                                            (section.content)(view, cx)
                                        })
                                        .unwrap_or_else(|_| div().into_any_element()),
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
                .resettable(false)
                .groups(groups)
        });

        window_shell(cx)
            .font_family("Microsoft YaHei UI")
            .child(
                TitleBar::new().bg(cx.theme().background).child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .child(tr(language, "设置", "Settings")),
                ),
            )
            .child(
                div().flex_1().min_h_0().child(
                    Settings::new(if language == LanguagePreference::Chinese {
                        "clipboard-settings-zh"
                    } else {
                        "clipboard-settings-en"
                    })
                    .default_selected_index(SelectIndex {
                        page_ix: self.page as usize,
                        group_ix: None,
                    })
                    .sidebar_width(px(224.))
                    .sidebar_size_range(px(212.)..px(280.))
                    .with_group_variant(GroupBoxVariant::Outline)
                    .pages(pages),
                ),
            )
            .children(Root::render_dialog_layer(window, cx))
    }
}
