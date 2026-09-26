# 历史列表返回顶部交互验证

- 使用 `--smoke-test --data-dir target/ui-scroll-top-20260925` 初始化隔离数据库，插入 120 条合成文本；未使用真实历史或写入系统剪贴板。
- 以 `--no-monitor` 打开调试版 GPUI 窗口。滚轮下移后，首条可见记录从编号 119 变为 116；点击“返回顶部”后重新显示编号 119，证明滚动句柄执行了严格定位。
- 初版浮动按钮遮挡末尾卡片操作区，已改为固定信息栏入口。Release 窗口中滚动后首条可见记录由编号 119 变为 116，点击入口后恢复为 119；按钮所在信息栏保持可见，未覆盖卡片操作区。
- `cargo fmt --all -- --check`、`cargo check --workspace --all-targets --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo test --workspace --locked --quiet` 和 `cargo build -p elegant-clipboard-gpui --release --locked` 均通过；测试结果为 212 通过、2 忽略。
