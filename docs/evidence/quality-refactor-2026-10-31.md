# 质量重构验证

日期：2026-10-31  
版本：2.0.0-beta.14  
对应评估：[项目质量与重构评估](project-quality-review-2026-10-31.md)

## 已完成的代码收敛

- 复制命令统一为 `Command::Copy { id, mode, purpose }` 与 `Command::Merge { ids, purpose }`。`CopyMode` 和 `CopyPurpose` 同时驱动后端分发、成功回执、失败归属和旧投递取消。
- `PasteCoordinator` 私有持有目标、请求、快捷槽位解析、合并状态和序列队列。单次投递分为等待复制与等待输入投递，队列可与当前投递并存。
- `PreferenceSnapshot` 复用现有 getter；`Service.initial_preferences` 替换平铺启动字段。备份过滤由偏好键常量生成并参数绑定，键名、迁移标记和未知键排除语义不变。
- 平台中立服务测试从 `windows/tests` 实际移到 `service/tests`。Windows 图标提取和信号同步仍按平台编译。
- 标签发布复用同一提交的三平台质量工作流，草稿使用该流程产出的包。失败命令保留真实退出码和日志。
- 新增 `clipboard-core` 示例 `measure_history`，只写指定的新目录或临时目录。

未改变数据库、备份 ZIP 布局、默认偏好或 GPUI Kit 控件。

## 本机验证

| 检查 | 结果 |
|---|---|
| 源码行数 | 290 个 Rust 文件，均不超过 500 行 |
| fmt / check / Clippy `-D warnings` | 通过；检查范围为 workspace all-targets |
| `cargo test --workspace --locked` | 461 通过，0 失败，2 忽略 |
| Linux x64 `clipboard-platform --all-targets` | 通过；使用 Zig 交叉编译。初次失败是测试导出仍被 `windows` 条件挡住，已改为 `cfg(test)` |
| 隔离剪贴板往返 | 互斥保护后的最终运行通过 |
| Windows Release 包 | `ElegantClipboard-v2.0.0-beta.14-windows-x64.zip`，SHA-256 `f20a5fef0af352015dd906d0aabc052a5a5ee5b051eed268ec3022d302f1ee87` |
| 构建目录与 ZIP 解压启动 | 均在隔离数据目录自动退出，退出码 0 |

测试分布在最终完整测试运行时为核心、平台和 GPUI 三项；之后只新增示例互斥模块，并对其执行了 Clippy all-targets 与隔离往返，没有为这一测试夹具改动重跑全部 461 项。

## 原生窗口与性能基线

禁用采集，数据目录含 10000 条合成文本和 24 张 2048×1152 PNG。Release 窗口约 570ms 可见，stderr 为空，退出码 0。

观察到：

- 初始列表显示图片缩略图，底部计数为 10024。
- 搜索框输入 `audit-common` 后计数变为 100，卡片高亮该词，并显示中文“审计标记”。
- 60 秒闲置、监视器关闭：私有内存从 268,193,792 字节降至 86,568,960 字节，CPU 增加 0.47 秒，句柄约 622，线程约 69。
- 搜索结果保持打开的另一次 60 秒采样 CPU 增加 7.63 秒。两者都不是滚动帧率、长期稳定性或多显示器验收。

Release 核心查询（预热后，30 次，不含 UI 防抖、worker 和绘制）：

| 查询 | 匹配数 | p50 | p95 |
|---|---:|---:|---:|
| 首页 | 10000 | 28.0ms | 29.9ms |
| `audit-common` | 100 | 38.6ms | 40.6ms |
| `审计标记` | 1000 | 41.4ms | 45.8ms |
| 未命中 | 0 | 31.8ms | 33.7ms |

播种 10000 条耗时 89.3 秒，其中 10 条为 1MiB。End/Home 注入后截图仍停在搜索首页，不能宣称键盘跳转已验证。

## 堆损坏与窗口站

首次隔离往返在并发运行期间以 `0xC0000374`（`STATUS_HEAP_CORRUPTION`）退出。转储中的采集线程位于 `read_content` → `clipboard-rs get_text` → `clipboard-win get_string` → `GetClipboardData`/`GlobalSize`。

空名称 `CreateWindowStationW` 对非管理员返回既有登录窗口站，多次运行都打印 `Service-0x0-12eea8$`。因此并发冒烟共享一个剪贴板。随后的单独运行、GDB 运行、进程级堆标志运行和互斥保护运行均通过，包括恢复请求归属与外部内容保护。

示例现在用 `Local\ElegantClipboardIsolatedClipboardSmoke` 互斥量拒绝重叠运行。这不修复依赖库内部的剪贴板读取实现，也不证明产品路径存在或已经消除该崩溃。没有向日常桌面剪贴板写入。

## 仍未完成

- 历史 Windows CI 运行 41166737177 与 41163949454 的失败日志仍返回 HTTP 403，需要仓库管理员权限。质量门槛已改为保留后续失败的退出码和日志，但不能回溯修复未知根因。
- 未在 macOS 或 Linux 桌面执行人工权限、跨应用回贴和 Wayland 验收。
- 自动恢复剪贴板仍只证明按键已发送，不证明目标应用已读取。
- 未测图片滚动帧率、长期运行、多显示器和完整 DPI 矩阵。
