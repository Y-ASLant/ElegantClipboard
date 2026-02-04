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

# 构建生产版本
npm run tauri build
```

## 项目架构

ElegantClipboard 是一个基于 Tauri 2.0 的剪贴板管理工具，采用 React 前端 + Rust 后端的架构。

### 整体结构

```
src/                    # React 前端
src-tauri/              # Rust 后端
├── src/
│   ├── main.rs         # 入口点
│   ├── lib.rs          # 核心库（Tauri 命令注册与设置）
│   ├── config.rs       # 配置文件管理
│   ├── commands/       # Tauri 命令处理器
│   ├── clipboard/      # 剪贴板监控模块
│   ├── database/       # SQLite 数据库（仓储模式）
│   ├── tray/           # 系统托盘
│   ├── keyboard_hook.rs    # 全局快捷键钩子
│   ├── input_monitor.rs    # 输入监控（点击外部检测）
│   └── win_v_registry.rs   # Win+V 替换功能
```

### Tauri 命令架构

**后端（Rust）**：所有命令在 `src-tauri/src/lib.rs:483-536` 通过 `invoke_handler` 注册。

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

**2. 服务模式（Service Pattern）**
- 位置：`src-tauri/src/clipboard/monitor.rs`
- `ClipboardMonitor` 管理剪贴板监控生命周期
- 使用独立线程运行，通过 Tauri 事件向前端推送更新

**3. 状态管理**
- **前端**：Zustand stores（`src/stores/`）
  - `clipboard.ts` - 剪贴板数据状态
  - `ui-settings.ts` - UI 设置（带持久化和多窗口同步）
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
- `focus: false` - 不可获取焦点（运行时设置）
- `alwaysOnTop: true` - 置顶显示
- `skipTaskbar: true` - 不显示在任务栏

**点击外部隐藏**：由于窗口不可获取焦点，使用 `input_monitor.rs` 中的全局鼠标监控检测外部点击。

## Win+V 替换功能

通过修改 Windows 注册表禁用系统 Win+V：
- 注册表项：`HKEY_CURRENT_USER\Software\Microsoft\Clipboard\EnableClipboardHotKey`
- 需要重启 Explorer 生效
- 位置：`src-tauri/src/win_v_registry.rs`

## 数据存储

- **配置文件**：`%LOCALAPPDATA%\ClipboardManager\config.json`
- **数据库**：`%LOCALAPPDATA%\ClipboardManager\clipboard.db`
- **图片缓存**：`%LOCALAPPDATA%\ClipboardManager\images\`

配置文件支持自定义数据路径（`data_path` 字段），并支持数据迁移功能。

## 数据库架构

位置：`src-tauri/src/database/schema.rs`

**表结构**：
- `clipboard_items` - 剪贴板历史
- `categories` - 用户分类
- `tags` - 标签系统
- `item_tags` - 多对多关系
- `settings` - 键值对配置

**特性**：
- FTS5 全文搜索（`text_content`、`preview` 字段）
- 内容哈希去重（UNIQUE 约束）
- 自动时间戳更新触发器

## 命名约定

- **Rust**：`snake_case` 函数/变量，`PascalCase` 类型
- **TypeScript**：`camelCase` 函数/变量，`PascalCase` 类型/组件
- **文件**：React 组件用 `PascalCase.tsx`，其他用 `kebab-case.ts`

## 编译缓存

Rust 编译缓存目录配置在 `src-tauri/.cargo/config.toml`，避免 `target` 目录占用过多空间。
