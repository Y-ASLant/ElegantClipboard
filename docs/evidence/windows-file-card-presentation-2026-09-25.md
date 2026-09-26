# GPUI 文件卡片类型和大小显示验证

- 在已有后台文件检查结果中记录单文件、单文件夹或多文件类型，单张受支持图片若超过本地 50 MiB 或 UNC 10 MiB 则标记“图片过大”。所有路径都是可读普通文件时合计实际文件大小；失效来源或目录不显示原先容易误解的路径 JSON 字节数。多文件卡片显示前 3 个文件名。
- 使用 `target/ui-file-card-kinds-20260925` 隔离目录与调试版 `--no-monitor`。5 条合成文件记录分别为失效 PNG、50 MiB 以上 PNG、目录、文本加 PNG 两文件、有效 PNG。桌面窗口中依次显示红色警告图标、普通文件图标及“图片过大”、文件夹图标、文件夹图标及两文件名；滚动到末尾后，有效 PNG 显示缩略图。超大文件显示实际 50.0 MiB，多文件显示约 4.5 KiB，目录和失效文件不显示路径字符串大小。未启用剪贴板监控或执行复制、粘贴。
- 本次未验证 UNC 文件、权限拒绝、图片解码失败和其他图片编码格式；这些情况继续使用现有回退路径。
- `cargo fmt --all -- --check`、`cargo check --workspace --all-targets --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo test --workspace --locked` 与 `cargo build -p elegant-clipboard-gpui --release --locked` 均通过；230 项测试通过、2 项按配置忽略。发布版隔离窗口重复核对了上述五类卡片；发布 EXE 为 Windows GUI 子系统（2），隔离 `--smoke-test` 退出码为 0。
