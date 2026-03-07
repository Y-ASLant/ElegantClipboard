import { useState } from "react";
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

// ============ Types ============

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

// ============ File Details Dialog ============

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

// ============ Move to Group (inline collapsible) ============

export function MoveToGroupSection({
  itemId,
  groups,
  selectedGroupId,
  moveItemToGroup,
}: {
  itemId: number;
  groups: { id: number; name: string }[];
  selectedGroupId: number | null;
  moveItemToGroup: (itemId: number, groupId: number | null) => Promise<void>;
}) {
  const [expanded, setExpanded] = useState(false);
  // 当前在默认分组：显示所有自定义分组；当前在自定义分组：显示默认 + 其他自定义分组
  const otherGroups = groups.filter((g) => g.id !== selectedGroupId);
  const showDefault = selectedGroupId !== null;
  if (!showDefault && otherGroups.length === 0) return null;
  return (
    <>
      <ContextMenuSeparator />
      <div
        role="menuitem"
        onClick={(e) => { e.preventDefault(); e.stopPropagation(); setExpanded((v) => !v); }}
        className="flex cursor-default select-none items-center rounded-sm px-2 py-1.5 text-sm outline-none focus:bg-accent focus:text-accent-foreground hover:bg-accent hover:text-accent-foreground"
      >
        <span>移动到分组</span>
        <ChevronDown16Regular
          className={cn("ml-auto h-4 w-4 transition-transform duration-150", expanded && "rotate-180")}
        />
      </div>
      {expanded && (
        <>
          {showDefault && (
            <ContextMenuItem className="pl-6" onClick={() => moveItemToGroup(itemId, null)}>
              默认分组
            </ContextMenuItem>
          )}
          {otherGroups.map((g) => (
            <ContextMenuItem className="pl-6" key={g.id} onClick={() => moveItemToGroup(itemId, g.id)}>
              {g.name}
            </ContextMenuItem>
          ))}
        </>
      )}
    </>
  );
}

// ============ Action Toolbar ============

interface ActionToolbarProps {
  item: ClipboardItem;
  onTogglePin: (e: React.MouseEvent) => void;
  onToggleFavorite: (e: React.MouseEvent) => void;
  onCopy: (e: React.MouseEvent) => void;
  onDelete: (e: React.MouseEvent) => void;
}

export const ActionToolbar = ({
  item,
  onTogglePin,
  onToggleFavorite,
  onCopy,
  onDelete,
}: ActionToolbarProps) => (
  <div
    className="absolute right-1 top-1 z-20 flex items-center gap-0.5 bg-background/95 rounded-md px-0.5 shadow-sm border opacity-0 group-hover:opacity-100 transition-opacity"
    data-drag-ignore="true"
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
);
