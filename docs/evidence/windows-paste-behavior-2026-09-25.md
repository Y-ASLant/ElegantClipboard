# GPUI 粘贴后窗口与排序设置验证（2026-09-25）

在独立 `--no-monitor --data-dir` 目录中运行 Windows x64 Release 应用，通过真实 GPUI 设置窗口的鼠标操作检查了以下控件：

- “粘贴后关闭窗口”可点击；对应隔离 SQLite 设置 `gpui_paste_close_window` 保存为 `false`。
- “粘贴后移到列表首位”可点击；对应隔离 SQLite 设置 `gpui_paste_move_to_top` 保存为 `false`。
- 同一设置窗口的“相对时间”和“仅图标”可点击；`gpui_display` 保存了 `time_format=relative` 和 `source_app_display=icon`。

验证命令：`cargo fmt --all -- --check`、`cargo check --workspace --all-targets --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo test --workspace --locked`、`cargo build -p elegant-clipboard-gpui --release --locked`，以及独立目录中的 `--smoke-test` 启动，均通过。常规测试共 205 项，另有 2 项隔离图标测试默认忽略。平台测试覆盖设置确认与重启加载、单条记录移到首位后的列表刷新；核心备份测试覆盖设置恢复。

这次桌面操作仅验证设置窗口和隔离数据库。未在常见目标应用中执行真实跨应用粘贴，也未验证目标应用实际接受了内容；Windows `SendInput` 的成功返回只能证明已发送按键。
