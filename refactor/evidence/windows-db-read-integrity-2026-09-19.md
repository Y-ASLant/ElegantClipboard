# Windows 数据库读取完整性验证（2026-09-19）

GPUI 与旧版共用的仓储原先在批量查询时跳过单行解码错误。历史列表已在前一提交改为显式报错；本次将同样的规则用于媒体引用、同步条目和设置的批量读取。媒体清理先读取待删路径；路径读取失败时停止清理，不把部分结果当作完整引用集合。

在临时 SQLite 数据库中将图片路径和文件载荷改为不匹配字段类型的 BLOB：引用查询返回错误，记录仍在；清理历史前读取待删路径失败时，记录和已保存图片均保留。验证通过：`cargo test --workspace --locked --quiet`（156 项）、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo fmt --all -- --check`、`cargo build -p elegant-clipboard-gpui --release --locked`，以及旧 Tauri 入口 `cargo check --manifest-path src-tauri/Cargo.toml --locked`。release 程序以 `--smoke-test --no-monitor --data-dir .\target\qa\db-read-integrity-smoke-20260919` 运行并以退出码 0 结束。这些用例验证损坏字段的失败路径，不代替真实磁盘故障或长时间运行测试。
