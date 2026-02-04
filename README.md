# ElegantClipboard

高性能 Windows 剪贴板管理工具，基于 Tauri 2.0 构建，可完全替代系统剪贴板。

## 功能特性

- **无限历史记录** - 自动记录所有复制内容，随时回溯
- **快速搜索** - 实时搜索历史记录
- **图片支持** - 支持复制和粘贴图片
- **点击即粘贴** - 点击记录直接粘贴到活动窗口
- **全局快捷键** - Alt+C 或 Win+V 快速唤出
- **置顶/收藏** - 重要内容可置顶或收藏
- **本地存储** - SQLite 数据库安全存储
- **系统托盘** - 最小化到托盘，开机自启
- **现代界面** - shadcn/ui 组件，简洁优雅

## 快捷键

| 快捷键 | 功能 |
|--------|------|
| Alt+C | 显示/隐藏窗口 |
| Win+V | 显示/隐藏窗口（可选，需在设置中开启） |
| 1-9 | 快速粘贴对应位置的记录 |

## 技术栈

**前端：**
- React 19 + TypeScript
- Vite 7
- Tailwind CSS 4
- shadcn/ui (Radix UI)
- Zustand 状态管理
- TanStack Virtual 虚拟列表

**后端：**
- Tauri 2.0
- Rust
- SQLite (rusqlite)
- clipboard-rs 剪贴板库

## 开发

### 环境要求

- Node.js 18+
- Rust 1.70+
- Windows 10/11

### 安装依赖

```bash
npm install
```

### 启动开发服务器

```bash
npm run tauri dev
```

### 构建生产版本

```bash
npm run tauri build
```

### Rust 编译缓存

项目配置了 Rust 编译缓存目录（`src-tauri/.cargo/config.toml`），避免 `target` 目录占用过多空间。

如需修改缓存位置，编辑 `target-dir` 配置项。

## 数据存储

- **数据库**: `%LOCALAPPDATA%\ClipboardManager\clipboard.db`
- **图片缓存**: `%LOCALAPPDATA%\ClipboardManager\images\`

## 许可证

MIT License
