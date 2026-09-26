# GPUI 图片悬停预览缩放控件验证

- 图片和单张图片文件的悬停浮窗显示缩小、当前百分比重置、放大按钮，缩放限制在 50% 至 400%。按钮采用设置中的图片缩放步进；Ctrl+滚轮沿用相同边界。
- 使用 `target/ui-expanded-image-20260925` 隔离数据目录启动调试版，附加 `--no-monitor`，不采集或写入真实剪贴板。
- 在单张图片文件的悬停浮窗中，点击放大、百分比重置、缩小，截图中的百分比依次为 100%、110%、100%、90%，图片尺寸随之变化。普通图片的扩大悬停浮窗显示同一组按钮。
- 单元测试覆盖普通步进和 50%／400% 边界。普通图片按钮点击、Ctrl+滚轮和跨显示器定位尚未进行完整桌面交互验收。
- `cargo fmt --all -- --check`、`cargo check --workspace --all-targets --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo test --workspace --locked` 和 `cargo build -p elegant-clipboard-gpui --release --locked` 均通过。工作区 231 项测试通过，2 项按现有配置跳过。发布版 EXE 的 PE 子系统为 Windows GUI（2），隔离目录中的 `--smoke-test` 退出码为 0。
