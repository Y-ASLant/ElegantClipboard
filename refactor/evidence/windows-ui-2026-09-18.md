# Windows GPUI 主窗口验证

日期：2026-09-18。本机 Windows、Rust 1.98.0、gpui-kit 0.6.2，debug 构建。

## 自动检查

| 检查 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | 通过 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 通过 |
| `cargo test --workspace --locked` | 87 项通过：82 核心 + 3 Windows 后端 + 2 应用 |
| `cargo build -p elegant-clipboard-gpui --locked` | 通过 |
| 启动 `--smoke-test --data-dir <独立目录>` | 窗口创建并自动退出，退出码 0 |
| 数据目录包含中文与空格 | 冒烟退出码 0 |
| 同目录第二实例 | 退出码 1，明确报告目录被占用 |

## 原生交互

使用 `--no-monitor` 运行，隔离目录位于 `target/qa/gpui-smoke`。通过 SQLite 写入 120 条合成记录，包含中文、URL、emoji、换行、长文本及字面 `100%_done`；未使用个人历史或实际剪贴板内容。

利用 Windows UI Automation 的 ValuePattern/InvokePattern 操作真实 GPUI 控件，并核对 SQLite：

1. 初始显示 120 条总记录；默认查询 100 条。
2. 点击“加载更多”，结果全部加载后该按钮消失。
3. 输入 `100%_done`，得到 30 条对应示例；`%` 与 `_` 未作为通配符展开。
4. 点击首条“置顶”，按钮变为“取消置顶”，SQLite 对应记录 `is_pinned = 1`。
5. 点击该条“删除”，总记录数变为 119，对应 ID 不再存在。
6. 输入不存在的关键词，列表没有复制操作按钮，显示空结果。
7. 连续切换搜索关键词，最终输入保持为最后一次查询；过期结果拒收另有状态测试覆盖。
8. 关闭主窗口，应用退出码 0；重新打开后保留删除后的 119 条记录。

通过 PrintWindow 捕获并查看实际窗口截图。初版长文本第二行有高度裁切，已增加虚拟行高度并固定正文行高，复查后两行文本正常显示。截图和合成数据库在被忽略的 `target/qa/` 内，不纳入提交。

## 未覆盖范围

- 验证过程禁用采集，未点击会写系统剪贴板的“复制”；实际采集/写回仍需原生往返验证。
- UI Automation 的 SetValue 验证搜索输入变化，不代表真实中文 IME 已验收。
- 暂停状态采用后端确认并有回归测试，但本次未用真实外部复制验证暂停/恢复。
- 未验证 release 构建、安装包、长时间常驻、其他 Windows 架构及 macOS/Linux。
- Windows CI 已更新配置，尚未在远端运行。
