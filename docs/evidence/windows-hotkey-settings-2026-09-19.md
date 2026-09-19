# Windows 唤出快捷键设置验证

- 顶部“唤出”提供 `Ctrl+Shift+V`、`Alt+C`、`Ctrl+Alt+V` 与“关闭”四个选项，使用与外观设置相同的按钮尺寸、间距和选中态。最小 420×520 窗口截图检查无重叠；截图保存在被 Git 忽略的 `target/qa/hotkey-layout.png`。
- 设置存于 `gpui_hotkey`，不会悄悄采用旧版 `global_shortcut`（旧版默认值为 `Alt+C`，即使用户未主动设置也存在）。切换时先注册目标组合键再请求后台保存；冲突或保存失败会尝试恢复原组合键。关闭时注销全局快捷键，托盘唤出仍可用。
- 真实 Windows UI Automation 验收使用 `--no-monitor` 与 `target/qa/hotkey-choice-3`：测试进程占用 `Alt+C` 时切换被拒绝，原 `Ctrl+Shift+V` 仍由应用持有且新设置未保存；释放后切换 `Alt+C` 成功；关闭快捷键后 `Alt+C` 可被测试进程注册，重启仍保持关闭；重新选择 `Alt+C` 后再重启，应用重新持有 `Alt+C`。所有测试实例结束后清理。
- 核心偏好与后台确认测试覆盖默认值、未知值回退、与旧版设置隔离、持久化和重启读取。全量 103 项测试、fmt、Clippy `-D warnings`、Windows debug/release 构建及隔离目录 release 窗口冒烟通过。
