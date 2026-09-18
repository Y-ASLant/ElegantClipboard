# Windows GPUI 备份与恢复验收（2026-09-19）

- 新增 GPUI ZIP 格式 v1：`manifest.json`、在线 SQLite 快照 `clipboard.db` 和被历史记录引用且可读取的 `images/`。导出仅保留 GPUI 的主题、唤出快捷键、暂停采集三项设置；文本、分组、收藏及图片路径随数据库恢复，图片路径在目标目录重建。普通文件记录只有路径引用，不包含原文件。
- 恢复只接受 GPUI 格式 v1，要求显式 `--data-dir`；目标目录中的数据库或图片存在时拒绝覆盖。归档图片名限于单层安全文件名；数据库校验和记录总数检查通过后才安装。旧版 Tauri ZIP 未适配，旧 `clipboard.db` 仍使用 `--import-db`。
- 自动化测试：在线导出含文本、收藏、分组、图片和主题的数据库，恢复后逐项读取；确认未知设置未导出、无效 ZIP 与路径穿越条目不安装数据库、重复导出与恢复均拒绝覆盖。Windows worker 测试验证命令事件和独立数据目录的 CLI 恢复。全 workspace 共 137 项测试通过，Clippy `-D warnings` 通过。
- 真实 GPUI 窗口在 `target/qa/backup-ui/` 隔离目录下以 `--no-monitor` 打开，“导出备份”成功打开 Windows 保存对话框并生成 ZIP。检查 ZIP 内含格式说明和数据库，`--import-backup` 恢复后 SQLite `quick_check=ok`，原自定义分组仍在；再次恢复报“目标数据目录已有数据库或图片”。该窗口样本有 0 条记录，含记录与图片的验证由自动化测试覆盖。
- Windows release 构建与 `--smoke-test --data-dir target/qa/backup-release-smoke` 通过；fmt 检查通过。
- 本次未触碰默认用户数据目录或交互桌面剪贴板。保存对话框默认定位用户文档目录；验收时创建的 ZIP 已移入隔离 QA 目录。
