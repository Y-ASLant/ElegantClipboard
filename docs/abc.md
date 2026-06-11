# 架构与性能审查报告

> 审查范围：模块边界、状态管理、IPC 通信、性能瓶颈、内存使用、可观测性
> 审查日期：2026-05-27

## 一、模块边界

### 优势
- **后端清晰分层**：`commands/` 按域拆分（clipboard/sync/translate/file_ops/groups），每个文件 < 800 行
- **Repository 模式**：`ClipboardRepository`、`SettingsRepository`、`GroupRepository` 隔离 SQL，便于测试
- **AppState 集中**：`AppState { db, monitor, active_group_id }` 通过 `Arc` 共享,依赖明确
- **`active_group_id` 共享所有权**：`ClipboardMonitor` 与 `AppState` 通过 `Arc<Mutex<Option<i64>>>` 共享，避免双向回调

### 问题

**1. `lib.rs` 臃肿（1062 行）** 🟡

```rust
// src-tauri/src/lib.rs:43-57
static CURRENT_SHORTCUT: parking_lot::RwLock<Option<String>> = parking_lot::RwLock::new(None);
static CURRENT_QUICK_PASTE_SHORTCUTS: parking_lot::RwLock<Vec<String>> = ...;
static CURRENT_FAVORITE_PASTE_SHORTCUTS: parking_lot::RwLock<Vec<String>> = ...;
static QUICK_PASTE_LOCK: parking_lot::Mutex<()> = ...;
static ACTIVE_QUICK_PASTE_SLOTS: ...;
static ACTIVE_FAVORITE_PASTE_SLOTS: ...;
```

混合了：全局快捷键状态、命令定义、`run()` 入口、`PasteKind` 枚举、setup 逻辑。建议拆出独立 `paste_shortcuts.rs` 模块。

**2. `webdav` 配置加载重复** 🟡

`webdav/mod.rs:load_config_and_options` 与 `commands/sync.rs:load_webdav_config` + `load_sync_options` 几乎完全相同。

**3. `App.tsx` 单文件 787 行** 🟡

集合了：搜索栏、工具栏、批量操作栏、列表、分组下拉、4 个对话框。建议拆 `GroupDropdown`、`Toolbar`、`AppDialogs` 子组件。

---

## 二、状态管理

### 优势
- **Zustand + Tauri Event 多窗口同步**：`ui-settings-changed`、`translate-settings-changed` 事件保证设置窗口和主窗口数据一致
- **`makeSetter` 抽象**：`ui-settings.ts:247` 用工厂函数消除了 35+ 个 setter 的样板代码
- **后端持久化**：从 localStorage 迁移到 SQLite settings 表（更可靠，跨窗口共享）

### 问题

**1. App.tsx 7 个独立 UISettings selector** 🟠

```ts
// src/App.tsx:75-81
const autoResetState = useUISettings((s) => s.autoResetState);
const searchAutoFocus = useUISettings((s) => s.searchAutoFocus);
const searchAutoClear = useUISettings((s) => s.searchAutoClear);
const cardDensity = useUISettings((s) => s.cardDensity);
const showCategoryFilter = useUISettings((s) => s.showCategoryFilter);
const toolbarButtons = useUISettings((s) => s.toolbarButtons);
const windowAnimation = useUISettings((s) => s.windowAnimation);
```

每个 selector 都注册独立订阅。`ClipboardItemCard.tsx` 13 处、`CardContentRenderers.tsx` 9 处也是相同模式。建议用 `useShallow`：

```ts
const { autoResetState, searchAutoFocus, /*...*/ } = useUISettings(useShallow(s => ({
  autoResetState: s.autoResetState, /* ... */
})));
```

**2. `translate-settings.ts` setter 缺少防抖** 🟡

所有 setter 立即调用 `saveSetting + broadcastChange`：

```ts
// src/stores/translate-settings.ts:132
setDeeplxEndpoint: (url) => {
  set({ deeplxEndpoint: url });
  get().saveSetting("translate_deeplx_endpoint", url);
  broadcastChange({ deeplxEndpoint: url });
},
```

导致 `TranslateTab.tsx` 必须用 `useTranslateSettings.setState(...)` 直接绕过 setter 来阻止每键存盘，再用 `debounced()` 延迟调用真正的 setter。这是反模式 —— 文本字段的 setter 本应内置防抖。

**3. `useWebDAVSettings.ts` 15 个独立 `useEffect`** 🟢

（已修复防抖共享问题）仍然是 14 个几乎相同的 `useEffect`，可考虑合并：

```ts
useEffect(() => {
  if (!loaded) return;
  saveSetting("webdav_enabled", String(enabled));
  saveSetting("webdav_auto_sync", String(autoSync));
  // ...
}, [/* deps */]);
```

但工作量较大，且当前实现可读性尚可。**优先级低**。

**4. `pickUISettingsData` 每次保存都重建对象** 🟢

```ts
// src/stores/ui-settings.ts:160-166
function pickUISettingsData(state: UISettings): UISettingsData {
  const next = {} as UISettingsData;
  for (const key of UI_SETTINGS_KEYS) {
    (next[key] as UISettingsData[typeof key]) = state[key];
  }
  return next;
}
```

35+ 字段的对象在每次设置变更时遍历构建。对当前规模影响可忽略（< 1ms），但 `updateAndPersist` 的 `{ ...pickUISettingsData(get()), ...patch }` 模式每次都做完整快照然后覆盖。当前实现是正确的，但可优化。

---

## 三、IPC 通信

### 优势
- **`get_settings_batch` 批量读取**：`useWebDAVSettings`、`translate-settings` 已使用，避免 N+1 IPC
- **窗口可见性事件分离**：`window-shown`、`window-hidden` 让前端按需响应（懒加载、暂停渲染）
- **后端 `RESUME_TX` 全局通道**：`with_paused_monitor` 用单一 mpsc 通道处理所有恢复请求，避免每次 spawn 新线程

### 问题

**1. `clipboard-updated` 触发全量刷新** 🔴 **高优先级**

```ts
// src/stores/clipboard.ts:268-275
setupListener: async () => {
  const unlisten = await listen<number>("clipboard-updated", async () => {
    playCopySound("immediate");
    await get().refresh();   // ← 重新查询整个列表
    playCopySound("after_success");
  });
  return unlisten;
}
```

后端实际上 `emit("clipboard-updated", id)` 已携带新条目 ID，但前端忽略 ID 并重新调用 `get_clipboard_items`（带 search/filter 参数）。万级数据下每次复制都触发完整查询：

- SQL 解析 + 索引扫描
- 全部行序列化为 JSON 通过 IPC
- 前端反序列化 + 虚拟列表重渲染

**优化方向**：

- 后端发送完整 `ClipboardItem` JSON（不只 ID）
- 前端检查当前过滤条件是否匹配，匹配则插入到列表头，否则忽略
- 仅在分组/搜索切换时才全量查询

**2. 全局快捷键管理状态分散** 🟡

- `CURRENT_SHORTCUT`、`CURRENT_QUICK_PASTE_SHORTCUTS`、`CURRENT_FAVORITE_PASTE_SHORTCUTS`、`QUICK_PASTE_LOCK`、`ACTIVE_QUICK_PASTE_SLOTS`、`ACTIVE_FAVORITE_PASTE_SLOTS`、`PASTE_IN_PROGRESS`、`SHORTCUTS_DISABLED` —— 8 个全局静态变量管理快捷键状态
- 测试困难，难以推理交互

建议封装成 `ShortcutManager` struct 由 `AppState` 持有。

**3. `clipboard-updated` 后端无防抖** 🟡

若用户快速复制 5 次，发出 5 个事件，前端做 5 次全量刷新。建议后端 100ms 节流。

---

## 四、性能瓶颈

### 优势
- **DB 读写分离 + WAL**：读连接 `SQLITE_OPEN_READ_ONLY | NO_MUTEX`，理论上读不阻塞写
- **PNG 编码前预估字节大小**：避免对超大截图做无意义 CPU 编码
- **`AtomicI64` 光标追踪**：高频鼠标移动事件无锁
- **`ConditionBuilder`**：动态 SQL 构建抽象，减少重复
- **`fill_files_valid` 仅浏览时执行**：搜索时跳过磁盘检查

### 问题

**1. `ConditionBuilder` 装箱热路径** 🟡

```rust
// src-tauri/src/database/repository.rs:130-140
struct ConditionBuilder {
    conditions: Vec<String>,
    params: Vec<Box<dyn rusqlite::ToSql>>,  // ← 每个参数堆分配
}
```

每次 `get_clipboard_items` 调用都做 N 次 `Box::new`。对 10K 行查询占比很小（< 1% 总耗时），但完全可避免：用枚举或 `&dyn ToSql` 引用 + `String`/`i64` 字段保持。**低优先级**。

**2. `with_paused_monitor` 强制 500ms 等待** 🟡

```rust
// src-tauri/src/commands/mod.rs:113-129
loop {
    match rx.recv_timeout(std::time::Duration::from_millis(500)) {
        Ok(monitor) => pending.push(monitor),
        Err(Timeout) => { /* 500ms 静默后才恢复 */ }
    }
}
```

意味着每次 paste 后剪贴板监控暂停至少 500ms。如果用户快速触发多次 paste，每次都会 reset 这个 500ms 窗口。

- 副作用：用户 paste 后立即手动 Ctrl+C 复制，可能被监控忽略（因为还没 resume）
- 但这是为了规避竞态（A 暂停 B 暂停 A 恢复 B 仍在运行）的合理设计

**3. `ClipboardMonitor.resume()` 在 mpsc 接收端串行执行** 🟢

单线程 batch resume 当前看不出瓶颈，但若并发暂停极多（不会发生），会串行化恢复。

**4. JSON 序列化整个 `Vec<ClipboardItem>`** 🟡

`get_clipboard_items` 默认无 limit，万条数据返回 ~5MB JSON。Tauri IPC 是字符串通道，序列化 + 反序列化为 hot path。

- 已做的优化：搜索时 `text_content` 置空
- 未做的：`html_content`、`rtf_content` 在列表展示时不需要，可在查询时排除

### 内存

**1. `ClipboardItem` 大量 `Option<String>`** 🟢

24 字段中 11 个 `Option<String>`/`Option<i64>`。每条记录 ~24 个堆指针，10K 条 = 240K 个。建议未来改用 `&str` 借用 + 自定义反序列化器，但工程量大，**当前不建议**。

**2. 后端 `Database` 频繁 clone** 🟢

`Database::clone` 仅克隆 `Arc`，廉价。但 `DB.clone() + thread::spawn(move)` 在每次手动同步、媒体上传都做。**无问题**。

**3. `saveTimersRef` Map 清理** 🟢

修复后留下的小问题：`useWebDAVSettings.ts` Map 中 timer 触发后未从 Map 删除条目。Map 最多增长到 14 个 key（设置数量），无内存泄漏。可优化但无需立即处理。

---

## 五、可观测性 / 测试

### 问题

**1. 测试覆盖近乎为零** 🔴

- Rust：`#[cfg(test)]` 仅 `clipboard/dedup.rs` 一处
- TypeScript：无测试文件

风险：复杂逻辑（quick paste 槽位管理、媒体同步、设备 ID 合并、收藏排序规整化）无回归保护。

**2. 日志层级偏粗** 🟢

广泛使用 `info!` 输出辅助信息，生产环境会产生大量日志。建议：

- 高频事件（剪贴板变化、鼠标位置）改 `debug!`
- 启动/配置变更等关键事件保留 `info!`

---

## 六、优先级总结

| 严重度 | 问题 | 位置 | 工作量 |
|--------|------|------|--------|
| 🔴 高 | `clipboard-updated` 全量刷新 | `clipboard.ts:268`、后端 emit | 中 |
| 🔴 高 | 测试覆盖近乎零 | 全项目 | 大 |
| 🟠 中 | App.tsx Zustand 多 selector | `App.tsx:75-81` 等 | 小 |
| 🟠 中 | `lib.rs` 1062 行混合关注 | `lib.rs` | 中 |
| 🟠 中 | 全局快捷键状态分散 8 个 static | `lib.rs:43-57` | 中 |
| 🟡 低 | `App.tsx` 787 行 | `App.tsx` | 中 |
| 🟡 低 | 配置加载重复 | `webdav/mod.rs`、`sync.rs` | 小 |
| 🟡 低 | `translate-settings` 缺防抖 | `translate-settings.ts` | 小 |
| 🟡 低 | `clipboard-updated` 无后端节流 | `monitor.rs` | 小 |
| 🟢 极低 | `ConditionBuilder` 装箱 | `repository.rs` | 小 |
| 🟢 极低 | `ClipboardItem` Option 字段多 | `repository.rs` | 大 |

---

## 七、关键建议（按 ROI 排序）

1. **优化 `clipboard-updated` 增量推送** —— 高频路径，万条数据下显著提升性能
2. **App.tsx 拆 3 个子组件** —— 提升可读性 + 缩小 Zustand 订阅范围
3. **`useShallow` 替换多 selector** —— 一次性改造，受益面广
4. **拆 `lib.rs` -> `paste_shortcuts.rs`** —— 8 个静态变量集中管理
5. **添加关键路径单测** —— `dedup`、`favorite_order normalize`、`build_media_map`、`extract_keyword_context` 都是纯函数，易测试

---

## 附录：本次会话已修复的 Bug

### 高优先级
- ✅ **`useWebDAVSettings.ts` 共享防抖计时器** —— 改为 per-key Map（commit 已写入）
- ✅ **`commands/sync.rs` 手动下载缺少图标** —— `spawn_media_download` 补齐 icons 处理

### 中优先级
- ✅ **运行时启用 WebDAV 插件后自动同步不生效** —— 新增 `webdav_enable_plugin` 命令 + `AUTO_SYNC_STARTED` 守卫，前端 `Settings.tsx` 启用时调用
