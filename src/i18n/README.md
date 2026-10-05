# i18n 模块

前端界面国际化。默认 **简体中文**，另支持 **English**、**繁體中文**。

## 目录结构

```
src/i18n/
  index.ts              # 对外 API（组件 import 此入口）
  runtime.ts            # Zustand store、useSyncExternalStore Hook、t、initLocale
  types.ts              # Locale、TranslationTree
  core/
    merge.ts            # 深度合并 locale 树
    translator.ts       # createTranslator、LOCALE_OPTIONS、DEFAULT_LOCALE
  messages/
    zh-CN/
      core.ts           # 基础文案（app、toolbar、groups、onboarding…）
      extended.ts       # re-export locales/zh-CN-ext.ts
      index.ts          # mergeDeep(core, extended) → export zhCN
    en/
    zh-TW/
  locales/
    zh-CN-ext.ts        # 扩展文案源（settings.*、卡片 UI 等）
    en-ext.ts
    zh-TW-ext.ts
    zh-CN.ts / en.ts / zh-TW.ts   # 兼容 re-export（deprecated）
```

## 使用

```tsx
import { useTranslation } from "@/i18n";

function MyComponent() {
  const { t, locale, setLocale } = useTranslation();
  return <span>{t("app.searchPlaceholder")}</span>;
}
```

带插值：

```tsx
t("app.batchSelected", { count: 3 })
```

非 React 模块（如 `constants.ts`、`format.ts`）：

```ts
import { t } from "@/i18n";
```

## 运行时

| 项目 | 说明 |
|------|------|
| 默认语言 | `zh-CN` |
| 持久化 | 数据库 `settings.language` |
| 多窗口同步 | `initLocale()` 先注册 `locale-changed` 监听，再读取数据库；切换成功后广播事件 |
| 主窗口启动 | `main.tsx` 并行初始化语言与 UI 设置，不等待完成即渲染；初始使用默认语言，读取配置后更新 |
| 设置窗口启动 | 独立入口 `settings-main.tsx` 等待语言、UI 设置与主题初始化后渲染 |
| 设置 UI | `GeneralTab` → 界面语言下拉 |

`setLocale()` 立即更新 store 和 `<html lang>`，保存 `settings.language` 后尝试更新托盘并广播 `locale-changed`；保存或广播失败时恢复本窗口原语言和文档语言，随后重新抛出原错误。这个回滚只作用于本窗口，不是数据库与多窗口广播的事务回滚；托盘更新失败不阻断切换。事件接收方更新 store、文档语言并尝试更新托盘，不再次写库或广播。读取失败、未知语言值均回退到 `zh-CN`。

`GeneralTab` 是当前唯一发起语言切换的 UI 调用方：捕获 `setLocale()` 的 rejection，再调用 `reportUserError(t("operationFeedback.userActions.saveSettings"), error, diagnosticContext)`。运行时 store 不弹提示，避免同一次失败由两层重复报告。

`t()` 对缺失文案先回退到简体中文，再返回 key；开发环境对仍缺失的 key 发出警告。插值使用 `{{name}}`，未匹配的占位符会被清空。

## 操作反馈与错误归属

三种语言的 `locales/{locale}-ext.ts` 均包含 `operationFeedback`：

| Key | 用途 |
|-----|------|
| `operations.{operation}` | 复制、粘贴、另存为、删除等操作的失败标题；名称对应 `src/lib/operation-feedback.ts` 的 `ClipboardOperation` |
| `reasons.{code}` | 后端 `OperationError` 的受控原因；保留原生 snake_case code，如 `resource_missing`、`clipboard_unavailable` |
| `unknownReason` | 不认识的 code、旧字符串错误或其它非结构化错误的安全兜底 |
| `userActions.{action}` | 设置保存、打开链接等非剪贴板操作的本地化失败标题 |
| `copySucceeded` / `pasteSucceeded` | 已确认成功后的提示 |
| `refreshAfterSuccess` | 操作已经完成，但后续条目列表刷新失败；不能把主操作改为失败 |

`getOperationErrorMessage()` 只把有效的 `{ code, detail }` 中的已知 code 映射为本地化原因；`detail`、原始异常文本、路径、凭据等诊断内容不进入 toast。未知错误使用 `unknownReason`，完整错误仅留在开发者日志。

例如，Windows 的 HGLOBAL 发布路径不能发布零字节原始格式；预检在修改剪贴板前拒绝这些内容（包括零字节虚拟文件 `FileContents`），使用 `unsupported_content` 映射为 `operationFeedback.reasons.unsupported_content`。分叉的 `ClipboardFormatUnsupportedError` 也映射到同一原生 code，而不是 `clipboard_unavailable`。用户只看到受控原因，具体 HGLOBAL 限制留在诊断日志；不填充字节或报告成功但实际未写入。这里描述实现契约，不宣称已完成非空写入/空数据拒绝的最终原生验证。

- `logError()` 是纯 `console.error` 诊断工具，不隐式弹提示；后台读取、监听与缓存失败只记录诊断。
- 用户操作的显式报告者使用 `reportUserError()` 或 `reportOperationError()`，同时记录诊断并显示安全的本地化提示。已有行内错误状态的组件使用 `getOperationErrorMessage()` 展示原因，并只调用 `logError()`，不再额外弹同一错误。
- `runClipboardOperation()` 返回 `{ status: "success", value }`、`{ status: "cancelled" }` 或 `{ status: "failed" }`。失败由该函数报告一次；调用方不得重复报告。`save_file_as` 返回 `false` 表示取消，不显示成功或错误提示。
- 只有 `success` 才触发成功动画、复制标志、选择清理和成功后的排序；后续刷新失败使用单独的 `refreshAfterSuccess` 提示，不改变已完成的主操作结果。

新增操作或错误 code 时，应同步更新此 namespace 的三语 key 和 `operation-feedback.ts` 的映射，而不是把 Rust `detail` 直接翻译或拼进用户提示。

## 新增文案

1. 按功能分区选择文件：
   - 主窗口 / 分组 / 工具栏 → `messages/{locale}/core.ts`
   - 设置页 / 卡片 / 对话框 → `locales/{locale}-ext.ts`
2. **三语同步**：`zh-CN`、`en`、`zh-TW` 各加相同 key
3. 组件中使用 `t("section.key")`，禁止硬编码用户可见字符串
4. key 命名：`settings.data.migrateHint` 形式，camelCase 末段

## 测试

- `src/test/setup.ts`：每个用例前 `useLocaleStore.setState({ locale: "zh-CN", loaded: true })`
- 组件测试断言用 `t("key")` 或 `t("key", { param })`，勿写死某一语言的字符串；验证翻译本身的 `i18n.test.ts` 则断言各语言的具体文案
- 运行前端测试：`npm test`；仅运行本模块：`npm test -- src/i18n/i18n.test.ts`
- 反馈边界与取消/失败结果见 `src/lib/logger.test.ts`；语言保存失败的回滚与 rejection 见 `src/i18n/i18n.test.ts`。可用 `npm test -- src/lib/logger.test.ts src/i18n/i18n.test.ts` 运行这两组测试。

## 未国际化

- 部分原生日志、其它后端功能的错误和系统通知仍有固定语言；剪贴板操作的结构化错误通过上述受控映射展示，不能用这个限制作为暴露原始 `detail` 的理由
- Rust 原生文件对话框中，选择数据目录、导入、导出标题及部分过滤器文案仍硬编码中文。`save_file_as` 的另存为标题已按数据库 `settings.language` 选择简体中文、English、繁體中文，未知语言回退简体中文

## 托盘菜单

Rust 原生托盘菜单使用 `src-tauri/src/tray/tray_i18n.rs` 独立翻译表（zh-CN / en / zh-TW），与 `settings.language` 联动：

- 启动时从数据库读取语言
- `runtime.ts` 在读取配置、切换语言以及收到同步事件时调用 `update_tray_language` command
- `locale-changed` 是前端多窗口事件；Rust 托盘通过上述命令更新，不直接监听该事件

## 添加新语言

1. 新建 `messages/{code}/core.ts`、`extended.ts`、`index.ts`
2. 在 `core/translator.ts` 导入新文案树，更新 `LOCALES`、`LOCALE_OPTIONS` 及其 `labelKey` 类型，并让 `normalizeLocale()` 接受新代码
3. 更新 `types.ts` 的 `Locale` 联合类型，为所有语言树增加对应的 `language.*` 选项文案；`GeneralTab` 从 `LOCALE_OPTIONS` 自动生成下拉项
4. 在 `src-tauri/src/tray/tray_i18n.rs` 添加原生托盘翻译；未知语言的托盘文案仍回退到简体中文
5. 增加语言切换与插值测试，保持各语言 key 一致
