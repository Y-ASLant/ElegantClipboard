# 监听内容类型交互验证

- 对照主分支 `AppFilterTab.tsx` 的六种监听类型，在 GPUI 设置窗口增加文本、网址、HTML、RTF、图片、文件按钮，至少保留一种。
- 使用 `--smoke-test --data-dir target/ui-monitor-types-20260925` 初始化隔离数据库，再以 `--no-monitor` 打开窗口。设置窗口下滚后能看到六个按钮；点击“文本”后，SQLite 中 `gpui_monitor_types` 为 `{"text":false,"url":true,"html":true,"rtf":true,"image":true,"files":true}`。
- 核心测试覆盖重启持久化、损坏配置回退及全部关闭被拒；平台测试覆盖文本与网址识别、富格式选择、图片/文件过滤以及在运行中保存设置后对后续采集生效。测试均使用临时数据，未向用户剪贴板写入内容。
