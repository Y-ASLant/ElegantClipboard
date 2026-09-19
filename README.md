# ElegantClipboard

[English](README_EN.md) | 中文

ElegantClipboard 是使用 Rust、GPUI 和 gpui-kit 构建的 Windows 原生剪贴板管理器。当前分支只有 GPUI 应用，不需要 Node.js、WebView 或 Tauri。

## 当前功能

- 采集和检索文本、URL、HTML/RTF、图片及文件路径
- 收藏、置顶、分组、拖拽排序、批量删除与历史清理
- 文本编辑、富文本/图片/文件预览以及复制、纯文本复制和自动粘贴
- 全局快捷键、系统托盘、开机启动、暂停采集和单实例唤回
- 明暗主题、窗口置顶和窗口尺寸记忆
- GPUI ZIP 备份与恢复，并可导入旧数据库或旧版 ZIP 备份

Windows 实现和已知边界详见 [Windows GPUI 状态](docs/WINDOWS_MVP.md)。

## 工程结构

```text
crates/
  clipboard-core/      数据模型、SQLite、查询、备份与业务规则
  clipboard-platform/  Windows 剪贴板、快捷键、托盘和系统集成
  clipboard-gpui/      GPUI 应用入口、状态与界面
scripts/               打包与验证辅助脚本
docs/                  架构决策、功能状态和验收证据
```

## 从源码运行

要求：

- Windows 10/11 x64
- Rust 1.98 或更高版本，MSVC 工具链
- 打包时需要 PowerShell 7（`pwsh`）

```powershell
cargo run -p elegant-clipboard-gpui --locked
```

也可以使用：

```powershell
make run
```

默认数据位于当前用户 LocalAppData 下的 `ElegantClipboard-GPUI` 应用目录。测试或隔离运行可显式指定目录：

```powershell
cargo run -p elegant-clipboard-gpui --locked -- --no-monitor --data-dir .\target\gpui-check
```

## 构建与验证

```powershell
make check
make test
make build
```

对应的 Release 可执行文件为 `target\release\elegant-clipboard-gpui.exe`。

生成未经签名的 Windows x64 独立 ZIP：

```powershell
make package
```

产物与 SHA-256 文件写入 `target\packages\`。当前不提供安装器、自动更新或 ARM64 制品。

## 版本管理

```powershell
.\scripts\bump-version.ps1 0.2.0
```

脚本更新根 Cargo 工作区版本和锁文件。发布标签必须与 Cargo 版本一致，例如 `v0.2.0`。

## 许可证

[MIT License](LICENSE)
