# ElegantClipboard

<p align="center">
  <img src="src-tauri/icons/icon.png" alt="ElegantClipboard" width="128" height="128">
</p>

<p align="center">
  高性能 Windows 剪贴板管理工具，基于 Tauri 2.0 构建，可完全替代系统剪贴板。
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-0.4.0-blue.svg" alt="version">
  <img src="https://img.shields.io/badge/platform-Windows-lightgrey.svg" alt="platform">
  <img src="https://img.shields.io/badge/license-MIT-green.svg" alt="license">
</p>

## 设计理念

**低占用 · 高性能 · 现代化 · 完全本地化离线**

- **低占用** - 托盘常驻，不打扰核心工作流，窗口不抢占焦点，仅可见时启用监控
- **高性能** - SQLite FTS5 全文搜索、虚拟列表处理万级记录、异步图像处理、内容哈希去重
- **现代化** - Tauri 2.0 + React 19 + Tailwind CSS 4，类型安全，优雅架构
- **本地化离线** - 数据完全本地存储，无网络请求，无云同步，隐私至上

## 功能特性

### 核心功能
- **无限历史记录** - 自动记录所有复制内容，随时回溯
- **全文搜索** - 实时搜索历史记录，支持 FTS5 全文索引
- **图片支持** - 复制图片自动生成缩略图预览
- **置顶/收藏** - 重要内容可置顶或收藏，不受自动清理影响
- **点击即粘贴** - 点击记录直接粘贴到活动窗口
- **全局快捷键** - Alt+C 或 Win+V 快速唤出
- **点击外部隐藏** - 点击窗口外部自动隐藏

### 自定义设置
- **自定义存储路径** - 支持数据迁移和路径自定义
- **历史记录限制** - 可设置最大记录数（0 为无限制）
- **内容大小限制** - 单条内容最大大小可配置
- **显示设置** - 预览行数、时间/字符数/大小显示可选
- **开机自启** - 支持系统启动时自动运行

### 性能优化
- **虚拟列表** - 万级记录流畅滚动
- **SQLite 存储** - 本地数据库安全存储
- **自动清理** - 超出限制自动删除旧记录及图片文件
- **低资源占用** - 优化的鼠标事件处理，降低 CPU 占用

## 快捷键

| 快捷键 | 功能 |
|--------|------|
| `Alt+C` | 显示/隐藏窗口（默认） |
| `Win+V` | 显示/隐藏窗口（可选，需在设置中开启） |
| `1-9` | 快速粘贴对应位置的记录 |
| `Esc` | 隐藏窗口 |

## 技术栈

| 类别 | 技术 |
|------|------|
| **框架** | Tauri 2.0 |
| **前端** | React 19 + TypeScript |
| **构建** | Vite 7 |
| **样式** | Tailwind CSS 4 |
| **组件** | shadcn/ui (Radix UI) + Fluent UI Icons |
| **状态** | Zustand 5 |
| **虚拟化** | TanStack Virtual |
| **后端** | Rust |
| **数据库** | SQLite (rusqlite) + FTS5 |
| **剪贴板** | clipboard-master + arboard |

## 安装

### 下载安装包

从 [Releases](https://github.com/Y-ASLant/ElegantClipboard/releases) 页面下载最新版本的安装包。

### 从源码构建

#### 环境要求

- Node.js 18+
- Rust 1.70+
- Windows 10/11

#### 构建步骤

```bash
# 克隆仓库
git clone https://github.com/Y-ASLant/ElegantClipboard.git
cd ElegantClipboard

# 安装依赖
npm install

# 开发模式
npm run tauri dev

# 构建生产版本
npm run tauri build
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
