# Windows 受控粘贴增量验证（2026-09-19）

从其他程序按全局快捷键唤出历史时，热键线程记录当时前台窗口的句柄和进程 ID。只有这个目标仍有效且不是本程序，界面才允许“粘贴”。后台复制完成事件携带记录 ID 和粘贴请求标记；普通复制确认不会触发粘贴。确认后隐藏历史窗口，核对目标窗口与进程、修饰键和恢复后的前台焦点，再用 `SendInput` 发送 Ctrl+V。任一步失败都保留已复制的剪贴板内容并提示手动粘贴；按键发出只报告“已发送”，不宣称目标应用已接受。

界面中各卡片共用相同的粘贴按钮布局；全文预览也提供相同入口。列表聚焦时可按 Ctrl+Enter。通过托盘或二次启动打开时没有可靠的原目标窗口，粘贴入口禁用。

验证：`cargo check --workspace --all-targets --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo test --workspace --locked`（119 项）、`cargo fmt --all -- --check`、`cargo build -p elegant-clipboard-gpui --release --locked` 均通过且没有编译告警。debug 与 release 的 `--smoke-test` 窗口启动/自动退出均通过，使用 `target/qa/` 下的隔离数据目录并禁用采集。隔离窗口站往返程序使用 `CopyForPaste` 验证复制完成事件携带粘贴请求标记，覆盖真实 Windows 剪贴板复制；该程序不执行 `SendInput`，避免把按键发送到交互桌面。

仍需在专用交互桌面会话中验证浏览器、编辑器、Office 和管理员窗口的最终粘贴效果。Windows 对高权限目标的输入注入可能拒绝；目标应用也可能不响应 Ctrl+V。当前设计将这些情况作为可手动粘贴的降级路径。
