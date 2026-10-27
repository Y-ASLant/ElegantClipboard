# 默认数据目录迁移验证（2026-10-27）

## 新旧目录

| 平台 | 新默认目录 | 兼容的旧 GPUI 默认目录 |
|---|---|---|
| Windows | `%LOCALAPPDATA%\ElegantClipboard` | `%LOCALAPPDATA%\ASLant\ElegantClipboard-GPUI\data` |
| macOS | `~/Library/Application Support/ElegantClipboard` | `~/Library/Application Support/com.ASLant.ElegantClipboard-GPUI` |
| Linux | `$XDG_DATA_HOME/elegantclipboard`（默认 `~/.local/share/elegantclipboard`） | `$XDG_DATA_HOME/elegantclipboard-gpui`（默认 `~/.local/share/elegantclipboard-gpui`） |

旧默认路径的显式 `--data-dir` 兼容指向新路径。其他自定义数据目录保持原样，包括无可用用户主目录时的显式目录运行。

## 迁移与清理规则

- 默认目录的迁移由独立文件锁串行保护。旧实例仍运行时优先唤回，不迁移正在使用的数据库。新旧实例文件锁在访问历史前检查；Windows 在全程持有迁移互斥锁时进行旧锁到新锁的交接。
- 新目录不存在或只有实例锁，且旧目录含历史数据库时，将旧目录整体移动到新位置。SQLite WAL、图片、图标、文件副本、原生格式 blob、其他用户文件全部随目录移动，不做递归复制后删除。
- 移动前写持久迁移标记；对数据库做完整性检查和正常 schema 升级，再单事务改写属于旧根内部的图片、图标、文件列表和 payload 引用。外部用户文件、备注、收藏、分组、排序、时间戳、全部设置和原生格式数据保持不变。路径修改失败回滚；迁移标记保留供下次启动重试。
- 后续“仅保存文件原路径”改动在 Service 开库后独立清理 `staged`：先事务剥离副本引用与已移除的副本设置，再仅删除当前数据目录内受管副本树。目录迁移过程中不复制这类资产，整目录移动后由清理入口处理；图片历史与原生格式 blob 保留。
- 开机启动命令更新到新目录：Windows 更新 Run 项并兼容旧 GPUI 值名，迁移已有 StartupApproved 状态；Linux 迁移 XDG desktop 文件，macOS 迁移 LaunchAgent 文件，保留 Hidden/Disabled 等原设置。新条目经验证后才移除旧项；不存在旧项时不自动启用自启，重试不覆盖新配置。
- 迁移成功后旧默认目录不再存在，仅向上删除确认为空且位于旧专属路径内的父目录，不删除作者目录里的其他应用数据。
- 新旧两处均有数据或未知内容时使用新目录，保留旧目录并提示；不会覆盖、静默合并或删除旧历史。符号链接与 Windows 重解析点不被迁移逻辑跟随。
- 备份格式标识和内部 Cargo 包名保持兼容，本次仅统一默认存储目录与相关自启名称。

## 自动验证

- 完整 workspace 编译、严格 Clippy、格式和源码大小检查通过；全部 Rust 文件均不超过 500 行。
- Windows workspace 测试：395 项通过（core 210、platform 128、GPUI 57），2 项原有忽略。
- core 新增 9 项隔离回归：真实目录移动后所有受管引用重定位、外部与缺失附件保持原样、JSON 未知字段原始字节保留、记录与设置/native blob 保真、重开与重复执行、schema 可选列升级、非法 JSON/SQL 故障回滚、Windows 路径别名和边界。
- 平台目录新增 8 项 Windows 隔离回归：正常迁移与空父清理、冲突保留、旧锁占用、损坏数据库重试、启动项更新失败后恢复、空新目录与未知文件保护、旧默认参数别名以及自定义目录隔离。Unix 另有符号链接拒绝回归。
- 自启迁移：Windows 3 项纯命令/路径测试；Linux 5 项与 macOS 5 项隔离文件回归在临时 Windows 测试入口执行，10 项通过。临时入口已删除，未写用户真实注册表或 HOME 启动目录。
- `cargo zigbuild -p clipboard-platform --lib --locked --target aarch64-apple-darwin --target-dir target/cross-macos` 与 Linux x64 对应命令均退出码 0；交叉编译不代表实际桌面或登录会话操作已通过。

## 原生窗口观察

最终 `cargo build -p elegant-clipboard-gpui --release --locked` 通过。使用独立 `--smoke-test --data-dir` 目录实际启动 Release，点击数据设置入口并查看截图，数据页及导航正常渲染，自定义目录产生 `clipboard.db`；退出码 0。交互桌面剪贴板序列号前后均为 `2901`。截图位于 Git 忽略目录 `target/qa/data-directory-native.png`；临时驱动已删除。

目录迁移本身通过隔离的默认路径夹具验证，没有在真实用户目录里执行迁移或清理。当前共有 256 个 Rust 源码、测试及示例文件，均符合 500 行限制。

## 验证边界

测试数据全部使用临时目录，未迁移或删除用户当前的真实历史目录，未读写真实剪贴板内容。macOS/Linux 实际登录启动与系统文件权限仍需对应系统验证。Windows 注册表项的实际写入未作为自动测试执行；其命令归属判断及 Unix 文件事务已分别覆盖。
