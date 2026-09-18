ElegantClipboard GPUI Windows 独立压缩包

1. 解压整个压缩包，双击 elegant-clipboard-gpui.exe。
2. 首次运行会在当前用户的 LocalAppData 中创建独立 GPUI 数据目录；不会自动读取旧版 Tauri 数据。
3. 关闭主窗口后程序留在系统托盘。可用 Ctrl+Shift+V 或托盘图标唤回；从托盘菜单选择“退出 ElegantClipboard”才会结束进程。
4. “窗口设置”中的开机启动默认关闭，按需手动开启。移动 exe 后需重新开启，以更新启动路径。
5. 如需导入旧版数据，请先退出程序，再在命令行使用 --import-db 或 --import-legacy-backup，并为恢复指定空的 --data-dir。请先保留原数据备份。

此包是未经签名的独立 Windows 测试制品，不包含安装器或自动更新。当前仅验证 Windows x64；搜索、分组创建和正文编辑的拼音输入已验收，其他输入法及输入场景、常见应用间的最终粘贴和长时间常驻仍需独立验收。
