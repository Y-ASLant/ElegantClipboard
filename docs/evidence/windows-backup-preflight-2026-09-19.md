# Windows GPUI ZIP 恢复预检查（2026-09-19）

- GPUI v1/v2 备份恢复在创建暂存目录前读取原始 ZIP 中央目录，拒绝重复路径，包括 ZIP 库公开条目列表会折叠的重复 `manifest.json` 与 `clipboard.db`。同时限制归档不超过 100,000 项、展开总量不超过 50 GiB，并检查数据库及媒体实际读取字节数。
- 新测试构造含重复格式说明、数据库和媒体路径的 ZIP，确认恢复失败且目标目录不产生任何文件；另覆盖条目数与展开大小限制。既有 v1 兼容、v2 媒体恢复及路径穿越测试继续通过。
- `cargo test --workspace --locked`：151 项通过（124 核心、22 Windows 后端、5 应用）；格式检查、严格 Clippy、Windows Debug/Release 构建无错误和警告。
- Release 命令行对真实备份 `--import-backup` 返回成功并创建数据库；对含重复 `manifest.json` 中央目录项的备份返回错误，目标目录只有实例锁，没有数据库或媒体。隔离 QA 脚本与生成文件位于忽略目录 `target/qa/`。
