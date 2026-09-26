# GPUI 卡片拖动区域验证

- 普通模式的历史卡片仅在左右两侧 32px 区域启动拖动。显示设置中的“显示卡片拖动区域”默认开启，可隐藏视觉提示而保留两侧拖动；批量模式不显示拖动入口。
- 使用 `target/ui-drag-areas-20260925` 隔离数据目录，执行 `--smoke-test` 初始化，然后通过 SQLite 插入三条合成文本记录，以 `--no-monitor` 启动调试版。没有启用剪贴板采集，也没有向用户剪贴板写入内容。
- 在隔离窗口中，从左侧拖动 Gamma 到 Beta 下方，再从右侧拖动 Alpha 到顶部；界面顺序和 SQLite 的排序值均随之改变。关闭提示后，从无提示的左侧区域拖动 Gamma 到顶部，排序仍成功，设置值已保存在 SQLite。
- 本轮未点击卡片中心，因为该动作会执行复制或受控粘贴。中心区域已移除拖动监听，但其点击行为未在隔离桌面中重新验收；批量模式与最小窗口尺寸下的拖动边界也未重新验收。
- `cargo fmt --all -- --check`、`cargo check --workspace --all-targets --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo test --workspace --locked --quiet` 和 `cargo build -p elegant-clipboard-gpui --release --locked` 均通过。测试为 225 项通过、2 项按原配置忽略。发布版 `target/release/elegant-clipboard-gpui.exe` 的 PE 子系统为 Windows GUI（2），隔离数据目录中的发布版 `--smoke-test` 退出码为 0。
