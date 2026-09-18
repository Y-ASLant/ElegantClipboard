# Windows 基础版

## 范围

Windows 优先，其他平台保留为后续目标。基础版使用新的根 Cargo workspace；原有 `src-tauri` 与本地 `clipboard-rs` 保持独立 workspace，不改变旧入口和旧锁文件。

首批功能：文本采集、历史持久化、精确去重、搜索、复制、删除、置顶、暂停/恢复。后续已增加全局唤出快捷键、HTML/RTF、PNG 图片和普通文件路径的采集、历史、预览与复制，以及经全局快捷键唤出后返回原窗口的受控粘贴；虚拟文件保真仍待开发。

## 当前实现

- `crates/clipboard-core`：复用原有数据库 schema、迁移、仓储和去重源码，提供 UI 无关的文本、HTML/RTF、图片与文件路径历史用例。
- `crates/clipboard-platform`：Windows 原生剪贴板事件监听及全局快捷键；独立 worker 串行处理历史操作；请求代次、实例锁、重复启动唤出、有界队列和关闭清理。
- `crates/clipboard-gpui`：Windows 原生主窗口、搜索、虚拟历史列表、复制/删除/置顶、暂停确认、加载更多、文本/富文本/图片/文件路径预览、收藏与收藏筛选、导入分组浏览、组内拖拽排序、系统托盘和错误反馈。

共享源码暂通过 `#[path]` 编译原有文件，避免复制一套数据库。旧壳接入共享 crate 时再物理移动源码；当前新核心不依赖 Tauri。

默认目录由 `ProjectDirs::from("com", "ASLant", "ElegantClipboard-GPUI")` 生成，位于 Windows 用户 LocalAppData。不自动读取旧版安装目录中的数据库；可通过显式导入命令复制旧库到空 GPUI 数据目录。自动测试使用临时目录、合成文本和仓库内 PNG 样本。

限制：单条文本最多 1 MiB，富文本的 HTML、RTF 原始字节与纯文本合计最多 1 MiB；图片最多 2500 万像素、编码 PNG 最多 50 MiB；文件路径列表最多 256 项或 1 MiB，仅保存原路径，不备份文件内容。普通未固定历史按原仓储规则保留一万条；空白文本不保存。重复的完全相同文本移动到顶部，文字和换行原样保存。队列满时报告采集失败，不把不同复制事件主动合并成最后一条。

## 依赖策略

2026-09-18 已查询原有直接依赖的 crates.io 元数据；新增 base64 于 2026-09-19 核对：

| 直接依赖 | 最新稳定版 |
|---|---|
| gpui-kit | 0.6.2 |
| tray-icon | 0.25.1 |
| raw-window-handle | 0.6.2 |
| rusqlite | 0.40.2 |
| clipboard-rs | 0.3.5（注册表版，启用 `image` 功能） |
| windows | 0.62.2 |
| anyhow | 1.0.104 |
| async-channel | 2.5.0 |
| blake3 | 1.8.7 |
| base64 | 0.23.1 |
| directories | 6.0.0 |
| parking_lot | 0.12.5 |
| serde | 1.0.229 |
| serde_json | 1.0.151 |
| tracing | 0.1.44 |
| tempfile（测试） | 3.27.0 |

工具链要求 Rust 1.98+；本机已使用 1.98.0。Cargo.lock 固定实际解析结果。依赖升级流程：查询最新稳定版本 → 更新 workspace requirements → `cargo update` → fmt/clippy/test/build → 原生交互验收。存在上游兼容约束的间接依赖不能强行替换为不同主版本。

**上游例外**：本轮继续接入最新稳定 gpui-kit，其 Windows 依赖路径 `gpui-kit → gpui-base/gpui-component → ropey` 使用 ropey 2.0.0-beta.1。详见 [技术决策](decisions/0001-windows-gpui.md)，不宣称整个依赖树均为稳定版。

## 启动与操作

```powershell
# 正常使用：启动原生窗口并记录后续复制的受支持内容
cargo run -p elegant-clipboard-gpui --locked

# 隔离检查：禁用采集，只浏览指定目录中的历史
cargo run -p elegant-clipboard-gpui --locked -- --no-monitor --data-dir .\target\gpui-check

# 窗口冒烟：禁用采集，约 3 秒后自动退出
cargo run -p elegant-clipboard-gpui --locked -- --smoke-test --data-dir .\target\gpui-smoke

# 从旧版数据库导入到尚无 clipboard.db 的 GPUI 数据目录；命令完成后退出
cargo run -p elegant-clipboard-gpui --locked -- --import-db "C:\旧版目录\clipboard.db"
```

- 搜索支持中文与字面 `%`/`_`，等待 150ms 后查询；回车复制当前选中结果。
- 列表支持 ↑/↓ 选择、Enter 复制、Ctrl+Enter 粘贴到原窗口、Delete 删除；Ctrl+F 聚焦搜索。
- 置顶、删除与复制按钮直接调用后台服务；文本复制使用完整正文，HTML/RTF 默认尝试一次写回纯文本及可用的富格式并读回检查结果，卡片“纯文本”与预览“复制纯文本”只写入其已保存的纯文本表示。没有纯文本表示时明确报错，不复制预览占位文案；图片从保存的文件解码写回剪贴板，文件路径在源文件仍存在时写回剪贴板。
- “暂停记录 / 恢复记录”状态保存在 GPUI 数据库，重启后先恢复状态再启动监听；保存成功后界面才切换。暂停时已有历史仍可搜索和复制。
- 点击“查看”或在列表按 Space 打开完整文本、HTML/RTF 的纯文本表示、图片或文件路径；文本和路径可滚动、选取，图片按原比例显示；富文本预览同时提供“复制纯文本”和“复制富文本”，窄窗口中按钮可换行。文本、网址与富文本可从预览进入编辑，保存后保留记录 ID、分组与收藏状态并刷新搜索结果；修改富文本会转为纯文本，空内容保存会报错，请使用独立“删除”按钮。Esc 在编辑时取消修改，在只读预览时返回列表。预览按需从后台读取，关闭后的旧响应不会重新打开预览。
- 点击条目“收藏 / 取消收藏”保存常用文本；“全部 / 收藏记录”切换视图。搜索、数量与加载更多均作用于当前视图，取消收藏不会删除记录。收藏状态持久化，启动默认显示全部。
- 自定义分组会在筛选栏下方显示，也可点击“＋ 新建”输入名称后创建。点击分组切换其历史，搜索、收藏筛选、数量与分页均限定在当前分组；选中自定义分组后可重命名或删除。删除前显示名称和记录数，确认后将组内记录移到默认分组，保留收藏与原始内容；与旧版级联删除记录的行为不同。分组栏在窄窗口中横向滚动。每张卡片的“分组”按钮可展开目标选择栏，将已有记录移至自定义分组或移回默认分组；保存后当前列表与组内数量更新。新采集内容仍进入默认分组。
- 文本、网址、富文本、图片和文件记录均可从卡片非按钮区域拖动；拖动浮层显示类型和摘要，目标上/下半区的主题色插入线表示可放置，红色插入线表示当前目标不可放置。按钮区保留各自点击操作，边缘拖动滚动已加载列表。顺序写入 SQLite，收藏列表维护独立顺序。置顶与普通记录分别排序，跨区拖动会提示先调整置顶状态。Esc 可取消拖动。各种记录使用统一的预览容器、操作区和行高；浮层、插入线及排序反馈共用动效时长并遵从系统减少动态效果设置。
- 默认加载 100 条，点击“加载更多”继续；仅渲染可见行。
- 顶部“窗口设置”默认收起，展开后“外观”提供跟随系统、浅色和深色；保存成功后生效，重启恢复选择。设置存于同一 SQLite 数据库，手动主题不随系统通知改变；未知设置值回退为跟随系统。
- 窗口使用 gpui-kit `TitleBar` 自定义标题栏，列表与全文预览共用；支持拖动、双击最大化/还原、最小化、边缘缩放和关闭。
- 关闭窗口后隐藏到系统托盘，左键托盘图标、右键菜单“打开剪贴板历史”或按全局唤出快捷键可恢复并聚焦；右键菜单“退出 ElegantClipboard”结束后台监听。展开顶部“窗口设置”后，“唤出”可选 `Ctrl+Shift+V`（默认）、`Alt+C`、`Ctrl+Alt+V` 或关闭，选择保存在 GPUI 设置中。组合键被占用时提示并恢复原快捷键，托盘入口仍可用；托盘初始化失败时关闭窗口正常退出。用全局快捷键从目标应用唤出后，卡片与预览上的“粘贴”按钮可先复制，再隐藏历史窗口并尝试向原窗口发送 Ctrl+V；托盘唤出或目标窗口失效时仅能手动复制。焦点不匹配、修饰键未松开或按键注入失败会显示错误并保留已复制的内容。Windows 无法从按键注入返回值确认目标应用已经完成粘贴，因此成功提示仅表示已发送快捷键。当前不支持 Shift+Insert、自定义粘贴键或任意按键组合录制。
- 同一数据目录下再次启动程序会唤回已有窗口并退出第二个进程；指定另一数据目录可并行运行隔离实例。不同实例之间全局快捷键仍只能由一个实例注册，冲突会在另一个窗口提示。
- 导入只读访问旧库，使用 SQLite 在线备份获取含 WAL 的一致快照，在新目录升级副本并校验；如果 GPUI 目录已有 `clipboard.db` 就拒绝覆盖。旧版默认数据库位于旧程序可执行文件旁；如旧版 `config.json` 配置了 `data_path`，应指定该目录中的 `clipboard.db`。可用 `--data-dir` 为导入选择另一个空目录。当前界面可浏览默认分组及自定义分组的文本、网址、HTML/RTF、图片和普通文件路径；导入的外部图片与文件保留原路径，导入不会复制原文件。文件记录若仅含旧版虚拟文件载荷而没有可用原路径，不能在新版本复制。不要把开发版的 `--data-dir` 指向旧版数据目录。
- 主窗口“导出备份”保存 v2 GPUI ZIP：在线 SQLite 快照、引用的可读取图片、图标及暂存文件。默认排除非 GPUI 设置（包括旧库潜在的密码或 token）；源资产缺失时显示数量，记录保留原路径。备份不包含普通文件记录指向的原文件，但包含可读取的暂存副本。恢复使用 `elegant-clipboard-gpui --import-backup <备份.zip> --data-dir <空目录>`，兼容 v1/v2；在创建暂存文件前拒绝重复归档路径、超过 10 万项或展开后超过 50 GiB 的备份，随后验证格式、数据库与资产引用后才安装，不覆盖已有数据库或资产目录。恢复期间目标目录不能由其他实例使用。
- 旧版 Tauri ZIP 可用 `elegant-clipboard-gpui --import-legacy-backup <旧版备份.zip> --data-dir <空目录>` 导入；它会升级数据库，恢复并重建 `images/`、`icons/`、`staged/` 路径，排除非 GPUI 设置。归档中未包含的外部图片保留原引用并计入报告。原文件丢失时，普通文件记录可使用恢复到 GPUI 目录中的暂存副本进行复制；删除或自动淘汰记录后会清理无引用的暂存副本。旧版虚拟文件的额外剪贴板格式仍不支持写回。单独的旧 `clipboard.db` 仍用 `--import-db`，该方式不会复制外部媒体。
- 分组栏“清理历史”在明确确认后删除当前分组中未置顶且未收藏的记录；搜索或收藏筛选不缩小清理范围。其他分组及受保护记录保留，本地受管理图片在无引用时删除。操作无法撤销，可先使用“导出备份”。
- 每张卡片标题区可点“选择”，随后可“全选已加载”或取消选择；“删除选中”需再次确认，并会删除所选的置顶和收藏记录。选择仅作用于当前已加载列表，切换搜索、分组或收藏视图会清空选择；后台校验列表代次和分组，在同一事务内删除，失败不部分删除，提交后按引用清理受管图片及暂存副本。

## 验证命令

在仓库根目录运行：

```powershell
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
cargo build -p elegant-clipboard-gpui --locked
cargo run -p clipboard-platform --example isolated_clipboard_smoke --locked
```

当前 148 项测试通过（核心 122、Windows 后端 22、应用状态/参数 4），覆盖暂停确认与重启恢复、搜索代次与选中 ID 保持、自定义分组浏览、创建、重命名、安全删除、批量删除的事务回滚与媒体清理、记录移动与组内排序、文本编辑与过期内容拒绝、HTML/RTF 二进制存储与混排拖动及纯文本复制判定、图片存储、文本/图片/文件路径预览、容量淘汰清理、文件源缺失、键盘边界、冒烟参数隔离，以及在线旧库导入、GPUI v1/v2 备份导出恢复、网址兼容、拒绝无效来源、实例唤出和快捷键设置。fmt、Clippy（warnings 视为错误）、Windows debug/release 构建及 release 窗口冒烟通过；分组栏、整卡拖动和最小尺寸布局已在隔离窗口验收。

原生窗口已通过启动/退出、中文路径、重复启动唤回，以及 120 条合成数据下的加载更多、字面搜索、空结果、置顶和删除检查；10,000 条合成文本的启动与末尾搜索也已验收。HTML/RTF、图片、文件卡片和对应预览均在隔离目录中截图核验；富文本卡片之间的真实鼠标拖动也已核对 SQLite 顺序。窗口交互使用隔离目录并禁用实际采集；另在非交互式窗口站中完成文本、HTML/RTF、图片和文件路径的真实系统剪贴板采集与写回，交互桌面的剪贴板序列号未变。仍未覆盖常见应用之间的真实复制粘贴、中文 IME、长时间运行、release 打包或跨平台运行。完整过程见 [验证记录](evidence/windows-ui-2026-09-18.md)、[规模验证](evidence/windows-scale-2026-09-19.md)和[隔离剪贴板记录](evidence/windows-private-clipboard-2026-09-19.md)。

Windows CI 包含 fmt、Clippy、locked 测试和应用构建，尚未在远端运行。CI 使用已核验的 [checkout v7.0.1](https://github.com/actions/checkout/releases/tag/v7.0.1) 与 [rust-cache v2.9.2](https://github.com/Swatinem/rust-cache/releases/tag/v2.9.2)。

收藏增量验证见 [2026-09-19 验证记录](evidence/windows-favorites-2026-09-19.md)。

主题增量验证见 [2026-09-19 主题验证记录](evidence/windows-theme-2026-09-19.md)。

界面间距与动画规则见 [UI 规范](UI_GUIDELINES.md)，拖拽验证见 [2026-09-19 排序验证记录](evidence/windows-reorder-2026-09-19.md)。

托盘验证见 [2026-09-19 托盘记录](evidence/windows-tray-2026-09-19.md)。

全局快捷键验证见 [2026-09-19 快捷键记录](evidence/windows-hotkey-2026-09-19.md)。

旧数据导入验证见 [2026-09-19 导入记录](evidence/windows-import-2026-09-19.md)。

重复启动验收见 [2026-09-19 实例唤出记录](evidence/windows-instance-2026-09-19.md)。

可配置快捷键验收见 [2026-09-19 快捷键设置记录](evidence/windows-hotkey-settings-2026-09-19.md)。

暂停状态持久化验收见 [2026-09-19 暂停记录](evidence/windows-pause-2026-09-19.md)。

图片存储、工作线程及界面验收分别见 [核心](evidence/windows-image-core-2026-09-19.md)、[后端](evidence/windows-image-platform-2026-09-19.md)和[界面](evidence/windows-image-ui-2026-09-19.md)。

普通文件路径接入验证见 [文件记录](evidence/windows-files-2026-09-19.md)。

HTML/RTF 接入验证见 [富文本记录](evidence/windows-rich-text-2026-09-19.md)。

受控粘贴实现与验证边界见 [粘贴记录](evidence/windows-auto-paste-2026-09-19.md)。

导入分组浏览与排序见 [分组记录](evidence/windows-groups-2026-09-19.md)。

新建分组见 [分组创建记录](evidence/windows-group-create-2026-09-19.md)。

记录移入分组见 [分组移动记录](evidence/windows-group-move-2026-09-19.md)。

分组重命名见 [重命名记录](evidence/windows-group-rename-2026-09-19.md)，整卡拖拽与统一动效见 [界面验证](evidence/windows-card-drag-2026-09-19.md)。

分组删除与记录保留见 [删除记录](evidence/windows-group-delete-2026-09-19.md)。

文本编辑与富文本降级见 [编辑记录](evidence/windows-text-edit-2026-09-19.md)。
