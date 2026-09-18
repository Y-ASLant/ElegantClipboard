# Windows 分组重命名验收（2026-09-19）

- 核心层校验名称非空、长度及同名冲突，拒绝不存在的分组；重命名保持组 ID 与已有记录归属不变。Windows 工作线程收到 `RenameGroup` 后先刷新分组列表，再返回保存结果。
- 单元测试覆盖重启后名称持久化、重复名称、空名称和无效 ID；平台测试覆盖成功确认与冲突错误。
- 使用 `target/qa/group-create-ui/clipboard.db` 的隔离备份，以 `--no-monitor` 打开真实 GPUI 窗口，选中 `WorkQA` 后点击“重命名”，编辑器预填旧名；输入 `RenamedQA` 并点击“保存名称”。SQLite 查询结果为 `(id=1, name='RenamedQA')`。截图位于忽略目录 `target/qa/group-rename/`。
- 没有修改真实用户数据目录或交互桌面剪贴板。删除分组仍待实现。
