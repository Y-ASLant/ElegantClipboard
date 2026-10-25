# 0002 原生格式保真、瞬时过滤、同文合并与 blob 存储

日期：2026-10-25　状态：已实现

## 背景

与 QcClip 的功能对比确认了若干核心基础能力差距。本决策记录四个交叉功能的存储与生命周期设计：额外原生格式捕获、瞬时内容确认窗口、同文条目格式合并、大格式外部存储。

## 决策

### 1. 额外原生格式（native_formats 表）

- 监听线程在五类主格式（text/html/rtf/files/image）之外用 `EnumClipboardFormats` 全量枚举；黑名单常量（标准格式 ID + 注册名）过滤 OLE 私有、Shell、Chromium/Firefox 内部、隐私标记与已由主采集表示的格式。零配置，不提供用户名单 UI。
- 主键 `(item_id, format_name)`；≤64 KiB 内联 BLOB，更大指向 `blobs/<hash>`。单格式 8 MiB、单次 32 MiB，超限跳过并以 `Event::Status` 汇总提示。
- 删除路径**显式** DELETE 行而非依赖 CASCADE：读连接未开 foreign_keys，且 GC 必须在删除前收集 blob 引用。
- 复制（Original 模式）时随主格式写回：富文本在既有单 guard 会话内追加；纯文本用独立 NoClear 会话；校验对 ≤1 MiB 字节比对。
- 备份 v2 新增 `blobs/` 资产类别（Manifest.missing_blobs）；blob 文件名即内容哈希，恢复无需重映射。image_tmp 刻意不入备份（可再生）。

### 2. 瞬时内容确认窗口（worker/transient.rs）

- 真实 watcher 的采集带 `deferred=true` 进 worker 侧 300ms 队列；到期时在 `CaptureState` 临界区内重读：
  - 序列号未变 → 入库；被自身写回取代（== ignored_sequence）→ 仍入库（保护「复制后立即从本应用粘贴」）；
  - 重读为空或内容不同 → 丢弃；重读失败顺延 250ms，3 轮后放弃；
  - pending 上限 8，溢出冲刷最旧（退化为旧行为）。
- 测试与 smoke 注入 `deferred=false` 绕过窗口，保证既有测试的同步语义。
- 确认入库后必须置 snapshot_pending 并 flush 快照——与直通路径一致，否则 UI/smoke 收不到列表更新。

### 3. 同文条目格式合并（same_text.rs + content_hash_aliases）

- 触发条件：默认分组、text-like 类型、`text_content` 精确相等。合并动作（单事务）：补缺 html/rtf 列、仅 text 目标升级类型、byte_size=三段和重算、preview 仅 NULL 时填、置顶、写别名行。
- 别名只解析默认分组（与去重 scope 一致）；`touch_by_hash` 直接哈希未命中时查一次别名。
- 残留别名（目标被删）解析 miss 即当新 hash 建新条目；删除路径按 content_hash 清别名，保留策略执行尾部 purge_stale_aliases 兜底。

### 4. blob 存储（blob_store.rs）

- 写入永远 pending→commit 两阶段：pending 期间对 GC 不可见，commit 用 persist_noclobber；GC 只删「查询时未被引用且名字为合法哈希」的文件。这套组合使 attach 与 GC 无需互斥锁。
- GC 挂在 cleanup_media（五个删除路径共用），used 集在删除**提交后**查询——沿用图片/暂存附件的既有顺序。

## 后果

- `DataSizeInfo` 未计入 blobs 与 image_tmp（后续可在数据页补充统计）。
- 写回额外格式对不支持的目标应用是静默增强，失败仅提示不阻塞主格式。
- HTML 预览不加载 http(s) 远程图片（gpui 无内置网络图片管线），在预览页说明文案与 README 已注明。
