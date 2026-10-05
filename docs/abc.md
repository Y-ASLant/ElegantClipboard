# 架构与性能审查报告

> 源码复核日期：2026-10-04
> 范围：统一操作检查/结果/错误、模块边界、状态管理、捕获与列表刷新、资源可用性、全项目质量门禁及验证证据。
> 本报告描述当前检出的代码。源码检查不等同于运行时验证；未测量的性能影响不作为结论。位置以文件和符号标识，不依赖行号或文件长度。

## 一、当前架构

### 后端分层与共享状态

- `src-tauri/src/commands/` 按剪贴板、文件操作、窗口、预览、设置、分组、同步、翻译及数据迁移等职责组织 Tauri 命令，`src-tauri/src/lib.rs::run` 注册命令并编排启动。
- `src-tauri/src/database/repository.rs` 中的 `ClipboardRepository`、`SettingsRepository`、`GroupRepository` 封装条目、设置和分组 SQL。
- `src-tauri/src/commands/mod.rs::AppState` 包含 `db`、`monitor`、`active_group_id` 和窗口定位缓存 `position_cache`。`lib.rs::run` 将其包装为 `Arc<AppState>` 并交由 Tauri 管理。
- `src-tauri/src/clipboard/monitor.rs::ClipboardMonitor::active_group_id` 返回共享的 `Arc<Mutex<Option<i64>>>`，供监控器和命令状态访问同一活动分组。
- `src-tauri/src/database/mod.rs::Database` 使用分别受锁保护的读、写连接；写连接启用 WAL，读连接以只读方式打开。该结构支持读写分离，但不是所有读操作彼此无锁并行，也不代表已测得特定吞吐量。

### 前端状态与设置

- `src/stores/clipboard.ts` 管理当前筛选下的条目、请求版本和批量选择；`fetchItems` 通过 `_fetchId` 避免旧请求覆盖新视图。
- `src/stores/ui-settings.ts` 使用 Zustand、SQLite 设置和 `ui-settings-changed` 事件完成持久化及多窗口同步，不使用 Zustand persist 中间件。`makeSetter` 统一调用 `updateAndPersist`。
- `src/stores/translate-settings.ts` 通过字段到数据库键的映射、变更快照和 `translate-settings-changed` 事件保存及同步设置。
- `src/hooks/useWebDAVSettings.ts::loadSettings` 批量读取设置；保存 effect 比较快照，文本及大小字段按键防抖。当前不是多个字段共用一个计时器，也不是旧报告中的逐字段重复 effect。
- `src-tauri/src/webdav/mod.rs::load_config_and_options` 是 WebDAV 配置和同步选项的共享加载入口；`src-tauri/src/commands/sync.rs::load_webdav_config` 仅转换未配置时的错误，不再重复解析配置。

## 二、捕获、列表查询与刷新

1. `src-tauri/src/clipboard/monitor.rs::ClipboardMonitor::start` 将捕获交给 worker 处理，成功处理后发出带条目 ID 的 `clipboard-updated` 事件。
2. `src/stores/clipboard.ts::setupListener` 检查事件 ID，合并连续捕获事件，并调用 `fetchItems` 查询当前筛选条件下的权威列表。防抖设置包含尾沿触发及最大等待时间；刷新进行中收到的请求会安排后续刷新，监听销毁时取消待触发防抖。
3. 这仍是**当前筛选列表重新查询**，不是逐条增量插入。它同步后端淘汰、去重及排序变化；不能再描述为“每个复制事件立即独立全量刷新”，也不能把单条详情查询拼入列表当作现行实现。
4. `src-tauri/src/database/repository.rs::ClipboardRepository::LIST_COLUMNS` 排除完整 `text_content`、`html_content`、`rtf_content` 和 `file_payload`。`SEARCH_COLUMNS` 仅额外取出搜索上下文所需的文本；`src-tauri/src/commands/clipboard.rs::get_clipboard_items` 生成关键词预览后清空返回项的 `text_content`。需要完整内容时由 `get_clipboard_item` 按 ID 获取。
5. `src/components/ClipboardList.tsx::ClipboardList` 在普通列表中使用 Virtuoso，瀑布流使用 `MasonryVirtualView`。虚拟化限制渲染范围，但不会自动分页后端查询或缩小 store 中已经加载的条目数组。

列表查询不检查磁盘，也不携带资源可用性字段；可见卡片的资源状态由独立 Hook 和后端状态命令提供。

## 三、资源可用性与预览状态

### 有效性来源和刷新时机

- `src/hooks/useItemResourceStatus.ts::useItemResourceStatus` 合并可见文件/缓存图片卡片的检查请求。文件调用 `batch_get_item_file_status`，缓存图片调用 `check_files_exist`；挂载、相关源信息变化、`window-shown` 和右键菜单打开时刷新，拖拽覆盖卡片不额外查询，不轮询。文本与链接不执行磁盘或网络检查。
- Hook 是可见卡片可用性快照的来源，不是操作执行授权。开始复查即撤销旧授权，来源与请求代数隔离过期结果；检查失败返回未知而不是缺失。后端操作按条目重新查库、解析资源并准备内容，键盘/快捷键不依赖已挂载卡片或旧路径。
- `src-tauri/src/commands/file_ops.rs::build_item_file_status` 分别返回 `all_exist` 资源真相、必填 `clipboard_usable` 复制格式能力、resolved 路径、实际检查结果及 `too_large`。存在文件即便 payload 不支持，也不会被假称缺失，仍可按 id 保存字节/定位。
- `src-tauri/src/clipboard/file_clipboard.rs::resolve_item_paths` / `resolve_paths` 优先保留存在的原路径，否则使用对应 staged 路径；`item_files_all_exist` 检查解析后的路径。因此源文件移动或删除后，若 staged 副本仍可用，条目不应被直接判定失效。

### 大小限制不代表文件存在

`too_large` 和 `FileContent` 的预览大小判断只控制图片加载。超限文件仍执行存在性及 staged 路径检查；状态检查使用路径存在性、目录信息及必要元数据，不读取正文生成预览。

`src/components/CardContentRenderers.tsx::ImageCard` / `FileContent` 只根据输入可用性展示资源状态。资源缺失显示「已失效」；图片加载或解码失败只显示「预览加载失败」，不加失效路径删除线，也不改变操作能力。预览错误和按路径缓存保留在渲染层，不能自行宣告文件失效。

### 统一操作契约与错误归属

- `clipboard/format_write.rs::prepare_item` / `prepare_text`、`file_clipboard.rs::prepare_files`、`merge_paste.rs::prepare_merge` 在副作用之前完成准备；失败不先暂停监控、写剪贴板、隐藏窗口或注入按键。
- 单项、文本/路径、合并、翻译复制、键盘及快速/收藏/重复快捷键共用执行路径。合并缺失成员明确拒绝，原始资源可按可信 payload 回退暂存；不支持的组合不静默删掉数据。
- `operation_error.rs` 的 code/detail 区分稳定业务原因与诊断内容；`operation-feedback.ts` 固定分发表返回 success/cancelled/failed。另存为取消无成功或错误反馈，普通 bool false 不被误当取消。
- 成功动画和依赖状态变化只在主操作成功后发生；主操作完成后的刷新错误明确独立提示，不把成功改成失败。来源更换、卸载或过期完成不发布错位反馈。
- `logError` 只记录开发诊断；用户错误由显式 reporter 或行内状态单独拥有，不重复 toast，也不显示原生内部对象、堆栈和凭据。快捷键在可见窗口发事件，隐藏时发本地化系统通知。
- 监控恢复以 RAII 保证展开/错误路径收敛；输入失败恢复原本可见窗口而不改变锁定或焦点。操作成功只证明本程序的写入/输入发送完成，不保证第三方目标程序消费格式。
- 本地分叉以真实 CF_DIB 及完整 header/mask/palette 保留位图，通过借用的 BMP 前缀避免上游 headerless V4/V5 偏移推断错误和整块复制；Windows 自动合成 CF_BITMAP。访问占用错误保持具体类型，不靠字符串猜测。
- 原生证明还确认 Windows HGLOBAL 路径无法发布零大小原始对象；当前明确拒绝该格式/零 FileContents，且准备阶段拒绝发生在任何剪贴板改变之前。非零虚拟内容可按真实 descriptor/contents 还原，不假补字节或用成功 no-op 绕过限制。


## 四、保留的现有关注点

以下均来自旧报告中的关注点，已核对当前代码；它们不表示已经复现故障，也不授权自动开展重构。

| 关注点 | 当前源码证据 | 判断边界 |
| --- | --- | --- |
| 启动编排和快捷键状态仍集中 | `src-tauri/src/lib.rs::run`、`PasteKind`、`apply_paste_shortcuts`、`CURRENT_SHORTCUT`、`QUICK_PASTE_LOCK` | 入口仍同时负责插件、共享状态、快捷键、主窗口和后台任务；是模块边界维护项，不用固定行数或静态变量数量判断严重度。 |
| 列表重新查询尚未分页 | `src/stores/clipboard.ts::fetchItems` / `setupListener`；`src-tauri/src/database/repository.rs::ClipboardRepository::list` | 调用方通常不传 `limit`，SQL 仅在显式提供时加分页。返回数据已轻量化且事件已合并；剩余开销需要实测，不能沿用旧 JSON 大小或“显著收益”断言。 |
| 内部暂停后的捕获窗口 | `src-tauri/src/commands/mod.rs::RESUME_TX` / `with_paused_monitor`；`src-tauri/src/clipboard/monitor.rs::MonitorHandler::is_active` / `capture_with_retry` | 恢复线程等待 500ms 静默期后递减暂停计数，新的恢复请求会延后静默期；暂停期间不捕获。这是旧报告已有的时序关注点，本次仅确认实现，没有复现原生复制/粘贴时序故障。 |
| 翻译文本设置依赖调用方防抖 | `src/components/settings/TranslateTab.tsx::debounced`；`src/stores/translate-settings.ts::updateAndPersist` | 输入先 `setState`，再按字段延迟调用 setter，卸载时 flush 待写值。该协议跨组件/store，不是“每键立即存盘”；是否收敛 API 属于维护选择。 |
| 动态 SQL 参数使用装箱 | `src-tauri/src/database/repository.rs::ConditionBuilder` / `ClipboardRepository::build_filter_conditions` | 参数确实存于 `Vec<Box<dyn ToSql>>`。列表过滤走 `build_filter_conditions`，并非直接使用 `ConditionBuilder`；没有耗时占比或分配基准，不将其列为已证实性能瓶颈。 |

单凭 selector、`useState`、字段或文件行数，不能认定性能问题。设置快照对象的构造存在，但旧报告的耗时、堆指针估算及批量恢复“不会发生”的场景推测均无测量依据，不保留为风险结论。

## 五、验证证据及限制

### 源码复核

源码与测试阅读用于核对模块边界；下节单独记录本次局部重构已经执行的检查与浏览器场景，源码阅读本身不构成原生运行时证明。

- 前端资源回归覆盖文件超限但缺失、窗口/右键刷新、暂存回退、缓存图片缺失、预览失败不影响另存为，以及来源切换、过期响应和检查失败撤销旧授权。相关测试位于 `ClipboardItemCard.test.tsx`、`CardContentRenderers.test.tsx` 和 `useItemResourceStatus.test.ts`。
- Rust 测试不仅在 `clipboard/dedup.rs`，还存在于 `clipboard/monitor.rs`、`database/mod.rs`、`database/repository.rs`、`commands/data_transfer.rs` 和 `lib.rs` 等模块。测试存在不等于本次已经运行或覆盖全部原生路径。
- `src-tauri/src/lib.rs::init_logging` 使用 INFO 过滤，并按配置启用文件日志；剪贴板处理、暂停/恢复和鼠标外部点击等辅助路径已有 debug 日志。不能沿用“高频辅助事件普遍使用 info，生产必然大量日志”的概括。

### 本次局部重构验证记录（2026-10-03）

- `node node_modules/vitest/vitest.mjs run`：23 个前端测试文件、220 项测试通过；这是当日执行记录，不是永久测试总数。
- `node node_modules/typescript/bin/tsc --noEmit` 与 `node node_modules/eslint/bin/eslint.js src`：TypeScript 和前端全量 ESLint 检查通过。
- `node node_modules/typescript/bin/tsc` 与 `node node_modules/vite/bin/vite.js build`：前端生产构建通过。验证直接使用项目已安装 CLI，没有修改 NVM 的信任配置。
- Edge 无头浏览器渲染实际卡片和样式，通过 Node 操作真实临时文件与图片字节：预览解码失败但原文件仍能另存为、缺失缓存图片禁用操作、缓存文件恢复、超大文件移动/恢复、暂存副本实际保存，以及检查请求失败回到未知状态均通过。Tauri IPC/事件和资源加载边界经过适配，并非原生 Tauri 运行时。
- 此记录不验证原生剪贴板监听时序、Tauri 多窗口或 WebDAV 端到端行为；Rust 和原生构建验证单独记录其实际结果。

### 原生构建补充记录（2026-10-03）

- 已安装微软 Visual Studio Build Tools 2022 的 MSVC x64/x86 工具与 Windows 11 SDK，加载 `vcvars64.bat` 的 x64 环境。实际链接器来自 MSVC `14.44.35207`，SDK 版本为 `10.0.26100.0`。
- `cargo test --manifest-path src-tauri/Cargo.toml --lib`：215 项 Rust 库测试通过，移除过时资源状态字段后的仓库、合并粘贴和同步调用方均编译通过。
- 项目安装的 Tauri CLI 执行 release 构建与 NSIS 打包成功；仅本次将前端构建命令指定为项目已安装的 TypeScript/Vite CLI，不改变仓库配置或 NVM 信任设置。
- 产物为 `src-tauri/target/release/elegant-clipboard.exe` 和 `src-tauri/target/release/bundle/nsis/ElegantClipboard_0.0.0_x64-setup.exe`；版本号来自当前仓库配置。
- 后续将恢复暂停计数的原子操作由 `fetch_update` 切换为 `try_update`，保留内存序和条件递减逻辑；16 项既有监控测试及临时重叠暂停/零计数边界 smoke 通过，临时用例已移除。重新生成 release 程序与 NSIS 安装包成功，未再出现该弃用警告。构建成功不等于已经安装并运行该安装包，也不替代原生剪贴板/多窗口端到端验证。

### 操作统一与项目质量验证（2026-10-04）

- `npm run typecheck` 覆盖应用、测试、构建配置与 E2E；`npm run lint` 全范围零警告。当前前端 25 个测试文件、262 项行为测试通过，旧 wiring/default/mock-echo 用例已移除或换成行为边界。
- 应用和本地分叉分别使用 Clippy `-D warnings`；分叉默认及无默认 feature 全 target 检查通过。应用 229 项测试通过；分叉自动纯测试默认 15 项、无默认 5 项通过，系统剪贴板/桌面用例不是自动并发运行项目。
- Chromium 实际主窗口组件通过 5 个操作场景：等待/拒绝复制不显示成功、键盘缺失文件失败、取消另存为无错误/成功、快捷键事件安全分发、失败删除保留键盘选择。平台 IPC 使用显式测试边界，不再过滤缺失 Tauri 异常冒充成功。
- 隔离的非交互 Windows 窗口站中运行实际 Win32 剪贴板：Unicode、拒绝预检保留原剪贴板、非零虚拟文件内容、拒绝不支持的零内容且不修改剪贴板、暂存/HDROP/伴生格式、合并原子失败/混合成功、CF_HTML/二进制 RTF、1×1/多行 RGBA/alpha/DIB 与 CF_BITMAP 合成、真实不同 HWND 占用错误、文件字节保存及同源保存保护均通过。仅操作临时站及文件，不覆盖交互用户剪贴板。
- 原生验证创建命名隔离站需要管理员授权；测试 ACL 仅作用于临时站，适配 clipboard-win 的匿名令牌 CloseClipboard 加固，不更改生产安全机制或用户站权限。隔离模型依据 [微软窗口站文档](https://learn.microsoft.com/en-us/windows/win32/winstation/window-stations)。
- 最终原项目入口 `make check`、`make test`、`make build` 全部通过：前后端及本地分叉默认/无默认配置无编译错误或警告，完整 NSIS 产物 `src-tauri/target/release/bundle/nsis/ElegantClipboard_0.0.0_x64-setup.exe` 已重新生成。临时原生站验证模块及执行脚本不随项目保留；12 份文档的 45 个本地链接/资源目标有效。
- 上述证据不宣称第三方接收程序必然接受粘贴、实际快捷键硬件连续输入、多窗口恢复或非 Windows 运行时均已端到端验证，也不表示已安装该安装包。

