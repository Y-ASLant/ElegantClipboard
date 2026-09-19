# Windows 分组拖拽边缘滚动验收（2026-09-19）

- 分组栏改为跟踪 GPUI 的横向滚动句柄。拖动自定义分组时，指针靠近栏左右 28 px 范围会每 60 ms 推进 18 px，并限制在有效滚动范围；离开边缘、放下或按 Esc 后停止。现有 42 px 布局、滚动条、分组按钮点击和卡片排序不变。
- 在隔离目录 `target/qa/group-drag-edge-final-20260919` 放入 14 个合成分组，禁用剪贴板采集。真实 Windows 桌面从 `Q01` 把手拖至分组栏右缘并停留 5 秒，最远 `Q14` 的 UI Automation 左边界由 2409 px 移至 1474 px，进入可见区域；随后放在 `Q14` 右半区，SQLite 顺序由 `Q01…Q14` 变为 `Q02…Q14,Q01`，`PRAGMA quick_check=ok`。脚本恢复原前台窗口和光标位置，结束自己启动的 GPUI 进程。
- `cargo test --workspace --locked`：160 项通过；严格 Clippy、格式检查、Windows Debug/Release 构建与 Release 隔离窗口冒烟通过。
