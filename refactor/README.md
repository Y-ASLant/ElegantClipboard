# GPUI 跨平台重构

本目录保存 ElegantClipboard 从 Tauri + React 迁移至 GPUI + gpui-kit 的规划与后续验证记录。

- [完整实施计划](PLAN.md)：目标、架构、平台矩阵、分阶段任务、验收、风险与发布切换。
- 当前分支：`gpui`。
- 规划基线：`d486df4`，2026-09-18。
- 当前状态：Windows 基础核心、后端与 GPUI 主窗口已接入，其他平台后续开发；具体测试结果见进度文档。
- [Windows 基础版进度](WINDOWS_MVP.md)：实现范围、依赖版本、检查命令与当前限制。
- [Windows 独立包说明](WINDOWS_PACKAGE_README.txt)：解压运行、数据目录、托盘与导入注意事项。

执行时按 PLAN.md 的阶段门槛推进，在本目录记录技术验证、功能映射和验收证据。勾选任务必须有实际结果；“能编译”不能代替桌面交互验收。
