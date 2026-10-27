# 跨平台正确性修复验证（2026-10-27）

本记录对应 `57e3e8e` 之后对跨平台能力、剪贴板格式、请求状态和采集生命周期的修复，纳入 `v2.0.0-beta.7` 测试版本。编译和测试结果与原生目标应用行为分别记录，不将输入事件发送成功视为目标已消费剪贴板。

## 修复范围

| 范围 | 修复内容 | 验证方式 |
|---|---|---|
| 序列粘贴 | 内部首条请求保留队列、固定目标与请求身份；等待输入投递期间保持忙碌；正常回贴隐藏和用户打断分开 | 队列状态、过期回执、目标变化回归；原生窗口启动观察 |
| 快捷键 | 候选配置注册、整组释放、关闭快速槽位保留推进键；注册/保存失败回滚；主键、槽位、推进键三向校验 | 保存/回滚状态与服务持久化回归 |
| Linux X11 | 事件消费者各自持有连接；停止后断连释放抓键、选择所有权和拖放窗口；查询连接单独缓存 | 显式私有 Xvfb 回归，已加入 CI |
| macOS | 保留 AX 窗口与应用对象身份；同应用不同窗口也可区分；检测失效即取消，不将 PID 视为窗口 | 平台库交叉编译；对应系统权限和实际窗口操作仍需实机 |
| 平台能力 | 自动粘贴、主热键、来源查询和定位分别判断；Wayland 说明按会话和语言显示 | 调用链核查及原生窗口观察 |
| 剪贴板恢复 | 快照归属到请求与写后序列；取消/新复制/关闭设置清理；恢复前拒绝覆盖外部新内容 | 纯状态回归和 Windows 独立窗口站真实往返 |
| 格式 | 安全 CF_HTML 范围解码、实际像素尺寸构造 DIB、Original-only 原生格式、单图 HTML 保存位图、RTF 解码、文件 URI 转义、Insert 扩展位 | 字节/边界回归和隔离剪贴板往返；Linux URI 与键事件结果需对应平台检查 |
| 采集 | 捕获消息保留原序列、延迟载荷保留配额、立即入库也触发刷新、重读期间核对序列 | 序列过滤、预算持有/释放、服务快照回归 |

## 已完成的验证

- `cargo test -p clipboard-core --locked`：201 项通过，包含 HTML 图片语义识别；未操作系统剪贴板。
- 最终 `cargo test --workspace --locked`：376 项通过（core 201、platform 118、GPUI 57）、2 项原有忽略；包含 RTF 二进制解码、DIB 尺寸/长度边界、恢复请求归属、序列忙碌/目标/过期回执、快捷键三向冲突与实际数据库写失败后重试。
- 最终源码大小、`cargo fmt --all -- --check`、`cargo check --workspace --all-targets --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings` 均通过；所有 241 个 Rust 源码/测试/示例文件均不超过 500 行。
- `cargo run -p clipboard-platform --example isolated_clipboard_smoke --locked`：退出码 0。
  - 先创建独立窗口站及桌面，拒绝 `WinSta0`；所有剪贴板样本和写回都在该窗口站内。
  - 新增场景验证损坏 CF_HTML 偏移后监听继续运行、仅图片 HTML 保存像素、外部缩小附件后 DIB 使用实际尺寸。
  - 验证纯文本和大写转换不再附带原记录额外原生格式。
  - 验证旧请求取消后不可由纯文本粘贴恢复；匹配请求与序列可恢复；外部新复制后拒绝恢复。
- `cargo zigbuild -p clipboard-platform --lib --locked --target aarch64-apple-darwin --target-dir target/cross-macos`：退出码 0。编译了真实 AX/objc2 实现，未原生运行或链接 GUI。
- `cargo zigbuild -p clipboard-platform --lib --locked --target x86_64-unknown-linux-gnu --target-dir target/cross-linux`：退出码 0。Linux X11/Unix 生产平台库交叉编译通过，不代表 Linux 桌面或 GUI 链接运行。

## Windows 原生窗口观察

通过临时 PowerShell 驱动启动 Release 的 `--smoke-test --data-dir <独立目录>`，实际观察捕获的主窗口和数据设置页。主窗口显示搜索、分类、分组与空列表提示；数据页正常渲染采集类型、瞬时过滤、去重与恢复设置。程序退出码 0；交互桌面的剪贴板序列号运行前后均为 `2805`。

截图保存在 Git 忽略目录 `target/qa/repair-window-0.png` 与 `target/qa/repair-window-1.png`。临时驱动仅做窗口枚举/渲染捕获和剪贴板序列读取，验证后删除。本次窗口观察不包含实际跨应用粘贴或快捷键的操作验收。

所有最终源码整合后，完整检查链（包括 `cargo build -p elegant-clipboard-gpui --release --locked`）退出码 0。再次使用最终 Release 和新的独立目录执行原生 smoke，退出码 0，交互桌面剪贴板序列号前后均为 `2810`。

随后通过实际原生设置窗口点击“快捷键”，确认主热键、推进键、粘贴按键与槽位区域完整渲染；点击推进键“停用”后收到 Kit Notification“序列粘贴推进键已保存”，页面显示“已停用”且停用按钮禁用。独立 smoke 未注册粘贴热键，也未操作目标应用；程序退出码 0，剪贴板序列号仍为 `2810`。截图为 `target/qa/repair-shortcuts.png`，临时驱动验证后删除。

## 原生与后端限制

- 原有 `--smoke-test` 自动打开主窗口和数据设置页后退出，并关闭剪贴板监听；它不能代替序列推进、目标窗口切换或 TCC 权限的实机验收。
- Linux 的连接回归仅允许显式私有 Xvfb 运行，要求 `CI=true` 与 `ELEGANT_X11_TEST_SERVER=private-xvfb`。使用私有选择名，不操作 `CLIPBOARD` 或 `PRIMARY`。默认单元测试不注册系统全局热键。
- 原剪贴板恢复仍默认关闭。按键注入不能确认目标读取完成；Unix 后端不提供剪贴板原子比较写入接口，写前核对序列不能提供 Windows 系统锁内同等级的原子保证。Wayland 也没有自动回贴能力。
- macOS 权限授予/撤销、同应用多窗口回贴、Linux X11/Wayland 桌面、Office 原生格式优先级及 NumLock 输入效果仍需相应系统的实际目标应用验证。
