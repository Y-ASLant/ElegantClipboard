# GPUI 工程文档

本目录记录 ElegantClipboard GPUI 应用的架构决策、Windows 功能状态和验证证据。

- [Windows GPUI 状态](WINDOWS_MVP.md)：当前实现、依赖、运行方式、验证命令与限制。
- [UI 规范](UI_GUIDELINES.md)：GPUI 界面样式与交互约定。
- [Windows 独立包说明](WINDOWS_PACKAGE_README.txt)：ZIP 制品的运行与数据目录说明。
- [技术决策](decisions/0001-windows-gpui.md)：GPUI 和 gpui-kit 的选择依据。
- `evidence/`：各项功能的历史验收记录。

当前仓库入口只有根 Cargo workspace；历史记录中出现的旧应用名称和路径仅用于说明迁移或兼容性测试，不是可构建入口。
