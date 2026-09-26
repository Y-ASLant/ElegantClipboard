# 自动粘贴按键界面验收（2026-09-25）

## 范围

- 使用 `target/debug/elegant-clipboard-gpui.exe --no-monitor --data-dir target/ui-paste-key-20260925` 和隔离 SQLite 数据库。
- `--no-monitor` 禁用剪贴板采集；本次没有向真实剪贴板写入内容，也没有向其他应用注入按键。

## 结果

- 设置窗口的“快速粘贴”卡片显示“自动粘贴使用按键”，默认选中 `Ctrl+V`。
- 点击 `Shift+Insert` 后，按钮变为选中；SQLite 的 `settings.gpui_paste_key` 为 `shift_insert`。
- 关闭并重启隔离实例，再次打开设置窗口，`Shift+Insert` 仍为选中状态。
- 自动测试覆盖两种按键序列、服务命令确认、重启读取，以及 GPUI 备份恢复。
- `cargo fmt --all -- --check`、`cargo check --workspace --all-targets --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo test --workspace --locked` 和 `cargo build -p elegant-clipboard-gpui --release --locked` 均通过。Release EXE 为 Windows GUI 子系统，主线程栈预留 8 MiB。

## 仍需验收

- 正常监听模式下，在支持两种粘贴按键的目标应用中验证实际粘贴；按键注入返回成功只表示事件已发送，无法证明目标应用完成粘贴。
