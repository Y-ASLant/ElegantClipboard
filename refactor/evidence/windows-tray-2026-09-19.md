# Windows 托盘基础验证

- Windows 依赖新增 crates.io 稳定版 `tray-icon 0.25.1` 与 `raw-window-handle 0.6.2`；根 Cargo.lock 锁定解析版本。托盘图标在 GPUI 线程创建并持有，由 Windows 消息循环处理图标和菜单事件。
- 保留唯一主窗口和后台 Service：关闭时通过 GPUI `on_window_should_close` 隐藏窗口；恢复时显示并激活原窗口，搜索和历史状态不重建。显式退出令 GPUI 退出，释放剪贴板监听、worker 和实例锁。
- 托盘初始化异常会在底部状态栏显示错误，窗口关闭时回退到正常退出，避免不可见且无法恢复的进程。
- 真实 Windows 验证：隔离目录 `target/qa/tray-smoke` 与 `--no-monitor` 启动，关闭窗口后 `IsWindowVisible=false` 且进程继续运行；展开系统托盘，图标可访问名称 `ElegantClipboard · 剪贴板历史`；左键恢复主窗口；右键菜单含“打开剪贴板历史”“退出 ElegantClipboard”，点击退出后进程退出码 0。
- `cargo test --workspace --locked --quiet`：97 项通过；fmt、Clippy `-D warnings`、Windows debug 构建通过。图标使用可复现的 RGBA 绘制，未引入图片资源文件。
- 尚未做长时间常驻、Windows Explorer 重启、安装包图标保留位置和真实采集；这几项在日常使用验收阶段继续检查。
