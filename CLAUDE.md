# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

### 开发命令

```bash
# 安装依赖
npm install

# 启动开发模式（前端 + 后端）
npm run tauri dev

# 仅启动前端开发服务器（端口 14200）
npm run dev

# 构建前端
npm run build

# 预览生产构建
npm run preview

# 构建生产版本
npm run tauri build

# 代码检查
npm run lint

# 自动修复代码问题
npm run lint:fix

# Rust 测试
cd src-tauri && cargo test
```

### 最近更新

**v0.x 新功能**：
- **窗口毛玻璃特效**：支持 Mica / Acrylic / Tabbed 三种 Windows 11 DWM 背景特效，Win10 自动回退
- **工具栏自定义**：可配置工具栏按钮的显示、隐藏和排序
- **数据清理**：三级数据清理操作（清空历史 / 恢复默认配置 / 重置所有数据）
- **图片预览窗口**：独立悬浮窗口显示图片，支持缩放和左右定位
- **动态主题系统**：支持 default/emerald/cyan/system 四种主题，system 主题跟随 Windows 系统强调色
- **搜索高亮**：搜索结果自动提取关键词上下文，提升搜索体验
- **一键回到顶部**：滚动超过 200px 显示悬浮按钮
- **窗口状态重置**：关闭时自动重置搜索和滚动位置（可通过 `autoResetState` 配置）
- **文件操作增强**：文件有效性检查、另存为、显示在资源管理器等功能

## 项目架构

ElegantClipboard 是一个基于 Tauri 2.0 的剪贴板管理工具，采用 React 前端 + Rust 后端的架构。

### 整体结构

```
src/                    # React 前端
├── components/
│   ├── ClipboardList.tsx        # 虚拟滚动列表
│   ├── ClipboardItemCard.tsx    # 卡片组件
│   ├── CardContentRenderers.tsx # 内容渲染器（图片/文件预览）
│   ├── settings/                # 设置页面组件
│   └── ui/                      # shadcn/ui 基础组件（Radix 封装）
├── hooks/
│   └── useSortableList.ts       # 拖拽排序 Hook
├── lib/
│   ├── constants.ts             # 常量（工具栏按钮注册表等）
│   ├── theme-applier.ts         # 主题/窗口特效应用器
│   └── utils.ts                 # 工具函数
├── stores/            # Zustand 状态管理
└── main.tsx           # 入口点（简单路由）

src-tauri/              # Rust 后端
├── src/
│   ├── main.rs             # 入口点
│   ├── lib.rs              # 核心库（Tauri 命令注册、窗口管理）
│   ├── config.rs           # 配置文件管理
│   ├── shortcut.rs         # 快捷键解析模块
│   ├── positioning.rs      # 窗口定位（多显示器支持）
│   ├── admin_launch.rs     # 管理员启动功能
│   ├── keyboard_hook.rs    # 窗口状态追踪
│   ├── input_monitor.rs    # 全局鼠标监控（点击外部检测）
│   ├── win_v_registry.rs   # Win+V 替换（注册表）
│   ├── commands/           # Tauri 命令（按功能拆分）
│   │   ├── mod.rs          # AppState 定义 + 模块导出
│   │   ├── clipboard.rs    # 剪贴板 CRUD 命令
│   │   ├── settings.rs     # 设置/监控/自启动命令
│   │   └── file_ops.rs     # 文件操作命令（并行检查）
│   ├── clipboard/          # 剪贴板监控模块
│   ├── database/           # SQLite 数据库
│   └── tray/               # 系统托盘
```

### Tauri 命令架构

**后端（Rust）**：命令按功能模块化组织，在 `lib.rs` 通过 `invoke_handler` 注册。

**命令模块**（`src-tauri/src/commands/`）：
- `clipboard.rs` - 剪贴板 CRUD：`get_clipboard_items`、`get_clipboard_item`、`get_clipboard_count`、`toggle_pin`、`toggle_favorite`、`move_clipboard_item`、`delete_clipboard_item`、`clear_history`、`clear_all_history`、`copy_to_clipboard`、`paste_content`
- `settings.rs` - 设置/监控：`get_setting`、`set_setting`、`get_all_settings`、`pause_monitor`、`resume_monitor`、`get_monitor_status`、`optimize_database`、`vacuum_database`、`reset_settings`、`reset_all_data`、`select_folder_for_settings`、`open_data_folder`、`is_autostart_enabled`、`enable_autostart`、`disable_autostart`、`get_system_accent_color`
- `file_ops.rs` - 文件操作：`check_files_exist`（rayon 并行）、`show_in_explorer`、`paste_as_path`、`get_file_details`、`save_file_as`

**窗口/系统命令**（`lib.rs`）：
- 窗口管理：`show_window`、`hide_window`、`set_window_pinned`、`set_window_effect`、`open_settings_window`
- 管理员启动：`is_admin_launch_enabled`、`enable_admin_launch`、`is_running_as_admin`
- 快捷键：`update_shortcut`、`enable_winv_replacement`

**前端（TypeScript）**：通过 `@tauri-apps/api/core` 的 `invoke()` 函数调用：

```typescript
import { invoke } from "@tauri-apps/api/core";

const items = await invoke<ClipboardItem[]>("get_clipboard_items", {
    search: query,
    limit: 100
});
```

### 关键架构模式

**1. 仓储模式（Repository Pattern）**
- 位置：`src-tauri/src/database/repository.rs`
- `ClipboardRepository`、`CategoryRepository`、`SettingsRepository`
- 提供数据库 CRUD 操作的抽象层
- 使用 `Arc<Mutex<Connection>>` 实现线程安全

**2. 服务模式（Service Pattern）**
- 位置：`src-tauri/src/clipboard/monitor.rs`
- `ClipboardMonitor` 管理剪贴板监控生命周期
- 使用独立线程运行（`clipboard-master`）
- 通过 Tauri 事件向前端推送更新：`app.emit("clipboard-updated", id)`

**3. 状态管理**
- **前端**：Zustand stores（`src/stores/`）
  - `clipboard.ts` - 剪贴板数据状态
  - `ui-settings.ts` - UI 设置（持久化 + 多窗口同步）
    - `cardMaxLines` - 卡片最大行数
    - `showTime/CharCount/ByteSize` - 元数据显示开关
    - `imagePreviewEnabled` - 图片预览开关
    - `previewZoomStep` - 缩放步进
    - `previewPosition` - 预览位置（auto/left/right）
    - `imageAutoHeight/imageMaxHeight` - 图片高度设置
    - `colorTheme` - 颜色主题（default/emerald/cyan/system）
    - `windowEffect` - 窗口特效（none/mica/acrylic/tabbed）
    - `toolbarButtons` - 工具栏按钮配置
    - `autoResetState` - 关闭时重置状态
- **后端**：`AppState` 通过 Tauri State 共享
  ```rust
  pub struct AppState {
      pub db: Database,
      pub monitor: ClipboardMonitor,
  }
  ```

### 事件驱动通信

**后端 → 前端**：
```rust
// Rust
app_handle.emit("clipboard-updated", id)?;

// TypeScript
import { listen } from "@tauri-apps/api/event";
listen("clipboard-updated", (event) => { ... });
```

**前端 ↔ 前端**（多窗口同步）：
```typescript
import { emit, listen } from "@tauri-apps/api/event";
emit("ui-settings-changed", state);
listen("ui-settings-changed", (event) => { ... });
```

## 窗口配置

主窗口（`main`）采用特殊配置以支持全局快捷键：
- `decorations: false` - 无边框窗口
- `focus: false` - 运行时设置 `set_focusable(false)`
- `alwaysOnTop: true` - 置顶显示
- `skipTaskbar: true` - 不显示在任务栏

**窗口定位**（`positioning.rs`）：
- `get_cursor_position()` - Windows API 获取光标位置
- `position_at_cursor()` - 智能边界检测，支持多显示器
- `calculate_best_position()` - 计算最佳窗口位置

**点击外部隐藏**：
- 由于窗口不可获取焦点，`onFocusChanged` 不会触发
- 使用 `input_monitor.rs` 中的全局鼠标监控（`rdev`）
- 仅在窗口可见时启用监控，降低 CPU 占用
- 使用 `AtomicI64` 实现无锁光标位置追踪

**窗口置顶锁定**（`set_window_pinned`）：
- 运行时控制窗口是否可被其他置顶窗口覆盖

## 管理员启动

位置：`src-tauri/src/admin_launch.rs`

通过 Windows 注册表 `AppCompatFlags\Layers` 实现：
- `is_admin_launch_enabled()` - 检查是否启用
- `enable_admin_launch()` / `disable_admin_launch()` - 启用/禁用
- `is_running_as_admin()` - 检查当前权限
- `restart_app()` - 支持 UAC 提权的重启

## 开机自启动（双机制）

位置：`src-tauri/src/commands/settings.rs`、`src-tauri/src/task_scheduler.rs`

Windows 的注册表 `Run` 键会静默跳过需要 UAC 提权的程序，因此管理员模式下自启动需要使用不同机制：
- **普通模式**：`tauri-plugin-autostart`（写入 `HKCU\..\Run` 注册表）
- **管理员模式**：Windows 任务计划程序（`schtasks`，`HIGHEST` 运行级别）

**自动迁移**（`lib.rs` setup）：应用启动时自动检测并切换机制：
- 管理员模式 + 已提权 + 旧注册表自启动 → 迁移到任务计划程序
- 普通模式 + 存在任务计划程序 → 迁移到注册表 `Run`

## Win+V 替换功能

通过修改 Windows 注册表禁用系统 Win+V：
- 注册表项：`HKEY_CURRENT_USER\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\Advanced\DisabledHotkeys`
- 添加 'V' 值禁用系统快捷键
- 需要重启 Explorer 生效
- 位置：`src-tauri/src/win_v_registry.rs`

## 数据存储

- **配置文件**：`%LOCALAPPDATA%\ElegantClipboard\config.json`
- **数据库**：`<数据目录>\clipboard.db`
- **图片缓存**：`<数据目录>\images\`

配置文件支持自定义数据路径（`data_path` 字段），并支持数据迁移功能（`config.rs::migrate_data`）。

## 数据库架构

位置：`src-tauri/src/database/schema.rs`

**表结构**：
- `clipboard_items` - 剪贴板历史
- `settings` - 键值对配置

**特性**：
- 内容哈希去重（`content_hash` UNIQUE 约束）
- 自动时间戳更新触发器
- 性能索引：`created_at`、`is_pinned`、`is_favorite`、`content_type`、`sort_order`、`access_count`
- 图片元数据：`image_width`、`image_height` 字段
- 运行时字段：`files_valid`（文��有效性检查结果，不存储）

**搜索实现**：
- 使用 SQL `LIKE` 查询，无需 FTS5
- 搜索时提取关键词上下文（`extract_keyword_context`），替换 `preview` 字段
- `text_content` 在搜索结果中置空以减少 IPC 传输
- 文件类型项搜索时跳过磁盘检查（`fill_files_valid` 仅在浏览时执行）

## 前端路由

位置：`src/main.tsx:38-46`

使用简单的基于路径的路由：
- `/` 或默认 → 主窗口（`App` 组件）
- `/settings` 或 `/settings.html` → 设置窗口（`Settings` 组件）
- `/image-preview.html` → 图片预览窗口（独立透明窗口）

## 图片预览窗口

位置：`src-tauri/src/lib.rs:show_image_preview`

独立透明悬浮窗口，用于大图预览：
- 窗口定位自动计算，填充主窗口左侧或右侧可用空间
- 支持缩放（previewZoomStep 配置，默认 15%）
- 鼠标离开预览区域或主窗口隐藏时自动隐藏
- CSS `object-fit` 处理图片尺寸，窗口大小不变

## 剪贴板处理

- **图片**：使用 `clipboard-rs`（更好的 Windows 支持）
- **文本**：使用 `arboard`
- **粘贴**：使用 `enigo` 模拟 Ctrl+V

## 关键依赖

**Rust 后端**：
- `tauri 2` - 应用框架
- `tauri-plugin-*` - 全局快捷键、自动启动、对话框、通知
- `rusqlite` - SQLite 数据库（bundled 特性）
- `tokio` - 异步运行时
- `rayon` - 并行处理（文件检查）
- `clipboard-master` - 剪贴板监控
- `clipboard-rs` / `arboard` - 剪贴板操作
- `enigo` - 键盘模拟粘贴
- `window-vibrancy` - 窗口背景特效（Mica/Acrylic/Tabbed）
- `wry` - WebView2 版本检测
- `rdev` - 全局鼠标/键盘监控
- `parking_lot` - 高性能锁
- `blake3` - 内容哈希去重
- `windows` / `winreg` - Windows API 和注册表
- `tracing` - 日志系统

**前端**：
- React 19 + TypeScript
- Vite 7 - 构建工具
- Tailwind CSS 4 - 样式
- Zustand 5 - 状态管理（persist 中间件）
- react-virtuoso - 虚拟列表
- @dnd-kit - 拖拽排序
- OverlayScrollbars - 自定义滚动条
- Fluent UI Icons - 图标库
- Radix UI - 无障碍组件基础（Dialog、Tooltip、Context Menu、Switch 等）

## 性能优化

项目以**低占用、高性能、完全本地化**为核心设计理念，面对万条数据依旧保持高性能低占用。

### 数据库层

位置：`src-tauri/src/database/mod.rs`

**读写连接分离**：
```rust
pub struct Database {
    write_conn: Arc<Mutex<Connection>>,  // 写操作专用
    read_conn: Arc<Mutex<Connection>>,   // 读操作专用
}
```
- WAL 模式支持读写并发，读操作互不阻塞
- 写连接：64MB 缓存，读连接：32MB 缓存
- `mmap_size = 256MB` 内存映射加速文件访问
- `temp_store = MEMORY` 临时表存内存

**索引优化**（`schema.rs`）：
- 部分索引：`WHERE is_pinned = 1` 仅索引匹配行，减小索引体积
- 降序索引：`created_at DESC` 优化常见查询模式
- 访问统计索引：`(access_count DESC, last_accessed_at DESC)` 支持常用内容排序

### 无锁设计

位置：`src-tauri/src/input_monitor.rs`

**原子变量追踪光标**：
```rust
static CURSOR_X: AtomicI64 = AtomicI64::new(0);
static CURSOR_Y: AtomicI64 = AtomicI64::new(0);
```
- 鼠标移动事件每秒触发数百次，使用 `AtomicI64` 避免锁竞争
- `Ordering::Relaxed` 最小化同步开销

**条件监控**：
- 窗口隐藏时完全跳过鼠标位置处理，CPU 占用趋近于零
- `MOUSE_MONITORING_ENABLED` 原子开关控制

### 剪贴板监控

位置：`src-tauri/src/clipboard/monitor.rs`

**原子暂停计数器**：
```rust
pause_count: Arc<AtomicU32>  // 计数器而非布尔值
```
- 解决多操作重叠时的竞态条件：A 暂停 → B 暂停 → A 恢复 → B 仍在运行
- 计数器确保所有操作完成后才恢复监控

**图片异步写入**（`handler.rs`）：
```rust
std::thread::spawn(move || {
    std::fs::write(&image_path, data).ok();
});
```
- 文件 I/O 在后台线程执行，不阻塞剪贴板监控
- BLAKE3 哈希生成文件名，自动去重

### 前端虚拟化

位置：`src/components/ClipboardList.tsx`

**react-virtuoso 配置**：
- `increaseViewportBy: { top: 400, bottom: 400 }` 预渲染缓冲区
- `defaultItemHeight` 预计算避免布局抖动
- `useMemo` / `useCallback` 防止不必要的重渲染

**万级数据表现**：
- DOM 节点数 = 可视区域项数（约 10-20 个），而非全部数据
- 滚动时仅更新可见项，内存占用恒定

### 锁优化

全局使用 `parking_lot` 替代 `std::sync`：
- Mutex 体积：40 字节 → 1 字节
- 无锁中毒机制，API 更简洁
- 自旋等待减少系统调用，竞争场景下性能提升 2-3 倍

## 动态主题系统

位置：`src/lib/theme-applier.ts`

支持四种颜色主题：`default`、`emerald`、`cyan`、`system`

**System 主题工作原理**：
- Windows 端监听注册表 `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Explorer\Accent\AccentColorMenu`
- 后台线程监听 `WM_SETTINGCHANGE` 消息（参数 `ImmersiveColorSet`），实时推送 `system-accent-color-changed` 事件
- 前端接收 HSL 格式颜色值，应用 CSS 变量 `--system-accent-h/s/l`
- 零 React 开销：模块级初始化，直接 DOM 操作

**主题应用流程**：
1. `initTheme()` 在窗口加载时调用（`App.tsx`）
2. 读取 zustand store 的 `colorTheme`
3. 若为 `system`，获取并应用系统强调色
4. 监听 store 变化和系统主题变化事件

**窗口特效**：
- 支持 `none` / `mica` / `acrylic` / `tabbed` 四种模式
- 后端通过 `window-vibrancy` crate 调用 DWM API
- 切换特效时操作 `WS_EX_LAYERED` 扩展窗口样式
- CSS 使用 `data-window-effect` 属性选择器覆盖背景色为半透明
- Windows 10 不支持 Mica/Tabbed 时自动回退到 `none`，前端 catch 后恢复 CSS 属性和 store 状态
- 启动时在 setup 中立即设置 `WS_EX_LAYERED` 防止透明闪烁

## 命名约定

- **Rust**：`snake_case` 函数/变量，`PascalCase` 类型
- **TypeScript**：`camelCase` 函数/变量，`PascalCase` 类型/组件
- **文件**：React 组件用 `PascalCase.tsx`，其他用 `kebab-case.ts`/`snake_case.rs`

## 编译缓存

Rust 编译缓存目录配置在 `src-tauri/.cargo/config.toml`：
- `target-dir = "H:/Rust_Cache"` - 自定义缓存位置
- `debug = 1` - 减少调试信息大小
- `opt-level = 2` - 开发模式下优化依赖

## 系统托盘

位置：`src-tauri/src/tray/mod.rs`

- 左键点击：切换窗口可见性
- 菜单项：显示/隐藏、暂停/恢复监控、退出

## 快捷键解析

位置：`src-tauri/src/shortcut.rs`

- 支持字母 A-Z、数字 0-9、功能键 F1-F12
- 修饰符：`CTRL`、`ALT`、`SHIFT`、`WIN`/`SUPER`/`META`/`CMD`
- 特殊键：`SPACE`、`TAB`、`ENTER`、`ESC`、方向键等
- 解析函数：`parse_shortcut()` → `Shortcut` 对象
- 快捷键注册：通过 `tauri-plugin-global-shortcut` 在运行时动态注册
- Win+V 替换模式：检测 `win_v_registry::is_win_v_hotkey_disabled()` 自动切换到 Win+V

## ESLint 配置

位置：项目根目录 `eslint.config.js`

- ESLint 9 flat config 格式
- `@typescript-eslint/parser` 解析 TypeScript
- `eslint-plugin-import` + `eslint-import-resolver-typescript` 管理导入排序
- 运行 `npm run lint` 检查代码规范
- 运行 `npm run lint:fix` 自动修复问题
