# macOS 右键菜单透明宿主修复（2026-11-05）

## 问题与修复

右键菜单使用独立的 GPUI PopupMenu 宿主。普通菜单预留 328×480 逻辑点，带子菜单时预留 656×480；未绘制菜单的区域必须透明。用户截图中这块预留区域出现了整窗圆角、边缘和阴影。

当前 GPUI macOS 后端在 `titlebar: None` 时仍创建 `Titled | FullSizeContentView` 窗口，透明背景保留极低 alpha 以维持原生阴影。

- `macos/window.rs` 在菜单显示前将宿主改为无边框，保留 NonactivatingPanel，关闭整窗阴影并设置完全透明的背景。菜单本身继续由 GPUI Kit 绘制。
- `ui/menu_window.rs` 为 macOS 单独处理定位，向 AppKit 传逻辑点，不乘 Retina 缩放；先调整尺寸再定位，避免调整尺寸改变顶边位置。
- macOS 渲染时不再重复调用 GPUI 透明背景设置，以免覆盖已经清空的原生背景。创建窗口时仍通过 WindowOptions 启用透明渲染。
- Windows 和 Linux 继续使用原有定位、背景处理。未调用拖动预览的鼠标穿透配置，菜单仍接收鼠标操作。

## 本地验证（Windows）

- `cargo check --workspace --all-targets --locked --offline`：通过。
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`：通过。
- `cargo test -p elegant-clipboard-gpui --locked --offline`：93 项通过。
- `scripts/check-source-size.ps1`：通过，300 个 Rust 文件均不超过 500 行。
- 两个修改的 Rust 文件单独执行 `rustfmt --edition 2024 --check`：通过。
- 全仓库 `cargo fmt --all -- --check` 未通过：原有 `ui/window_render.rs` 的 render_content 链式调用需要换行；该文件未修改。

以上检查不覆盖 macOS 条件编译分支的编译或 AppKit 的实际显示效果；本次未在 macOS 编译、打包或运行。

## macOS 实机验收待办

1. 分别右键文件、纯文本和富文本记录：只显示菜单表面，周围没有整块玻璃背板、边缘或阴影。
2. 展开“转换后复制”等子菜单，确认显示完整、移动鼠标及点击操作正常。
3. 关闭菜单后重新打开，并切换深浅主题，确认透明区域没有重新出现。
4. 在 Retina、普通外接屏和屏幕边缘打开菜单，确认尺寸和位置正确。
5. 检查键盘导航、Esc 关闭、点击空白关闭和切换应用后的菜单关闭行为。
