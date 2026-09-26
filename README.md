# ElegantClipboard

[English](README_EN.md) | 中文

ElegantClipboard 是使用 Rust、GPUI 和 gpui-kit 构建的 Windows 原生剪贴板管理器。当前分支只有 GPUI 应用，不需要 Node.js、WebView 或 Tauri。

## 当前功能

- 采集和检索文本、URL、HTML/RTF、图片及文件路径
- 新数据目录首次打开时提供四步中文/英文引导，介绍采集、搜索、收藏与快捷键，可跳过或按 Esc 结束
- 设置窗口按常规、显示、外观、数据、应用过滤、音效、快捷键和关于分类切换
- 常规设置可选择工具栏清理当前分组历史时跳过确认，默认仍要求确认
- 可分别配置复制与受控粘贴音效的开关、立即或成功后播放，并在设置页试听
- 关于页提供作者、项目仓库和问题反馈入口，可在默认浏览器中打开
- 设置窗口可分别启停文本、网址、HTML、RTF、图片和文件的历史采集，至少保留一种类型
- 来源应用过滤支持黑名单、白名单、手工规则及运行中应用选择，规则可按应用名、进程名或路径匹配并支持 `*`、`?`
- 收藏、置顶、分组、通过卡片两侧区域拖拽排序、批量删除与历史清理；拖动区域提示可在显示设置中隐藏
- 文本编辑、富文本/图片/文件预览、单张图片文件卡片缩略图与失效来源提示，以及复制、纯文本复制和自动粘贴
- 可配置的独立悬停预览：图片默认开启，文本与文件可分别开启；支持延时、位置、图片缩放步进、缩放按钮和百分比重置，以及按图片尺寸扩大浮窗
- 全局快捷键显隐切换、系统托盘、开机启动、暂停采集和单实例唤回
- 可在设置中启用或关闭快速粘贴，录制或输入单个槽位的组合键，并分别对 10 个普通槽位和 10 个收藏槽位全部停用或恢复默认；默认 Alt+1～Alt+0 粘贴当前分组的前十条记录，Ctrl+Alt+1～3 粘贴前三条收藏记录，数字键支持数字小键盘；自动粘贴按键可选 Ctrl+V 或 Shift+Insert
- 简体中文/English 界面（默认简体中文）、明暗主题、窗口置顶、点击外部隐藏、唤出位置，以及粘贴后窗口显隐和排序设置
- 可在设置页拖动调整顺序或用上移/下移按钮调整顺序，并配置显隐的主窗口工具栏（设置入口始终保留）
- 可隐藏分类筛选栏，并可选择紧凑、标准或宽松的历史卡片密度
- 历史列表较长时，固定信息栏提供返回顶部入口
- 卡片预览行数、绝对/相对时间、字符数、大小和来源应用信息可在设置窗口配置；来源应用可按名称、图标或两者显示，图标缓存为 PNG
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
