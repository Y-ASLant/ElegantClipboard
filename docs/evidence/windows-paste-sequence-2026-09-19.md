# Windows 自动粘贴剪贴板变化保护（2026-09-19）

后台复制成功事件现在携带写回后的 Windows 剪贴板序列号。历史窗口隐藏并恢复目标窗口后，发送 Ctrl+V 前再次读取序列号；若为 0 或已经变化，则取消自动按键、重新显示历史窗口并提示检查剪贴板后手动粘贴。该检查减少复制确认到按键发送之间粘贴错误内容的风险，但无法消除最后一次检查与按键之间的并发改写，也不能证明目标应用完成了粘贴。[Microsoft 文档](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getclipboardsequencenumber)说明序列号按窗口站区分，内容改变或清空时递增；访问剪贴板失败可返回 0。

在非交互式窗口站运行 `cargo run -p clipboard-platform --example isolated_clipboard_smoke --locked`：文本复制确认携带非零序列号，与当前值相同；随后模拟其他程序改写剪贴板，新值不同；富文本、纯文本、图片和文件路径的采集与写回继续通过。`cargo test --workspace --locked --quiet`（156 项）、严格 Clippy、fmt 和 release 构建通过；release 程序以 `--smoke-test --no-monitor --data-dir .\target\qa\paste-sequence-smoke-20260919` 启动并以退出码 0 结束。仍需在专用交互桌面中验证常用目标应用的焦点恢复和最终粘贴。
