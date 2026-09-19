# Windows 富文本纯文本复制验收（2026-09-19）

- 富文本卡片增加“纯文本”按钮，预览同时提供“复制纯文本”和“复制富文本”；现有“复制”与受控粘贴保持原来的富格式路径。文本、网址也可由后台纯文本命令处理；图片和文件拒绝该命令，缺少纯文本表示的富文本明确报错。
- 在独立 Windows 窗口站运行 `cargo run -p clipboard-platform --example isolated_clipboard_smoke --locked`：同一 HTML/RTF 记录先按原样复制并读回三种格式，再按纯文本复制，读回正文且 HTML/RTF 格式均不可用。交互桌面的剪贴板未被选中写入。
- 420 × 520 px 隔离窗口截图确认富文本卡片新增按钮仍在首张卡片内完整可见；富文本预览中的按钮在窄窗口换行且可访问。截图保存在忽略目录 `target/qa/rich-ui/plain-narrow.png` 和 `preview-plain.png`。
- `cargo test --workspace --locked` 共 146 项通过；fmt、严格 Clippy 与 Windows debug/release 构建、release 窗口冒烟通过。
