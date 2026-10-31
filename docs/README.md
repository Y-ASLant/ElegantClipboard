# GPUI 工程文档

本目录记录 ElegantClipboard GPUI 应用的架构决策、Windows 功能状态和验证证据。

- [变更日志](CHANGELOG.md)：从 GPUI 重构版 2.0.0 开始的版本记录。
- [Windows GPUI 状态](WINDOWS_MVP.md)：当前实现、依赖、运行方式、验证命令与限制。
- [UI 规范](UI_GUIDELINES.md)：GPUI 界面样式与交互约定。
- [Windows 独立包说明](WINDOWS_PACKAGE_README.txt)：ZIP 制品的运行与数据目录说明。
- [技术决策](decisions/0001-windows-gpui.md)：GPUI 和 gpui-kit 的选择依据。
- [技术决策 0002](decisions/0002-native-fidelity-transient-merge.md)：原生格式保真、瞬时过滤、同文合并与 blob 存储。
- [技术决策 0003](decisions/0003-core-paste-parity.md)：恢复剪贴板、去重策略、文本转换、按格式粘贴与条目备注。
- [技术决策 0004](decisions/0004-sequence-paste.md)：多选记录逐条序列粘贴的设计与快捷键。
- [跨平台正确性修复验证](evidence/cross-platform-correctness-2026-10-27.md)：平台能力、粘贴目标与请求归属、格式编码和延迟采集修复的验证范围。
- [默认数据目录迁移验证](evidence/data-directory-migration-2026-10-27.md)：三平台统一目录、旧数据引用与自启迁移、冲突保护和清理规则。
- [普通文件仅保存路径](evidence/file-paths-only-2026-10-27.md)：移除文件副本、启动清理及旧备份兼容行为。
- [首次引导布局与动效](evidence/onboarding-motion-2026-10-27.md)：beta.9 四步动态预览的历史验证。
- [首次使用与配置引导](evidence/onboarding-setup-2026-10-28.md)：快捷键、复制粘贴教程、初始偏好与原生窗口验证。
- [全应用主题与动效](evidence/chakra-theme-2026-10-28.md)：Chakra 风格的原生组件统一、可调圆角与Windows实机验证。
- `evidence/`：各项功能的历史验收记录。

当前仓库入口只有根 Cargo workspace；历史记录中出现的旧应用名称和路径仅用于说明迁移或兼容性测试，不是可构建入口。

## 模块边界与维护约束

依赖方向为 `clipboard-gpui -> clipboard-platform -> clipboard-core`，GPUI 可以直接调用核心层。业务校验、SQLite 和备份格式属于核心层；共享服务协议、剪贴板线程及各系统原生 API 属于平台层；窗口、托盘、交互状态和 GPUI Kit 组件属于界面层。

- 单个 `.rs` 文件最多 **500 物理行**，空行、注释、示例与测试均计入；Cargo.lock、文档和生成的构建产物不属于源码行数限制。
- 同一类型可在职责明确的私有子模块中分别实现方法；跨模块可见性只开放到实际需要的父模块，不为了拆文件扩大 crate 公共 API。
- 测试与对应业务模块就近组织，共用必要的隔离 fixture；不按行号编号分片，不用 `include!` 将大文件藏在外部文本中。
- `scripts/check-source-size.ps1` 是唯一的行数上限检查入口，`just check` 和 `.github/workflows/gpui.yml` 都会调用。直接运行 `cargo check` 不包含该结构检查。
- 删除代码前确认生产调用、测试用途、资源引用及兼容导入路径。历史验收记录不是死代码，不因重构删除；旧数据库和 ZIP 导入仍是受支持功能。

结构检查命令：

```powershell
.\scripts\check-source-size.ps1
```

### 核心层

| 路径（相对 `crates/clipboard-core/src`） | 职责 |
|---|---|
| `database/mod.rs`、`schema.rs` | 连接生命周期、初始化、维护及 schema |
| `database/migrations.rs`、`content_migration.rs`、`data_migration.rs` | 迁移顺序与中断恢复、内容类型升级、语义哈希与排序回填 |
| `database/repository.rs`、`repository/` | 记录模型与查询条件；查询、写入、保留策略分别实现 |
| `database/groups.rs`、`settings.rs` | 分组和设置仓储 |
| `preferences.rs`、`preferences/types.rs` | 设置键与持久化；设置值类型、默认值和校验 |
| `backup_common.rs::check_central_directory_names` | 新旧 ZIP 共用原始目录项去重与数量校验，避免依赖合并同名条目后漏检 |
| `backup.rs`、`backup_common.rs`、`import.rs`、`legacy_backup.rs` | GPUI 备份、共享归档校验及旧数据兼容导入 |
| 各模块的 `tests.rs` / `tests/` | 就近维护的隔离业务测试；仓储测试按记录、查询、保留、分组及设置分类 |

数据库打开时只转换旧的 UNIQUE 内容哈希索引，已完成迁移的非唯一索引不再重建。查询继续使用现有条件构造器，以 SQLite `Value` 和迭代绑定参数；媒体路径重映射借用原字段，未修改的字段保持原值。数据库测试通过 `TempDir` 管理临时目录，连接与仓库须先于目录释放。

### 平台层与共享服务

`clipboard-platform/src/service/` 保存三平台唯一的 Service、命令协议、队列和 Worker，避免复制数据库与偏好业务。`windows/` 保留 Windows 原生格式及剪贴板写回；`unix/` 实现 clipboard-rs 读写、数据目录隔离的本地 socket 单实例唤回及 Linux 文件 URI 处理。`macos/`、`linux/` 分别实现原生热键、来源、自动启动、外部点击、粘贴目标及拖放。

| 路径（相对 `crates/clipboard-platform/src`） | 职责 |
|---|---|
| `service/protocol.rs`、`service/lifecycle.rs` | 完整命令/事件协议、服务启动及线程关闭 |
| `service/capture.rs`、`service/read_content.rs`、`service/watcher.rs`、`service/queue.rs` | 采集快照、监听、有界队列和反馈背压 |
| `service/data.rs`、`service/file_actions.rs` | 数据目录、实例锁、导入恢复、占用统计及系统文件定位 |
| `service/worker/` | 共用命令分发、采集入库、历史、偏好、备份及快照 |
| `windows/read_formats.rs`、`windows/worker/`、`windows/privacy.rs` | Windows 格式保真、写回和隐私标记 |
| `unix/read_formats.rs`、`unix/clipboard.rs`、`unix/instance.rs`、`unix/file_list.rs` | Unix 剪贴板读写、单实例、本地文件 URI 编码与校验 |
| `linux/` | X11/XRes 来源、XTEST、XInput2、被动按键抓取、XDND/INCR 传输和 XDG 自动启动 |
| `macos/` | Carbon 快捷键、NSWorkspace 来源、NSEvent、NSDraggingSession、Accessibility 粘贴及 LaunchAgent |
| `sequence.rs`、`unix/x11_sequence.rs` | Windows 序列号、macOS changeCount、X11 XFixes generation 和 Wayland 内容身份 |
| `notification.rs`、`notification/` | Windows WinRT、macOS UserNotifications 与 Linux D-Bus 系统通知；由界面层在后台调用 |
| `windows/tests/` | 原有 Windows 服务与剪贴板行为回归；Unix 的隔离测试位于所属模块 |

Linux X11 采集内容与来源查询整体受前后 selection generation 校验保护；不以无关前台窗口冒充 clipboard owner。XDND 在释放鼠标后锁定目标和位置，等待最终响应期间不重新投放到另一窗口。Wayland 必须支持 data-control，按完整载荷轮询检测变化，不能区分相同内容的再次复制。

`window_effect.rs` 提供当前系统的真实材质能力及降级策略，不覆盖已保存的偏好；Windows DWM、macOS 模糊和 Linux 不透明分别由 GPUI 原生层应用。

### GPUI 界面层

`crates/clipboard-gpui/src/design_system.rs` 集中配置主题语义色、字体、圆角与动效，`visual.rs` 管理应用布局和内容动画。`crates/clipboard-gpui/src/ui.rs` 保留共享视图状态和动作声明；入口、渲染及交互实现位于私有 `ui/` 子模块，不改变 `main.rs` 的 `ui::run` 调用。

GPUI 原生分派在 `main.rs` 按目标选择 `windows/` 或 `unix/`；Unix 的窗口实现分别在 `macos/` 和 `linux/`，浮窗几何在 `popup_geometry.rs` 共用。`ui/`、`state.rs`、`visual.rs` 和 `window_effect.rs` 由三平台共用，不直接调用 Win32。音效的 `Sound` 枚举、启停判定与 WAVE 合成位于三平台共用的 `sound_core.rs`，`windows/` 与 `unix/` 的 `sound.rs` 只保留各自播放后端。`capabilities.rs` 将 Wayland、粘贴按键和音效能力映射到设置说明与禁用状态。

`tray.rs` 共用托盘菜单与事件，`linux/tray.rs` 负责 Linux 宿主检测；Linux GTK 事件在 GPUI 前台任务中分派。`unix/` 使用真实窗口目标、剪贴板身份和修饰键校验控制自动粘贴，macOS 的 `Cmd+V` 需要辅助功能授权。平台限制和本机构建命令见根 README。

| `ui/` 模块 | 职责 |
|---|---|
| `startup`、`view_init`、`window_lifecycle`、`window_render` | 应用启动、状态初始化、窗口生命周期及主窗口渲染 |
| `events`、`history_events`、`preference_events`、`shortcut_events` | 平台事件路由和各领域状态更新 |
| `navigation`、`history_list`、`history_card`、`row_actions`、`row_menu` | 列表导航、虚拟列表、卡片、按钮及上下文菜单 |
| `drag`、`drag_visual`、`selection`、`groups`、`confirmation` | 拖拽状态与动效、批量选择、分组及确认交互 |
| `editor_actions`、`file_cards` | 按需加载文本编辑、备注保存及文件卡片信息；主窗口无整页预览 |
| `hover_lifecycle`、`hover_window` | 悬停请求、浮窗生命周期和内容 |
| `settings_window`、`settings_*` | 设置窗口与按功能划分的页面；快捷键、过滤、音效、悬停和关于独立实现 |
| `shortcuts`、`shortcut_labels`、`paste_actions`、`storage`、`data_import` | 快捷键注册与显示、受控粘贴、数据占用与恢复到独立目录 |
| `image_assets`、`thumbnail_cache` | 可见范围图片资源、后台缩略图与纹理生命周期 |
| `onboarding`、`onboarding_shortcut`、`onboarding_practice`、`onboarding_preferences`、`content`、`presentation` | 首次引导导航、快捷键配置、隔离练习教程与初始偏好、内容展示助手、主题及本地化展示 |

纯状态算法仍在 `state.rs`，对应行为测试在 `state/tests.rs`；UI 专属布局、拖拽和预览测试保留在其所属子模块。`ui/thumbnail_cache.rs` 负责列表缩略图的后台解码、两任务并发限制、外部文件版本和过期结果校验；`ui/image_assets.rs` 将其与可见范围、拖动引用和纹理回收衔接，原始图片仍用于悬停预览与复制。

确认操作仅由 `HistoryConfirmation` 表示当前类型与分组作用域，保存中的状态仍按操作分别保留。普通偏好使用 `PendingPref<T>`，仅在保存成功后替换生效值；快捷键注册回滚、语言和显示布局等副作用仍由专用处理器负责。窗口隐藏复用清理与隐藏动作，各入口的保护条件不合并。

