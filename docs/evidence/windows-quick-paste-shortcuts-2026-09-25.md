# 快速粘贴槽位界面验收（2026-09-25）

## 范围

- 使用 `target/debug/elegant-clipboard-gpui.exe --no-monitor --data-dir target/ui-shortcuts-20260925`，数据只写入隔离目录。
- `--no-monitor` 禁用剪贴板采集和快速粘贴全局快捷键注册；本次没有向真实剪贴板写入内容，也没有尝试向其他应用发送粘贴按键。

## 结果

- 设置窗口正常打开；“快速粘贴”卡片可展开普通记录槽位。窄设置卡片中，槽位名称与当前组合占一行，修改、停用、默认操作占下一行。
- 点击普通记录第 1 槽的“修改”，编辑框显示原值 `Alt+1`，位于该槽位下方。输入 `Ctrl+Alt+Z` 并保存后，界面显示成功反馈和新组合。
- 隔离目录的 SQLite `settings.gpui_paste_shortcuts` 中，普通第 1 槽确认为 `Ctrl+Alt+Z`。
- 诊断设置窗口栈溢出时，同一调试 EXE 的 PE 主线程栈预留从 1 MiB 调整到 8 MiB 后窗口可打开；随后在应用构建脚本中固定该设置，并使用正常构建的 EXE 重做上述界面操作。
- `cargo build -p elegant-clipboard-gpui --release --locked` 成功；Release EXE 的 PE Subsystem 为 Windows GUI，主线程栈预留为 8 MiB。
- 用 Release EXE 和 `target/ui-shortcuts-release-20260925` 隔离数据目录再次打开设置窗口。收藏第 4 槽输入 `Ctrl+Alt+F4` 后保存成功，再停用为未设置；收藏第 2 槽停用后恢复默认 `Ctrl+Alt+2`。最终 SQLite 设置值与界面一致。
- 使用调试 EXE 和 `target/ui-shortcut-batch-capture-20260925` 隔离目录再次验收。普通第 1 槽点击“录制”，按 `Ctrl+Alt+Z` 后输入框显示该组合，保存后槽位显示新值。
- 普通与收藏两组分别点击“全部停用”和“全部恢复默认”；界面状态随保存确认更新。最终 SQLite 的普通十槽和收藏十槽均与默认配置一致。
- 本轮 `cargo fmt --all -- --check`、`cargo check --workspace --all-targets --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo test --workspace --locked` 和 `cargo build -p elegant-clipboard-gpui --release --locked` 均通过，共 212 项测试通过、2 项默认跳过；Release EXE 为 Windows GUI 子系统，栈预留 8 MiB。

## 仍需验收

- 正常监听模式下的全局快捷键注册、冲突提示、数字小键盘变体和跨应用粘贴。
- 收藏第 5 至第 10 槽逐项修改和备份恢复后的实际界面操作；这些路径已有隔离自动测试覆盖数据与命令逻辑。
