# Windows 外观设置增量验证

## 实现

- 新核心 `Preferences` 使用现有 SettingsRepository，独立键 `gpui_theme_mode`，不改变旧应用设置语义或数据库 schema。
- 启动时只初始化一次数据库，历史和设置仓储共享其连接；在创建视图时应用保存的主题。
- Windows worker 串行保存设置并发送专用结果事件，GPUI 收到成功才切换主题；失败保持原主题并显示错误，恢复切换按钮。
- 顶部提供跟随系统、浅色、深色，列表、预览、Kit 标题栏共用主题；仅跟随系统模式响应窗口外观通知。
- 未增加依赖或修改 Cargo.lock。

## 验证

- `cargo check --workspace --locked`、`cargo fmt --all -- --check`、`git diff --check`：通过。
- `cargo test --workspace --locked --quiet`：94 项通过（84 核心、6 Windows 后端、4 应用）。新增测试覆盖缺省值、三种值往返保存、未知值回退、关闭重开后的值，以及后台保存确认。
- `cargo clippy --workspace --all-targets --locked -- -D warnings`、Windows debug 构建：通过。
- 真实 GPUI 窗口采用隔离合成数据与 `--no-monitor`：依次选择深色、关闭重开、浅色、跟随系统。每步检查 SQLite 值和截图，四次退出码均为 0。
- 截图确认深色/浅色覆盖标题栏、外观栏、正文预览及操作按钮；重开后依然为深色，跟随系统选择成功写入 `system`。

未修改 Windows 系统外观，因此未验证真实系统主题变化通知；保存失败 UI、混合 DPI、高对比度和其他平台尚未原生验收。未读写系统剪贴板，未提交或推送 Git。
