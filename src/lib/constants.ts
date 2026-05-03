import type { ToolbarButton } from "@/stores/ui-settings";

/** 工具栏按钮注册表 */
export const TOOLBAR_BUTTON_REGISTRY: Record<
  ToolbarButton,
  { label: string; description: string }
> = {
  clear: { label: "清空历史", description: "清空所有非置顶的历史记录" },
  pin: { label: "置顶窗口", description: "置顶窗口保持显示，再次点击可取消置顶" },
  batch: { label: "批量选择", description: "进入批量选择模式，支持 Ctrl 多选、Shift 连选，批量删除" },
  settings: { label: "设置", description: "打开设置窗口" },
};

/** 逻辑类型 → 后端 contentType 映射（需要前端二次过滤的类型） */
export const LOGICAL_TYPE_BACKEND_MAP: Record<string, { backendType: string; logicalType: string }> = {
  text:  { backendType: "text,html,rtf",  logicalType: "text" },
  url:   { backendType: "text,html,rtf",  logicalType: "url" },
  code:  { backendType: "text,html,rtf",  logicalType: "code" },
  files: { backendType: "files", logicalType: "files" },
  video: { backendType: "video", logicalType: "video" },
};

/** 内容类型分类（App 标签页和键盘导航共用） */
export const CATEGORIES = [
  { label: "全部", value: null },
  { label: "文本", value: "text" },
  { label: "图片", value: "image" },
  { label: "文件", value: "files" },
  { label: "URL", value: "url" },
  { label: "代码", value: "code" },
  { label: "视频", value: "video" },
] as const;

export type CategoryValue = (typeof CATEGORIES)[number]["value"];

/** 监听类型 → 对应的底部分类标签值（不勾选某监听类型时隐藏对应分类） */
export const MONITOR_TYPE_TO_CATEGORIES: Record<string, string[]> = {
  text: ["text", "url", "code"],
  image: ["image"],
  files: ["files"],
  video: ["video"],
};

/** 根据启用的监听类型，计算应该显示的分类标签列表 */
export function getVisibleCategories(enabledMonitorTypes: string[]): typeof CATEGORIES[number][] {
  const hiddenValues = new Set<string>();
  for (const [monitorType, categoryValues] of Object.entries(MONITOR_TYPE_TO_CATEGORIES)) {
    if (!enabledMonitorTypes.includes(monitorType)) {
      categoryValues.forEach((v) => hiddenValues.add(v));
    }
  }
  return CATEGORIES.filter((c) => c.value === null || !hiddenValues.has(c.value));
}
