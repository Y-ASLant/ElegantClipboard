# Windows 基础版

## 范围

Windows 优先，其他平台保留为后续目标。基础版使用新的根 Cargo workspace；原有 `src-tauri` 与本地 `clipboard-rs` 保持独立 workspace，不改变旧入口和旧锁文件。

首批功能：文本采集、历史持久化、精确去重、搜索、复制、删除、置顶、暂停/恢复。全局快捷键、托盘、自动粘贴、图片/文件等后续开发。

## 当前实现

- `crates/clipboard-core`：复用原有数据库 schema、迁移、仓储和去重源码，提供 UI 无关的文本历史用例。
- `crates/clipboard-platform`：Windows 原生剪贴板事件监听；独立 worker 串行处理历史操作；请求代次、实例锁、有界队列和关闭清理。
- GPUI 应用入口：待框架依赖例外确认后接入，当前 workspace 不包含空程序或该预发布依赖链。

共享源码暂通过 `#[path]` 编译原有文件，避免复制一套数据库。旧壳接入共享 crate 时再物理移动源码；当前新核心不依赖 Tauri。

默认目录由 `ProjectDirs::from("com", "ASLant", "ElegantClipboard-GPUI")` 生成，位于 Windows 用户 LocalAppData。不自动读取或迁移旧版安装目录中的数据库。测试只使用临时目录及合成文本。

限制：单条文本最多 1 MiB；普通未固定历史按原仓储规则保留一万条；空白文本不保存。重复的完全相同文本移动到顶部，文字和换行原样保存。队列满时报告采集失败，不把不同复制事件主动合并成最后一条。

## 依赖策略

2026-09-18 已查询 crates.io 元数据：

| 直接依赖 | 最新稳定版 |
|---|---|
| gpui-kit（候选，尚未接入） | 0.6.2 |
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

**待确认**：最新稳定 gpui-kit 仍引用 ropey 2.0.0-beta.1。需要决定“最新稳定版”是否只约束直接依赖并允许此上游例外；严格禁止任何预发布包将阻塞当前 Kit 版本。曾在临时入口解析后用 `cargo tree -p elegant-clipboard-gpui -i ropey` 确认依赖路径为 `gpui-kit → gpui-base/gpui-component → ropey`；确认前已撤去该入口和依赖，当前锁文件仅包含核心与平台后端。

## 验证命令

在仓库根目录运行：

```powershell
cargo test -p clipboard-core -p clipboard-platform
cargo clippy -p clipboard-core -p clipboard-platform --all-targets -- -D warnings
cargo fmt --all -- --check
```

目前核心与后端 84 项测试通过，包括继承的数据库/去重测试和新增的持久化、Unicode 内容、字面搜索、worker 顺序、实例排他和背压关闭测试。原生剪贴板往返和界面操作尚未验收；单元测试未读写实际剪贴板内容。

根 workspace 的 fmt、Clippy（warnings 视为错误）与 locked 测试已通过；解析的 126 个包中没有预发布版本。Windows CI 配置已添加，尚未在远端运行。CI 使用已核验的 [checkout v7.0.1](https://github.com/actions/checkout/releases/tag/v7.0.1) 与 [rust-cache v2.9.2](https://github.com/Swatinem/rust-cache/releases/tag/v2.9.2)。
