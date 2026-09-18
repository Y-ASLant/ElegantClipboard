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


## 全文预览增量验证

- 直接依赖通过 crates.io API 再次核对，版本无需变更，没有新增依赖。
- `cargo test --workspace --locked`：89 项通过（82 + 4 + 3）。新增测试覆盖中文、emoji、CRLF 与超过卡片预览长度的完整正文、删除后读取错误、关闭/切换/重开预览时的过期请求。
- fmt、Clippy（`-D warnings`）、Windows debug build 通过。
- 在 `--no-monitor` 和合成数据库下，真实 GPUI 窗口打开 519 字符正文；截图确认显示超过 240 字符的全文，UI Automation ValuePattern 读取到 519 字符。
- 聚焦正文后执行 Ctrl+A、输入字符、Backspace，读取值未改变；只读键盘操作通过。未调用复制操作或写入系统剪贴板。
- 首次检查发现 Input 自身的 Escape 绑定遮蔽预览快捷键，补充 `Preview > Input` 绑定后，Esc 返回及 Space 再次打开均通过。
- 字面搜索 `100%_done` 后打开预览，正文匹配；返回后删除对应合成记录，再打开下一条预览正常。窗口关闭退出码 0。
- 上游控件的 UI Automation `IsReadOnly` 元数据曾报告 false，因此只读以实际输入/删除不改变正文的结果为依据；无障碍元数据一致性仍需后续检查。
- 未覆盖真实剪贴板写回、真实 IME 和最大 1 MiB 文本的交互性能；保留前述未覆盖范围。


## 自定义窗口增量验证

- 使用 gpui-kit 0.6.2 的 `TitleBar::window_options()` 和 `TitleBar::title_bar_options()`，保留 Windows 窗口标题、最小尺寸和应用 ID；历史列表、全文预览共用外层标题栏。
- Windows debug build、Clippy（warnings 视为错误）、fmt 与 diff 空白检查通过；本次没有更改数据处理逻辑或依赖。
- 隔离数据、禁用采集，通过实际鼠标操作及 Win32 状态检查验证最小化、最大化、还原、拖动、双击标题栏最大化、右边缘缩放和关闭；关闭退出码 0。自动化操作需在移动鼠标后等待命中区域刷新，加入等待后全流程通过。
- PrintWindow 截图确认系统标题栏已由 Kit 标题栏替换，没有重复窗口控制按钮；全文预览的只读、Esc 返回、Space 打开及搜索/删除后的预览回归通过。
- 未验证 Windows Snap Layouts、多显示器混合 DPI 或其他平台的自定义窗口行为。
