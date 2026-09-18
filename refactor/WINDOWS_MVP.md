# Windows 基础版

## 范围

Windows 优先，其他平台保留为后续目标。基础版使用新的根 Cargo workspace；原有 `src-tauri` 与本地 `clipboard-rs` 保持独立 workspace，不改变旧入口和旧锁文件。

首批功能：文本采集、历史持久化、精确去重、搜索、复制、删除、置顶、暂停/恢复。全局快捷键、托盘、自动粘贴、图片/文件等后续开发。

## 当前实现

- `crates/clipboard-core`：复用原有数据库 schema、迁移、仓储和去重源码，提供 UI 无关的文本历史用例。
- `crates/clipboard-platform`：Windows 原生剪贴板事件监听；独立 worker 串行处理历史操作；请求代次、实例锁、有界队列和关闭清理。
- `crates/clipboard-gpui`：Windows 原生主窗口、搜索、虚拟历史列表、复制/删除/置顶、暂停确认、加载更多、全文只读预览、收藏与收藏筛选、拖拽排序和错误反馈。

共享源码暂通过 `#[path]` 编译原有文件，避免复制一套数据库。旧壳接入共享 crate 时再物理移动源码；当前新核心不依赖 Tauri。

默认目录由 `ProjectDirs::from("com", "ASLant", "ElegantClipboard-GPUI")` 生成，位于 Windows 用户 LocalAppData。不自动读取或迁移旧版安装目录中的数据库。测试只使用临时目录及合成文本。

限制：单条文本最多 1 MiB；普通未固定历史按原仓储规则保留一万条；空白文本不保存。重复的完全相同文本移动到顶部，文字和换行原样保存。队列满时报告采集失败，不把不同复制事件主动合并成最后一条。

## 依赖策略

2026-09-18 已查询 crates.io 元数据：

| 直接依赖 | 最新稳定版 |
|---|---|
| gpui-kit | 0.6.2 |
| rusqlite | 0.40.2 |
| clipboard-rs | 0.3.5（注册表版；文本阶段不需要本地图片补丁） |
| windows | 0.62.2 |
| anyhow | 1.0.104 |
| async-channel | 2.5.0 |
| blake3 | 1.8.7 |
| directories | 6.0.0 |
| parking_lot | 0.12.5 |
| serde | 1.0.229 |
| serde_json | 1.0.151 |
| tracing | 0.1.44 |
| tempfile（测试） | 3.27.0 |

工具链要求 Rust 1.98+；本机已使用 1.98.0。Cargo.lock 固定实际解析结果。依赖升级流程：查询最新稳定版本 → 更新 workspace requirements → `cargo update` → fmt/clippy/test/build → 原生交互验收。存在上游兼容约束的间接依赖不能强行替换为不同主版本。

**上游例外**：本轮继续接入最新稳定 gpui-kit，其 Windows 依赖路径 `gpui-kit → gpui-base/gpui-component → ropey` 使用 ropey 2.0.0-beta.1。详见 [技术决策](decisions/0001-windows-gpui.md)，不宣称整个依赖树均为稳定版。

## 启动与操作

```powershell
# 正常使用：启动原生窗口并记录后续复制的文本
cargo run -p elegant-clipboard-gpui --locked

# 隔离检查：禁用采集，只浏览指定目录中的历史
cargo run -p elegant-clipboard-gpui --locked -- --no-monitor --data-dir .\target\gpui-check

# 窗口冒烟：禁用采集，约 3 秒后自动退出
cargo run -p elegant-clipboard-gpui --locked -- --smoke-test --data-dir .\target\gpui-smoke
```

- 搜索支持中文与字面 `%`/`_`，等待 150ms 后查询；回车复制当前选中结果。
- 列表支持 ↑/↓ 选择、Enter 复制、Delete 删除；Ctrl+F 聚焦搜索。
- 置顶、删除与复制按钮直接调用后台服务；复制使用完整正文，不是卡片预览。
- 点击“查看”或在列表按 Space 打开完整文本；正文可滚动、选取，点击“复制全文”复制原始内容；Esc 返回列表并恢复键盘焦点。预览按需从后台读取，关闭后的旧响应不会重新打开预览。
- 点击条目“收藏 / 取消收藏”保存常用文本；“全部 / 收藏记录”切换视图。搜索、数量与加载更多均作用于当前视图，取消收藏不会删除记录。收藏状态持久化，启动默认显示全部。
- 每条记录可从左侧“⠿”手柄拖动，落在目标上/下半区即插到前/后；边缘拖动滚动已加载列表。顺序写入 SQLite，收藏列表维护独立顺序。置顶与普通记录分别排序，跨区拖动会提示先调整置顶状态。Esc 可取消拖动。
- 默认加载 100 条，点击“加载更多”继续；仅渲染可见行。
- 顶部“外观”提供跟随系统、浅色和深色；保存成功后生效，重启恢复选择。设置存于同一 SQLite 数据库，手动主题不随系统通知改变；未知设置值回退为跟随系统。
- 窗口使用 gpui-kit `TitleBar` 自定义标题栏，列表与全文预览共用；支持拖动、双击最大化/还原、最小化、边缘缩放和关闭。
- 关闭窗口即退出；当前未提供托盘、全局快捷键或自动粘贴。
- 请勿将开发版的 `--data-dir` 指向正在使用的旧版数据目录；正式迁移尚未实现。

## 验证命令

在仓库根目录运行：

```powershell
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
cargo build -p elegant-clipboard-gpui --locked
```

当前 97 项测试通过（核心 86、Windows 后端 7、应用状态/参数 4），新增暂停确认、搜索代次与选中 ID 保持、键盘边界以及冒烟参数隔离测试。全文预览新增长文本原样返回、已删除记录错误及关闭/重开后旧响应拒收测试。fmt、Clippy（warnings 视为错误）和 Windows debug 构建通过。

原生窗口已通过启动/退出、中文路径、重复实例拦截，以及 120 条合成数据下的加载更多、字面搜索、空结果、置顶和删除检查。所有交互使用隔离目录并禁用实际采集；没有覆盖系统剪贴板读写、真实中文 IME、长时间运行、release 打包或跨平台运行。完整过程见 [验证记录](evidence/windows-ui-2026-09-18.md)。

Windows CI 包含 fmt、Clippy、locked 测试和应用构建，尚未在远端运行。CI 使用已核验的 [checkout v7.0.1](https://github.com/actions/checkout/releases/tag/v7.0.1) 与 [rust-cache v2.9.2](https://github.com/Swatinem/rust-cache/releases/tag/v2.9.2)。

收藏增量验证见 [2026-09-19 验证记录](evidence/windows-favorites-2026-09-19.md)。

主题增量验证见 [2026-09-19 主题验证记录](evidence/windows-theme-2026-09-19.md)。

界面间距与动画规则见 [UI 规范](UI_GUIDELINES.md)，拖拽验证见 [2026-09-19 排序验证记录](evidence/windows-reorder-2026-09-19.md)。
