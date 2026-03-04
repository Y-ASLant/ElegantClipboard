# ElegantClipboard

[English](README_EN.md) | 中文

> 说明：本文档中的界面截图可能与最新版本略有差异，当前截图拍摄于 **v0.5.0**。

<p align="center">
  <img src="src-tauri/icons/icon.png" alt="ElegantClipboard" width="128" height="128">
</p>
<p align="center">
  低占用 · 高性能 · 现代化 · 完全本地化离线剪贴板。
</p>


<p align="center">
  <a href="https://github.com/Y-ASLant/ElegantClipboard/releases"><img src="https://img.shields.io/github/v/release/Y-ASLant/ElegantClipboard?label=version&color=blue" alt="version"></a>
  <img src="https://img.shields.io/badge/platform-Windows-lightgrey.svg" alt="platform">
  <img src="https://img.shields.io/badge/license-MIT-green.svg" alt="license">
  <a href="https://github.com/Y-ASLant/ElegantClipboard/actions/workflows/ci.yml"><img src="https://github.com/Y-ASLant/ElegantClipboard/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
</p>

## 界面截图（v0.5.0）

### 外观主题

#### 跟随系统强调色

![跟随系统](img/theme_0.png)

| 经典黑白 | 翡翠绿 | 天空青 |
|:-:|:-:|:-:|
| ![经典黑白](img/theme_1.png) | ![翡翠绿](img/theme_2.png) | ![天空青](img/theme_3.png) |

#### 暗色模式

自动跟随系统深色/浅色模式，实时切换

### 设置界面

| 数据管理 | 显示设置 | 快捷按键 |
|:-:|:-:|:-:|
| ![数据管理](img/setting_1.png) | ![显示设置](img/setting_2.png) | ![快捷按键](img/setting_3.png) |

### 图片悬浮预览

![图片预览](img/preview_mode.png)

### 文本悬浮预览

文本悬浮预览与图片悬浮预览共用预览位置与悬浮预览延时设置（默认 500ms，文本预览默认关闭）。

### 启动通知

![启动通知](img/startup_notification.png)

## 设计理念

**低占用 · 高性能 · 现代化 · 完全本地化离线**

- **低占用** - 托盘常驻，不打扰核心工作流，窗口不抢占焦点，仅可见时启用监控
- **高性能** - 优化的 LIKE 搜索（兼容 CJK 文本）、虚拟列表处理万级记录、异步图像处理、内容哈希去重
- **现代化** - Tauri 2.0 + React 19 + Tailwind CSS 4，类型安全，优雅架构
- **本地化离线** - 数据完全本地存储，无网络请求，无云同步，隐私至上

## 功能特性

完整功能列表与术语约定见 [FEATURES.md](FEATURES.md)。

## 快捷键

### 全局快捷键

| 快捷键 | 功能 |
|--------|------|
| `Alt+C` | 显示/隐藏窗口（默认，可自定义） |
| `Win+V` | 显示/隐藏窗口（可选，需在设置中开启） |

### 窗口内快捷键

| 快捷键 | 功能 |
|--------|------|
| `↑` / `↓` | 上下选择剪贴板条目 |
| `←` / `→` | 切换分组标签（全部/文本/图片/文件/收藏） |
| `Enter` | 粘贴选中条目 |
| `Shift+Enter` | 以纯文本粘贴选中条目 |
| `Delete` | 删除选中条目 |
| `ESC` | 关闭对话框/隐藏窗口 |
| `Ctrl+滚轮` | 缩放图片预览 / 滚动文本预览 |

## 技术栈

| 类别 | 技术 |
|------|------|
| **框架** | Tauri 2.0 |
| **前端** | React 19 + TypeScript |
| **构建** | Vite 7 |
| **样式** | Tailwind CSS 4 |
| **组件** | shadcn/ui (Radix UI) + Fluent UI Icons |
| **状态管理** | Zustand 5（持久化 + 多窗口同步） |
| **虚拟列表** | react-virtuoso |
| **拖拽排序** | @dnd-kit |
| **后端** | Rust |
| **数据库** | SQLite (rusqlite) + 优化的 LIKE 查询（支持 CJK 文本） |
| **哈希** | BLAKE3（内容去重） |
| **锁** | parking_lot（高性能 Mutex/RwLock） |
| **并行** | rayon（文件检查并行化） |
| **剪贴板** | clipboard-master + arboard + clipboard-rs |
| **窗口特效** | window-vibrancy（Mica/Acrylic/Tabbed） |
| **键盘模拟** | enigo |
| **输入监控** | Win32 LL Hook（WH_MOUSE_LL + WH_KEYBOARD_LL，仅窗口可见时启用键盘钩子） |
| **自动更新** | 基于 GitHub Release 的检查与下载 |
| **CI/CD** | GitHub Actions（CI + Tag 触发 Release） |

## 安装

### 下载安装包

从 [Releases](https://github.com/Y-ASLant/ElegantClipboard/releases) 页面下载最新版本的安装包。

### winget

```powershell
winget install Y-ASLant.ElegantClipboard
```

### Scoop

```powershell
scoop bucket add elegantclipboard https://github.com/Y-ASLant/ElegantClipboard
scoop install elegantclipboard
```

### 从源码构建

#### 环境要求

- Node.js 18+（推荐 LTS 版本）
- Rust 1.80+（需要 `std::sync::LazyLock`）
- Windows 10/11

#### 构建步骤

```bash
# 克隆仓库
git clone https://github.com/Y-ASLant/ElegantClipboard.git
cd ElegantClipboard

# 安装依赖
npm install

# 仅构建前端静态资源（dist/）
npm run build

# 开发模式
npm run tauri dev

# 构建生产版本（默认仅当前机器架构）
npm run tauri build

# 分别构建 x64 / arm64 安装包（需执行两次）
npm run tauri build -- --target x86_64-pc-windows-msvc
npm run tauri build -- --target aarch64-pc-windows-msvc

# 代码检查
npm run lint
```

说明：
- `npm run build` 只会执行 `tsc && vite build`，用于前端资源构建，不会生成安装包。
- 安装包由 `npm run tauri build` 生成；不指定 `--target` 时只构建当前环境对应架构。
- 需要同时发布 `x64` 和 `arm64` 时，需分别执行两次带 `--target` 的构建命令（或在 CI 中分架构构建）。

#### 版本管理

```powershell
# 统一修改三处版本号（package.json, tauri.conf.json, Cargo.toml）
.\scripts\bump-version.ps1 0.5.0
```

或直接推送 tag，Release workflow 自动同步版本号并构建：

```bash
git tag v0.5.0
git push origin v0.5.0
```

## 数据存储

默认存储位置（可在设置中修改）：

| 类型 | 路径 |
|------|------|
| 配置文件 | `%LOCALAPPDATA%\ElegantClipboard\config.json` |
| 数据库 | `<数据目录>\clipboard.db` |
| 图片缓存 | `<数据目录>\images\` |

## 许可证

[MIT License](LICENSE)

## 作者

**ASLant** - [@Y-ASLant](https://github.com/Y-ASLant)
