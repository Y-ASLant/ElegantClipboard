# Windows 数据库整理验收（2026-09-19）

- “窗口设置”的数据占用区域增加“整理数据库”，与刷新和打开目录共用小号 ghost 按钮、控件高度及窄窗口换行规则。整理和统计互斥，重复点击不会排队；后台完成或失败都会解除按钮状态。
- Windows worker 串行执行维护，不阻塞 GPUI 渲染线程。核心顺序为 `PRAGMA optimize`、`VACUUM`、`PRAGMA wal_checkpoint(TRUNCATE)`；最后一步不可省略，否则 WAL 模式下紧凑页可能仍留在日志中，主数据库和界面统计不会立即反映空间回收。日志仍被读取时明确失败，不显示成功。
- 自动测试使用 4 MiB 临时空页，验证整理后数据库文件缩小且现有历史仍可读取；Windows 后端测试验证完成事件携带更新后的数据占用并保持历史。全工作区 170 项测试通过（核心 133、Windows 后端 32、应用状态/参数 5），格式检查、严格 Clippy 及 Windows Debug/Release 构建通过且无编译警告。
- 真实 GPUI 窗口使用 `target/qa/database-maintenance-ui-20260919-v1/` 隔离数据目录并关闭采集。首次验证发现仅 VACUUM 时文件未立即缩小，据此补充 WAL 截断；复测从 4,284,416 字节降到 90,112 字节。系统最小尺寸请求后的窗口外框为 436×528，“刷新 / 整理数据库 / 打开目录”均在边界内，完成状态为“数据库已整理，数据占用已更新”。截图位于忽略目录中的 `database-maintenance-min-window.png`；测试未打开资源管理器，也未触碰用户数据。
- 重新生成 Windows x64 独立 ZIP，SHA-256 为 `5b62e5945d550b1cced69dd9cd3b03c2f37b509c1ebe6cff925f19ece7624284`；包内 exe 与 Release exe 的 SHA-256 均为 `ec301475c98a4ef519dbfc31213e1bf90b5bbbe6de58269792ee174b52e0fd4c`。解压到 `target/qa/database-maintenance-package-6ebb910a1bfa408c82ea16e53e56c914/` 后运行隔离窗口冒烟，退出码为 0。
