# GPUI 可靠性修复验证（2026-10-09）

基准为 `gpui@ee064bd`，本记录覆盖其上的未提交可靠性补丁。测试在 Windows x64 本机执行，未使用用户真实历史目录。

## 修复范围

- RTF 哈希规范化改用字节扫描。无花括号的 `\*\datastore`、截断和不平衡分组均能返回；转义括号和 UTF-8 不再导致错误索引。标准 ASCII datastore 与 RSID 样本保留已有哈希字节，原始 RTF 载荷仍原样存储。该函数仍是局部规范化器，不是完整 RTF 解析器。
- 监听在读取来源应用和各格式之前、之后校验剪贴板序列号，变化时整组重读，最多四次。无法取得稳定快照时报告错误，不保存混合内容；本进程写回抑制仍由同一锁保护。
- 后台命令保持统一 FIFO；普通命令保留 64 个配额，观察到的采集另有 1,024 条及 100 MiB 动态载荷配额。配额包含 worker 正在处理的载荷。界面队列满时只合并采集引起的快照，查询和操作确认保持原顺序；告警保留次数和最近原因，空间恢复后重发。关闭时停止接收新命令，解除事件发送阻塞，保存已接受的采集并保持前置过滤设置顺序。
- 设置窗口底部使用 GPUI Kit Alert 显示通用结果，主窗口隐藏时也能看到导出、快捷键、试听等现有消息。反馈区最高 120px，可滚动，新消息重置滚动位置；打开设置不会清除已有错误级别。

达到采集条数或字节上限仍会拒绝新内容，并提示重新复制；本补丁不承诺无限负载无损采集、磁盘故障下成功保存或进程被强制终止后的内存队列恢复。后台采集错误不被随后合并的旧状态提示降级覆盖。

## 自动检查

以下检查通过：

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build -p elegant-clipboard-gpui --release --locked
```

测试结果：core 149 通过；platform 69 通过、2 忽略；GPUI 31 通过；合计 249 通过、0 失败、2 忽略。两个忽略项仍是依赖 Windows 系统程序图标的既有测试。本次新增 20 项测试，覆盖 RTF 异常输入、历史哈希样本、稳定快照重试、配额与 FIFO、满事件队列继续保存、延后告警以及退出排空。

## 隔离剪贴板往返

执行 `cargo run -p clipboard-platform --example isolated_clipboard_smoke --locked`。示例在写入前选择并核对非交互窗口站，使用临时数据库；本次窗口站为 `Service-0x0-8dd8b$`，不是 `WinSta0`。

既有文本、富文本写回、纯文本转换、图片、文件、路径、合并和超限错误检查均通过。新增步骤通过真正的监听路径读取 HTML/RTF/纯文本组合，并检查同一记录保存的三种表示；还把此前导致死循环的 `\*\datastore` 写入隔离剪贴板，验证能采集、写回及继续处理后续图片和文件。

新增 HTML 断言首次误将 CF_HTML 文档包装与原始片段比较，导致示例失败。修正为比较当前 API 返回的完整 HTML，并确认包含原片段后，整套示例通过；应用实现未为此改变。

关键输出：

```text
observed rich snapshot and malformed RTF roundtrip ok
image capture/copy ok
files capture/copy ok
file path copy ok
merged text/files copy ok
capture error isolation ok
isolated clipboard roundtrip passed
```

## 窗口启动检查与限制

Release 程序通过 `--smoke-test --data-dir <独立临时目录>` 运行，自动打开主窗口及数据设置页后正常退出，退出码 0。该模式禁用剪贴板监听。

这次窗口检查证明启动、渲染生命周期和自动退出没有报错；未执行最小窗口、长路径消息、所有设置失败分支的人工视觉验收，也未覆盖浏览器/Office 的完整跨应用粘贴矩阵或长期常驻压力测试。
