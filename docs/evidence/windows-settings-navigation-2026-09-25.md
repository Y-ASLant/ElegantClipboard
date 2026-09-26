# GPUI 设置导航验证

- 对照主分支设置页的分类导航，将 GPUI 已实现的设置卡片整理到常规、显示、外观、数据、应用过滤、快捷键和关于七页。主分支的音效、插件、WebDAV 和翻译功能尚未移植，导航没有虚设入口。
- 用 `target/ui-settings-nav-20260925` 隔离数据目录和 `--no-monitor` 启动调试版；跳过首次使用引导后，在 820×500 设置窗口逐页点击并截图检查。常规、显示、外观、数据、应用过滤、快捷键和关于均显示对应内容；点击 English 后，标题和导航立即切换到英文。
- 隔离检查未启用采集、全局快捷键或真实剪贴板写入。尚未在最小设置窗口尺寸和长列表滚动边界进行完整鼠标验收。
- `cargo fmt --all -- --check`、`cargo check --workspace --all-targets --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo test --workspace --locked` 和 `cargo build -p elegant-clipboard-gpui --release --locked` 均通过；测试为 220 项通过、2 项忽略，发布 EXE 的 PE 子系统为 Windows GUI（2）。
