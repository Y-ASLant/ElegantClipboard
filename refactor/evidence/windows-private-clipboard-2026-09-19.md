# Windows 隔离剪贴板往返验证（2026-09-19）

## 目的与隔离

用系统剪贴板实际执行监听、持久化、复制和读回，不替换当前交互桌面的剪贴板。测试程序先用 `CreateWindowStationW` 取得非交互式窗口站，核对其名称不是 `WinSta0`，再关联进程并创建测试桌面；若核对失败，则在写入前退出。微软文档说明窗口站包含各自的剪贴板，`SetProcessWindowStation` 控制进程访问的窗口站：[窗口站](https://learn.microsoft.com/en-us/windows/win32/winstation/window-stations)、[创建窗口站](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-createwindowstationw)、[切换进程窗口站](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setprocesswindowstation)。

测试使用临时数据库与仓库内图片样本。系统可能在同一登录会话复用未命名非交互式窗口站，因此程序不要求其剪贴板初始为空；它只保证不会把内容写入交互窗口站。

## 执行

```powershell
cargo run -p clipboard-platform --example isolated_clipboard_smoke --locked
```

本机输出：

```text
window station: Service-0x0-acfbc94$
text capture/copy ok
rich capture/copy ok
image capture/copy ok
files capture/copy ok
isolated clipboard roundtrip passed
```

同一个 PowerShell 进程调用 `GetClipboardSequenceNumber`，在测试程序运行前后读到的交互桌面序列号均为 `10`，测试退出码为 `0`。该检查只说明本次测试没有触发交互桌面的剪贴板更新，不能代替跨应用粘贴验收。

程序逐项验证文本、同时含纯文本/HTML/RTF 的富文本、128×128 图片和现存文件路径：外部写入隔离剪贴板，后台监听生成对应历史项，服务写回剪贴板，最后按原始格式读回。富文本还检查后台预览文本。服务、监听和历史均使用正式生产代码路径。

## 范围限制

非交互式窗口站证明了 Windows 剪贴板 API 与后台服务的往返能力；浏览器、Office、资源管理器以及窗口焦点/快捷键等交互桌面行为仍需单独验收。测试会覆盖该非交互式窗口站已有的剪贴板内容，请勿将其用于其他任务的数据保存。
