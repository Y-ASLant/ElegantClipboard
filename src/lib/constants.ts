import type { ToolbarButton } from "@/stores/ui-settings";

/** Toolbar button registry — defines all available toolbar buttons */
export const TOOLBAR_BUTTON_REGISTRY: Record<
  ToolbarButton,
  { label: string; description: string }
> = {
  clear: { label: "清空历史", description: "清空所有非置顶的历史记录" },
  pin: { label: "锁定窗口", description: "锁定窗口防止自动隐藏" },
  settings: { label: "设置", description: "打开设置窗口" },
};

/** Category groups — shared between App segment tabs and ClipboardList keyboard navigation */
export const GROUPS = [
  { label: "全部", value: null },
  { label: "收藏", value: "__favorites__" },
  { label: "文本", value: "text,html,rtf" },
  { label: "图片", value: "image" },
  { label: "文件", value: "files" },
] as const;

export type GroupValue = (typeof GROUPS)[number]["value"];
