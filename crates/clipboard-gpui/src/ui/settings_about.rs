use super::*;
use gpui_kit::component::alert::Alert;

/// 「关于」页特性分组。`items` 与 README 的「当前功能」逐条对应，
/// 保证应用内展示与项目说明不脱节；`en` 文案直接取自 README_EN.md。
struct FeatureGroup {
    chinese: &'static str,
    english: &'static str,
    items: &'static [(&'static str, &'static str)],
}

const FEATURE_GROUPS: &[FeatureGroup] = &[
    FeatureGroup {
        chinese: "剪贴板历史",
        english: "Clipboard history",
        items: &[
            (
                "采集和检索文本、URL、HTML/RTF、图片及文件路径",
                "Capture and search text, URLs, HTML/RTF, images, and file paths",
            ),
            (
                "文本编辑、富文本/图片/文件预览、单张图片文件卡片缩略图与失效来源提示，以及复制、纯文本复制和自动粘贴",
                "Edit text, preview rich text/images/files, see image thumbnails and missing-source hints, plus copy, copy as plain text, and auto-paste",
            ),
            (
                "历史列表较长时，固定信息栏提供返回顶部入口",
                "A pinned footer bar offers a back-to-top entry for long history lists",
            ),
        ],
    },
    FeatureGroup {
        chinese: "整理与分组",
        english: "Organize and group",
        items: &[
            (
                "收藏、置顶、通过 Select 选择分组并在 GPUI Kit Dialog 中新建或重命名分组（支持删除和上移/下移排序）、通过卡片两侧区域拖拽排序、批量删除与历史清理；拖动区域提示可在显示设置中隐藏",
                "Favorite and pin, choose a group via Select, create or rename groups in a GPUI Kit Dialog (delete and reorder supported), drag cards from their side areas to reorder, batch delete and clear history; the drag-area hint can be hidden in display settings",
            ),
            (
                "卡片预览行数、绝对/相对时间、字符数、大小和来源应用信息可在设置窗口配置；来源应用可按名称、图标或两者显示，图标缓存为 PNG",
                "Configure card preview lines, absolute/relative time, character count, size, and source-app info in the settings window; source apps can show by name, icon, or both, with icons cached as PNG",
            ),
            (
                "设置页可隐藏分类筛选栏，并选择紧凑、标准或宽松的历史卡片密度。默认两行文字预览较图片卡片更矮；正文或居中的图片缩略图在上，时间、大小和来源应用在下，悬停卡片显示操作按钮",
                "Hide the category filter bar in settings and pick compact, standard, or spacious card density. Text cards default to two preview lines and are shorter than image cards; text or centered image thumbnail on top, time/size/source app below, hover for action buttons",
            ),
        ],
    },
    FeatureGroup {
        chinese: "采集规则",
        english: "Capture rules",
        items: &[
            (
                "设置窗口可分别启停文本、网址、HTML、RTF、图片和文件的历史采集，至少保留一种类型",
                "Enable or disable history capture for text, URLs, HTML, RTF, images, and files independently in the settings window, keeping at least one type",
            ),
            (
                "来源应用过滤支持黑名单、白名单、手工规则及运行中应用选择，规则可按应用名、进程名或路径匹配并支持 `*`、`?`",
                "Source-app filtering supports blacklist, whitelist, manual rules, and picking running apps; rules match on app name, process name, or path and support `*` and `?`",
            ),
            (
                "可分别配置复制与受控粘贴音效的开关、立即或成功后播放，并在设置页试听",
                "Configure copy and controlled-paste sounds independently, choose immediate or after-success playback, and preview them in settings",
            ),
        ],
    },
    FeatureGroup {
        chinese: "快捷键与粘贴",
        english: "Shortcuts and paste",
        items: &[
            (
                "全局快捷键显隐切换、系统托盘、开机启动、暂停采集和单实例唤回",
                "Global shortcut to toggle visibility, system tray, autostart, pause capture, and single-instance wake-up",
            ),
            (
                "快速粘贴快捷键默认关闭，可在设置的“快捷键”页按需启用或再次关闭；单个槽位支持录制或输入组合键，两组各 10 个槽位可批量停用或恢复默认。启用后预置 Alt+1～Alt+0 粘贴当前分组前十条记录，Ctrl+Alt+1～3 粘贴前三条收藏记录，数字键支持数字小键盘；自动粘贴按键可选 Ctrl+V 或 Shift+Insert",
                "Quick-paste shortcuts are off by default and can be enabled or disabled again on the Shortcuts settings page. Each slot supports recording or typing a key combination, and the two 10-slot groups can be batch-disabled or reset. When enabled, Alt+1 through Alt+0 paste the top ten items of the current group and Ctrl+Alt+1 through 3 paste the first three favorites; numpad digits work, and the auto-paste key can be Ctrl+V or Shift+Insert",
            ),
        ],
    },
    FeatureGroup {
        chinese: "悬停预览",
        english: "Hover preview",
        items: &[(
            "可配置的独立悬停预览：图片、文本、网址及 HTML/RTF 默认开启，文件可单独开启；文本浮窗按延时读取完整正文并可滚动。支持配置延时、位置、图片缩放步进和按钮，以及按图片尺寸扩大浮窗；已保存的关闭设置保持生效。图片浮窗支持滚轮无极缩放和左键拖拽平移，按住标题可拖动整个浮窗，浮窗出现后固定 5 秒自动关闭",
            "A configurable standalone hover preview: images, text, URLs, and HTML/RTF are on by default while files can be enabled separately. The text popup reads the full body after a delay and scrolls. Delay, position, image zoom step and buttons, and enlarging the popup by image size are all configurable, and saved dismiss choices keep working. Image popups support continuous wheel zoom and left-button drag-to-pan, the whole popup can be dragged by its title, and it closes itself five seconds after appearing",
        )],
    },
    FeatureGroup {
        chinese: "新手引导",
        english: "Onboarding",
        items: &[(
            "新数据目录首次打开时提供四步中文/英文引导，介绍采集、搜索、收藏与快捷键，可跳过或按 Esc 结束",
            "A four-step bilingual onboarding on first open with a new data directory, introducing capture, search, favorites, and shortcuts; it can be skipped or ended with Esc",
        )],
    },
    FeatureGroup {
        chinese: "窗口与界面",
        english: "Window and interface",
        items: &[
            (
                "简体中文/English 界面（默认简体中文）、明暗主题、窗口置顶、点击外部隐藏、唤出位置，以及粘贴后窗口显隐和排序设置",
                "Simplified Chinese/English UI (Chinese by default), light and dark themes, always-on-top, hide on outside click, summon position, and post-paste visibility and ordering settings",
            ),
            (
                "无顶部标题栏的主窗口：按住搜索框上方的浅色短横握柄移动窗口，顶部边缘仍可缩放；搜索框独占一行，分类标签与 GPUI Kit 分组 Select 同行；从卡片“选择”或列表 Ctrl+A 进入多选，使用批量操作栏退出；清理历史、窗口置顶和设置入口位于托盘菜单",
                "A main window with no title bar: drag the light grip above the search field to move the window while the top edge stays resizable. The search field occupies its own row and category tabs share a row with the GPUI Kit group Select. Enter multi-select from a card's Select or the list's Ctrl+A and leave via the batch action bar; clear history, always-on-top, and settings live in the tray menu",
            ),
            (
                "常规设置可选择从托盘菜单清理当前分组历史时跳过确认，默认仍要求确认",
                "General settings can skip the confirmation when clearing the current group's history from the tray menu, though confirmation remains the default",
            ),
            (
                "主窗口、设置窗口与悬停预览浮窗支持 Windows 系统材质：可选关闭、亚克力、云母或云母 Alt，磨砂浓度可选 40%/55%/72%/85%。开启后窗口改用无边框 + DWM 系统背板，侧边栏、卡片与标签一并半透明，窗口边缘仍可缩放；仅 Windows 10 1809 及以上生效",
                "Windows system materials for the main window, settings window, and hover preview popup: off, Acrylic, Mica, or Mica Alt, with a 40%/55%/72%/85% frost opacity. Enabling one switches the window to a borderless frame with a DWM system backdrop and makes the sidebar, cards, and tabs translucent while keeping the edge resizable; requires Windows 10 1809 or later",
            ),
        ],
    },
    FeatureGroup {
        chinese: "备份与数据",
        english: "Backup and data",
        items: &[(
            "GPUI ZIP 备份与恢复，并可导入旧数据库或旧版 ZIP 备份",
            "GPUI ZIP backup and restore, plus importing legacy databases or older ZIP backups",
        )],
    },
];

impl SettingsWindowView {
    pub(super) fn settings_about_details_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_entity) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let language = owner_entity.read(cx).language;

        div()
            .flex()
            .flex_col()
            .gap_2()
            .children(
                [
                    (
                        "about-author",
                        tr(language, "作者", "Author"),
                        "ASLant",
                        IconName::User,
                        ProjectLink::Author,
                    ),
                    (
                        "about-repository",
                        "GitHub",
                        "ElegantClipboard",
                        IconName::Github,
                        ProjectLink::Repository,
                    ),
                    (
                        "about-issues",
                        tr(language, "反馈", "Feedback"),
                        tr(language, "提交问题", "Submit issue"),
                        IconName::TriangleAlert,
                        ProjectLink::Issues,
                    ),
                ]
                .map(|(id, caption, label, icon, link)| {
                    div()
                        .w_full()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(caption),
                        )
                        .child(
                            Button::new(id)
                                .small()
                                .ghost()
                                .icon(icon)
                                .label(label)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.link_error =
                                        links::open(link).err().map(|error| error.to_string());
                                    cx.notify();
                                })),
                        )
                }),
            )
            .when_some(self.link_error.clone(), |panel, error| {
                panel.child(Alert::error("about-link-error", error).small())
            })
            .into_any_element()
    }

    pub(super) fn settings_about_intro_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_entity) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let language = owner_entity.read(cx).language;

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(
                        language,
                        "使用 Rust、GPUI 和 GPUI Kit 构建的 Windows 原生剪贴板管理器。设置窗口按常规、显示、外观、数据、应用过滤、音效、快捷键和关于分类，本节与项目 README 的特性清单保持一致。",
                        "A native Windows clipboard manager built with Rust, GPUI, and GPUI Kit. The settings window is grouped into General, Display, Appearance, Data, App filter, Audio, Shortcuts, and About; this section mirrors the feature list in the project README.",
                    )),
            )
            .children(FEATURE_GROUPS.iter().map(|group| {
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(language, group.chinese, group.english)),
                    )
                    .children(group.items.iter().map(|(chinese, english)| {
                        div()
                            .flex()
                            .items_start()
                            .gap_2()
                            .child(
                                Icon::new(IconName::CircleCheck)
                                    .small()
                                    .text_color(cx.theme().primary)
                                    .mt(px(3.)),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .text_sm()
                                    .child(tr(language, chinese, english)),
                            )
                    }))
            }))
            .into_any_element()
    }
}
