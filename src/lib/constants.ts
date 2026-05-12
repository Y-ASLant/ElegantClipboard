import { translate } from "@/i18n";
import type { ToolbarButton } from "@/stores/ui-settings";

/** 工具栏按钮注册表 */
export function getToolbarButtonRegistry(): Record<
  ToolbarButton,
  { label: string; description: string }
> {
  return {
    clear: {
      label: translate("清空历史"),
      description: translate("清空所有非置顶的历史记录"),
    },
    pin: {
      label: translate("锁定窗口"),
      description: translate("锁定窗口防止自动隐藏"),
    },
    batch: {
      label: translate("批量选择"),
      description: translate("进入批量选择模式，支持 Ctrl 多选、Shift 连选，批量删除"),
    },
    settings: {
      label: translate("设置"),
      description: translate("打开设置窗口"),
    },
  };
}

/** 分类分组（App 标签页和键盘导航共用） */
export function getGroups() {
  return [
    { label: translate("全部"), value: null },
    { label: translate("收藏"), value: "__favorites__" },
    { label: translate("文本"), value: "text,html,rtf" },
    { label: translate("其它"), value: "image,files" },
  ] as const;
}

export type GroupValue = ReturnType<typeof getGroups>[number]["value"];
