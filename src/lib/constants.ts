/** Category groups — shared between App segment tabs and ClipboardList keyboard navigation */
export const GROUPS = [
  { label: "全部", value: null },
  { label: "收藏", value: "__favorites__" },
  { label: "文本", value: "text,html,rtf" },
  { label: "图片", value: "image" },
  { label: "文件", value: "files" },
] as const;

export type GroupValue = (typeof GROUPS)[number]["value"];
