# Windows 后台错误与界面操作状态隔离（2026-09-19）

剪贴板监听、后台采集和实例唤出服务的错误现在通过 `BackgroundError` 上报。界面仍在状态栏显示错误，但不会因这些独立事件清空排序、备份、暂停、分组编辑或粘贴等待确认状态。用户命令本身失败时仍使用原有操作错误路径。

回归测试在未监听真实剪贴板的 worker 中先提交超限文本采集，再提交外观设置保存：先收到后台错误，后收到成功的设置确认。隔离窗口站的实际剪贴板往返程序写入超过 1 MiB 的文本，确认监听器发出后台错误；文本、富文本、图片、文件的正常往返仍通过。`cargo test --workspace --locked --quiet` 共 157 项通过，严格 Clippy、fmt 和 release 构建通过；release 程序以 `--smoke-test --no-monitor --data-dir .\target\qa\background-errors-smoke-20260919` 运行并以退出码 0 结束。
