import { memo, useState } from "react";
import {
  Pin16Regular,
  Pin16Filled,
  Star16Regular,
  Star16Filled,
  Delete16Regular,
  Copy16Regular,
  Document16Regular,
  Folder16Regular,
  Warning16Regular,
  ChevronDown16Regular,
  Tag16Regular,
  Translate16Regular,
} from "@fluentui/react-icons";
import { Button } from "@/components/ui/button";
import {
  ContextMenuItem,
  ContextMenuSeparator,
} from "@/components/ui/context-menu";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import type { ClipboardItem } from "@/stores/clipboard";

// ============ 类型定义 ============

export interface FileListItem {
  name: string;
  path: string;
  isDir: boolean;
  exists: boolean;
}

export interface ContextMenuItemConfig {
  icon: React.ComponentType<{ className?: string }>;
  label: string;
  onClick: () => void;
  disabled?: boolean;
  destructive?: boolean;
  separator?: boolean;
}

// ============ 文件详情对话框 ============

interface FileDetailsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  fileListItems: FileListItem[];
}

export const FileDetailsDialog = ({
  open,
  onOpenChange,
  fileListItems,
}: FileDetailsDialogProps) => (
  <Dialog open={open} onOpenChange={onOpenChange}>
    <DialogContent className="sm:max-w-lg max-h-[70vh]">
      <DialogHeader>
        <DialogTitle className="flex items-center gap-2">
          {fileListItems.length > 1 ? (
            <Folder16Regular className="h-5 w-5" />
          ) : (
            <Document16Regular className="h-5 w-5" />
          )}
          已复制的文件 ({fileListItems.length})
        </DialogTitle>
      </DialogHeader>
      <div className="space-y-2 max-h-[50vh] overflow-y-auto pr-2">
        {fileListItems.map((file, idx) => (
          <div
            key={idx}
            className={cn(
              "flex items-start gap-3 p-2 rounded-md border",
              file.exists
                ? "bg-muted/30"
                : "bg-red-50 dark:bg-red-950/30 border-red-200 dark:border-red-900",
            )}
          >
            <div className="shrink-0 mt-0.5">
              {!file.exists ? (
                <Warning16Regular className="h-4 w-4 text-red-500" />
              ) : file.isDir ? (
                <Folder16Regular className="h-4 w-4 text-blue-500" />
              ) : (
                <Document16Regular className="h-4 w-4 text-blue-500" />
              )}
            </div>
            <div className="flex-1 min-w-0">
              <p
                className={cn(
                  "text-sm font-medium truncate",
                  !file.exists && "text-red-500",
                )}
              >
                {file.name}
                {!file.exists && (
                  <span className="ml-1 text-xs font-normal">(已失效)</span>
                )}
              </p>
              <p className="text-xs text-muted-foreground truncate mt-0.5">
                {file.path}
              </p>
            </div>
          </div>
        ))}
      </div>
      {fileListItems.some((f) => !f.exists) && (
        <p className="text-xs text-red-500 mt-2">
          部分文件已被移动或删除，无法粘贴
        </p>
      )}
    </DialogContent>
  </Dialog>
);

// ============ 标签管理（内联折叠） ============

export function TagAssignSection({
  itemId,
  allTags,
  itemTagIds,
  onAddTag,
  onRemoveTag,
}: {
  itemId: number;
  allTags: { id: number; name: string }[];
  itemTagIds: Set<number>;
  onAddTag: (itemId: number, tagId: number) => Promise<void>;
  onRemoveTag: (itemId: number, tagId: number) => Promise<void>;
}) {
  const [expanded, setExpanded] = useState(false);
  if (allTags.length === 0) return null;
  return (
    <>
      <ContextMenuSeparator />
      <div
        role="menuitem"
        onClick={(e) => { e.preventDefault(); e.stopPropagation(); setExpanded((v) => !v); }}
        className="flex cursor-default select-none items-center rounded-sm px-2 py-1.5 text-sm outline-none focus:bg-accent focus:text-accent-foreground hover:bg-accent hover:text-accent-foreground"
      >
        <span>标签</span>
        <ChevronDown16Regular
          className={cn("ml-auto h-4 w-4 transition-transform duration-150", expanded && "rotate-180")}
        />
      </div>
      {expanded && (
        <>
          {allTags.map((t) => {
            const isAssigned = itemTagIds.has(t.id);
            return (
              <ContextMenuItem
                className="pl-6"
                key={t.id}
                onClick={() => isAssigned ? onRemoveTag(itemId, t.id) : onAddTag(itemId, t.id)}
              >
                <span className={cn("mr-2", isAssigned ? "text-primary" : "text-muted-foreground")}>
                  {isAssigned ? "✓" : " "}
                </span>
                {t.name}
              </ContextMenuItem>
            );
          })}
        </>
      )}
    </>
  );
}

// ============ 操作工具栏 ============

interface ActionToolbarProps {
  item: ClipboardItem;
  onTogglePin: (e: React.MouseEvent) => void;
  onToggleFavorite: (e: React.MouseEvent) => void;
  onCopy: (e: React.MouseEvent) => void;
  onDelete: (e: React.MouseEvent) => void;
  onTag?: (e: React.MouseEvent) => void;
  onTranslate?: (e: React.MouseEvent) => void;
}

export const ActionToolbar = memo(function ActionToolbar({
  item,
  onTogglePin,
  onToggleFavorite,
  onCopy,
  onDelete,
  onTag,
  onTranslate,
}: ActionToolbarProps) { return (
  <div
    className="absolute right-1 top-1 z-20 flex items-center gap-0.5 bg-background/95 rounded-md px-0.5 shadow-sm border opacity-0 group-hover:opacity-100 transition-opacity"
    data-drag-ignore="true"
    data-action-toolbar
  >
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant="ghost"
          size="icon"
          onClick={onTogglePin}
          className="h-6 w-6"
        >
          {item.is_pinned ? (
            <Pin16Filled className="w-3.5 h-3.5 text-primary" />
          ) : (
            <Pin16Regular className="w-3.5 h-3.5" />
          )}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{item.is_pinned ? "取消置顶" : "置顶"}</TooltipContent>
    </Tooltip>
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant="ghost"
          size="icon"
          onClick={onToggleFavorite}
          className="h-6 w-6"
        >
          {item.is_favorite ? (
            <Star16Filled className="w-3.5 h-3.5 text-yellow-500" />
          ) : (
            <Star16Regular className="w-3.5 h-3.5" />
          )}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{item.is_favorite ? "取消收藏" : "收藏"}</TooltipContent>
    </Tooltip>
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant="ghost"
          size="icon"
          onClick={onCopy}
          className="h-6 w-6"
        >
          <Copy16Regular className="w-3.5 h-3.5" />
        </Button>
      </TooltipTrigger>
      <TooltipContent>复制</TooltipContent>
    </Tooltip>
    {onTranslate && (
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant="ghost"
            size="icon"
            onClick={onTranslate}
            className="h-6 w-6"
          >
            <Translate16Regular className="w-3.5 h-3.5" />
          </Button>
        </TooltipTrigger>
        <TooltipContent>翻译</TooltipContent>
      </Tooltip>
    )}
    {onTag && (
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant="ghost"
            size="icon"
            onClick={onTag}
            className="h-6 w-6"
          >
            <Tag16Regular className="w-3.5 h-3.5" />
          </Button>
        </TooltipTrigger>
        <TooltipContent>标签</TooltipContent>
      </Tooltip>
    )}
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant="ghost"
          size="icon"
          onClick={onDelete}
          className="h-6 w-6 hover:text-destructive"
        >
          <Delete16Regular className="w-3.5 h-3.5" />
        </Button>
      </TooltipTrigger>
      <TooltipContent>删除</TooltipContent>
    </Tooltip>
  </div>
); });
