# Windows 收藏增量验证

## 实现范围

- 沿用现有 SQLite `is_favorite`、`favorite_order` 和仓储事务，不修改旧应用源码或 schema。
- 新核心提供收藏切换及带收藏筛选的查询/计数；Windows worker 保持搜索、筛选、分页条件一致。
- GPUI 条目增加收藏/取消收藏，“全部 / 收藏记录”切换会清空旧列表、重置分页和滚动、取消待执行搜索并更新请求代次。
- 收藏状态重启后保留；启动默认全部视图。取消收藏不删除历史，置顶与收藏保持独立。
- 本轮重新查询 crates.io，直接依赖版本无需变更，没有新增依赖。

## 自动验证

- `cargo check --workspace --locked`：通过。
- `cargo test --workspace --locked --quiet`：92 项通过（83 核心、5 Windows 后端、4 应用）。
- `cargo clippy --workspace --all-targets --locked -- -D warnings`：通过。
- `cargo fmt --all -- --check` 与 `git diff --check`：通过。
- `cargo build -p elegant-clipboard-gpui --locked --quiet`：通过。
- 新测试覆盖收藏重开持久化、字面搜索与计数、重复采集保留收藏、取消收藏保留历史、缺失记录错误、收藏视图下新增采集及切换筛选后的旧结果拒收。

## 原生窗口验证

使用 `--no-monitor` 与 `target/qa/gpui-smoke` 合成数据库，Windows UI Automation 操作真实 GPUI 窗口：

1. 无收藏时收藏列表为空；全部视图仍有历史。
2. 点击收藏后显示取消收藏，收藏视图出现对应记录。
3. 收藏视图搜索无匹配词时为空；清空搜索恢复记录。
4. 取消收藏后该视图为空，返回全部仍存在记录。
5. 再次收藏，直接读取隔离数据库确认状态已持久化。
6. 超过 100 条合成收藏下，“加载更多”加载剩余记录后消失；切换全部和收藏后均重新从 100 条加载。
7. PrintWindow 截图确认筛选按钮和条目操作正常显示；窗口关闭退出码 0。

测试未读写真实系统剪贴板，未验证真实 IME、长期运行或其他平台。本轮未进行远端 CI 或 Git 提交。
