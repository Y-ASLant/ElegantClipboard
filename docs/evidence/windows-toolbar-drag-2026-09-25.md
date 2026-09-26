# GPUI 工具栏拖动排序验证

- 设置窗口“显示”页中的工具栏增加拖动手柄。将可见按钮拖到目标行的上半区或下半区可改变顺序；现有上移、下移和显隐按钮保留。保存仍走原有设置命令，设置按钮保持可见。
- 使用 `target/ui-toolbar-drag-20260925` 隔离数据目录和 `--no-monitor` 启动调试版。跳过引导后打开设置窗口：将“清理历史”拖到末尾，再拖回开头，最后将“批量选择”拖到“清理历史”之前。设置界面逐次反映新的按钮顺序；SQLite 中的 `gpui_toolbar` 最终保存顺序为 `batch, clear, pin, settings`。
- 增加核心测试覆盖向目标前后插入、无变化时拒绝重复保存及工具栏条目有效性。隔离验收未启用剪贴板监听，也未写入用户剪贴板。尚未在最小设置窗口尺寸下验收拖动。
- `cargo fmt --all -- --check`、`cargo check --workspace --all-targets --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo test --workspace --locked --quiet` 和 `cargo build -p elegant-clipboard-gpui --release --locked` 均通过。测试为 226 项通过、2 项按原配置忽略。发布版 EXE 的 PE 子系统为 Windows GUI（2），隔离数据目录中的发布版 `--smoke-test` 退出码为 0。
