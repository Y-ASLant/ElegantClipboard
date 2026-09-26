# 来源应用过滤交互验证

- 对照主分支 `AppFilterTab.tsx` 与过滤规则函数，在 GPUI 中加入黑名单、白名单、规则列表、手工输入和运行中应用选择器。规则可匹配应用名、进程名或完整路径；普通规则按不区分大小写的子串匹配，`*`、`?` 规则按不区分大小写的完整通配符匹配。来源无法识别或规则列表为空时继续记录。
- 在隔离目录 `target/ui-app-filter-20260925` 中以 `--no-monitor` 打开调试版窗口。手工添加 `notepad.exe`，启用过滤并切换白名单；SQLite 保存为 `{"enabled":true,"mode":"whitelist","rules":["notepad.exe"]}`。重启后设置窗口保留该状态。
- 运行中应用选择器列出当前可见窗口的进程、窗口标题及可提取的图标；从列表添加 `ChatGPT.exe` 后规则立即显示，点击“移除”后恢复。输入 `calc.exe` 并按 Enter 也能添加，随后移除；最终隔离配置仍只有 `notepad.exe`。
- 核心测试覆盖模式、名称/进程/路径及通配符匹配、设置持久化与无效值回退；后台服务测试覆盖黑白名单切换后对新采集生效；备份测试覆盖过滤与监听类型设置的导出恢复。测试只用临时目录和合成内容，未写入用户剪贴板。
- `cargo fmt --all -- --check`、`cargo check --workspace --all-targets --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo test --workspace --locked --quiet` 及 `cargo build -p elegant-clipboard-gpui --release --locked` 均通过：核心 147、平台 53、应用 18，共 218 项通过，另有 2 项默认忽略。Release PE 为 GUI 子系统（2），以同一隔离目录启动并打开设置窗口后，白名单规则正常显示。
