# GPUI 图片悬浮预览尺寸验证

- 显示设置中的“大图使用更大浮窗”默认关闭，开启后按历史中已保存的图片宽高调整图片悬浮窗口，窗口尺寸限制在当前显示器内。缺失或异常尺寸沿用默认大小；文字和文件悬浮预览保持原大小。设置写入 SQLite，并沿用 GPUI 设置备份。
- 使用 `target/ui-expanded-image-20260925` 隔离数据目录和 `--no-monitor` 调试版。用标准库生成 1200×800 的合成 PNG，直接插入一条图片历史；没有启用剪贴板采集，也没有写入用户剪贴板。
- 关闭选项时，卡片悬停显示约 460×300 的图片浮窗。开启选项后再次悬停，窗口截图区域为 1226×851（包含窗口边框），内容完整显示；SQLite 中 `gpui_hover_preview.expanded_image` 为 `true`。
- 窗口定位单元测试覆盖默认尺寸、1200×800 图片、超大图片受显示器边界限制和无效尺寸回退。跨显示器、高 DPI、图片文件缺失与其他内容类型的实际浮窗交互仍需验收。
- `cargo fmt --all -- --check`、`cargo check --workspace --all-targets --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo test --workspace --locked` 和 `cargo build -p elegant-clipboard-gpui --release --locked` 均通过。测试为 227 项通过、2 项按原配置忽略。发布 EXE 的 PE 子系统为 Windows GUI（2），隔离数据目录中的发布版 `--smoke-test` 退出码为 0。
