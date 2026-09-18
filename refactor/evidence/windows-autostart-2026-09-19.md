# Windows GPUI 开机启动验收（2026-09-19）

- “窗口设置”增加显式开机启动开关，默认关闭，以当前用户 Run 键为事实来源；条目名由规范化数据目录生成，不覆盖其他隔离数据目录。开关保存成功后才更新界面，失败显示错误。启动命令包含当前程序路径、`--start-hidden` 和数据目录路径。
- 隔离桌面上，真实鼠标点击“开启”后读取到独立 Run 项，执行该命令可以唤回现有实例；点击“关闭”后条目消失。测试使用 `target/qa/autostart-ui-<pid>`，退出时复核并清理测试项，未触碰默认用户数据目录。展开设置截图在忽略目录 `target/qa/autostart-expanded.png`。
- 以 `--start-hidden --no-monitor` 启动隔离实例，确认进程仍在、没有可见窗口；再次启动同一目录可唤回原进程窗口。托盘不可用时的可见回退由代码路径覆盖，尚未注入托盘失败做系统验收。
- `cargo test --workspace --locked`：152 项通过（124 核心、23 Windows 后端、5 应用）；`cargo fmt --all -- --check`、严格 Clippy、Windows Debug/Release 构建与 Release 隔离窗口冒烟通过，无编译警告。
