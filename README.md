# ElegantClipboard

[English](README_EN.md) | 中文

ElegantClipboard 是使用 Rust、GPUI 和 gpui-kit 构建的原生剪贴板管理器，Windows、macOS 和 Linux 共用界面与数据服务，系统集成分别实现。当前分支只有 GPUI 应用，不需要 Node.js、WebView 或 Tauri。平台能力和验证边界见下文。

## 当前功能

- 采集和检索文本、URL、HTML/RTF、图片及文件路径
- 除上述主类型外，剪贴板上的其余原生格式（含应用自定义注册格式）按内置黑名单全量捕获存储，复制历史记录时随主格式一并写回（保真回贴）；大格式超过 64 KiB 转内容寻址 blob 存储（zstd 压缩，单格式 8 MiB、单次 32 MiB 上限），随删除与保留策略自动回收，备份导出/恢复包含 blob
- 复制后 300ms 确认窗口默认过滤瞬时内容（划词工具临时复制、被立即覆盖的碎片），可在数据设置关闭；复制后立即从本应用粘贴不受影响
- 相同纯文本再次以 HTML/RTF 复制时并入既有条目（补全格式并置顶），不产生重复记录；历史条目支持按原格式保真还原
- HTML 条目在完整预览页与悬停浮窗渲染富文本排版（含本地图片）；列表与搜索保持纯文本摘要
- 图片复制时自动携带文件格式（CF_HDROP + CF_DIB + PNG），可直接在资源管理器或聊天窗口按 Ctrl+V 粘贴为图片文件
- 新数据目录首次打开时提供四步中文/英文引导，介绍采集、搜索、收藏与快捷键，可跳过或按 Esc 结束
- 设置窗口按常规、显示、外观、数据、应用过滤、音效、快捷键和关于分类切换
- 常规设置可选择从托盘菜单清理当前分组历史时跳过确认，默认仍要求确认
- 可分别配置复制与受控粘贴音效的开关、立即或成功后播放，并在设置页试听
- 关于页提供作者、项目仓库和问题反馈入口，可在默认浏览器中打开
- 设置窗口可分别启停文本、网址、HTML、RTF、图片和文件的历史采集，至少保留一种类型
- 来源应用过滤支持黑名单、白名单、手工规则及运行中应用选择，规则可按应用名、进程名或路径匹配并支持 `*`、`?`
- 收藏、置顶、通过 Select 选择分组并在 GPUI Kit Dialog 中新建或重命名分组（支持删除和上移/下移排序）、通过卡片两侧区域拖拽排序、批量删除与历史清理；拖动区域提示可在显示设置中隐藏
- 文本编辑、富文本/图片/文件预览、单张图片文件卡片缩略图与失效来源提示，以及复制、纯文本复制和自动粘贴
- 可配置的独立悬停预览：图片、文本、网址及 HTML/RTF 默认开启，文件可单独开启；文本浮窗按延时读取完整正文并可滚动。支持配置延时、位置、图片缩放步进和按钮，以及按图片尺寸扩大浮窗；已保存的关闭设置保持生效。图片浮窗支持滚轮无极缩放和左键拖拽平移，按住标题可拖动整个浮窗，浮窗出现后固定 5 秒自动关闭
- 全局快捷键显隐切换、系统托盘、开机启动、暂停采集和单实例唤回
- 快速粘贴快捷键默认关闭，可在设置的“快捷键”页按需启用或再次关闭；单个槽位支持录制或输入组合键，两组各 10 个槽位可批量停用或恢复默认。启用后预置 Alt+1～Alt+0 粘贴当前分组前十条记录，Ctrl+Alt+1～3 粘贴前三条收藏记录，数字键支持数字小键盘；自动粘贴按键可选 Ctrl+V 或 Shift+Insert
- 简体中文/English 界面（默认简体中文）、明暗主题、窗口置顶、点击外部隐藏、唤出位置，以及粘贴后窗口显隐和排序设置
- 主窗口、设置窗口与悬停预览浮窗按系统能力提供关闭、亚克力、云母或云母 Alt；不支持的已保存选项保留，并明确显示降级说明。背景遮罩强度可选 40%/55%/72%/85%，仅重绘背景，不重复设置原生材质。材质只作用于窗口外壳，控件、卡片与通知保留原有主题表面；开启后保持无边框与边缘缩放
- 无顶部标题栏的主窗口：按住搜索框上方的浅色短横握柄移动窗口，顶部边缘仍可缩放；搜索框独占一行，分类标签与 GPUI Kit 分组 Select 同行；从卡片“选择”或列表 Ctrl+A 进入多选，使用批量操作栏退出；清理历史、窗口置顶和设置入口位于托盘菜单
- 设置页可隐藏分类筛选栏，并选择紧凑、标准或宽松的历史卡片密度。默认两行文字预览较图片卡片更矮；正文或居中的图片缩略图在上，时间、大小和来源应用在下，悬停卡片显示操作按钮
- 历史列表较长时，右下角悬浮按钮返回顶部；记录数固定显示在列表下方，不随滚动移动
- 静态列表图片按显示尺寸与 DPI 后台生成缩略图，有界并发和可见范围缓存；完整预览、复制与导出保留原图
- 卡片预览行数、绝对/相对时间、字符数、大小和来源应用信息可在设置窗口配置；来源应用可按名称、图标或两者显示，图标缓存为 PNG
- 数据设置可导出 GPUI ZIP，并将 GPUI/旧版 ZIP 或旧数据库恢复到独立空目录，显示结果并打开恢复的数据
- 分组、分类与关键词组合筛选；主唤出快捷键可录制或输入，冲突或保存失败保留原配置
- 按分组配置历史数量和天数，保护收藏与置顶；首次启用或收紧策略需确认，旧数据升级不自动扩大清理范围
- 可选普通文件副本默认关闭，单文件 50 MiB、总预算 256 MiB；图片可作为文件复制，HTML 缺少纯文本时自动转换
- 复制后读回校验，回贴有界等待修饰键释放并核对目标；采集尊重应用声明的本地历史隐私标记
- 已知限制：HTML 预览中引用 http(s) 的远程图片不加载（gpui 无内置网络图片管线，本地图片正常）；编辑 HTML 条目保存后仍降级为纯文本；图片回贴临时文件位于数据目录 image_tmp/（24 小时或超过 64 个时启动清扫）

Windows 实现和历史验收详见 [Windows GPUI 状态](docs/WINDOWS_MVP.md)。上述功能受以下平台能力限制。

## 平台差异

| 平台 | 系统集成与限制 |
|---|---|
| Windows | 保留现有剪贴板原生格式、热键、托盘、OLE 拖放、受控粘贴与 DWM 材质 |
| macOS | Carbon 热键、菜单栏托盘、LaunchAgent 登录启动、AppKit 拖放；自动粘贴使用 `Cmd+V`，需在系统设置授予辅助功能权限，不支持 `Shift+Insert`。来源应用是前台应用的尽力归因，不能当作可靠的剪贴板所有者；材质支持模糊，不支持 Windows Mica |
| Linux X11 | X11 热键、XTEST 粘贴、XInput2 外部点击、XDND 拖放及 XDG 自动启动；来源优先按 selection owner 的 XRes PID 识别，无法确定时保持未知；需要支持相应扩展的 X server |
| Linux Wayland | 剪贴板需要 compositor 的 data-control 协议，不会静默退回不完整的 XWayland 采集；全局快捷键、自动粘贴、来源识别和精确窗口/悬停浮窗定位不可用。保留内部排序及文件/图片原生拖出，不支持文本/富文本拖出。重复复制完全相同内容不能通过轮询识别为新事件 |

Linux 托盘需要 StatusNotifier/XEmbed 宿主，GNOME 通常需要 AppIndicator 扩展；无宿主时不会把窗口隐藏到不可访问的托盘。Linux 音效需要 `paplay`（`pulseaudio-utils`，可使用 PipeWire 的 PulseAudio 兼容服务），链接和文件定位需要 `xdg-open` / `dbus-send`。macOS 使用系统 `open` / `afplay`。

新增 macOS/Linux 后端不等于已经通过实机验收：当前 Windows 原生窗口与隔离剪贴板往返已验证，其他平台的桌面交互、权限、打包产物仍需对应系统验收；CI 已配置原生系统检查，不应将配置存在视为 CI 已通过。

## 工程结构

```text
crates/
  clipboard-core/      数据模型、SQLite、查询、备份与业务规则
  clipboard-platform/  共享剪贴板服务及 Windows/macOS/Linux 系统集成
  clipboard-gpui/      GPUI 应用入口、状态与界面
scripts/               打包与验证辅助脚本
docs/                  架构决策、功能状态和验收证据
```

依赖方向固定为 `clipboard-gpui -> clipboard-platform -> clipboard-core`，界面层也可直接使用核心层；核心层不依赖 GPUI 或 Windows API。托盘属于 `clipboard-gpui`，平台层提供操作系统集成。

Rust 源码、示例和测试文件均不得超过 **500 物理行**（包含空行和注释）。按职责拆分私有子模块，测试放在对应模块的测试文件中；不要用编号分片或 `include!` 绕过限制。`just check` 和 GPUI CI 均执行 `scripts/check-source-size.ps1`。模块职责见[工程文档](docs/README.md)。

## 从源码运行

要求：

- Windows 10/11 x64 + MSVC；或 macOS + Xcode Command Line Tools；或 Linux + 下列系统依赖
- Rust 1.98 或更高版本，使用当前系统的原生工具链
- 结构检查及打包需要 PowerShell 7（`pwsh`）
- [`just`](https://github.com/casey/just)（可选，用于下文的快捷命令）

macOS 先安装开发工具：`xcode-select --install`。Ubuntu 24.04 开发依赖：

```sh
sudo apt-get update
sudo apt-get install build-essential clang cmake pkg-config libssl-dev libgtk-3-dev libayatana-appindicator3-dev libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libvulkan-dev libx11-xcb-dev libxcb-shape0-dev libxcb-xfixes0-dev libxcb-randr0-dev libasound2-dev libudev-dev mesa-vulkan-drivers xdg-utils dbus-x11 pulseaudio-utils
```

Linux 运行时需要可用的图形会话及 Vulkan 驱动；不要在无显示服务的 SSH 会话中直接启动 GUI。

```powershell
cargo run -p elegant-clipboard-gpui --locked
```

也可以使用：

```powershell
just run
```

默认数据使用当前系统的应用数据目录（Windows 为 LocalAppData，macOS 为 Application Support，Linux 为 XDG data home）。测试或隔离运行可显式指定目录：

```powershell
cargo run -p elegant-clipboard-gpui --locked -- --no-monitor --data-dir .\target\gpui-check
```

## 构建与验证

```powershell
just check
just test
just build
```

Release 可执行文件为 Windows 的 `target/release/elegant-clipboard-gpui.exe`，或 macOS/Linux 的 `target/release/elegant-clipboard-gpui`。

生成未经签名的 Windows x64 独立 ZIP：

```powershell
just package
```

在 macOS 或 Linux **本机**打包：

```sh
just package-unix
# 或 pwsh -NoProfile -File scripts/package-gpui-unix.ps1
```

macOS 生成 `ElegantClipboard-v<版本>-<架构>-apple-darwin.dmg`，内含 `ElegantClipboard.app` 和指向 `/Applications` 的快捷方式。打开 DMG，将应用拖到 Applications，推出磁盘映像后从“应用程序”启动。应用仅进行本地 ad-hoc 签名，未使用 Developer ID 签名或 Apple 公证；确认下载来源可信后，如首次启动被拦截，可在“系统设置 → 隐私与安全性”中选择“仍要打开”，不要关闭全局安全检查。自动粘贴需授予辅助功能权限。

Linux 生成原生二进制、许可证和说明的 `.tar.gz`，不会捆绑系统动态库，需保留上述运行依赖。文件名包含实际 Rust host target；脚本支持 macOS/Linux 的 x86_64 和 aarch64 原生工具链。macOS CI 与发布任务会校验 DMG、只读挂载、检查 Applications 快捷方式与应用签名，并从映像内启动隔离数据目录的窗口 smoke。

产物与 SHA-256 文件写入本地 `target/packages/`，CI artifact 与 GitHub Release 仅上传平台包，不上传 `.sha256` 文件。不提供安装向导或自动更新。CI/发布矩阵分别在 Windows、Ubuntu 和 macOS runner 上构建，不从 Windows 伪造 Mac/Linux 包。

## 版本管理

```powershell
.\scripts\bump-version.ps1 2.0.1
```

脚本更新根 Cargo 工作区版本和锁文件。GPUI 重构版从 `2.0.0` 开始；后续发布标签必须与 Cargo 版本一致，例如 `v2.0.1`。版本记录见 [变更日志](docs/CHANGELOG.md)。

## 许可证

[MIT License](LICENSE)
