# 卡片排序与窗口外拖放（2026-10-15）

基准：`gpui@0de1ee8` 加本次工作区补丁。

## 实现

- 整卡正文、图片和信息栏可开始拖动；操作按钮保留独立点击，批量模式禁用拖动。
- 预览复用卡片正文和信息栏，使用 prepaint 实测尺寸，只有背景透明度为 80%；保留文字、图片及来源图标，不增加阴影和操作按钮。源条目为空白同尺寸占位，去掉插入线。
- 其他卡片以 260ms ease-in-out 让位，打断后从已显示位置继续。松手保存期间保持预览顺序，收藏顺序独立持久化。交互层不跨置顶分区改变置顶状态；核心既有原子跨分区 API 保留。
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

## 同日 simplify 优化复验

基准为 `dd9a8fc`，审阅范围为相对上游的两个本地提交。四个并行角度（复用、简化、效率、实现层次）去重后应用五项改善：

- `HistoryCardSnapshot` 只保存卡片类型和来源显示字段，不再克隆整条数据库记录；搜索结果的完整正文不随每次渲染重复保留在预览快照中。
- 图片转换使用 `into_rgb8` 消费原图，BMP 编码缓冲区原地移除文件头，减少整幅像素缓冲区的重复分配；仍保留 24 位 DIB 表示。
- 删除没有消费者的 `on_move` 回调，保留 COM 边界保护及原生预览移动。
- 移除恒为真的 `DropTarget.allowed`，保留有效手势与同置顶分区校验。
- 批量删除、当前分组清理和全部清理共用私有收尾函数，保留先完成回执、再报告刷新失败的顺序。

`just check`、`just test` 和常规 `just build` 均通过，137 个 Rust 文件符合行数限制，270 项测试通过、2 项原有测试忽略，无编译或 Clippy 警告。产物为 `target/release/elegant-clipboard-gpui.exe`，38,355,968 字节，SHA-256 `FB4B73998A2C0E54DDD2A58FB9C6B98D43E790F0A6A71C7AA233D78D866BA7BF`。未新增实现绑定测试；未测量帧率或内存峰值，不将减少分配宣称为实测性能增幅。未采纳扩展共享文件接口等超出局部范围的建议，也未改变外拖载荷的准备时机。

本轮原生检查使用新的 `target/qa/simplify-native/data` 合成数据，关闭监听及热键。`held-cancel-holding-crop.png` 展示完整两行文本、来源信息、透明背景和邻卡让位；`after-cancel-restored-screen-crop.png` 确认取消后恢复原列表。取消手势剪贴板序列号 311→311。首次图片外拖的预览正常（`image-drop-holding-crop.png`），但接收测试窗口已退出，没有生成接收文件，不能据此宣称接收成功；该手势序列号为 317→317。手势之间的序列值不同，因此仅确认各受控手势期间未改变剪贴板，不声称整段桌面会话序列恒定。恢复测试接收窗口后仅重试一次，序列号 322→322；接收窗口可见，但源窗口和预览已隐藏，没有生成接收文件。应用仍存活且 stderr 为空，本轮图片外拖的真实接收未验证完成，未继续扩大排障；DIB 像素及 COM 格式／所有权自动测试通过，上文此前版本的完整接收证据仍单独保留。取消前后独立读取 SQLite 顺序均为 `[201,203,202]`；该顺序在本轮驱动手势前已存在，不将种子到基线的变化归因于测试。验收结束后按启动时间确认并结束本轮 EC 进程，两个测试接收窗口已退出；三个临时脚本已删除，鼠标已释放。

## 拖动丢失与卡顿修复

基准 `2986ab1`。应用 `CardDragSession` 接管起拖后的指针采样、落点与松手，清除框架临时 drag，避免 GPUI 的文件拖放退出／MouseUp 与自建 OLE 同时决定会话生命周期。源内释放单独返回 `ReleasedInSource`，由应用更新落点并提交内部排序；按住回源保留同一会话。鼠标释放后的 click 抑制只影响鼠标入口，不阻止键盘激活或快捷键隐藏。旧会话仅能清理自己的预览，同代次历史快照拖动期间合并延后。

预览窗口隐藏复用，位置跟随使用主窗帧信号而不是固定 16ms 周期；仅坐标变化才移动，100ms watchdog 保证最小化无帧时仍能收尾。所有原生窗口变更仍位于 GPUI 借用外。让位使用 prepaint offset，布局尺寸不变；命中与反向续接共用实际绘制进度，偏移仅保存受影响行。左键按下就预取轻量格式，DIB 在一个专用线程上渲染，最多一个运行和一个排队任务，弱引用跳过已放弃的排队载荷；GetData 缓存结果但每次提供独立 HGLOBAL。

最终 `just check`、`just test`、`just build` 通过：140 个 Rust 文件均不超过 500 行，275 项通过（core 154、platform 84、GPUI 37），2 项原有忽略测试，无编译或 Clippy 警告。Release 为 `target/release/elegant-clipboard-gpui.exe`，38,468,608 字节，SHA-256 `333095B35C464EBF74422BE30C5FE356C49D3E369EAC798D9FC3DAEA108DF536`。

Debug 原生验证使用 `target/qa/session-native` 的新合成数据库、禁用监听与热键：

- 内部排序从第二位移动到第三位，独立 SQLite 读取确认持久化；Unicode 外拖由 WinForms 接收窗口真实取得精确文本。
- 小图 320×160 与大图 2000×1000 的 DIB 均为 24bit，逐像素验证预期两色，HDROP 接收的 PNG 与源文件逐字节一致。
- 快速出窗松手成功产生真实文本接收；回源立即松手成功提交内部排序；Escape 没有增加接收次数。
- 连续 20 次往返内部拖动，每次主窗均可见、HWND 集合不变，预览 HWND 始终复用；首条和批次终态由 SQLite 验证，未声称每一步都独立读取持久化。
- 总计 27 次手势，剪贴板序列号均为 378，stderr 为空。截图 `internal-reorder-holding.png`、`small-image-out-holding.png`、`large-image-out-holding.png`、`reuse-01/10/20-holding.png` 和真实接收文件仅保留本地。

停止 Debug 测试进程的委托被自动审批拒绝，理由是当前可见上下文缺少创建记录；未绕过，Debug 与接收窗口保留。最终 Release 使用另一个新隔离目录 `target/qa/session-release-final` 复验通过：内部排序持久化为 `[301,303,302,304,305,306]`（301 仍置顶）；图片通过跨屏拖放被真实接收，`session-native/drops/drop-05` 的 DIB 为 320×160/24bit、两色像素与 PNG 字节均正确；Escape 取消后主窗仍可见。三次手势均保持主窗可见、同一预览 HWND 复用、剪贴板 378→378，stderr 为空。对应 `release-internal-holding.png`、`release-image-out-holding.png`、`release-escape-holding.png` 已观察，鼠标已释放，三个临时验收脚本已删除。遵守停止操作的审批拒绝，Debug PID 21316、接收窗口 PID 26556 及新 Release PID 20460 保留运行。

## 预览裁切纠正

用户截图指出右边及底边被截断。复查原始 `session-native/unicode-out-holding.png` 和 `session-release-final/release-image-out-holding.png`，同样存在右／下边框与圆角缺失；此前只核对内容、投递和窗口可见性，遗漏四边检查。撤回上文中暗示“四边完整”的视觉结论，载荷字节、排序持久化和窗口复用结果仍独立有效。

根因是 `resize_drag_preview` 把卡片要求的客户区尺寸直接传给要求外框尺寸的 `SetWindowPos`。GPUI PopUp 仍可能带非客户区差额，实际 viewport 小于固定卡片布局，所以右下被裁。另修正复用顺序：先移动至当前显示器，等待同步 DPI 更新，再用浮窗实际 DPI 向上取整客户区尺寸；不再先按源屏 DPI 在旧屏 resize 后二次缩放。原生 resize 按实测 `GetWindowRect−GetClientRect` 补足外框，未添加任意像素补丁。

本次 `just check`、275 项测试和 Release 构建通过。裁切修复产物为 38,470,144 字节，SHA-256 `71750D79B8989D74329D65D7EFFAC1A90472D46C7EC451179315BA7BB0D7820E`。新隔离原生检查 `target/qa/edges-native/text-edge-held-1-window-3016460.png` 已逐边观察：四边和四角完整。实测 popup DPI=96，外框 700×184，客户区 684×176，非客户区差额 16×8；源窗口客户区同为 684 宽，卡片两侧与上下 padding 均保留。该手势剪贴板 388→388，主窗口保持可见。后续同一 popup 的图片客户区为 684×124、缩窄主窗后为 544×176，外框始终补足 16×8；图片左右跨屏以及长文本和缩窄宽度截图均逐边确认完整。两屏实际 DPI 均为 96，本轮未在不同 DPI 显示器上实测。最终 Release 的 `target/qa/edges-release/release-image-held-1-window-3016406.png` 与 `release-width-held-1-window-3016406.png` 均确认四边四角完整，同 popup 从 684×124 客户区复用到 544×176，主窗保持可见、剪贴板 388→388、stderr 为空。临时脚本已删除；停止进程委托被自动审批拒绝（缺少可见创建记录），未绕过，保留本轮 Debug PID 3028 和 Release PID 27752。

## 限制

- 本轮未测量高刷新率／不同 DPI 下的帧耗时分布，不据截图宣称已验证性能改善。最小化 watchdog、同代次快照合并和减少动态效果的所有原生组合尚未逐项实机覆盖。
- 同步 COM 目标很早请求 DIB 时仍需等待专用线程；历史 worker 已有长任务、网络文件元信息也可能延迟轻量准备。准备未就绪就出窗释放会提示重试，不能保证任意零等待手势都能投递。
- DIB 解码损坏或像素越界在 GetData 返回格式错误，已准备的 HDROP 仍可供文件消费者使用；不把坏图片伪装成有效 DIB。
- 目标程序决定接受何种格式，不提供任意 Windows 原生格式或虚拟文件 IStream。
- 图片 DIB 兼容表示为 RGB，原图片文件保留原有透明通道。
- HDROP 引用原文件或恢复的受管路径；拖动期间文件被删除可能失效，不提供临时副本保活。
- 首轮为保留预先运行的 EC 实例，使用独立 `target/qa/verified-build` 输出目录；同日 simplify 复验前未检测到运行中的 EC，常规 `target/release` 构建成功。
