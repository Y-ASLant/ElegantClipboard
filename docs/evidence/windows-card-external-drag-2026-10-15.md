# 卡片排序与窗口外拖放（2026-10-15）

基准：`gpui@0de1ee8` 加本次工作区补丁。QcClip 仅作为交互与视觉参考，未修改其代码，未复制其实现。

## 实现

- 整卡正文、图片和信息栏可开始拖动；操作按钮保留独立点击，批量模式禁用拖动。
- 预览复用卡片正文和信息栏，使用 prepaint 实测尺寸，只有背景透明度为 80%；保留文字、图片及来源图标，不增加阴影和操作按钮。源条目为空白同尺寸占位，去掉插入线。
- 其他卡片以 260ms ease-in-out 让位，打断后从已显示位置继续。松手保存期间保持预览顺序，收藏顺序独立持久化。与 QcClip 一致，交互层不跨置顶分区改变置顶状态；核心既有原子跨分区 API 保留。
- 窗内外使用同一透明、不激活、鼠标穿透的预览窗口，保持物理像素抓取点。后台按 ID 读取完整记录并准备载荷，响应带手势 token，旧响应不会重新启动拖动。
- Windows 平台新增 OLE IDataObject、IDropSource 和 IEnumFORMATETC；复制效果，不写系统剪贴板。Unicode、HTML、RTF、文件 HDROP 和图片 DIB/文件路径均使用已保存内容。图片解码有大小及像素限制。
- OLE 在 UI 线程、GPUI 借用之外运行，防止 Windows 消息泵重入借用冲突；返回源窗口可继续内部拖动，Escape 取消。

## 自动验证

- `just check` 通过：137 个 Rust 文件不超过 500 物理行；fmt、workspace/all-targets check、Clippy `-D warnings` 均通过。
- `just test` 通过：core 154、platform 82、GPUI 34，共 270 项通过、0 失败；2 项原有系统图标相关测试忽略。
- `cargo build -p elegant-clipboard-gpui --release --locked --target-dir target/qa/verified-build` 通过，最终无编译警告或错误。产物位于 `target/qa/verified-build/release/elegant-clipboard-gpui.exe`，38,369,280 字节，SHA-256 `3BEF798FA090B17BD59C786A2270CE35DA6E236992D3CE40960622875F02B21C`。
- 新增回归覆盖真实 COM 格式枚举和 GetData、HGLOBAL 独立所有权、无效格式请求、HTML UTF-8 字节偏移与 RTF 字节、DragQueryFileW 多路径、DIB 解码、缺失及超限图片、外部取消/返回状态、后台完整正文和 token 回执、快速反向让位动画、不同高度卡片重复悬停不反复换位、离开后反向拖动和置顶分区边界。
- 删除已不再使用的旧中点命中函数及仅针对该算法的测试，保留可变高度布局、持久化、回滚等既有行为测试。

## 原生验收状态

实际窗口验收发现并修复了两项视觉路径缺陷。首次拖动在副显示器触发同步窗口移动回调，导致 GPUI `RefCell already borrowed`；Debug 堆栈确认 `SetWindowPos -> handle_move_msg -> set_display -> AsyncApp::update`。已将原生样式和移动移到 GPUI 借用范围之外，并指定源显示器。另补齐分层窗口全局 alpha=255 初始化，卡片自身背景仍为 80%，预览字体与主窗一致。修复后 Debug 首次内部拖动无崩溃，Alpha 从第 4 条保存为第 2 条，剪贴板序列号仍为 306。邻行悬停让位无效另定位为 VirtualList 给行根元素固定 `prepaint_at` 原点，已改成固定行根下的可移动子卡片；补齐不同高度卡片重复悬停去抖及快速松手的最新目标同步。最终 Release 的 `drag-run-release-visual/dark-mixed-held-holding-1.png` 已观察到：鼠标尚未松开，图片已经上移让位，五行文本源位置为空白，浮动卡片保留五行正文与信息栏，无操作按钮和插入线；松手后的顺序已持久化。

Debug 实际外拖已验证 Unicode、HTML、RTF、文件和图片：接收窗口准确取得完整文本、CF_HTML 片段、RTF 字节以及可复制的文件路径；图片提供 320×160、24bpp DIB，接收的原 PNG 与合成文件逐字节一致，系统剪贴板序列号保持 306。RTF 首次测试样本错误使用原文存储，改为 EC 的 `b64:` 格式后通过，未为错误样本改变应用行为。`fixed-drops/drop-01.json`、`drop-02.json`、`drop-04.json` 至 `drop-06.json` 记录接收格式。

外部 Escape 取消后数据库顺序不变且未增加 drop；按住回源窗口后继续排序，Alpha 从第 2 条移动到第 5 条并持久化，没有向外部接收窗口提交。`unicode-out-holding-2.png`、`image-out-holding-2.png`、`leave-return-holding-2.png` 展示真实窗口；外部预览保留完整文字/图片/信息栏，无悬停操作按钮。

验收使用独立数据库、合成文本/富文本/文件/图片和 WinForms 接收窗口；临时驱动检查每次鼠标按下所指窗口的 PID，检查进程存活及剪贴板序列号。两轮所有手势结果均为剪贴板序列号 306→306；鼠标已释放，未操作既有用户 EC 实例。第一轮截图和日志位于 `target/qa/drag-run-20261015-073956`。六个临时脚本（`drag_seed.py`、`drag_visual_seed.py`、`drag_launch.ps1`、`drag_receiver.ps1`、`drag_driver.ps1`、`drag_cleanup.ps1`）已在验收后删除，合成数据、截图、接收结果和日志保留在忽略目录，未发布到外部服务。

首次测试启动含 `ExecutionPolicy Bypass` 被自动审批拒绝，随后已移除；完整读取脚本和搜索确认没有该参数后，普通 `powershell.exe -NoProfile -File .../drag_launch.ps1` 仍被拒绝，理由为重复此前被拒绝动作并要求确认误报。用户随后明确允许启动隔离窗口，普通命令执行成功，未修改执行策略或权限配置。验收后停止测试 PID 20620、24504 的操作被自动审批拒绝，后者理由为压缩后的可见上下文缺少原始创建调用；未重试或绕过。两个测试 EC 实例及接收窗口 PID 29672 保留运行。

最终 Release 原生复验通过，证据位于 `target/qa/drag-run-release-visual`：

- `dark-mixed-held-holding-1.png`：深色主题，五行文本与不同高度图片之间实时让位；松手后通过独立 SQLite 读取确认新顺序。
- `rapid-reverse-holding-1.png` 至 `rapid-reverse-holding-3.png`、`rapid-reverse-after.png`：快速往返手势和中间帧正常，Escape 恢复原顺序，无崩溃。
- `pin-boundary-after.png`：拖过置顶卡片后仍保持置顶项 108 在首位，普通项只在普通分区内改变顺序。
- `release-external-holding-2.png`：从右屏跨到左屏后保持完整五行卡片预览，接收窗口取得精确文本（第一轮目录下 `fixed-drops/drop-07.json`）。

早期 `release-baseline`、`release-visible`、`release-top`、`release-main` 截图被旧浅色测试窗口遮挡，不作为最终 Release 的验收证据。最终实例移到右屏后已确认实际深色九条记录及 PID。此前左屏到右屏再 Escape 亦通过（第一轮的 `cross-display-holding-2.png`）。没有穷举全部 DPI/缩放组合，没有逐帧测量 260ms 动画时长；持久化由独立 SQLite 读取确认，本轮未完成关闭后重启同数据库的复验。

## 限制

- 目标程序决定接受何种格式，不提供任意 Windows 原生格式或虚拟文件 IStream。
- 图片 DIB 兼容表示为 RGB，原图片文件保留原有透明通道。
- HDROP 引用原文件或恢复的受管路径；拖动期间文件被删除可能失效，不提供临时副本保活。
- 保留预先运行的 EC 实例，不覆盖被占用的 `target/release`，Release 使用独立 `target/qa/verified-build` 输出目录。
