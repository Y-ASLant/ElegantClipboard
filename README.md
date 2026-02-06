# ElegantClipboard

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

## 设计理念

**低占用 · 高性能 · 现代化 · 完全本地化离线**

- **低占用** - 托盘常驻，不打扰核心工作流，窗口不抢占焦点，仅可见时启用监控
- **高性能** - SQLite FTS5 全文搜索、虚拟列表处理万级记录、异步图像处理、内容哈希去重
- **现代化** - Tauri 2.0 + React 19 + Tailwind CSS 4，类型安全，优雅架构
- **本地化离线** - 数据完全本地存储，无网络请求，无云同步，隐私至上

## 功能特性

### 剪贴板管理
- **多类型支持** - 文本、图片、文件、HTML、RTF 五种内容类型
- **无限历史记录** - 自动记录所有复制内容，随时回溯
- **全文搜索** - 实时搜索历史记录，支持 FTS5 全文索引和前缀匹配
- **内容去重** - BLAKE3 哈希自动去重，相同内容不重复存储
- **置顶/收藏** - 重要内容可置顶或收藏，不受自动清理影响
- **拖拽排序** - 卡片支持拖拽排序，跨置顶/普通区域拖拽自动切换状态
- **点击即粘贴** - 点击记录直接粘贴到活动窗口

### 图片预览
- **缩略图预览** - 图片类型自动生成缩略图（Asset Protocol 零开销加载）
- **单图片文件预览** - 复制的图片文件自动显示图片预览（失败时回退为文件卡片）
- **悬浮放大预览** - 鼠标悬停 300ms 弹出独立预览窗口，支持大图查看
- **Ctrl+滚轮缩放** - 预览窗口支持平滑缩放（CSS transition 动画，零窗口 resize）
- **缩放百分比显示** - 缩放时右下角显示百分比徽章，1.2 秒后自动淡出
- **预览位置可选** - 支持自动/左侧/右侧三种预览位置偏好

### 文件管理
- **文件有效性检测** - 并行检查文件是否存在（rayon），失效文件显示红色警告
- **右键菜单** - 粘贴、粘贴为路径、在资源管理器中显示、查看详细信息
- **文件详情对话框** - 查看已复制文件的完整信息，标注失效文件

### 窗口管理
- **全局快捷键** - 自定义快捷键唤出/隐藏窗口（默认 Alt+C）
- **Win+V 替换** - 可选替换系统 Win+V（通过注册表禁用系统热键）
- **点击外部隐藏** - 全局鼠标监控，点击窗口外部自动隐藏（仅窗口可见时启用）
- **窗口固定** - 锁定窗口防止自动隐藏
- **跟随光标** - 可选在光标位置显示窗口
- **多显示器支持** - 智能定位，保持窗口在屏幕边界内

### 自定义设置
- **自定义存储路径** - 支持数据迁移和路径自定义
- **历史记录限制** - 可设置最大记录数（0 为无限制）
- **内容大小限制** - 单条内容最大大小可配置
- **显示设置** - 预览行数（1-10 行）、时间/字符数/大小显示可选
- **图片预览设置** - 启用/禁用悬浮预览、缩放步进（5%-50%）、预览位置偏好
- **开机自启** - 支持系统启动时自动运行
- **管理员启动** - 可选以管理员权限运行（UAC 提升）
- **数据库优化** - 手动触发 OPTIMIZE / VACUUM

### 系统集成
- **系统托盘** - 左键切换窗口、右键菜单（显示/隐藏、暂停/恢复监控、退出）
- **非焦点窗口** - 窗口不抢占焦点，不影响当前操作
- **键盘模拟** - 使用 enigo 模拟 Ctrl+V 实现粘贴
- **暗色模式** - 跟随系统主题自动切换

## 快捷键

| 快捷键 | 功能 |
|--------|------|
| `Alt+C` | 显示/隐藏窗口（默认，可自定义） |
| `Win+V` | 显示/隐藏窗口（可选，需在设置中开启） |
| `Ctrl+滚轮` | 缩放图片预览 |

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
| **数据库** | SQLite (rusqlite) + FTS5 全文搜索 |
| **哈希** | BLAKE3（内容去重） |
| **锁** | parking_lot（高性能 Mutex/RwLock） |
| **并行** | rayon（文件检查并行化） |
| **剪贴板** | clipboard-master + arboard + clipboard-rs |
| **键盘模拟** | enigo |
| **输入监控** | rdev（点击外部检测） |
| **CI/CD** | GitHub Actions（CI + Tag 触发 Release） |

## 安装

### 下载安装包

从 [Releases](https://github.com/Y-ASLant/ElegantClipboard/releases) 页面下载最新版本的安装包。

### 从源码构建

#### 环境要求

- Node.js 22+
- Rust 1.80+（需要 `std::sync::LazyLock`）
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

# 代码检查
npm run lint
```

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

## 项目结构

```
src/                          # React 前端
├── components/
│   ├── CardContentRenderers.tsx  # 图片/文件/文本渲染器
│   ├── ClipboardItemCard.tsx     # 卡片组件（交互+菜单+布局）
│   ├── ClipboardList.tsx         # 虚拟列表 + 拖拽排序
│   └── settings/                 # 设置页面各 Tab
├── stores/
│   ├── clipboard.ts              # 剪贴板状态（Zustand）
│   └── ui-settings.ts            # UI 设置（持久化+多窗口同步）
├── lib/
│   ├── format.ts                 # 工具函数（格式化/解析/判断）
│   └── utils.ts                  # 通用工具（cn）
├── hooks/
│   └── useSortableList.ts        # 拖拽排序 Hook
├── App.tsx                       # 主窗口
└── pages/Settings.tsx            # 设置窗口

src-tauri/src/                # Rust 后端
├── lib.rs                        # 应用初始化 + 窗口管理 + 预览窗口
├── shortcut.rs                   # 快捷键解析
├── commands/
│   ├── mod.rs                    # 命令注册 + 共享辅助函数
│   ├── clipboard.rs              # 剪贴板 CRUD + 复制粘贴
│   ├── settings.rs               # 设置/监控/数据库/自启动
│   └── file_ops.rs               # 文件验证/详情/路径粘贴
├── clipboard/
│   ├── handler.rs                # 内容处理（文本/HTML/RTF/图片/文件）
│   └── monitor.rs                # 剪贴板监控服务
├── database/
│   ├── schema.rs                 # 数据库 Schema + FTS5
│   ├── repository.rs             # CRUD 操作 + 查询构建
│   └── mod.rs                    # 读写分离连接管理
├── config.rs                     # 配置文件管理 + 数据迁移
├── keyboard_hook.rs              # 窗口状态追踪
├── input_monitor.rs              # 全局鼠标监控
├── positioning.rs                # 窗口定位工具
├── tray/mod.rs                   # 系统托盘
├── win_v_registry.rs             # Win+V 注册表管理
└── admin_launch.rs               # 管理员启动

public/
└── image-preview.html            # 独立预览窗口页面
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
