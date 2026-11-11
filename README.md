# ElegantClipboard

[English](README_EN.md) | 中文

ElegantClipboard 是使用 Rust、GPUI 和 gpui-kit 构建的原生剪贴板管理器，Windows、macOS 和 Linux 共用界面与数据服务，系统集成分别实现。当前分支只有 GPUI 应用，不需要 Node.js、WebView 或 Tauri。

## 功能

- 采集并搜索文本、URL、HTML/RTF、图片及文件路径
- 保真回贴：其余原生格式（含应用自定义格式）按内置黑名单全量捕获，复制历史记录时随主格式一并写回；大格式转内容寻址 blob 存储（zstd 压缩），随删除和保留策略自动回收，备份包含 blob
- 去重与合并：相同纯文本再以 HTML/RTF 复制时并入既有条目；重复复制默认移到列表顶部，也可在数据设置改为各存一条；复制后 300ms 确认窗口过滤瞬时碎片（可在数据设置关闭）；条目备注可编辑并参与搜索
- 富文本按单一格式（纯文本/HTML/RTF）复制或粘贴到原窗口，缺少所选格式时报错不降级；纯文本支持转换后复制/粘贴（去首尾空白、去换行、大小写）；粘贴后恢复原剪贴板可在设置开启（默认关闭）
- 序列粘贴：多选记录后逐条粘贴到原窗口，由全局推进键（默认 Ctrl+Shift+G）逐条送出，队列耗尽自动结束，离开面板或复制/删除会取消队列
- HTML 条目在悬停预览窗口渲染富文本排版（含本地图片），列表保持纯文本摘要；图片既可按图片粘贴到聊天窗口，也可按文件粘贴到资源管理器
- 首次启动在新数据目录提供三步引导：选择语言、查看并录制快捷键、配置主题与开机启动；演示记录隔离存放，不影响真实历史；设置 → 关于可重看引导
- 设置按常规、显示、外观、数据、应用过滤、音效、快捷键和关于分类：可分别启停各类型的历史采集（至少保留一种）、按规则或运行中应用过滤来源、配置复制与粘贴音效
- 分组与条目管理：收藏、置顶，分组可新建、重命名、删除和排序；卡片两侧区域拖拽排序、多选批量删除与清理历史；卡片预览行数、时间格式、字符数、大小与来源显示均可配置
- 独立悬停预览：透明、鼠标穿透且不抢焦点的浮层，优先显示在主窗口侧边；Ctrl＋滚轮缩放图片或滚动文本，离开来源卡片即关闭；普通文件只保存原始路径，原件移动或删除后提示路径失效
- 界面：简体中文/English、明暗主题、组件圆角选项、毛玻璃/云母/不透明背景材质（按平台能力提供）、窗口置顶/点击外部隐藏/唤出位置；无边框主窗口可按住搜索框上方握柄拖动，向下滚动后可一键返回顶部；快速粘贴默认关闭，槽位与粘贴键、**受控/连贴投递节奏**均可自定义（默认受控，连贴适合按住 Alt 连按数字）
- 数据管理：备份导出为 GPUI ZIP，支持恢复 GPUI/旧版备份或导入旧数据库到独立空目录；按分组配置数量与天数保留上限，保护收藏与置顶；全局快捷键显隐切换、系统托盘、开机启动、暂停采集和单实例唤回

已知限制：HTML 预览中引用 http(s) 的远程图片不加载；编辑 HTML 条目保存后降级为纯文本；图片回贴的临时文件位于数据目录 `image_tmp/`，启动时自动清扫。

实现细节、依赖与验证命令详见[跨平台 GPUI 应用状态](docs/WINDOWS_MVP.md)；历史验收记录见 `docs/evidence/`。

## 平台差异

| 平台 | 集成与限制 |
|---|---|
| Windows | 剪贴板原生格式、热键、托盘、OLE 拖放、受控粘贴与 DWM 材质 |
| macOS | Carbon 热键、菜单栏托盘、LaunchAgent 登录启动、AppKit 拖放；自动粘贴使用 `Cmd+V`，需授予辅助功能权限，不支持 `Shift+Insert`；来源应用为前台应用的尽力归因；材质支持模糊，不支持 Mica |
| Linux X11 | X11 热键、XTEST 粘贴、XInput2 外部点击、XDND 拖放、XDG 自动启动；来源优先按 selection owner 的 XRes PID 识别，无法确定时保持未知；需要支持相应扩展的 X server |
| Linux Wayland | 需要 compositor 的 data-control 协议，不退回 XWayland 采集；全局快捷键、自动粘贴、来源识别和精确定位不可用；保留内部排序及文件/图片拖出；重复复制完全相同内容无法被识别为新事件 |

Linux 托盘需要 StatusNotifier/XEmbed 宿主（GNOME 通常需要 AppIndicator 扩展）；音效需要 `paplay`（`pulseaudio-utils`），链接和文件定位需要 `xdg-open` / `dbus-send`。macOS 使用系统 `open` / `afplay`。

项目按三平台编译，但实机验收目前只覆盖 Windows（原生窗口与隔离剪贴板往返已验证）；macOS 与 Linux 的桌面交互、权限和打包产物仍需本机验收。

## 工程结构

```text
crates/
  clipboard-core/      数据模型、SQLite、查询、备份与业务规则
  clipboard-platform/  共享剪贴板服务及 Windows/macOS/Linux 系统集成
  clipboard-gpui/      GPUI 应用入口、状态与界面
scripts/               打包与验证辅助脚本
docs/                  架构决策、功能状态和验收证据
```

依赖方向：`clipboard-gpui -> clipboard-platform -> clipboard-core`；核心层不依赖 GPUI 或系统 API。Rust 源文件（含示例与测试）不得超过 **640 物理行**，由 `just check` 与 CI 强制执行。模块职责见[工程文档](docs/README.md)。

## 从源码运行

要求：

- Windows 10/11 x64 + MSVC；或 macOS + Xcode Command Line Tools；或 Linux + 下列系统依赖
- Rust 1.98 或更高版本，当前系统的原生工具链
- 检查及打包需要 PowerShell 7（`pwsh`）
- [`just`](https://github.com/casey/just)（可选，用于下文的快捷命令）

macOS 先运行 `xcode-select --install`。Ubuntu 24.04 开发依赖：

```sh
sudo apt-get update
sudo apt-get install build-essential clang cmake pkg-config libssl-dev libgtk-3-dev libayatana-appindicator3-dev libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libvulkan-dev libx11-xcb-dev libxcb-shape0-dev libxcb-xfixes0-dev libxcb-randr0-dev libasound2-dev libudev-dev mesa-vulkan-drivers xdg-utils dbus-x11 pulseaudio-utils
```

Linux 运行需要可用的图形会话及 Vulkan 驱动，不要在无显示服务的 SSH 会话中启动 GUI。

```powershell
just run    # 或 cargo run -p elegant-clipboard-gpui --locked
```

默认数据目录：

| 平台 | 路径 |
|---|---|
| Windows | `%LOCALAPPDATA%\ElegantClipboard` |
| macOS | `~/Library/Application Support/ElegantClipboard` |
| Linux | `$XDG_DATA_HOME/elegantclipboard`（默认 `~/.local/share/elegantclipboard`） |

旧版本数据目录和开机启动项不自动搬迁；1.2.10 的历史可导入旧数据库或旧版 ZIP。启动时清理历史 `staged` 文件副本及其数据库引用，保留原始路径。`--data-dir` 显式指定后原样使用：

```powershell
cargo run -p elegant-clipboard-gpui --locked -- --no-monitor --data-dir .\target\gpui-check
```

## 诊断日志

诊断日志默认关闭，可在常规设置开启。日志写在数据目录下的 `logs`（每次启动一个独立会话目录），记录版本、进程、启停、显示器坐标与 DPI、窗口及悬停预览生命周期。

- `application.log` 按大小轮转（4 MiB × 3 份），`panic.log` 记录 panic 的线程、位置和堆栈
- 不记录剪贴板正文、搜索词、图片像素或分组名称；错误消息和堆栈可能含本机路径，分享前请检查
- 系统强制结束、断电及 fail-fast 可能绕过 panic hook——没有 `panic.log` 不代表没有崩溃；如遇闪退请保留对应会话目录，并附 Windows「事件查看器 → Windows 日志 → 应用程序」中的错误记录

## Windows 管理员启动

仅 Windows 提供「以管理员身份启动」（常规设置）：经 UAC 重启为提权进程，用于监听并向其他提权应用（部分游戏、终端）粘贴；拒绝 UAC 则按普通权限继续运行。

## 构建与验证

```powershell
just check
just test
just build
```

Release 可执行文件为 `target/release/elegant-clipboard-gpui[.exe]`。

打包：

| 平台 | 命令 | 产物 |
|---|---|---|
| Windows | `just package` | x64 ZIP 与 NSIS 安装包（需本机安装 NSIS 3） |
| macOS | `just package-unix` | ad-hoc 签名的 `.dmg` |
| Linux | `just package-unix` | `.tar.gz`、`.deb`、`.rpm`（需 `dpkg-dev` 和 `rpm`） |

打包必须在对应平台本机进行；产物写入 `target/packages/`。macOS 未做公证，首次启动如被拦截可在「隐私与安全性」选择"仍要打开"。安装包卸载不删除历史数据。不提供自动更新。

## 版本管理

```powershell
.\scripts\bump-version.ps1 2.0.1
```

脚本更新根 Cargo 工作区版本和锁文件；发布标签必须与 Cargo 版本一致，例如 `v2.0.1`。版本记录见 [变更日志](docs/CHANGELOG.md)。

## 许可证

[MIT License](LICENSE)
