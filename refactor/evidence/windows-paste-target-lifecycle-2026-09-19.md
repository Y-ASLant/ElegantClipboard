# Windows 自动粘贴目标生命周期验收（2026-09-19）

- 热键从其他应用唤出 GPUI 时，记录当时的原窗口；GPUI 失去激活时清除原窗口及尚未完成的待粘贴请求。用户切到别处再直接点回历史窗口时，“粘贴”入口保持禁用，并提示需从目标应用重新唤出；再次从目标应用按热键后，入口与提示一起恢复。后台复制已确认的正常路径会先取走待粘贴请求再隐藏 GPUI，仍可继续执行焦点及剪贴板序列号检查。
- 用 `target/qa/paste-target-activation-20260919` 隔离数据库中的一条合成文本，禁用剪贴板采集。在真实 Windows 11 桌面会话中，Debug 和 Release 程序均从临时测试窗口按 `Ctrl+Shift+V` 唤出，切回测试窗口，再直接点回 GPUI，最后从测试窗口再次唤出。Release 四个状态的裁剪截图保存在 Git 忽略目录 `target/qa/paste-target-initial.png`、`paste-target-hotkey.png`、`paste-target-reactivated.png`、`paste-target-rehotkey.png`：粘贴按钮依次为禁用、可用、禁用、可用；后两次状态栏分别说明目标失效与重新唤出成功。
- 本次没有点击粘贴，也没有读取或改写当前用户的剪贴板内容；测试前后该交互窗口站的剪贴板序列号均为 `10`。GPUI Kit 当前按钮的 UI Automation `IsEnabled` 始终返回 `true`，因此以真实窗口截图、焦点变化与生产代码状态路径验收，不把该属性误当作按钮可用性的证据。
- 仍需在受控交互环境中完成常见目标应用的最终粘贴验收；本记录只覆盖目标绑定、失效和重新获取。
- `cargo test --workspace --locked --quiet`：160 项通过。严格 Clippy、格式检查、Debug/Release 构建及 Release 窗口验收通过，无编译告警。
