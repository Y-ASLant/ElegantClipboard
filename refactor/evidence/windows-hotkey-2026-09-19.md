# Windows 全局快捷键验证

- `clipboard-platform` 独立消息线程注册 `Ctrl+Shift+V`，收到 `WM_HOTKEY` 后通过有界通道通知 GPUI 主线程。退出时发送 `WM_QUIT`、注销快捷键并回收线程。
- 注册失败会在窗口状态栏说明错误；不影响托盘恢复入口。初版组合键固定为 `Ctrl+Shift+V`；后续已增加四个预设选项，见 [快捷键设置记录](windows-hotkey-settings-2026-09-19.md)。
- 原生验收使用 `target/qa/hotkey-smoke` 隔离目录并关闭剪贴板采集。启动后从另一个进程注册同一组合键失败，确认应用已持有；发送 `WM_CLOSE` 后窗口不可见但进程仍运行；模拟 `Ctrl+Shift+V` 后窗口重新可见且最终取得前台焦点。测试结束强制终止该隔离实例，未触及真实历史数据库。
- 冲突验收先由测试进程占用该组合键，再启动第二个隔离实例。窗口保持运行，底部红色状态提示“Ctrl+Shift+V 注册失败：热键已注册”；释放测试占用后组合键可重新注册，说明应用没有在失败后错误持有快捷键。截图保存在被 Git 忽略的 `target/qa/hotkey-conflict.png`。
- `cargo test --workspace --locked --quiet`：97 项通过；`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo fmt --all -- --check`、`cargo build -p elegant-clipboard-gpui --locked`：通过。
