// 剪贴板卡片上下文菜单组件（从 ClipboardItemCard.tsx 提取）

import { Fragment } from "react";
import {
  TagAssignSection,
  FileDetailsDialog,
  type FileListItem,
  type ContextMenuItemConfig,
} from "@/components/CardSubComponents";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { useTagStore } from "@/stores/tags";

interface ClipboardContextMenuProps {
  children: React.ReactNode;
  contextMenuItems: ContextMenuItemConfig[];
  itemId: number;
  contentType: string;
  localTags: { id: number; name: string }[];
  itemTagIds: Set<number>;
  setLocalTags: (tags: { id: number; name: string }[]) => void;
  setItemTagIds: React.Dispatch<React.SetStateAction<Set<number>>>;
  detailsOpen: boolean;
  setDetailsOpen: (open: boolean) => void;
  fileListItems: FileListItem[];
}

export function ClipboardContextMenu({
  children,
  contextMenuItems,
  itemId,
  contentType,
  localTags,
  itemTagIds,
  setLocalTags,
  setItemTagIds,
  detailsOpen,
  setDetailsOpen,
  fileListItems,
}: ClipboardContextMenuProps) {
  return (
    <>
      <ContextMenu onOpenChange={(open) => {
        if (open) {
          const tagStore = useTagStore.getState();
          setLocalTags(tagStore.tags);
          tagStore.getItemTags(itemId).then((tagList) => {
            setItemTagIds(new Set(tagList.map((t) => t.id)));
          });
        }
      }}>
        <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
        <ContextMenuContent className="w-48">
          {contextMenuItems.map((mi, idx) => (
            <Fragment key={idx}>
              {mi.separator && <ContextMenuSeparator />}
              <ContextMenuItem
                onClick={mi.onClick}
                disabled={mi.disabled}
                className={mi.destructive ? "text-destructive focus:text-destructive" : undefined}
              >
                <mi.icon className="mr-2 h-4 w-4" />
                <span>{mi.label}</span>
              </ContextMenuItem>
            </Fragment>
          ))}
          {/* 标签管理 */}
          <TagAssignSection
            itemId={itemId}
            allTags={localTags}
            itemTagIds={itemTagIds}
            onAddTag={async (id, tagId) => {
              await useTagStore.getState().addTagToItem(id, tagId);
              setItemTagIds((prev) => new Set([...prev, tagId]));
            }}
            onRemoveTag={async (id, tagId) => {
              await useTagStore.getState().removeTagFromItem(id, tagId);
              setItemTagIds((prev) => { const next = new Set(prev); next.delete(tagId); return next; });
            }}
          />
        </ContextMenuContent>
      </ContextMenu>
      {(contentType === "files" || contentType === "video") && (
        <FileDetailsDialog
          open={detailsOpen}
          onOpenChange={setDetailsOpen}
          fileListItems={fileListItems}
        />
      )}
    </>
  );
}
