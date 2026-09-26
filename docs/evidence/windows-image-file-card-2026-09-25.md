# GPUI 单张图片文件卡片缩略图验证

- 文件卡片从后台历史快照读取路径，在独立线程检查元数据；单张 PNG/JPEG/GIF/WebP/BMP 仅在是普通文件、大小非零且不超过本地 50 MiB 或 UNC 10 MiB 时，将路径交给 GPUI 图片组件。列表绘制不执行文件元数据读取，检查结果按记录 ID 和路径内容缓存，并在记录离开当前列表后清理。
- 使用 `target/ui-expanded-image-20260925` 隔离目录与调试版 `--no-monitor`。其中单张 1200×800 合成 PNG 文件卡片显示蓝色缩略图与文件名；新插入的失效 PNG 路径卡片只显示文件名。主窗口仍可显示其他文本和图片记录，未启用剪贴板采集或执行复制/粘贴。
- 单元测试覆盖有效小文件、缺失路径、目录和超过 50 MiB 的文件。真实 UNC 路径、文件被外部修改后的缓存刷新、其他图片编码格式与受管暂存副本在卡片上的显示仍需验收。
- `cargo fmt --all -- --check`、`cargo check --workspace --all-targets --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo test --workspace --locked` 和 `cargo build -p elegant-clipboard-gpui --release --locked` 全部通过；230 项测试通过、2 项按配置忽略。发布版隔离窗口也显示有效图片的卡片缩略图及失效路径的文字回退。发布 EXE 为 Windows GUI 子系统（2），隔离 `--smoke-test` 退出码为 0。
