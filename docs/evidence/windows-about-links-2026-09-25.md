# GPUI 关于页链接验证

- 对照主分支 `AboutTab`，GPUI 关于页加入作者、仓库和问题反馈三个入口。URL 固定在编译后的 Windows 模块中；`ShellExecuteW` 返回失败时在关于页显示错误。
- 在 `target/ui-about-20260925` 隔离数据目录用 `--no-monitor` 启动调试版，进入关于页检查中文布局和版本；点击作者入口后，默认浏览器打开 `Y-ASLant (ASLant) - 个人` 页面。验收后关闭该页面。仓库和问题反馈入口的固定 HTTPS 目标由单元测试核对，未逐个在浏览器中点击。
- 隔离检查未启用剪贴板监听或真实剪贴板写入。
- `cargo fmt --all -- --check`、`cargo check --workspace --all-targets --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo test --workspace --locked` 和 `cargo build -p elegant-clipboard-gpui --release --locked` 均通过；225 项测试通过、2 项忽略。发布 EXE 的 PE 子系统为 Windows GUI（2），隔离目录下 `--smoke-test` 退出码为 0。
