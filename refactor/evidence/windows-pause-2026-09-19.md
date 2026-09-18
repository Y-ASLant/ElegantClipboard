# Windows 暂停采集状态验证

- `gpui_capture_paused` 独立保存在 GPUI 数据库。首次启动默认为继续采集；用户暂停或恢复时后台先写入设置，成功后更新监听状态和发送界面确认。读取到未知值时按暂停处理，避免设置损坏后意外恢复采集。
- 核心偏好测试覆盖默认、暂停、恢复、重启和未知值；Windows 后台测试确认 `Pause(true)` 的确认到达后重开服务仍为暂停。
- 原生窗口使用 `target/qa/pause-smoke` 隔离数据库，预先写入暂停设置后以正常监听模式启动。UI Automation 确认顶部显示“恢复记录”，随后直接结束测试进程；未解除暂停，也未读取或改写当前系统剪贴板内容。
- 全量 104 项测试、fmt、Clippy `-D warnings`、Windows debug/release 构建和 release 隔离窗口冒烟通过。
