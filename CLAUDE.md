# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 编码原则

### 1. 先想后写
- 明确假设，不确定就问
- 多种解法存在时，列出让用户选，不要默默挑一个
- 有更简单的方案就说出来，该 push back 就 push back

### 2. 极简优先
- 只写解决问题的最少代码，不搞投机性设计
- 单次使用的代码不抽象
- 没要求的"灵活性""可配置"不加
- 不可能发生的场景不处理
- 200 行能 50 行搞定就重写

### 3. 精准改动
- 只动必须动的，不"顺手改进"周边代码
- 匹配现有风格，哪怕你会写得不一样
- 自己引入的孤儿（未使用导入/变量/函数）自己清
- 不删与任务无关的已有死代码

### 4. 目标驱动
- 把任务转化为可验证的目标
- 多步任务列出计划：`步骤 → 验证方式`
- 先写测试复现问题，再修复，再验证通过

## 文档与实现来源

本文件描述当前 Tauri 分支，核对日期：2026-10-03。功能说明见 `FEATURES.md` / `FEATURES_EN.md`，历史发布记录见 `docs/Changlog.md`。依赖版本以 `package.json`、`package-lock.json`、`src-tauri/Cargo.toml` 和 `src-tauri/Cargo.lock` 为准；不要把旧版本更新记录当成当前架构。

## 开发命令

在项目根目录执行：

原生 Windows 构建需要 MSVC x64/x86 编译/链接组件和 Windows SDK。`link.exe not found` 属于工具链缺失或环境未加载，先补齐微软 Build Tools 并加载 x64 开发环境；不能用降级 Rust 依赖或切换 GNU target 绕过。安装与终端使用方式见 `README.md` / `README_EN.md` 的环境要求。

```bash
npm ci
npm run dev                 # 仅前端，端口 14200；浏览器中没有真实 Tauri IPC
npm run tauri dev           # 前端 + Tauri 后端
npm run build               # TypeScript 检查 + Vite 生产构建
npm run preview             # 预览前端生产构建
npm run tauri build         # Tauri 生产安装包
npm run lint
npm run lint:fix
npm run typecheck           # 应用、测试、构建配置与 E2E TypeScript 检查
npm test                    # Vitest 单元/组件测试
npm run bench               # 单独运行性能基准
npm run test:e2e             # Playwright 浏览器测试，不等同于原生 Tauri 验证
cargo test --manifest-path src-tauri/Cargo.toml
```

`Makefile` 使用 PowerShell：

- `make check`：前端全量 lint/类型检查、应用与本地分叉格式检查、Cargo check，以及应用/分叉默认和无默认 feature 的 Clippy；所有警告均作为错误。
- `make test`：前端行为回归、后端 all-target 测试、本地分叉默认和无默认 feature 的纯测试；系统剪贴板用例需显式选择并隔离执行。
- `make build`：Tauri release 安装包。
- `make run`：前端生产构建后运行 Rust 后端。
- `make format`：前端导入自动修复和应用/本地分叉 `cargo fmt`；从当前 `rustc --print sysroot` 找到工具，不强制要求 `rustup`。

## 项目架构

ElegantClipboard 是 Windows 剪贴板管理工具，采用 Tauri 2、React 19、TypeScript、Vite 8 和 Tailwind CSS 4。

### 前端

- `src/main.tsx`：主入口。立即渲染，异步初始化语言和 UI 设置；不是等待 `initLocale()` 完成后才渲染。
- `settings.html` / `src/settings-main.tsx`：设置窗口的独立构建入口。
- `src/App.tsx`：主窗口、搜索/筛选、分组与窗口事件。
- `src/components/ClipboardList.tsx`：列表生命周期、虚拟滚动和拖拽。
- `src/components/MasonryVirtualView.tsx`：瀑布流虚拟视图。
- `src/components/ClipboardItemCard.tsx`：卡片操作、资源操作能力消费、文本悬浮预览。
- `src/components/CardContentRenderers.tsx`：图片/文件内容、失效标记和卡片底栏。
- `src/components/CardSubComponents.tsx`：工具栏、文件详情、分组菜单。
- `src/hooks/`：拖拽、输入焦点、插件设置与动作等 Hook。
- `src/hooks/useItemResourceStatus.ts`：文件与缓存图片的资源可用性、批量检查、刷新和过期响应隔离。
- `src/stores/`：Zustand 状态。UI 设置保存到 SQLite `settings.ui_settings_json`，通过 Tauri 事件同步；没有使用 Zustand persist 中间件保存到 localStorage。
- `src/lib/theme-applier.ts`：主题、深浅色和窗口特效应用。
- `src/lib/file-preview-limits.ts`：文件图片预览大小限制，不负责判断文件是否存在。
- `src/i18n/`：三语文案和语言状态，详见该目录 README。
- `public/image-preview.html` / `public/text-preview.html`：独立原生悬浮窗口使用的静态页面。

### 后端

- `src-tauri/src/main.rs` / `lib.rs`：入口、插件注册、状态初始化、快捷键和 `invoke_handler` 注册。
- `commands/mod.rs`：`AppState` 与共享操作辅助函数。状态包含 `db`、`monitor`、`active_group_id` 和 `position_cache`，后两者通过 `Arc<parking_lot::Mutex<_>>` 共享。
- `commands/clipboard.rs`：剪贴板 CRUD、复制/粘贴、合并粘贴。
- `commands/settings.rs`：配置键值、监控、自启动、数据重置、系统强调色。
- `commands/file_ops.rs`：文件状态、资源管理器定位、路径粘贴、另存为和数据大小。
- `commands/window.rs`：窗口显示/隐藏、锁定、设置窗口和特效。
- `commands/preview.rs`：图片/文本悬浮预览及 lease。
- `commands/groups.rs`、`data_transfer.rs`、`sync.rs`、`translate.rs`：分组、导入/导出与迁移、WebDAV、翻译。
- `commands/window_utils.rs`：共享窗口样式辅助函数。
- `clipboard/monitor.rs`：剪贴板 watcher、捕获和串行 worker。
- `clipboard/handler.rs`：内容哈希、去重、资源落盘与历史清理。
- `clipboard/file_clipboard.rs`：文件路径、CF_HDROP/伴生格式和暂存副本解析。
- `database/mod.rs` / `schema.rs` / `repository.rs`：连接配置、迁移、表结构和仓库。
- `admin_launch.rs` / `task_scheduler.rs`：管理员偏好、自提权和按需提权任务。
- `positioning.rs`、`input_monitor.rs`、`keyboard_hook.rs`：位置计算、全局输入追踪和键盘状态。
- `main_thread.rs` / `webview_runtime.rs`：UI 线程操作与 WebView 运行时恢复。
- `updater.rs`、`proxy.rs`、`webdav/`、`tray/`：更新、代理、同步和原生托盘。

### IPC 与状态

前端使用 `@tauri-apps/api/core` 的 `invoke()`；剪贴板操作统一由 `src/lib/operation-feedback.ts` 的固定分发表执行。`save_file_as` 和 `show_in_explorer` 使用条目 `id`，后端重新查库及解析暂存路径，不能传入缓存的 `sourcePath` / `path` 作为执行授权。所有命令注册以 `lib.rs` 的 `invoke_handler` 为准。

- 后端成功捕获后发出 `clipboard-updated`；剪贴板 store 合并事件并重新获取权威轻量列表，以同步淘汰记录与当前筛选，而不是把完整条目直接插到列表头部。
- `window-shown` / `window-hidden` 驱动窗口状态、预览和可见文件卡片检查。
- `ui-settings-changed`、`locale-changed` 等事件同步多窗口状态。
- `system-accent-color-changed` 推送系统强调色。
- `paste-sound-immediate` / `paste-sound-success` 由后端统一通知粘贴音效。

仓库类型为 `ClipboardRepository`、`GroupRepository` 和 `SettingsRepository`。

### 操作检查、结果与错误归属

- `operation_error.rs` 定义原生 `OperationError { code, detail }`；稳定 snake_case code 用于三语理由，detail 只保留诊断上下文，不把内部对象、堆栈或凭据展示给用户。
- `prepare_item` / `prepare_text` / `prepare_files` / `prepare_merge` 在暂停监控、写剪贴板、隐藏窗口和发送输入之前完成资源/内容检查；键盘、快速/收藏快捷键及重复按键走同一检查。
- 合并操作逐项检查；缺失记录、缺失资源和损坏 payload 不会被静默省略。暂存副本可回退；只有实际可还原的虚拟格式才可用，descriptor-only 记录不可用。Windows 当前 HGLOBAL 路径不支持零字节原始格式/零 FileContents，在任何写入前明确拒绝，不填充、不跳过、不伪装成功；不支持的多虚拟 payload 合并同样拒绝。
- 前端 `OperationResult<T>` 区分 success、cancelled、failed；只有主操作成功才显示成功状态或更新依赖选择/排序。另存为取消不是失败，也不是成功；普通 bool false（如取消收藏）仍可表示成功值。
- 主操作成功后的排序/列表刷新错误单独提示，不改写成功结果，也不能提示主操作失败。过期或卸载后的异步完成不能给其它条目显示成功。
- `logError` 只记录开发日志；用户操作使用 `reportOperationError` / `reportUserError`，已有行内错误不叠加 toast，后台清理和加载不弹误导提示。快捷键错误在可见主窗口发事件，在隐藏时用本地化系统通知。
- 监控恢复使用 RAII，错误/展开也安排恢复；若恢复队列不可用立即恢复。粘贴输入发送失败时恢复原本可见窗口，不抢焦点、不改变锁定状态。成功仅代表系统写入/输入发送完成，不保证目标程序接受该格式。


## 国际化（i18n）

- 支持 `zh-CN`（默认）、`en`、`zh-TW`。
- 语言保存到数据库 `settings.language`，通过 `locale-changed` 同步。
- React 使用 `useTranslation()`；非 React 上下文使用 `t()`。
- `messages/{locale}/core.ts` 是基础文案，`locales/{locale}-ext.ts` 是扩展文案源，经 `messages/{locale}/extended.ts` 合并。
- 新增 UI 字符串必须走 `t()`，三语同步添加 key。
- `src/test/setup.ts` 每个用例前重置为 `zh-CN`；组件测试用 `t("key")` 断言，不硬编码中文。
- 托盘文案由 `src-tauri/src/tray/tray_i18n.rs` 原生渲染，随语言设置和 `update_tray_language` 更新。

```tsx
import { useTranslation, t } from "@/i18n";

function Component() {
  const { t } = useTranslation();
  return <span>{t("app.searchPlaceholder")}</span>;
}

t("groups.all", { count: 3 });
```

## 窗口与路由

主窗口配置见 `src-tauri/tauri.conf.json`：无边框、透明、初始隐藏、置顶、不显示在任务栏，默认不获取焦点。运行时在显示/隐藏等路径设置不可聚焦；搜索等操作可临时恢复焦点。

- 位置计算在 `positioning.rs`，支持光标位置与显示器边界；定位配置缓存于 `AppState.position_cache`。
- 点击外部隐藏依赖 `input_monitor.rs` 的 Win32 全局输入监控，不依赖窗口失焦事件；窗口隐藏后关闭鼠标监控。
- 光标坐标使用 `AtomicI64`，避免高频坐标更新获取 Mutex。
- `set_window_pinned` 控制窗口锁定状态，影响点击外部隐藏和粘贴后隐藏；不是修改 `alwaysOnTop` 的开关。
- 主入口 `/` 渲染 `App`；`/editor` 或 `/editor.html` 懒加载 `TextEditor`；`/translate-result` 懒加载 `TranslateResult`。
- 设置窗口加载独立 `settings.html`，不再由主入口 pathname 路由到 `Settings`。
- 图片和文本预览窗口加载 `public/` 中对应静态 HTML。

悬浮预览命令在 `commands/preview.rs`，不是旧文档中的 `lib.rs:show_image_preview`。前端使用 lease 避免过期请求覆盖新预览；主窗口隐藏时关闭预览。图片预览支持左右定位和缩放，`previewZoomStep` 默认 15；悬浮延时 `hoverPreviewDelay` 当前默认 128ms。其它默认值以 `src/stores/ui-settings.ts` 为准。

## 管理员启动与自启动

- 管理员偏好保存在安装目录 `config.json` 的 `run_as_admin`，不再通过 `AppCompatFlags\Layers` 设置当前偏好。
- 应用启动时根据偏好检查权限；优先使用有效的 `ElegantClipboard_AdminElevation` 计划任务自提权，否则走 UAC。
- 提权任务通过 Windows Task Scheduler COM API 创建，无登录触发器，仅作为按需免 UAC 启动工具。
- 开机自启动始终使用 `tauri-plugin-autostart` 的注册表 `Run` 项，偏好保存到 `settings.autostart_enabled`；不按管理员模式切换成任务计划自启动。
- 启动时清理旧版 `ElegantClipboard_AutoStart` 任务和旧兼容性注册表项。
- 实现来源：`admin_launch.rs`、`task_scheduler.rs`、`commands/settings.rs` 与 `lib.rs`。

Win+V 替换在 `win_v_registry.rs` 与 `lib.rs`：修改 Explorer 的 `DisabledHotkeys` 禁用系统快捷键，再注册应用快捷键；注册表变更需要重启 Explorer 生效。快捷键字符串解析在 `shortcut.rs`。

## 数据存储与查询

默认数据目录为可执行文件所在目录，可通过设置修改数据目录。安装版目录可能需要写入权限；便携模式用 exe 同级是否存在 `uninstall.exe` 判断，并不保证任意目录都有写权限。

| 类型 | 路径 |
|---|---|
| 启动配置 | `<exe目录>\config.json`，不随数据迁移移动 |
| 数据库 | `<数据目录>\clipboard.db`，以及 SQLite WAL/SHM 文件 |
| 图片 | `<数据目录>\images\` |
| 来源应用图标 | `<数据目录>\icons\` |
| 文件暂存副本 | `<数据目录>\staged\` |
| 文件日志 | `<数据目录>\app.log`，由日志配置控制 |

路径来源为 `config.rs` 与 `database/mod.rs`。数据迁移使用 SQLite Backup 一致快照及资产复制，重启时补齐最终快照、成功打开目标库后切换路径；实现见 `config.rs`、`commands/data_transfer.rs` 和 `lib.rs`。

### 数据库

- 表为 `clipboard_items`、`groups`、`settings`。
- `clipboard_items` 包含 `group_id`、`file_payload`、内容/语义哈希、图片尺寸、访问统计等。
- `content_type` 在应用层使用 `ContentType`；数据库没有旧 CHECK 限制。
- 去重由应用策略执行；内容/语义哈希是分组相关的普通索引，允许 `always_new` 策略保存重复内容，不是全局 UNIQUE 约束。
- 使用更新时间触发器；置顶/收藏采用部分索引，时间/访问统计/排序采用对应索引。
- 写连接和只读连接分别用 `Arc<parking_lot::Mutex<Connection>>` 保护；同一个读连接上的查询仍会串行，不是所有读操作互不阻塞。
- WAL 支持读写并发；连接 cache_size 分别为 -64000 / -32000 KiB，mmap_size 为 256MiB，临时表使用内存。配置不是运行时内存占用保证。

### 列表与搜索

- 使用 SQL `LIKE`，无需 FTS5。
- 常规列表投影已排除完整文本、HTML、RTF 和 `file_payload`，减少 IPC 和前端正文驻留。
- 搜索查询内部读取文本用于关键词上下文，返回前清空 `text_content`。
- 完整正文按 ID 通过详情命令读取。
- 列表查询不检查磁盘，也不携带资源可用性字段；可见卡片通过独立 Hook 和状态命令检查。

### 资源可用性与预览状态

- `useItemResourceStatus` 返回资源快照 `unknown` / `checking` / `available` / `unavailable` 与独立的 `clipboardUsable`；状态只存在于运行时，不写入数据库。
- 文件状态区分 `all_exist`（物理/可还原资源存在）和必填 `clipboard_usable`（伴生 payload 是否支持剪贴板写入），不能把不支持的复制格式当作文件丢失。文件通过 `batch_get_item_file_status` 查询，缓存图片通过 `check_files_exist` 查询；文本/HTML/RTF/URL 不做磁盘或网络检查。
- 卡片加载、相关源信息变化、主窗口再次显示和右键菜单打开时检查。拖拽覆盖卡片不额外查询；没有轮询或文件系统监听器，不承诺文件移动瞬间自动更新。
- 原始文件路径失效时，后端可回退到 `file_payload` 中仍存在的暂存副本；检查失败返回未知状态，不当作路径缺失。过期响应不能覆盖新来源或新请求的状态。
- 文件和缓存图片缺失才显示「已失效」。复制/粘贴使用存在性与剪贴板能力；另存为/定位使用实际磁盘路径，所以不支持的伴生格式不阻止已有文件的字节保存。路径文本粘贴、删除、收藏与分组不依赖预览成功。
- 预览加载/解码失败只显示「预览加载失败」，不改变资源可用性；文件仍存在时可以另存为。大小限制只跳过预览，不跳过路径检查。预览加载状态与错误缓存留在渲染组件，不能自行宣布资源失效。
- 文件图片预览本地默认 50MiB，UNC 路径固定 10MiB；本地设置无限制时仍有 100MiB 安全上限。实现见 `src/lib/file-preview-limits.ts` 与 `src-tauri/src/file_preview_limits.rs`。
- `save_file_as` 使用条目 id 重新解析当前可读资源；仅保存单个普通文件，文件字节保存不依赖预览解码。取消单独返回，源/目标/权限错误使用稳定 code。错误原因按当前语言显示，详细原生上下文仅写诊断日志。

## 剪贴板捕获与粘贴

- 读写/监听使用本地 `clipboard-rs` 分叉，支持 text、HTML、RTF、image、files 和 watcher；补丁以分叉代码/文档为准。
- watcher 捕获后把独立内容按顺序交给串行 worker；捕获重试期间重新校验来源、暂停状态与剪贴板序列，丢弃不稳定快照。
- 图片先写捕获临时 PNG，worker 流式计算哈希并将临时文件移动到内容寻址缓存，再提交条目；不是为每张图片启动后台线程并忽略写入结果。
- 内部暂停使用 `Arc<AtomicU32>` 计数，避免重叠操作提前恢复；用户暂停状态单独保存。
- Windows 粘贴使用 `SendInput`，可选 Ctrl+V（默认）或 Shift+Insert；音效由后端统一发事件。

## 主题、虚拟化与工具配置

- 主题支持 `default` / `emerald` / `cyan` / `system`；由各入口的 `initTheme()` 初始化并订阅 store，不是由 `App.tsx` 初始化。
- 系统强调色由后端注册表读取及 `WM_SETTINGCHANGE` 监听推送，前端更新 CSS 变量。
- 语义颜色、间距和阴影在 `public/surface-tokens.css`，由 HTML 直链加载；`src/index.css` 负责 Tailwind utilities 与组件样式。
- 窗口特效为 `none` / `mica` / `acrylic` / `tabbed`，后端通过 `window-vibrancy` 与 `WS_EX_LAYERED` 处理。
- 初始化应用特效失败时 CSS 回退 `none`，保留持久化偏好；设置页主动切换失败时恢复之前的状态。不要声称所有失败都会重置配置为 `none`。
- 列表使用 react-virtuoso，预渲染缓冲为上下各 400px，`defaultItemHeight` 根据卡片行数估算；瀑布流使用独立虚拟视图。虚拟化限制挂载 DOM，不保证总内存恒定或特定速度倍率。
- 使用 `parking_lot` 保护共享状态；锁大小和性能差异依平台/负载而变，不写未经测量的倍数承诺。
- 托盘左键切换窗口；菜单包含监控暂停/恢复、临时禁用/恢复快捷键、设置、检查更新、重启和退出。
- ESLint 10 使用 flat config，`@typescript-eslint/parser`、`eslint-plugin-import-x` 与 TypeScript resolver 管理导入排序。
- `src-tauri/.cargo/config.toml` 未设置有效的 `target-dir`，使用 Cargo 默认构建目录；dev debug=1，依赖 opt-level=2。

命名约定：Rust 用 snake_case/PascalCase；TypeScript 用 camelCase/PascalCase；React 组件文件用 PascalCase.tsx，其它沿用仓库既有风格。
