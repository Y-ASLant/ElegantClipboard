ElegantClipboard GPUI Windows 独立压缩包

1. 解压整个压缩包，双击 elegant-clipboard-gpui.exe。
2. 首次运行会在当前用户的 LocalAppData 中创建独立 GPUI 数据目录；不会自动读取旧版 Tauri 数据。
3. 关闭主窗口后程序留在系统托盘。可用 Ctrl+Shift+V 或托盘图标唤回；自动粘贴仅在从目标应用直接唤出、期间没有切走历史窗口时可用。从托盘菜单选择“退出 ElegantClipboard”才会结束进程。
4. 设置中的开机启动默认关闭，按需手动开启。移动 exe 后需重新开启，以更新启动路径。
5. 可在“设置 → 数据”导出备份，或将旧数据库、旧版 ZIP、GPUI 备份恢复到独立空目录；恢复报告提供“打开恢复数据”操作。也可使用命令行 --import-db 或 --import-legacy-backup，并为恢复指定空的 --data-dir。请先保留原数据备份。
6. 文件历史仅保存原始路径，不自动复制普通文件。新版启动时清理当前数据目录内旧 staged 副本及引用，保留原始文件路径与截图；旧备份导入不会重新保存副本。默认数据目录为 %LOCALAPPDATA%\ElegantClipboard。升级时自动迁移旧 %LOCALAPPDATA%\ASLant\ElegantClipboard-GPUI\data，修正附件和开机启动路径，成功后清理旧目录；新旧两处已有数据时保留两边并提示，不覆盖历史。需要使用其他目录时可传入 --data-dir；旧默认路径自动重定向，新自定义路径不迁移。--no-monitor 可关闭该实例的剪贴板监听。

此包是未经签名的独立 Windows 测试制品，不包含安装器或自动更新。当前仅验证 Windows x64；搜索、分组创建和正文编辑的拼音输入，以及隔离窗口站中的合并内容和纯文本路径写回已验收，其他输入法及输入场景、常见应用间的最终粘贴和长时间常驻仍需独立验收。
