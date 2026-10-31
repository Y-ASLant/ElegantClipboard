# ElegantClipboard

[English](README_EN.md) | 中文

ElegantClipboard 是使用 Rust、GPUI 和 gpui-kit 构建的原生剪贴板管理器，Windows、macOS 和 Linux 共用界面与数据服务，系统集成分别实现。当前分支只有 GPUI 应用，不需要 Node.js、WebView 或 Tauri。平台能力和验证边界见下文。

## 当前功能

- 采集和检索文本、URL、HTML/RTF、图片及文件路径
- 除上述主类型外，剪贴板上的其余原生格式（含应用自定义注册格式）按内置黑名单全量捕获存储，复制历史记录时随主格式一并写回（保真回贴）；大格式超过 64 KiB 转内容寻址 blob 存储（zstd 压缩，单格式 8 MiB、单次 32 MiB 上限），随删除与保留策略自动回收，备份导出/恢复包含 blob
- 复制后 300ms 确认窗口默认过滤瞬时内容（划词工具临时复制、被立即覆盖的碎片），可在数据设置关闭；复制后立即从本应用粘贴不受影响
- 相同纯文本再次以 HTML/RTF 复制时并入既有条目（补全格式并置顶），不产生重复记录；历史条目支持按原格式保真还原
- 去重策略可在数据设置切换：相同内容再次复制时移到列表顶部（默认）或各存一条保留时间线；条目备注可从右键菜单编辑并参与搜索
- 富文本条目可按单一格式（纯文本/HTML/RTF）复制或粘贴到原窗口，缺少所选格式时明确报错不降级；文本条目支持转换后复制/粘贴（去首尾空白、去换行、大小写），可在设置开启粘贴后恢复原剪贴板（默认关闭）
- 序列粘贴：多选记录后逐条粘贴到原窗口——首条自动送出，其后在目标应用中按全局推进键（默认 Ctrl+Shift+G，可在快捷键设置录制或停用）逐条送出，队列耗尽自动结束；离开面板、复制或删除操作会取消队列
- HTML 条目在独立悬停浮窗渲染富文本排版（含本地图片）；列表与搜索保持纯文本摘要
- 图片复制时自动携带文件格式（CF_HDROP + CF_DIB + PNG），可直接在资源管理器或聊天窗口按 Ctrl+V 粘贴为图片文件
- 新数据目录首次打开时，在当前剪贴板面板中提供三步中文/英文引导：随时唤出、找回内容、开始使用。语言切换位于第一步；可查看真实快捷键注册状态并直接录制、编辑、保存，使用隔离演示记录体验搜索、侧边图片预览、Ctrl＋滚轮缩放及纯文本插入，不改写系统剪贴板或真实历史。最后配置主题、开机启动与搜索聚焦并选择“开始使用”；无跳过按钮，引导不响应 Esc。已保存的快捷键保持不变，设置→关于可重新查看引导
- 设置窗口按常规、显示、外观、数据、应用过滤、音效、快捷键和关于分类切换
- 常规设置可选择从托盘菜单清理当前分组历史时跳过确认，默认仍要求确认
- 可分别配置复制与受控粘贴音效的开关、立即或成功后播放，并在设置页试听
- 关于页提供作者、项目仓库和问题反馈入口，可在默认浏览器中打开
- 设置窗口可分别启停文本、网址、HTML、RTF、图片和文件的历史采集，至少保留一种类型
- 来源应用过滤支持黑名单、白名单、手工规则及运行中应用选择，规则可按应用名、进程名或路径匹配并支持 `*`、`?`
- 收藏、置顶、通过 Select 选择分组并在 GPUI Kit Dialog 中新建或重命名分组（支持删除和上移/下移排序）、通过卡片两侧区域拖拽排序、批量删除与历史清理；拖动区域提示可在显示设置中隐藏
- 对话框文本编辑与备注，富文本/图片/文件独立悬停预览，单张图片文件卡片缩略图与失效来源提示，以及复制、纯文本复制和自动粘贴；主窗口保持历史列表，不提供整页预览
- 独立悬停预览：透明、鼠标穿透且不抢焦点的显示器工作区绘制层，内容优先放在主窗口侧边。图片初始等比适配 480×480，内部圆角跟随主题设置，使用统一浮层阴影、蓝色曲线与端点光晕；类型、大小、分辨率/真实比例、来源及操作提示显示在外部信息标签中。以 190ms ease-out-quint 从来源卡片方向展开并淡入，减少动态效果时直接显示；缩放不重播入场动画。鼠标留在来源卡片上 Ctrl＋滚轮缩放图片或滚动文本，离开来源即关闭；文本/HTML 按实际内容测量，避免固定大块空白
- 全局快捷键显隐切换、系统托盘、开机启动、暂停采集和单实例唤回
- 快速粘贴快捷键默认关闭，可在设置的“快捷键”页按需启用或再次关闭；单个槽位支持录制或输入组合键，两组各 10 个槽位可批量停用或恢复默认。启用后预置 Alt+1～Alt+0 粘贴当前分组前十条记录，Ctrl+Alt+1～3 粘贴前三条收藏记录，数字键支持数字小键盘；自动粘贴按键可选 Ctrl+V 或 Shift+Insert
- 简体中文/English 界面（默认简体中文）、Chakra 风格的统一明暗主题与动效；内部组件圆角可选跟随系统、关闭、4px、16px，新用户默认跟随系统，已有明确保存的圆角不变。跟随系统恢复主题默认组件圆角，所有选项都不修改原生窗口圆角；旧8px配置自动迁移到跟随系统，设置随重启和备份保留。支持窗口置顶、点击外部隐藏、唤出位置，以及粘贴后窗口显隐和排序设置
- 主窗口与设置窗口的「背景材质」提供无（默认）、毛玻璃、云母，仅列出本机支持的选项：Windows 使用原生亚克力/云母，macOS 使用系统毛玻璃，Linux 当前保守提供不透明背景。平台不支持的保存偏好仍保留，并提示本机降级效果；旧云母 Alt 迁移为云母，浓度不变。「背景浓度」提供 40%/55%/72%/85%，越高越不透明，仅改变背景染色，不改变控件透明度或模糊半径；实际无材质时隐藏浓度及说明，保留浓度设置供重新开启材质时使用。预览及拖动绘制层保持透明，不叠加系统背板
- 材质与组件使用统一表面层级：设置分组、侧栏及普通历史卡片直接共享窗口材质，避免巨大白色面板或重复透明涂染产生灰块；输入、按钮、分段控件与菜单/通知/对话框保留清晰表面。共用间距、字号、图标、描边与阴影token；设置选项最大640px，音效时机最大360px。Windows原生背板明暗与应用主题同步，组件圆角不修改窗口圆角；启动及主题/材质刷新时读取系统客户区动画开关。
- 无顶部标题栏的主窗口：按住搜索框上方的浅色短横握柄移动窗口，顶部边缘仍可缩放；搜索框独占一行，分类标签与 GPUI Kit 分组 Select 同行；从卡片“选择”或列表 Ctrl+A 进入多选，使用批量操作栏退出；清理历史、窗口置顶和设置入口位于托盘菜单
- 设置页可隐藏分类筛选栏，并选择紧凑、标准或宽松的历史卡片密度。默认两行文字预览较图片卡片更矮；正文或居中的图片缩略图在上，时间、大小和来源应用在下，悬停卡片显示操作按钮
- 历史列表向下滚动后，右下角显示浅色描边的返回顶部按钮；点击平滑回顶，用户滚动可打断，到顶后按钮隐藏。记录数固定显示在列表下方，不随滚动移动
- 静态列表图片按显示尺寸与 DPI 后台生成缩略图，有界并发和可见范围缓存；悬停预览、复制与导出保留原图
- 卡片预览行数、绝对/相对时间、字符数、大小和来源应用信息可在设置窗口配置；来源应用可按名称、图标或两者显示，图标缓存为 PNG
- 数据设置可导出 GPUI ZIP，并将 GPUI/旧版 ZIP 或旧数据库恢复到独立空目录，显示结果并打开恢复的数据
- 分组、分类与关键词组合筛选；主唤出快捷键可录制或输入，冲突或保存失败保留原配置
- 按分组配置历史数量和天数，保护收藏与置顶；首次启用或收紧策略需确认，旧数据升级不自动扩大清理范围
- 普通文件只保存原始路径，不复制文件实体；原件移动或删除后提示路径失效。图片历史仍保存像素并可作为文件复制，HTML 缺少纯文本时自动转换
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

默认数据目录统一使用应用名称，不包含作者或框架名称：

| 平台 | 默认数据目录 |
|---|---|
| Windows | `%LOCALAPPDATA%\ElegantClipboard` |
| macOS | `~/Library/Application Support/ElegantClipboard` |
| Linux | `$XDG_DATA_HOME/elegantclipboard`，默认 `~/.local/share/elegantclipboard` |

升级时自动迁移旧 GPUI 默认目录中的数据库、图片、图标及原生格式 blob，并修正附件引用和开机启动路径；迁移成功后清理旧目录及空的专属父目录。新版启动时移除历史 `staged` 文件副本及数据库中的副本引用，保留普通文件原始路径；导入或恢复旧备份也不重新落盘副本。新旧目录均有数据时使用新目录、保留旧目录并提示，不自动覆盖或合并。旧版仍运行时先唤回旧实例，退出后再迁移。指向旧默认目录的 `--data-dir` 会兼容重定向到新目录，其他自定义目录保持原样。

测试或隔离运行可显式指定目录：

```powershell
cargo run -p elegant-clipboard-gpui --locked -- --no-monitor --data-dir .\target\gpui-check
```

## 诊断日志

诊断日志默认开启，不需要设置 `RUST_LOG`。Windows 日志位于 `%LOCALAPPDATA%\ElegantClipboardLogs`，每次启动使用独立会话目录，记录版本、进程、启动/退出、显示器坐标与 DPI、窗口和悬停预览的关键生命周期及缩放操作。日志目录与剪贴板数据目录分开，不影响数据库迁移或备份恢复的空目录检查。

- `application.log` 按大小轮转，每份上限 4 MiB，另保留最多 3 份轮转文件；启动时仅清理旧的非活动会话，保留最近 5 个，不删除其他运行实例的日志。
- Rust panic 单独写入 `panic.log`，包含线程、代码位置及强制采集的堆栈，并同步落盘；默认 stderr panic 输出也保留。
- 不主动记录剪贴板正文、搜索词、图片像素或分组名称。错误消息和堆栈可能含本机路径，分享前请检查。
- 系统强制结束、断电及 Windows fail-fast 等情况可能绕过 panic hook；没有 `panic.log` 不代表没有崩溃。再次遇到闪退时，请保留对应时间的整个会话目录，并附上 Windows「事件查看器 → Windows 日志 → 应用程序」中的错误记录。

## 构建与验证

```powershell
just check
just test
just build
```

Release 可执行文件为 Windows 的 `target/release/elegant-clipboard-gpui.exe`，或 macOS/Linux 的 `target/release/elegant-clipboard-gpui`。

生成未经签名的 Windows x64 独立 ZIP 和 NSIS 安装包。安装包需要本机已安装 NSIS 3，且 `makensis` 可在 PATH 中找到：

```powershell
just package
```

在 macOS 或 Linux **本机**打包：

```sh
just package-unix
# 或 pwsh -NoProfile -File scripts/package-gpui-unix.ps1
```

macOS 生成 `ElegantClipboard-v<版本>-<架构>-apple-darwin.dmg`，内含 `ElegantClipboard.app` 和指向 `/Applications` 的快捷方式。打开 DMG，将应用拖到 Applications，推出磁盘映像后从“应用程序”启动。应用仅进行本地 ad-hoc 签名，未使用 Developer ID 签名或 Apple 公证；确认下载来源可信后，如首次启动被拦截，可在“系统设置 → 隐私与安全性”中选择“仍要打开”，不要关闭全局安全检查。自动粘贴需授予辅助功能权限。

Linux 额外生成 `.deb` 和 `.rpm`，文件名与 `.tar.gz` 一样包含 Cargo 版本和实际 Rust host target（`x86_64-unknown-linux-gnu` 或 `aarch64-unknown-linux-gnu`）。`.tar.gz` 仍是不捆绑系统动态库的二进制、许可证和说明。安装包把命令放到 `/usr/bin/elegant-clipboard`（实际二进制为 `elegant-clipboard-gpui`），并安装桌面项和 `hicolor` 图标；卸载不删除 `$XDG_DATA_HOME/elegantclipboard`（默认 `~/.local/share/elegantclipboard`）中的历史数据。链接库依赖由 `dpkg-shlibdeps` 和 RPM `rpmdeps` 按本机二进制生成，另外声明运行时加载的 Vulkan、Wayland、Ayatana AppIndicator（或旧 AppIndicator）、`xdg-utils` 和 `dbus`。`pulseaudio-utils` 与 `mesa-vulkan-drivers` 是推荐依赖。生成安装包需要 `dpkg-dev` 和 `rpm`。可用 `sudo apt install ./ElegantClipboard-v<版本>-<target>.deb` 或 `sudo dnf install ./ElegantClipboard-v<版本>-<target>.rpm` 安装，用包名 `elegant-clipboard` 卸载。脚本支持 macOS/Linux 的 x86_64 和 aarch64 原生工具链。macOS CI 与发布任务会校验 DMG、只读挂载、检查 Applications 快捷方式与应用签名，并从映像内启动隔离数据目录的窗口 smoke。

产物与 SHA-256 文件写入本地 `target/packages/`，CI artifact 与 GitHub Release 仅上传平台包，不上传 `.sha256` 文件。Windows 同时生成 `ElegantClipboard-v<版本>-windows-x64.zip` 与 `ElegantClipboard-v<版本>-windows-x64-setup.exe`；Linux 同时生成 `.tar.gz`、`.deb` 和 `.rpm`。安装包按当前用户安装到 `%LOCALAPPDATA%\Programs\ElegantClipboard`，开始菜单创建快捷方式，完成页可选桌面快捷方式和立即运行；卸载删除安装目录、这两处快捷方式，以及命令指向该程序的当前用户开机启动项。历史数据仍在 `%LOCALAPPDATA%\ElegantClipboard`，卸载不删除。不提供自动更新。CI/发布矩阵分别在 Windows、Ubuntu 和 macOS runner 上构建，不从 Windows 伪造 Mac/Linux 包。

## 版本管理

```powershell
.\scripts\bump-version.ps1 2.0.1
```

脚本更新根 Cargo 工作区版本和锁文件。GPUI 重构版从 `2.0.0` 开始；后续发布标签必须与 Cargo 版本一致，例如 `v2.0.1`。版本记录见 [变更日志](docs/CHANGELOG.md)。

## 许可证

[MIT License](LICENSE)
