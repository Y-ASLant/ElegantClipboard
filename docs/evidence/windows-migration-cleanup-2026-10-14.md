# 数据迁移与附件回收验证（2026-10-14）

基准：`gpui@2f800bb` 加本次工作区补丁。

## 修复范围

- `sort_order`、`char_count`、`favorite_order` 的新增列和回填使用局部事务，回填失败不提交新增列。图片尺寸和来源应用字段逐列补齐，恢复旧版本中断留下的缺列，保留已有字段值。不会无条件改写已存在的用户排序，也不能凭列存在性识别旧版本已经遗漏的排序回填。
- 删除结果区分数据库提交与媒体回收：四种删除 API 返回删除数及可选回收警告，缺失文件视为已经回收，共享引用和目录外文件仍受保护。一个候选失败不阻止继续处理其他候选及另一类媒体。
- 删除提交后即使快照刷新失败，也先发送成功完成回执，再通过独立后台错误报告刷新问题。自动淘汰和文本编辑遇回收失败仅记录警告，不误报已保存操作失败。
- 沿用主窗口和设置窗口现有 GPUI Kit Notification；未增加附件自动回收重试队列。

## 自动验证

- `just check` 通过：131 个 Rust 文件均不超过 500 行，格式、workspace/all-targets check、Clippy `-D warnings` 均通过，无编译警告。
- `just test` 通过：core 154、platform 73、GPUI 32，共 259 项通过；2 项原有 Windows 系统图标相关测试保持忽略。
- 迁移回归覆盖四种配对字段缺失，读取原记录后再次重开；用 SQLite 触发器使字段回填失败，验证新增字段回滚、原数据保存，并在解除故障后成功重试，收藏样本顺序与 ID 相反。
- 附件回归使用隔离临时目录，将受管候选替换成目录触发真实删除失败，验证单删、批删、分组清理、全部清理均保留数据库提交并报告残留，其他可删除附件继续回收。
- 服务回归覆盖四种删除路径的媒体失败以及提交后列表刷新失败，验证成功完成回执与独立后台错误的顺序；测试没有写入真实剪贴板。
- `cargo run -p clipboard-platform --example isolated_clipboard_smoke --locked` 通过；在独立 Windows 窗口站完成文本、HTML/RTF、图片、文件、合并复制和错误隔离往返，没有写入交互桌面的剪贴板。

## 构建与原生验收

- 常规 `just build` 的源码编译未报告警告，但最终覆盖 `target/release/elegant-clipboard-gpui.exe` 时因预先运行的 EC 实例占用而报 Windows `os error 5`。未关闭该实例，改用独立 `target/qa/verified-build` 目录构建同一 Release 配置。
- `cargo build -p elegant-clipboard-gpui --release --locked --target-dir target/qa/verified-build` 成功，包含依赖的完整构建无警告、无错误。产物 `target/qa/verified-build/release/elegant-clipboard-gpui.exe` 为 37,614,080 字节，SHA-256：`FF3A719DCF19495AB5DF122CB2741BE9A60795978243493513C89F371C69562C`。现有 `target/release` 运行实例保持不变。
- 使用该 Release、`--no-monitor`、禁用热键及独立 `target/qa/cleanup-native-uoa3dse0` 数据目录启动真实原生窗口。合成文本记录挂接受管测试图片，以不允许删除的共享方式打开图片，聚焦搜索框后按 Down、Delete。观察截图：记录数由 1 变 0，通知显示“记录已删除；部分附件未能回收”及 Windows 文件占用错误；6 秒后通知自动消失，没有常驻底栏。关闭测试实例后检查数据库为 0 条记录，被锁附件仍存在。
- 原生验收前后交互桌面剪贴板序列号保持 301，监听未启用，未执行复制或粘贴。仅结束本次启动的隔离验收实例，预先运行的 EC 保持运行。截图保留在上述忽略目录的 `before-delete.png`、`after-delete.png`、`after-notification.png`，临时验收脚本在验证后删除。
- 原生通知文字未出现在 UI Automation 名称列表，首次基于名称的断言失败；改为实际截图观察、持久化状态检查和序列号校验后通过。本次原生操作验证单删，另外三条删除路径由服务及核心回归覆盖，未声称全部 GUI 入口逐一点击验收。
