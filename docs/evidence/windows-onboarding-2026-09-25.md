# GPUI 首次使用引导验证

- 对照主分支四步引导，在 GPUI 历史窗口内加入采集、搜索、置顶收藏、键盘快捷键说明；按当前 GPUI 的复制与受控粘贴规则调整了文案。
- 在隔离目录 `target/ui-onboarding-20260925` 以 `--no-monitor` 启动调试版，逐项点击“下一步”，第四步点击“开始使用”后进入空历史界面。数据库记录 `gpui_onboarding_completed=true`；重启后直接进入历史界面。
- 分别在 `target/ui-onboarding-esc-20260925` 和 `target/ui-onboarding-skip-20260925` 以 `--no-monitor` 验证 Esc 与“跳过”都结束引导，Esc 路径的数据库也写入完成状态。上述操作未写入用户剪贴板。
- 核心和平台测试覆盖完成状态持久化、服务事件、仅在自定义分组中有历史时自动跳过，以及备份恢复。`cargo fmt --all -- --check`、`cargo check --workspace --all-targets --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo test --workspace --locked` 和 `cargo build -p elegant-clipboard-gpui --release --locked` 均通过；测试结果为 220 通过、2 忽略，发布 EXE 的 PE 子系统为 Windows GUI（2）。窗口布局已在默认 562×761 外框下检查；最小外框布局未单独验收。
