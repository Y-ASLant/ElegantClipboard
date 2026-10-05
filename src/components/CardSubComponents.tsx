import { useState } from "react";
import {
  Pin16Regular,
  Pin16Filled,
  Star16Regular,
  Star16Filled,
  Delete16Regular,
  Copy16Regular,
  Translate16Regular,
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
import { useTranslation } from "@/i18n";
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
}: FileDetailsDialogProps) => {
  const { t } = useTranslation();
  return (
  <Dialog open={open} onOpenChange={onOpenChange}>
    <DialogContent className="sm:max-w-lg max-h-[70vh]">
      <DialogHeader>
        <DialogTitle className="flex items-center gap-2">
          {fileListItems.length > 1 ? (
            <Folder16Regular className="h-5 w-5" />
          ) : (
            <Document16Regular className="h-5 w-5" />
          )}
          {t("clipboard.fileDetailsTitle", { count: fileListItems.length })}
        </DialogTitle>
      </DialogHeader>
      <div className="space-y-2 max-h-[50vh] overflow-y-auto pr-2">
        {fileListItems.map((file, idx) => (
          <div
            key={idx}
            className={cn(
              "flex items-start gap-3 p-2 rounded-md border",
              file.exists
                ? "bg-muted-surface-faint"
                : "bg-destructive-subtle border-destructive-subtle",
            )}
          >
            <div className="shrink-0 mt-0.5">
              {!file.exists ? (
                <Warning16Regular className="h-4 w-4 text-destructive" />
              ) : file.isDir ? (
                <Folder16Regular className="h-4 w-4 text-primary" />
              ) : (
                <Document16Regular className="h-4 w-4 text-primary" />
              )}
            </div>
            <div className="flex-1 min-w-0">
              <p
                className={cn(
                  "text-sm font-medium truncate",
                  !file.exists && "text-destructive",
                )}
              >
                {file.name}
                {!file.exists && (
                  <span className="ml-1 text-xs font-normal">{t("clipboard.fileInvalid")}</span>
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
        <p className="text-xs text-destructive mt-2">
          {t("clipboard.filePartialInvalid")}
        </p>
      )}
    </DialogContent>
  </Dialog>
  );
};

// ============ 移动到分组（内联折叠） ============

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
  const { t } = useTranslation();
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
        className="flex cursor-default select-none items-center rounded-md px-2 py-1.5 text-sm outline-none focus:bg-accent focus:text-accent-foreground hover:bg-accent hover:text-accent-foreground"
      >
        <span>{t("groups.moveToGroup")}</span>
        <ChevronDown16Regular
          className={cn("ml-auto h-4 w-4 transition-transform duration-150", expanded && "rotate-180")}
        />
      </div>
      {expanded && (
        <>
          {showDefault && (
            <ContextMenuItem className="pl-6" onClick={() => moveItemToGroup(itemId, null)}>
              {t("groups.defaultGroupFull")}
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

// ============ 操作工具栏 ============

interface ActionToolbarProps {
  item: ClipboardItem;
  copyDisabled: boolean;
  onTogglePin: (e: React.MouseEvent) => void;
  onToggleFavorite: (e: React.MouseEvent) => void;
  onCopy: (e: React.MouseEvent) => void;
  onDelete: (e: React.MouseEvent) => void;
  onTranslate?: (e: React.MouseEvent) => void;
  translateActive?: boolean;
}

export const ActionToolbar = ({
  item,
  copyDisabled,
  onTogglePin,
  onToggleFavorite,
  onCopy,
  onDelete,
  onTranslate,
  translateActive,
}: ActionToolbarProps) => {
  const { t } = useTranslation();
  return (
  <div
    className="absolute right-1 top-1 z-20 flex items-center gap-0.5 surface-glass rounded-md p-0.5 elevation-floating border opacity-0 group-hover:opacity-100 transition-surface"
    data-drag-ignore="true"
  >
    <Tooltip>
      <TooltipTrigger asChild>
        <Button variant="ghost" size="icon" onClick={onTogglePin} className="h-7 w-7">
          {item.is_pinned ? (
            <Pin16Filled className="w-3.5 h-3.5 text-primary" />
          ) : (
            <Pin16Regular className="w-3.5 h-3.5" />
          )}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{item.is_pinned ? t("clipboard.unpin") : t("clipboard.pin")}</TooltipContent>
    </Tooltip>
    <Tooltip>
      <TooltipTrigger asChild>
        <Button variant="ghost" size="icon" onClick={onToggleFavorite} className="h-7 w-7">
          {item.is_favorite ? (
            <Star16Filled className="w-3.5 h-3.5 text-chart-4" />
          ) : (
            <Star16Regular className="w-3.5 h-3.5" />
          )}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{item.is_favorite ? t("clipboard.unfavorite") : t("clipboard.favorite")}</TooltipContent>
    </Tooltip>
    <Tooltip>
      <TooltipTrigger asChild>
        <Button variant="ghost" size="icon" onClick={onCopy} disabled={copyDisabled} aria-label={t("clipboard.copy")} className="h-7 w-7">
          <Copy16Regular className="w-3.5 h-3.5" />
        </Button>
      </TooltipTrigger>
      <TooltipContent>{t("clipboard.copy")}</TooltipContent>
    </Tooltip>
    {onTranslate && (
      <Tooltip>
        <TooltipTrigger asChild>
          <Button variant="ghost" size="icon" onClick={onTranslate} className="h-7 w-7">
            <Translate16Regular className={`w-3.5 h-3.5 ${translateActive ? "text-primary" : ""}`} />
          </Button>
        </TooltipTrigger>
        <TooltipContent>{translateActive ? t("clipboard.collapseTranslate") : t("clipboard.translate")}</TooltipContent>
      </Tooltip>
    )}
    <Tooltip>
      <TooltipTrigger asChild>
        <Button variant="ghost" size="icon" onClick={onDelete} className="h-7 w-7 hover:text-destructive">
          <Delete16Regular className="w-3.5 h-3.5" />
        </Button>
      </TooltipTrigger>
      <TooltipContent>{t("common.delete")}</TooltipContent>
    </Tooltip>
  </div>
  );
};
