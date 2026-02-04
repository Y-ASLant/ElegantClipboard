# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 开发命令

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
```

## 项目架构

ElegantClipboard 是一个基于 Tauri 2.0 的剪贴板管理工具，采用 React 前端 + Rust 后端的架构。

### 整体结构

```
src/                    # React 前端
├── components/         # shadcn/ui 组件
├── pages/             # 设置页面
├── stores/            # Zustand 状态管理
└── main.tsx           # 入口点（简单路由）

src-tauri/              # Rust 后端
├── src/
│   ├── main.rs         # 入口点
│   ├── lib.rs          # 核心库（Tauri 命令注册与设置）
│   ├── config.rs       # 配置文件管理
│   ├── commands/       # Tauri 命令处理器
│   ├── clipboard/      # 剪贴板监控模块（monitor.rs, handler.rs）
│   ├── database/       # SQLite 数据库（schema.rs, repository.rs）
│   ├── tray/           # 系统托盘
│   ├── keyboard_hook.rs    # 窗口状态追踪
│   ├── input_monitor.rs    # 全局鼠标监控（点击外部检测）
│   └── win_v_registry.rs   # Win+V 替换（注册表）
```

### Tauri 命令架构

**后端（Rust）**：所有命令在 `src-tauri/src/lib.rs:480-533` 通过 `invoke_handler` 注册。

```rust
.invoke_handler(tauri::generate_handler![
    // 窗口管理
    show_window,
    hide_window,
    set_window_visibility,
    // 剪贴板操作
    commands::get_clipboard_items,
    commands::toggle_pin,
    commands::copy_to_clipboard,
    // ... 更多命令
])
```

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
- `focus: false` - **运行时设置**（`lib.rs:470` `set_focusable(false)`）
- `alwaysOnTop: true` - 置顶显示
- `skipTaskbar: true` - 不显示在任务栏
- `visibleOnAllWorkspaces: true` - 所有工作区可见

**窗口切换逻辑**（`lib.rs:244-266`）：
- 使用 `always_on_top` 技巧确保窗口出现
- 管理 `WindowState`（Hidden/Visible）通过 `keyboard_hook.rs`

**点击外部隐藏**：
- 由于窗口不可获取焦点，`onFocusChanged` 不会触发
- 使用 `input_monitor.rs` 中的全局鼠标监控（`rdev`）
- 仅在窗口可见时启用监控，降低 CPU 占用
- 使用 `AtomicI64` 实现无锁光标位置追踪

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
- `clipboard_items` - 剪贴板历史（支持 FTS5）
- `categories` - 用户分类
- `tags` - 标签系统
- `item_tags` - 多对多关系
- `settings` - 键值对配置

**特性**：
- FTS5 全文搜索（`text_content`、`preview` 字段）
- 内容哈希去重（`content_hash` UNIQUE 约束）
- 自动时间戳更新触发器
- 性能索引：`created_at`、`is_pinned`、`is_favorite`、`category_id`、`content_type`、`content_hash`

## 前端路由

位置：`src/main.tsx:38-46`

使用简单的基于路径的路由：
- `/` 或默认 → 主窗口（`App` 组件）
- `/settings` 或 `/settings.html` → 设置窗口（`Settings` 组件）

## 剪贴板处理

- **图片**：使用 `clipboard-rs`（更好的 Windows 支持）
- **文本**：使用 `arboard`
- **粘贴**：使用 `enigo` 模拟 Ctrl+V

## 性能优化

- **虚拟列表**：`@tanstack/react-virtual` 处理万级记录
- **鼠标事件**：无锁原子操作，仅窗口可见时监控
- **SQLite**：WAL 模式，内存临时存储

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

位置：`src-tauri/src/lib.rs:24-113`

- 支持字母 A-Z、数字 0-9、功能键 F1-F12
- 修饰符：`CTRL`、`ALT`、`SHIFT`、`WIN`/`SUPER`/`META`/`CMD`
- 特殊键：`SPACE`、`TAB`、`ENTER`、`ESC`、方向键等
- 解析函数：`parse_shortcut()` → `Shortcut` 对象
