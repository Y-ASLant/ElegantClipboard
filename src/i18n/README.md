# i18n 模块

前端界面国际化。默认 **简体中文**，另支持 **English**、**繁體中文**。

## 目录结构

```
src/i18n/
  index.ts              # 对外 API（组件 import 此入口）
  runtime.ts            # Zustand store、useTranslation、t、initLocale
  types.ts              # Locale、TranslationTree
  core/
    merge.ts            # 深度合并 locale 树
    translator.ts         # createTranslator、LOCALE_OPTIONS、DEFAULT_LOCALE
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
| 多窗口同步 | Tauri 事件 `locale-changed` |
| 启动 | `main.tsx` 中 `await initLocale()` 后再 `createRoot().render()` |
| 设置 UI | `GeneralTab` → 界面语言下拉 |

## 新增文案

1. 按功能分区选择文件：
   - 主窗口 / 分组 / 工具栏 → `messages/{locale}/core.ts`
   - 设置页 / 卡片 / 对话框 → `locales/{locale}-ext.ts`
2. **三语同步**：`zh-CN`、`en`、`zh-TW` 各加相同 key
3. 组件中使用 `t("section.key")`，禁止硬编码用户可见字符串
4. key 命名：`settings.data.migrateHint` 形式，camelCase 末段

## 测试

- `src/test/setup.ts`：每个用例前 `useLocaleStore.setState({ locale: "zh-CN" })`
- 断言用 `t("key")` 或 `t("key", { param })`，勿写死某一语言的字符串
- 运行：`make test` 或 `npx vitest run`

## 未国际化

- 部分后端错误信息 / 日志（仅开发者可见）
- 原生窗口标题（翻译窗口、文本编辑器等）仍由 Rust 侧硬编码，与 React 内文案可能不一致

## 托盘菜单

Rust 原生托盘菜单使用 `src-tauri/src/tray/tray_i18n.rs` 独立翻译表（zh-CN / en / zh-TW），与 `settings.language` 联动：

- 启动时从数据库读取语言
- 设置页切换语言 → `update_tray_language` command
- 多窗口 `locale-changed` 事件同步托盘文案

## 添加新语言

1. 新建 `messages/{code}/core.ts`、`extended.ts`、`index.ts`
2. 在 `core/translator.ts` 的 `LOCALES`、`LOCALE_OPTIONS` 注册
3. 更新 `types.ts` 的 `Locale` 联合类型
4. 在 `GeneralTab` 语言下拉与 `language.*` 文案中增加选项
