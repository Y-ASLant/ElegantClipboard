# GPUI 工程文档

本目录记录 ElegantClipboard GPUI 应用的架构决策、Windows 功能状态和验证证据。

- [GPUI 2.0 基础版规划](GPUI_2_0_PLAN.md)：2026-10-09 的规划基线，保留未完成需求与发布门槛；部分交互目标已调整，当前行为以功能状态和 UI 规范为准。
- [变更日志](CHANGELOG.md)：从 GPUI 重构版 2.0.0 开始的版本记录。
- [Windows GPUI 状态](WINDOWS_MVP.md)：当前实现、依赖、运行方式、验证命令与限制。
- [UI 规范](UI_GUIDELINES.md)：GPUI 界面样式与交互约定。
- [Windows 独立包说明](WINDOWS_PACKAGE_README.txt)：ZIP 制品的运行与数据目录说明。
- [技术决策](decisions/0001-windows-gpui.md)：GPUI 和 gpui-kit 的选择依据。
- [技术决策 0002](decisions/0002-native-fidelity-transient-merge.md)：原生格式保真、瞬时过滤、同文合并与 blob 存储。
- `evidence/`：各项功能的历史验收记录。

当前仓库入口只有根 Cargo workspace；历史记录中出现的旧应用名称和路径仅用于说明迁移或兼容性测试，不是可构建入口。

## 模块边界与维护约束

依赖方向为 `clipboard-gpui -> clipboard-platform -> clipboard-core`，GPUI 可以直接调用核心层。业务校验、SQLite 和备份格式属于核心层；Windows API、剪贴板线程与服务协议属于平台层；窗口、托盘、交互状态和 GPUI Kit 组件属于界面层。

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

### Windows 平台层

`crates/clipboard-platform/src/window_effect.rs` 独立负责 Windows 版本探测、材质能力与降级策略，不修改已保存的偏好。界面层的 `ui/settings_window_effect.rs` 显示可用选项和保存值的降级说明；`acrylic.rs` 只在初始化或有效材质变化时配置原生窗口，遮罩强度变化只更新背景并重绘。

| 路径（相对 `crates/clipboard-platform/src/windows`） | 职责 |
|---|---|
| `mod.rs`、`protocol.rs` | 对外入口；命令、事件、采集载荷与失败分类 |
| `service.rs` | 服务启动、线程与监听器生命周期、关闭 |
| `capture.rs`、`queue.rs` | 剪贴板监听、有界队列及反馈背压 |
| `data.rs`、`file_actions.rs` | 数据目录、实例锁、导入恢复与占用统计；文件定位和另存为 |
| `worker/` | 命令分发、采集保存、剪贴板写回、历史操作、偏好设置和快照 |
| `tests/mod.rs` | 有超时、区分通道关闭的事件等待；严格下一事件与按条件匹配分别保留顺序语义 |
| `tests/` | 采集、剪贴板、数据、事件投递、分组、历史、生命周期、偏好和预览测试 |

### GPUI 界面层

`crates/clipboard-gpui/src/ui.rs` 保留共享视图状态和动作声明；入口、渲染及交互实现位于私有 `ui/` 子模块，不改变 `main.rs` 的 `ui::run` 调用。

| `ui/` 模块 | 职责 |
|---|---|
| `startup`、`view_init`、`window_lifecycle`、`window_render` | 应用启动、状态初始化、窗口生命周期及主窗口渲染 |
| `events`、`history_events`、`preference_events`、`shortcut_events` | 平台事件路由和各领域状态更新 |
| `navigation`、`history_list`、`history_card`、`row_actions`、`row_menu` | 列表导航、虚拟列表、卡片、按钮及上下文菜单 |
| `drag`、`drag_visual`、`selection`、`groups`、`confirmation` | 拖拽状态与动效、批量选择、分组及确认交互 |
| `preview_actions`、`preview_render`、`file_preview`、`file_cards` | 完整预览、编辑和文件卡片信息 |
| `hover_lifecycle`、`hover_window` | 悬停请求、浮窗生命周期和内容 |
| `settings_window`、`settings_*` | 设置窗口与按功能划分的页面；快捷键、过滤、音效、悬停和关于独立实现 |
| `shortcuts`、`shortcut_labels`、`paste_actions`、`storage`、`data_import` | 快捷键注册与显示、受控粘贴、数据占用与恢复到独立目录 |
| `image_assets`、`thumbnail_cache` | 可见范围图片资源、后台缩略图与纹理生命周期 |
| `onboarding`、`content`、`presentation` | 首次引导、内容展示助手、主题及本地化展示 |

纯状态算法仍在 `state.rs`，对应行为测试在 `state/tests.rs`；UI 专属布局、拖拽和预览测试保留在其所属子模块。`ui/thumbnail_cache.rs` 负责列表缩略图的后台解码、两任务并发限制、外部文件版本和过期结果校验；`ui/image_assets.rs` 将其与可见范围、拖动引用和纹理回收衔接，原始图片仍用于完整预览与复制。

确认操作仅由 `HistoryConfirmation` 表示当前类型与分组作用域，保存中的状态仍按操作分别保留。普通偏好使用 `PendingPref<T>`，仅在保存成功后替换生效值；快捷键注册回滚、语言和显示布局等副作用仍由专用处理器负责。窗口隐藏复用清理与隐藏动作，各入口的保护条件不合并。

