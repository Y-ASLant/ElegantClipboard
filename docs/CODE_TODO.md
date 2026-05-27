# 代码质量待修复清单

> 验证日期：2026-05-27
> 验证方式：逐项 grep/read 确认，非 LLM 猜测

---

## P0 - 维护风险高（改一处漏一处）

### 1. `KEY_CODE_MAP` 重复定义
- **文件 A**: `src/components/settings/ShortcutsTab.tsx:37-53`
- **文件 B**: `src/components/settings/TranslateTab.tsx:13-19`
- **状态**: ✅ 已验证，内容完全相同
- **修复**: 提取到 `src/lib/shortcut-helpers.ts`

### 2. Lease 管理函数重复
- **文件 A**: `src/components/ClipboardItemCard.tsx:82-94`（textPreviewLease）
- **文件 B**: `src/components/CardContentRenderers.tsx:81-94`（imagePreviewLease）
- **状态**: ✅ 已验证，结构完全相同，仅变量名和 invoke 命令不同
- **修复**: 提取 `createLeaseManager()` 工厂函数

### 3. `WS_EX_LAYERED` 窗口特效代码重复 3 处
- **文件 A**: `src-tauri/src/commands/window.rs:249-319`
- **文件 B**: `src-tauri/src/commands/preview.rs:386-405`
- **文件 C**: `src-tauri/src/lib.rs:808-842`
- **状态**: ✅ 已验证，三处都是 GetWindowLongW → 操作 WS_EX_LAYERED → SetWindowLongW → SetWindowPos
- **修复**: 提取到共享工具函数（如 `window_utils.rs`）

### 4. 翻译模块 6 个函数共享相同 HTTP 响应处理模式
- **文件**: `src-tauri/src/commands/translate.rs`
- **重复模式**: `resp.status()` → `resp.text()` → `if !status.is_success()` → `serde_json::from_str`
- **出现次数**: 6 次（行号 58、95、133、180、242、311）
- **状态**: ✅ 已验证
- **修复**: 提取 `fn parse_translation_response(resp, provider_name) -> Result<Value, String>`

---

## P1 - 可维护性（文件/函数过大）

### 5. 超大文件（>600 行）

| 文件 | 行数 | 阈值 |
|------|------|------|
| `src-tauri/src/database/repository.rs` | 1384 | 超标 2.3x |
| `src-tauri/src/webdav/mod.rs` | 1349 | 超标 2.2x |
| `src-tauri/src/lib.rs` | 1061 | 超标 1.8x |
| `src/components/ClipboardItemCard.tsx` | 881 | 超标 1.5x |
| `src/components/CardContentRenderers.tsx` | 843 | 超标 1.4x |
| `src/components/settings/DataTab.tsx` | 843 | 超标 1.4x |
| `src/components/settings/ShortcutsTab.tsx` | 820 | 超标 1.4x |
| `src/App.tsx` | 786 | 超标 1.3x |
| `src-tauri/src/commands/translate.rs` | 768 | 超标 1.3x |
| `src-tauri/src/commands/clipboard.rs` | 764 | 超标 1.3x |
| `src-tauri/src/clipboard/handler.rs` | 678 | 超标 1.1x |

### 6. `lib.rs::run()` 函数过长
- **位置**: `src-tauri/src/lib.rs:631-940`（约 310 行）
- **状态**: ✅ 已验证，包含初始化、快捷键注册、窗口管理、WebDAV 启动、更新检查等全部逻辑
- **修复**: 拆分为 `init_plugins()`、`init_shortcuts()`、`init_window()`、`init_background_tasks()`

### 7. 空 catch 块（静默吞错）
- **TranslateTab.tsx:129** — `try { await invoke("update_translate_selection_shortcut", ...) } catch {}`
- **TranslateTab.tsx:131** — 同上
- **TranslateTab.tsx:153** — 同上
- **TranslateTab.tsx:155** — 同上
- **TranslateResult.tsx:72** — `} catch {}`
- **状态**: ✅ 已验证，共 5 处
- **修复**: 至少添加 `console.error` 或 toast 提示

### 8. `format_size` 函数重复
- **文件 A**: `src-tauri/src/commands/sync.rs:367-375`
- **文件 B**: `src-tauri/src/commands/data_transfer.rs:10-18`
- **状态**: ✅ 已验证，实现完全相同（B/KB/MB 格式化）
- **修复**: 提取到 `src-tauri/src/utils.rs` 或类似共享模块

---

## P2 - 值得优化

### 9. 内置 MD5 实现（58 行）
- **位置**: `src-tauri/src/commands/translate.rs:337-394`
- **状态**: ✅ 已验证，函数名 `md5_hash`
- **修复**: 使用 `md5` crate 替代

### 10. 自定义 URL 编码
- **位置**: `src-tauri/src/commands/translate.rs:324-335`
- **状态**: ✅ 已验证，函数名 `urlencoded`
- **修复**: 使用 `urlencoding` crate 替代

### 11. 死代码：`get_invalid_file_paths_set`
- **位置**: `src-tauri/src/database/repository.rs:1065-1067`
- **调用点**: `commands/sync.rs`、`webdav/mod.rs`
- **状态**: ✅ 已验证，注释明确写"始终返回空集"
- **修复**: 删除函数及所有调用点

### 12. 错误消息中英文混用
- **状态**: ✅ 已验证
- **示例**: `"Item not found"` vs `"收藏槽位 {} 没有可用的收藏条目"`、`"导出成功 ({})"` vs `"Failed to access clipboard: {}"`
- **修复**: 统一为中文（面向中文用户）

### 13. `DataTab.tsx` 状态过多（16 个 useState）
- **位置**: `src/components/settings/DataTab.tsx:202-223`
- **修复**: 将相关状态合并，或提取为自定义 hooks

### 14. WebDAV 配置加载逻辑重复
- **文件 A**: `src-tauri/src/commands/sync.rs:9-56`（`load_webdav_config`）
- **文件 B**: `src-tauri/src/webdav/mod.rs:942-1013`（`load_config_and_options`）
- **修复**: 统一为一个配置加载入口

---

## P3 - 小改善

### 15. `DataTab.tsx` 格式化逻辑重复
- **位置**: `src/components/settings/DataTab.tsx:662-666` 和 `686-690`
- **模式**: `max_content_size_kb === 0 ? "无限制" : >= 1024 ? "MB" : "KB"` 出现两次
- **修复**: 提取 `formatKB(kb: number): string`

### 16. `ClipboardList.tsx` 嵌套三元表达式
- **位置**: `src/components/ClipboardList.tsx:458`
- **代码**: `cardDensity === "compact" ? "pb-1" : cardDensity === "spacious" ? "pb-3" : "pb-2"`
- **修复**: 改为查找表 `DENSITY_PADDING[cardDensity]`

### 17. `CardContentRenderers.tsx` 三层嵌套三元
- **位置**: `src/components/CardContentRenderers.tsx:181-186`
- **修复**: 改为 if/else 或独立函数

---

## 统计

| 优先级 | 数量 | 类型 |
|--------|------|------|
| P0 | 4 | 重复代码（维护风险） |
| P1 | 4 | 文件/函数过大 + 静默错误 |
| P2 | 6 | 冗余代码 + 不一致 |
| P3 | 3 | 小改善 |
| **合计** | **17** | |
