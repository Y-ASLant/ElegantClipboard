# GPUI 单张图片文件预览验证

- 使用已有的 `target/ui-expanded-image-20260925` 隔离数据目录和发布版 `--no-monitor`，该目录含一条指向 1200×800 合成 PNG 的文件历史。未启用剪贴板采集，也未执行复制或粘贴。
- 鼠标停在文件卡片后，独立“文件预览”浮窗显示 PNG 图像，并在底部保留路径。点击该卡片的“查看”后，“文件详情”页显示图像、文件名、原始路径和 4.5 KiB 文件大小。
- 候选判断要求仅一项、文件存在、元数据可读、有非零且不超限的大小、受支持的图片扩展名；本地上限 50 MiB，UNC 上限 10 MiB。单元测试覆盖多项、未知/超限大小、非图片扩展名、失效路径、目录和元数据错误。图片解码失败时 GPUI 显示空白回退，仍可查看文件信息。
- `cargo fmt --all -- --check`、`cargo check --workspace --all-targets --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo test --workspace --locked`、`cargo build -p elegant-clipboard-gpui --release --locked` 均通过。测试为 229 项通过、2 项按原配置忽略。发布 EXE 的 PE 子系统为 Windows GUI（2），隔离目录 `--smoke-test` 退出码为 0。历史卡片缩略图、其他图片格式和真实文件来源仍未验收。
