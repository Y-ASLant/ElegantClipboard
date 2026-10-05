# ElegantClipboard 更新日志

记录口径：每版记录上一实际版本标签到本版标签的最终净变化，同一功能的开发过程合并为用户可见的结果；不把标签之后的改动计入该版。各版比较链接对应相邻标签，历史行为及默认值以当时版本为准。原有历史发布日期沿用旧记录；标为「标签日期」的日期仅表示 Git 标签时间，不代表 GitHub Release 的发布时间。

## v1.2.9
**标签日期：** 2026年10月5日  
**版本比较：** [v1.2.8 → v1.2.9](https://github.com/Y-ASLant/ElegantClipboard/compare/v1.2.8...v1.2.9)

- fix: 文件和缓存图片在卡片加载、窗口再次显示及打开右键菜单时重新检查资源；超出预览大小限制也不再跳过有效性检查。原文件失效时可使用仍存在的暂存副本，资源确实不可用时显示「已失效」并禁用依赖该资源的操作。
- fix: 复查和虚拟滚动重新挂载时保留上次状态标签，减少「已失效／检测中」闪烁，但检查期间不使用旧结果放行资源操作；检查失败显示未知状态，过期响应不覆盖新条目状态。
- fix: 预览加载或解码失败与文件失效分开提示；文件仍存在时可以另存为或定位，不再因预览失败而禁用这些操作。
- fix: 复制、粘贴、路径粘贴、合并粘贴和快捷键在修改剪贴板、隐藏窗口或发送按键前检查内容与资源。合并中缺失或损坏的条目不再被静默省略；另存为和资源管理器定位重新解析条目的当前资源，而非使用界面缓存路径。
- fix: Windows 不支持写回的空原始格式、空虚拟文件内容及不支持的多虚拟文件合并会在写入前明确报错，不再报告虚假成功。资源存在与剪贴板格式可写回是独立判断，不支持复制格式的已有文件仍可保存或定位。
- fix: 等待、失败和取消不再显示成功反馈；另存为取消单独处理，键盘删除失败保留选择。主操作已成功但后续排序或刷新失败时分别提示，不把已完成的操作改报失败。
- fix: 用户操作错误提供简体中文、繁体中文和英文原因；原生诊断细节不直接展示，后台和已有行内错误不重复弹窗。快捷键失败在可见主窗口提示，窗口隐藏时使用系统通知。
- fix: 操作出错后仍恢复剪贴板监控；粘贴按键发送失败时恢复原本可见的窗口，不抢焦点或改变锁定状态。
- fix: 修复 Windows DIB V4/V5 图片解码，保留位图头、颜色掩码和调色板信息；真实剪贴板访问错误不再被当作成功或空内容吞掉。
- feat: 关于页新增三语「全新 2.0 重构版本 · GPUI」链接，通过默认浏览器打开 [GPUI 分支](https://github.com/Y-ASLant/ElegantClipboard/tree/gpui)。本版仍为 Tauri + React 1.x，不切换应用架构。

## v1.2.8
**标签日期：** 2026年10月2日  
**版本比较：** [v1.2.7 → v1.2.8](https://github.com/Y-ASLant/ElegantClipboard/compare/v1.2.7...v1.2.8)

- fix: 修复 Shift+Insert 粘贴中 Insert 扩展键标记缺失导致的 Shift 状态丢失；检查按键注入结果，失败时释放本次补按的修饰键。https://github.com/Y-ASLant/ElegantClipboard/issues/149
- fix: 浏览器复制单张图片时按图片记录，带文字的富文本仍优先保留富文本。https://github.com/Y-ASLant/ElegantClipboard/issues/163
- fix: 剪贴板捕获按顺序处理独立内容，不再把连续复制合并丢弃；重试时重新核对来源、暂停状态与剪贴板序列，避免保存不稳定快照。异常 CF_HTML 偏移切入 UTF-8 字符时返回格式错误，不再导致程序退出。
- fix: 捕获后刷新权威轻量列表，同步已淘汰记录与当前组合筛选；旧请求不污染新视图，完整正文不再随捕获结果长期驻留列表。
- fix: 历史数量清理只清理实际删除记录关联的图片与暂存文件，保留其它分组或受保护条目仍引用的资源。
- fix: 数据导入完整解包并验证 SQLite 后再提交数据库和资产，打开失败或安装中断时恢复原数据；导出包含嵌套暂存文件。
- fix: 数据目录迁移采用一致数据库快照，重启时补齐最终快照、成功打开目标库后才切换配置，并重定位托管资产路径。
- fix: WebDAV 纯文本同步不再上传图片、文件和应用图标；媒体索引读取失败时停止更新和清理，拒绝非法哈希、扩展名和文件名形成的越界路径。
- fix: 应用内更新使用标准 UAC 提权启动安装器，取消授权或启动失败时保留当前会话。https://github.com/Y-ASLant/ElegantClipboard/issues/156
- fix: Scoop 更新保留用户数据目录。https://github.com/Y-ASLant/ElegantClipboard/issues/153
- chore: 更新前后端依赖及安全修复。
- chore: 配套 Tauri 2.12 同步 Windows/windows-core 0.62 与 webview2-com 0.39，避免不同 COM 绑定版本造成类型不兼容；上游依赖更新移除 unic-* 停止维护告警
- fix: 自定义 NSIS 模板适配新版 Tauri 的 Restart Manager 进程检查，加载所需宏并传入安装目录下可执行文件的完整路径，修复安装包生成失败

## v1.2.7
**发布日期：** 2026年8月9日  
**版本比较：** [v1.2.6 → v1.2.7](https://github.com/Y-ASLant/ElegantClipboard/compare/v1.2.6...v1.2.7)

- fix: WebView2 运行时更新或断连导致设置、翻译、预览和编辑窗口失联时，通过限流自动重启恢复。https://github.com/Y-ASLant/ElegantClipboard/issues/147
- fix: 悬停预览中的 Ctrl+滚轮只滚动文本预览或缩放图片，不再同时滚动剪贴板列表。https://github.com/Y-ASLant/ElegantClipboard/issues/138
- fix: 不允许同时隐藏系统托盘和工具栏设置入口；旧版已失去设置入口的配置在启动时自动恢复设置按钮。https://github.com/Y-ASLant/ElegantClipboard/issues/150

## v1.2.6
**发布日期：** 2026年7月26日  
**版本比较：** [v1.2.5 → v1.2.6](https://github.com/Y-ASLant/ElegantClipboard/compare/v1.2.5...v1.2.6)

- fix: WebDAV 按大小限制同步时不再让其它设备看到没有内容的破损条目；校验媒体完整性，拒绝无对应文件的孤立记录。https://github.com/Y-ASLant/ElegantClipboard/issues/132
- fix: 同步媒体下载完成后自动刷新列表，无需重启即可看到图片预览。
- fix: 远程桌面（mstsc、RustDesk 等）复制文件时正确显示文件名。
- fix: 系统托盘菜单及设置窗口标题随语言切换更新，切换语言不再丢失监控暂停或快捷键禁用状态。

## v1.2.5
**发布日期：** 2026年7月26日  
**版本比较：** [v1.2.4 → v1.2.5](https://github.com/Y-ASLant/ElegantClipboard/compare/v1.2.4...v1.2.5)

- fix: 文件类型的单张图片预览应用大小限制：本地默认 50MiB，UNC／网络路径固定 10MiB；本地设为无限制时仍有 100MiB 预览安全上限，避免 NAS 超大图片启动时反复整文件读取造成网络占用暴涨或闪退。该版对已知超限文件也跳过批量状态检查。https://github.com/Y-ASLant/ElegantClipboard/issues/143
- perf: 文件图片预览失败结果在会话内保留，避免重复加载失败资源。
- fix: 图片缓存不再生成遗留 `.dib` 伴侣文件，启动时清理旧版 `.dib`，解决清空历史后仍长期占用磁盘的问题。https://github.com/Y-ASLant/ElegantClipboard/issues/142
- fix: 子菜单支持边界避让；视口变窄时关闭右键菜单，缓解菜单位置异常。
- fix: RTF 去重忽略易变字段，减少同一富文本被重复记录。
- chore: 更新 `crossbeam-epoch` 安全依赖。

## v1.2.4
**发布日期：** 2026年7月18日  
**版本比较：** [v1.2.3 → v1.2.4](https://github.com/Y-ASLant/ElegantClipboard/compare/v1.2.3...v1.2.4)

- feat: 设置 → 快捷键新增粘贴按键选择，支持 Ctrl+V（默认）和 Shift+Insert。https://github.com/Y-ASLant/ElegantClipboard/issues/134
- feat: 清空历史确认框新增「不再询问」，开启后直接清空并显示结果提示。https://github.com/Y-ASLant/ElegantClipboard/issues/133
- feat: 系统托盘新增「检查更新」。
- feat: WebDAV 显示最近同步时间，手动同步完成后自动清理状态提示；避免并发重复同步，图片和文件可分别限制大小并独立同步。
- fix: 修复跨设备同步媒体的落地路径，不再要求两端数据目录一致。
- fix: 修复配置未加载完就错误显示新手引导，以及设置页主题预览色块异常。

## v1.2.3
**发布日期：** 2026年7月5日  
**版本比较：** [v1.2.0 → v1.2.3](https://github.com/Y-ASLant/ElegantClipboard/compare/v1.2.0...v1.2.3)

- feat: 列表新增支持虚拟滚动的瀑布流布局。https://github.com/Y-ASLant/ElegantClipboard/issues/126
- fix: 文本与图片预览实时同步主窗口主题、圆角和字体，修复文本预览背景与主窗口卡片不一致；统一界面圆角、阴影及悬停过渡。
- perf: 设置页使用独立入口，主窗口按需加载编辑和翻译页面，减少启动及打开设置时不必要的加载。
- chore: 悬停预览默认延迟由 150ms 调整为 128ms，粘贴后移至顶部默认开启。

## v1.2.0
**发布日期：** 2026年7月4日  
**版本比较：** [v1.1.6 → v1.2.0](https://github.com/Y-ASLant/ElegantClipboard/compare/v1.1.6...v1.2.0)

- feat: 完善条目级自定义分组、组内排序和分组筛选，导入导出包含分组数据。
- feat: 文件剪贴板保留 CF_HDROP 伴生格式和暂存副本，支持文件内容还原与合并粘贴；批量检查文件有效性。
- feat: 本地 clipboard-rs 分叉增加 CF_DIB 图片读写；RTF 备份和恢复使用原始字节，避免格式丢失。
- feat: 工具栏新增 WebDAV 同步按钮，WebDAV／翻译入口随插件启用状态更新。
- feat: 粘贴音效覆盖普通粘贴、合并粘贴、快捷键和路径粘贴，设置页支持试听。
- fix: 数据迁移移除旧内容类型约束，避免历史异常数据导致迁移失败。https://github.com/Y-ASLant/ElegantClipboard/issues/125
- fix: 关闭 URL 监听时不再将 URL 当作普通文本捕获；扩展图片哈希，编辑为文本时清除旧图片或文件路径，重复条目更新不再覆盖创建时间。
- fix: 修复翻译失败通知和翻译窗口初始白屏，统一设置卡片及切换布局。

## v1.1.6
**标签日期：** 2026年7月2日  
**版本比较：** [v1.1.5 → v1.1.6](https://github.com/Y-ASLant/ElegantClipboard/compare/v1.1.5...v1.1.6)

- fix: 减少 Firefox／Zen 等应用短时间重复触发剪贴板事件造成的重复记录；「始终新建」策略仍允许重复内容。
- fix: 修复旧表迁移时内容类型约束不接受 URL 的问题；URL 条目支持文本悬浮预览和编辑。

## v1.1.5
**发布日期：** 2026年7月1日  
**版本比较：** [v1.1.0 → v1.1.5](https://github.com/Y-ASLant/ElegantClipboard/compare/v1.1.0...v1.1.5)

- refactor: 剪贴板读写与监听统一使用 clipboard-rs，内容处理移至独立串行工作线程；新捕获条目增量更新列表，减少全量刷新。
- fix: RTF 按 base64 保存原始字节，改善多格式写回的保真性。
- perf: 图片按元数据预留显示高度，减少加载时布局跳动；图片自动高度默认关闭，并改善失效文件标记和回到顶部交互。
- feat: 日志路径可点击打开；翻译错误及选项标签支持多语言。
- fix: 管理员模式切回普通权限时同步启动新实例，避免单实例检查冲突。
- fix: WebDAV 设置无变更时不重复保存；自动同步线程无法启动时记录错误，不再因此触发程序异常退出。
- fix: 数据库语义哈希回填仅在对应迁移时执行，避免后续启动重复处理。

## v1.1.0
**发布日期：** 2026年6月30日  
**版本比较：** [v1.0.3 → v1.1.0](https://github.com/Y-ASLant/ElegantClipboard/compare/v1.0.3...v1.1.0)

- feat: 支持 RTF 读取和写回，富文本可同时写入 HTML、RTF 和 Unicode 文本；本版剪贴板操作统一使用 arboard。
- fix: 划词翻译支持选中内容与当前剪贴板相同的情况；翻译前后的剪贴板备份恢复包含 HTML 和图片，不再丢失富文本。
- fix: 剪贴板读取期间检测序列变化并重试，避免读取被其它应用改写的内容。
- fix: 修复管理员模式下托盘重启或导入后重启产生双实例，以及设置加载失败后自动保存永久关闭的问题。
- fix: 修复便携包发布遗漏；窗口显示复用定位设置，减少重复查询，并调整翻译结果操作栏和复制成功反馈。

## v1.0.3
**发布日期：** 2026年6月30日  
**版本比较：** [v1.0.2 → v1.0.3](https://github.com/Y-ASLant/ElegantClipboard/compare/v1.0.2...v1.0.3)

- fix: 修复设置页初始化时误触发自动保存的问题。https://github.com/Y-ASLant/ElegantClipboard/issues/119
- feat: 原生托盘菜单支持随界面语言更新。

## v1.0.2
**发布日期：** 2026年6月30日  
**版本比较：** [v1.0.1 → v1.0.2](https://github.com/Y-ASLant/ElegantClipboard/compare/v1.0.1...v1.0.2)

- fix: 修复复制内容时异常闪退的问题。
- feat: 翻译结果窗口支持固定；未固定时失去焦点自动关闭。

## v1.0.1
**发布日期：** 2026年6月29日  
**版本比较：** [v1.0.0 → v1.0.1](https://github.com/Y-ASLant/ElegantClipboard/compare/v1.0.0...v1.0.1)

- fix: 修复鼠标点击条目后再次呼出窗口时键盘导航失效。
- chore: 增加 `0xc0000409` 等原生异常的诊断日志，便于排查闪退。

## v1.0.0
**发布日期：** 2026年6月28日  
**版本比较：** [v0.9.30 → v1.0.0](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.9.30...v1.0.0)

> WebDAV 和翻译插件默认关闭，升级后需按需手动启用。

- feat: 新增 WebDAV 多设备同步（坚果云、Nextcloud 等）及文本翻译插件；翻译支持条目和划词方式，以及 Microsoft、Google、百度、DeepLX、OpenAI 兼容服务等。
- feat: 新增简体中文、英文、繁体中文、新手引导、关于页更新日志，以及系统托盘显示开关。
- feat: 自动识别 URL 类型（http(s)、ftp、www）并迁移存量记录；快速粘贴扩展至 10 个槽位，包含数字 0。
- fix: 修复 0.9.x 升级后无法启动、安装更新失败、开机自启失效，以及管理员模式和同步的相关兼容问题。
- fix: 修复收藏与列表排序跳动、多屏不同 DPI 下的位置和尺寸异常、双屏唤醒首次不居中。
- fix: 超大图片应用大小限制，避免卡死；改善搜索上下文和图片加载，修复搜索偶发白屏及预览无法消失、预览配置初始状态错误。
- fix: Explorer 重启后继续保持托盘隐藏；资源管理器重命名时 Win+V 不再打断输入。修复小键盘快捷键和 Win+V 模式下清空快速粘贴快捷键的问题。
- fix: 修复显示区域提示无法持久保存、卡片间距无效，以及设置 Tab 的滚动位置和导航指示器不同步。
- style: 增加复制成功和列表入场反馈，拖拽使用系统移动光标；调整设置页布局、移除设置内搜索，并改善字体列表加载。

## v0.9.30
**发布日期：** 2026年3月21日  
**版本比较：** [v0.9.27 → v0.9.30](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.9.27...v0.9.30)

- fix: 修复键盘导航进入剪贴板后点击关闭的异常，以及预览窗口偶尔无法消失。
- fix: 唤醒位置记忆在重启后仍有效；更新卡片及更新对话框的显示和交互。

## v0.9.27
**发布日期：** 2026年3月10日  
**版本比较：** [v0.9.19 → v0.9.27](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.9.19...v0.9.27)

- feat: 新增收藏页粘贴快捷键及自定义字体。
- fix: 修复小键盘数字快捷键和键盘导航失效；改善重复内容判断。
- style: 改善拖拽区域提示，澄清有歧义的设置描述。

## v0.9.19
**发布日期：** 2026年3月8日  
**版本比较：** [v0.9.18 → v0.9.19](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.9.18...v0.9.19)

- chore: 更新前后端依赖。

## v0.9.18
**标签日期：** 2026年3月8日  
**版本比较：** [v0.9.16 → v0.9.18](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.9.16...v0.9.18)

- feat: 检查和下载更新读取 Windows 系统代理，改善代理环境下的更新连接。
- fix: 无效的唤醒位置配置回退为跟随光标，避免设置页使用不支持的值。

## v0.9.16
**标签日期：** 2026年3月8日  
**版本比较：** [v0.9.3 → v0.9.16](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.9.3...v0.9.16)

- feat: 新增来源应用过滤及剪贴板内容类型监听选项。
- feat: 提供免安装便携版；新增跟随光标、当前屏幕居中、固定位置三种唤醒定位方式。靠近屏幕底部时向上钳位，不再沿 Y 轴翻转到光标另一侧。
- fix: 修复长时间运行后预览窗口层级异常及粘贴后预览残留；文本预览支持主窗口特效，图片文件名可选择显示。

## v0.9.3
**发布日期：** 2026年3月7日  
**版本比较：** [v0.9.0 → v0.9.3](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.9.0...v0.9.3)

- feat: 新增批量选择和删除，支持 Shift 连选；批量模式不触发粘贴、拖拽或悬浮预览。
- chore: 窗口入场动画改为默认关闭。

## v0.9.0
**标签日期：** 2026年3月8日  
**版本比较：** [v0.8.5 → v0.9.0](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.8.5...v0.9.0)

- feat: 托盘新增暂停／恢复监控和临时禁用／恢复快捷键。
- feat: 新增窗口入场动画开关；返回顶部悬浮按钮可拖动并记住位置，改善卡片拖拽体验。
- feat: 启动时自动检查更新，默认开启且可关闭。
- feat: 设置新增独立音效页，可选择复制和粘贴音效的触发时机；调整设置分区布局。

## v0.8.5
**发布日期：** 2026年3月4日  
**版本比较：** [v0.7.77 → v0.8.5](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.7.77...v0.8.5)

- feat: 新增适配直角和深色模式的文本预览；图片预览支持超出屏幕边界的无界模式，最高缩放 500%。
- feat: 新增可关闭的区域提示，默认开启；调整拖拽与内容显示区域，修复偶尔点击无法粘贴和窗口焦点异常。
- chore: 自动重置状态默认关闭，预览触发延迟默认 500ms；改善数据清理和设置界面显示。

## v0.7.77
**发布日期：** 2026年3月3日  
**版本比较：** [v0.7.65 → v0.7.77](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.7.65...v0.7.77)

- fix: 修复更新后开机自启配置失效。
- fix: 修复图片预览显示问题，改善预览窗口效果。

## v0.7.65
**发布日期：** 2026年3月2日  
**版本比较：** [v0.7.39 → v0.7.65](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.7.39...v0.7.65)

- feat: 新增分组及右键移动至分组，支持粘贴后移至顶部。
- refactor: 底部类型分类改为「文本」与「其它」，其它包含文件、截图等。
- fix: 修复文本编辑窗口的高级窗口效果丢失。

## v0.7.39
**发布日期：** 2026年2月28日  
**版本比较：** [v0.7.36 → v0.7.39](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.7.36...v0.7.39)

- fix: 改善深色主题，修复深色模式开启 Mica 等窗口效果后显示不完整。
- ci: 发布流程支持自动向 winget 仓库提交版本更新。

## v0.7.36
**发布日期：** 2026年2月27日  
**版本比较：** [v0.7.35 → v0.7.36](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.7.35...v0.7.36)

- fix: 截图读取增加重试，减少剪贴板锁竞争造成的漏记；补充捕获与操作失败的诊断日志。

## v0.7.35
**发布日期：** 2026年2月27日  
**版本比较：** [v0.7.22 → v0.7.35](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.7.22...v0.7.35)

- feat: 新增 Mica、Acrylic、Tabbed 窗口特效，支持工具栏显示项及顺序自定义。
- feat: 新增数据清理和搜索框一键清空；搜索栏默认不自动获取焦点。
- feat: 提供 Scoop 安装配置。

## v0.7.22
**发布日期：** 2026年2月26日  
**版本比较：** [v0.7.16 → v0.7.22](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.7.16...v0.7.22)

- fix: 修复混合 DPI 多屏下无法获取焦点，以及固定窗口粘贴时焦点错误造成无法粘贴的问题。
- fix: 修复快捷键粘贴连键；调整设置布局及窗口行为选项，补充输入与窗口诊断日志。

## v0.7.16
**发布日期：** 2026年2月22日  
**版本比较：** [v0.7.1 → v0.7.16](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.7.1...v0.7.16)

> 此版调整数据存储路径以适配便携使用；升级后需手动选择原数据目录才能加载旧版剪贴板数据。

- feat: 新增数据导入导出、窗口大小自定义、深浅色切换及更多显示和行为选项，增加操作反馈。
- fix: 修复高 DPI 下底部组件错位、锁定面板无法粘贴，以及取消收藏后卡片未消失。
- fix: 更新下载可中断；修复 Esc、Delete 失效及托盘点击触发异常。

## v0.7.1
**发布日期：** 2026年2月21日  
**版本比较：** [v0.7.0 → v0.7.1](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.7.0...v0.7.1)

- fix: 更新对话框只显示当前版至目标版之间的日志，修复下载更新时选择错误架构。
- feat: 增加搜索栏自定义选项。

## v0.7.0
**发布日期：** 2026年2月21日  
**版本比较：** [v0.6.15 → v0.7.0](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.6.15...v0.7.0)

- fix: 修复 Firefox 内核浏览器输入时光标异常显示忙碌状态、管理员模式开机自启和复制来源识别问题。
- feat: 支持左右方向键切换分类。

## v0.6.15
**发布日期：** 2026年2月21日  
**版本比较：** [v0.6.10 → v0.6.15](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.6.10...v0.6.15)

- feat: 新增键盘导航，可选择条目并执行粘贴、纯文本粘贴或删除。

## v0.6.10
**发布日期：** 2026年2月20日  
**版本比较：** [v0.6.9 → v0.6.10](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.6.9...v0.6.10)

- feat: 底部新增内容分类切换。

## v0.6.9
**发布日期：** 2026年2月20日  
**版本比较：** [v0.6.8 → v0.6.9](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.6.8...v0.6.9)

- fix: 修复窗口被任务栏遮挡，以及条目序号仅显示 1～9 的问题。
- feat: 支持将诊断日志保存到文件。

## v0.6.8
**发布日期：** 2026年2月19日  
**版本比较：** [v0.6.7 → v0.6.8](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.6.7...v0.6.8)

- feat: 新增快速粘贴快捷键，可直接粘贴指定槽位的历史条目。

## v0.6.7
**发布日期：** 2026年2月18日  
**版本比较：** [v0.6.6 → v0.6.7](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.6.6...v0.6.7)

- ci: 发布构建支持 Windows ARM64；修复在线更新下载错误架构的问题。
- feat: 新增自动清理历史记录，默认保留 30 天。
- fix: 修复按键可能卡住和托盘重启时权限异常。

## v0.6.6
**发布日期：** 2026年2月13日  
**版本比较：** [v0.6.5 → v0.6.6](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.6.5...v0.6.6)

- feat: 支持在独立窗口编辑纯文本条目，不包含富文本格式编辑。
- fix: 修复自定义快捷键不生效。

## v0.6.5
**发布日期：** 2026年2月12日  
**版本比较：** [v0.6.4 → v0.6.5](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.6.4...v0.6.5)

- feat: 新增手动检查和下载在线更新；此版尚无启动自动检查。
- feat: 新增适合 Windows 10 的直角主题。
- fix: 限制为单实例运行，重复启动时通知用户。

## v0.6.4
**发布日期：** 2026年2月11日  
**版本比较：** [v0.6.3 → v0.6.4](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.6.3...v0.6.4)

- feat: 增强复制来源显示及其自定义选项，统一界面动效。
- fix: 修复管理员模式下开机自启失效。

## v0.6.3
**发布日期：** 2026年2月11日  
**版本比较：** [v0.6.2 → v0.6.3](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.6.2...v0.6.3)

- fix: 修复条目顺序变化后卡片序号未及时更新。

## v0.6.2
**标签日期：** 2026年2月11日  
**版本比较：** [v0.6.1 → v0.6.2](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.6.1...v0.6.2)

- feat: 记录并显示复制来源应用的名称和图标，可在设置中关闭来源显示。
- style: 滚动条颜色随主题变化。

## v0.6.1
**发布日期：** 2026年2月10日  
**版本比较：** [v0.6.0 → v0.6.1](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.6.0...v0.6.1)

- fix: 修复多屏及混合 DPI 下的显示异常，调整滚动触发阈值。
- feat: 支持 Esc 关闭设置窗口；统一主题样式并为预览增加阴影。

## v0.6.0
**发布日期：** 2026年2月10日  
**版本比较：** [v0.5.6 → v0.6.0](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.5.6...v0.6.0)

- feat: 右键菜单新增另存为；数据页支持手动刷新存储大小统计。
- fix: 搜索支持关键词高亮，修复搜索异常和总记录数只显示 100 条。
- feat: 关闭窗口时支持重置剪贴板视图状态。

## v0.5.6
**发布日期：** 2026年2月10日  
**版本比较：** [v0.5.5 → v0.5.6](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.5.5...v0.5.6)

- fix: 修复动态主题颜色转换精度及设置页主题色块显示，使强调色更接近系统颜色。

## v0.5.5
**发布日期：** 2026年2月10日  
**版本比较：** [v0.5.1 → v0.5.5](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.5.1...v0.5.5)

- feat: 新增跟随系统强调色的动态主题，通过 Windows 设置变更通知更新颜色，无需定时轮询。
- style: 设置按主题、数据等类别重新组织，统一显示样式。

## v0.5.1
**发布日期：** 2026年2月9日  
**版本比较：** [v0.5.0 → v0.5.1](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.5.0...v0.5.1)

- feat: 新增返回顶部悬浮按钮，支持 Esc 关闭弹窗或隐藏剪贴板。
- chore: 图片悬浮预览默认关闭，图片最大高度默认启用并设为 512px。
- fix: 修复卸载时无法停止运行中的旧版程序。

## v0.5.0
**发布日期：** 2026年2月9日  
**版本比较：** [v0.4.9 → v0.5.0](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.4.9...v0.5.0)

- fix: 修复更新时无法终止运行中的程序，以及自定义快捷键注册后的唤醒行为。
- fix: 图片缓存同步写入，避免后续读取早于文件落盘；修复监控暂停计数下溢。
- fix: 修复跨置顶区域拖拽的位置丢失，回弹动画仅在实际拖拽结束后播放。
- fix: 设置加载完成后才允许自动保存；卡片正文、预览和文件路径变化时正确刷新，旧搜索或刷新响应不再覆盖新结果。

## v0.4.9
**发布日期：** 2026年2月8日  
**版本比较：** [v0.4.8 → v0.4.9](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.4.8...v0.4.9)

- feat: 图片在关闭悬浮预览时仍可使用较大的卡片显示；设置中可调整此模式与悬浮预览开关。

## v0.4.8
**发布日期：** 2026年2月8日  
**版本比较：** [v0.4.7 → v0.4.8](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.4.7...v0.4.8)

- fix: 更换列表滚动容器并统一滚动条样式；改善滚动条点击与拖拽的交互，修复相关显示异常。

## v0.4.7
**标签日期：** 2026年2月7日  
**版本比较：** [v0.4.5 → v0.4.7](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.4.5...v0.4.7)

- fix: 主窗口隐藏或粘贴后同步关闭图片预览，重新显示时恢复预览置顶，避免预览残留或被遮挡。
- fix: 重复复制的条目移至列表顶部；置顶和普通条目的显示序号连续，搜索无结果时显示明确提示。
- fix: 启动时保持主窗口隐藏并通知唤醒快捷键；输入监听异常后尝试恢复。
- perf: 图片尺寸读取不再完整解码图片，调整图片写回以减少重复转换。

## v0.4.5
**发布日期：** 2026年2月6日  
**版本比较：** [v0.4.0 → v0.4.5](https://github.com/Y-ASLant/ElegantClipboard/compare/v0.4.0...v0.4.5)

- feat: 图片悬停 300ms 后显示独立预览窗口，支持 Ctrl+滚轮缩放、短暂显示缩放百分比，以及自动／左侧／右侧位置偏好。
- feat: 设置可开关悬浮预览并调整 5%～50% 的缩放步进；复制单张图片文件也可预览。
- perf: 图片通过本地资源协议加载，不再转换为 base64；隐藏预览时清空图片，避免下次显示闪烁旧图。

## v0.4.0
**发布日期：** 2026年2月4日  
**版本基线：** [v0.4.0 标签](https://github.com/Y-ASLant/ElegantClipboard/tree/v0.4.0)，本日志最早的版本号标签，不与未编号的 Beta 标签比较。

- feat: 基线具备图片预览及 Ctrl+滚轮缩放、预览信息自定义、管理员启动和跟随鼠标定位。
- feat: 支持文件与文本类型区分、文件复制和信息统计、列表菜单，以及卡片拖动和置顶。
- perf: 包含面向大量历史记录的查询与虚拟列表滚动支持。
