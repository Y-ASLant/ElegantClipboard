import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  DndContext,
  closestCenter,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import { restrictToVerticalAxis, restrictToParentElement } from "@dnd-kit/modifiers";
import {
  SortableContext,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import {
  Add16Regular,
  ArrowSortDown16Regular,
  ArrowSortUp16Regular,
  Delete16Regular,
  Dismiss16Regular,
  Edit16Regular,
  ReOrderDotsVertical16Regular,
  Tag16Regular,
  TagMultiple16Regular,
  TextSortAscending16Regular,
  Clock16Regular,
  ReOrderDotsVertical16Filled,
} from "@fluentui/react-icons";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { SortableTagItemRow, TagItemRow } from "@/components/tags/TagItemRows";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { focusWindowImmediately } from "@/hooks/useInputFocus";
import { logError } from "@/lib/logger";
import { cn } from "@/lib/utils";
import { useClipboardStore, type ClipboardItem } from "@/stores/clipboard";
import { useTagStore, type Tag } from "@/stores/tags";
import { useUISettings } from "@/stores/ui-settings";

function SortableTagItem({
  tag,
  isSelected,
  onSelect,
  onContextMenu,
}: {
  tag: Tag;
  isSelected: boolean;
  onSelect: () => void;
  onContextMenu: (e: React.MouseEvent) => void;
}) {
  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id: tag.id });

  const style = useMemo(() => ({
    transform: CSS.Transform.toString(transform),
    transition,
    opacity: isDragging ? 0.5 : undefined,
  }), [transform, transition, isDragging]);

  return (
    <div
      ref={setNodeRef}
      style={style}
      onClick={onSelect}
      onContextMenu={onContextMenu}
      className={cn(
        "relative flex items-center gap-0.5 mx-1 px-1 py-1.5 text-xs cursor-default rounded-md transition-colors duration-150",
        isSelected
          ? "bg-accent text-foreground font-medium"
          : "text-muted-foreground hover:bg-accent/50 hover:text-foreground",
      )}
    >
      <button
        {...attributes}
        {...listeners}
        className="w-4 h-4 flex items-center justify-center text-muted-foreground/40 cursor-grab active:cursor-grabbing shrink-0 touch-none"
        onClick={(e) => e.stopPropagation()}
      >
        <ReOrderDotsVertical16Regular className="w-3 h-3" />
      </button>
      <Tag16Regular className={cn("w-3.5 h-3.5 shrink-0", isSelected ? "text-primary" : "text-muted-foreground/60")} />
      <span className="flex-1 min-w-0 truncate">{tag.name}</span>
      <span className={cn(
        "text-[10px] tabular-nums shrink-0 min-w-[1.25rem] text-center rounded-full px-1",
        isSelected
          ? "text-primary/80"
          : "text-muted-foreground/60",
      )}>
        {tag.item_count}
      </span>
    </div>
  );
}

export function TagsView() {
  const { tags, fetchTags, createTag, renameTag, deleteTag, reorderTags, reorderTagItems } = useTagStore();

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 5 } })
  );

  const handleDragEnd = useCallback((event: DragEndEvent) => {
    const { active, over } = event;
    if (!over || active.id === over.id) return;
    const oldIdx = tags.findIndex((t) => t.id === Number(active.id));
    const newIdx = tags.findIndex((t) => t.id === Number(over.id));
    if (oldIdx < 0 || newIdx < 0) return;
    const next = [...tags];
    const [moved] = next.splice(oldIdx, 1);
    next.splice(newIdx, 0, moved);
    reorderTags(next.map((t) => t.id));
  }, [tags, reorderTags]);
  const searchQuery = useClipboardStore((s) => s.searchQuery);
  const [selectedTagId, setSelectedTagId] = useState<number | null>(null);
  const [tagItems, setTagItems] = useState<ClipboardItem[]>([]);
  const [isLoading, setIsLoading] = useState(false);

  // Inline new-tag input
  const [isCreating, setIsCreating] = useState(false);
  const [createName, setCreateName] = useState("");
  const createInputRef = useRef<HTMLInputElement>(null);

  // Dialogs
  const [renameOpen, setRenameOpen] = useState(false);
  const [renameTarget, setRenameTarget] = useState<Tag | null>(null);
  const [renameName, setRenameName] = useState("");
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<Tag | null>(null);

  // Context menu
  const [ctxMenuTag, setCtxMenuTag] = useState<Tag | null>(null);
  const [ctxMenuPos, setCtxMenuPos] = useState({ x: 0, y: 0 });
  const ctxMenuRef = useRef<HTMLDivElement>(null);

  // Draggable splitter
  // Sort mode for right-side items
  const [sortMode, setSortMode] = useState<"custom" | "time-desc" | "time-asc" | "alpha-asc" | "alpha-desc">("custom");

  const [leftWidth, setLeftWidth] = useState(144);
  const [isDragging, setIsDragging] = useState(false);
  const startXRef = useRef(0);
  const startWidthRef = useRef(0);

  const handleDividerMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    setIsDragging(true);
    startXRef.current = e.clientX;
    startWidthRef.current = leftWidth;

    const onMouseMove = (ev: MouseEvent) => {
      const delta = ev.clientX - startXRef.current;
      const newWidth = Math.max(80, Math.min(300, startWidthRef.current + delta));
      setLeftWidth(newWidth);
    };
    const onMouseUp = () => {
      setIsDragging(false);
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("mouseup", onMouseUp);
    };
    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
  }, [leftWidth]);

  // Close context menu on click outside
  useEffect(() => {
    if (!ctxMenuTag) return;
    const handler = (e: MouseEvent) => {
      if (ctxMenuRef.current && !ctxMenuRef.current.contains(e.target as Node)) {
        setCtxMenuTag(null);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [ctxMenuTag]);

  // Auto-focus inline create input
  useEffect(() => {
    if (isCreating) {
      const t = setTimeout(async () => {
        await focusWindowImmediately();
        createInputRef.current?.focus();
      }, 50);
      return () => clearTimeout(t);
    }
  }, [isCreating]);

  const timeFormat = useUISettings((s) => s.timeFormat);

  useEffect(() => {
    fetchTags();
  }, [fetchTags]);

  // Auto-select first tag
  useEffect(() => {
    if (selectedTagId === null && tags.length > 0) {
      setSelectedTagId(tags[0].id);
    }
  }, [tags, selectedTagId]);

  // Fetch items when selected tag or search changes
  const fetchTagItems = useCallback(async (tagId: number | null, search?: string) => {
    if (tagId === null) {
      setTagItems([]);
      return;
    }
    setIsLoading(true);
    try {
      const items = await invoke<ClipboardItem[]>("get_clipboard_items", {
        search: search || null,
        contentType: null,
        pinnedOnly: false,
        favoriteOnly: false,
        tagId,
        limit: null,
        offset: 0,
      });
      setTagItems(items);
    } catch (error) {
      logError("Failed to fetch tag items:", error);
      setTagItems([]);
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchTagItems(selectedTagId, searchQuery);
  }, [selectedTagId, searchQuery, fetchTagItems]);

  useEffect(() => {
    const unlisten = listen("tags-updated", async () => {
      await fetchTags();
      await fetchTagItems(selectedTagId, searchQuery);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, [fetchTags, fetchTagItems, selectedTagId, searchQuery]);

  const selectedTag = useMemo(
    () => tags.find((t) => t.id === selectedTagId) ?? null,
    [tags, selectedTagId],
  );

  const tagIds = useMemo(() => tags.map((t) => t.id), [tags]);

  const sortedItems = useMemo(() => {
    if (sortMode === "custom") return tagItems;
    const items = [...tagItems];
    switch (sortMode) {
      case "time-desc":
        return items.sort((a, b) => b.created_at.localeCompare(a.created_at));
      case "time-asc":
        return items.sort((a, b) => a.created_at.localeCompare(b.created_at));
      case "alpha-asc":
        return items.sort((a, b) => {
          const ta = a.text_content ?? a.preview ?? "";
          const tb = b.text_content ?? b.preview ?? "";
          return ta.localeCompare(tb, "zh-CN");
        });
      case "alpha-desc":
        return items.sort((a, b) => {
          const ta = a.text_content ?? a.preview ?? "";
          const tb = b.text_content ?? b.preview ?? "";
          return tb.localeCompare(ta, "zh-CN");
        });
      default:
        return items;
    }
  }, [tagItems, sortMode]);

  const sortedItemIds = useMemo(() => sortedItems.map((it) => it.id), [sortedItems]);

  const handleItemDragEnd = useCallback((event: DragEndEvent) => {
    const { active, over } = event;
    if (!over || active.id === over.id || selectedTagId === null) return;
    const oldIdx = tagItems.findIndex((it) => it.id === Number(active.id));
    const newIdx = tagItems.findIndex((it) => it.id === Number(over.id));
    if (oldIdx < 0 || newIdx < 0) return;
    const next = [...tagItems];
    const [moved] = next.splice(oldIdx, 1);
    next.splice(newIdx, 0, moved);
    setTagItems(next);
    reorderTagItems(selectedTagId, next.map((it) => it.id));
  }, [tagItems, selectedTagId, reorderTagItems]);

  const handleCreate = async () => {
    const trimmed = createName.trim();
    if (!trimmed) return;
    const tag = await createTag(trimmed);
    setCreateName("");
    setIsCreating(false);
    if (tag) setSelectedTagId(tag.id);
  };

  const handleRename = async () => {
    if (!renameTarget || !renameName.trim()) return;
    await renameTag(renameTarget.id, renameName.trim());
    setRenameOpen(false);
    setRenameTarget(null);
  };

  const handleDelete = async () => {
    if (!deleteTarget) return;
    const id = deleteTarget.id;
    await deleteTag(id);
    if (selectedTagId === id) {
      setSelectedTagId(tags.find((t) => t.id !== id)?.id ?? null);
    }
    setDeleteOpen(false);
    setDeleteTarget(null);
  };

  const handleRemoveTagFromItem = async (itemId: number) => {
    if (!selectedTagId) return;
    try {
      await invoke("remove_tag_from_item", { itemId, tagId: selectedTagId });
      setTagItems((prev) => prev.filter((i) => i.id !== itemId));
      fetchTags();
    } catch (error) {
      logError("Failed to remove tag from item:", error);
    }
  };

  const handlePaste = async (item: ClipboardItem) => {
    try {
      await invoke("paste_content", { id: item.id, closeWindow: useUISettings.getState().pasteCloseWindow });
    } catch (error) {
      logError("Failed to paste:", error);
    }
  };

  return (
    <div className="flex flex-col h-full">
      <div className={cn("flex flex-1 overflow-hidden", isDragging && "select-none")}>
        {/* Left: tag list */}
        <div style={{ width: leftWidth }} className="shrink-0 flex flex-col bg-muted/30">
          <div className="flex-1 overflow-y-auto py-0.5">
            <DndContext
              sensors={sensors}
              collisionDetection={closestCenter}
              modifiers={[restrictToVerticalAxis, restrictToParentElement]}
              onDragEnd={handleDragEnd}
            >
              <SortableContext items={tagIds} strategy={verticalListSortingStrategy}>
                {tags.map((tag) => (
                  <SortableTagItem
                    key={tag.id}
                    tag={tag}
                    isSelected={selectedTagId === tag.id}
                    onSelect={() => setSelectedTagId(tag.id)}
                    onContextMenu={(e) => {
                      e.preventDefault();
                      setCtxMenuTag(tag);
                      setCtxMenuPos({ x: e.clientX, y: e.clientY });
                    }}
                  />
                ))}
              </SortableContext>
            </DndContext>
            {tags.length === 0 && !isCreating && (
              <div className="flex flex-col items-center gap-2 py-8 px-3">
                <TagMultiple16Regular className="w-6 h-6 text-muted-foreground/40" />
                <p className="text-xs text-muted-foreground text-center">暂无标签</p>
              </div>
            )}
          </div>
          {/* Inline create / add button */}
          <div className="shrink-0 border-t p-1">
            {isCreating ? (
              <div className="flex items-center gap-1 px-1">
                <input
                  ref={createInputRef}
                  type="text"
                  placeholder="标签名称"
                  value={createName}
                  onChange={(e) => setCreateName(e.target.value)}
                  onFocus={() => focusWindowImmediately()}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") handleCreate();
                    if (e.key === "Escape") { setIsCreating(false); setCreateName(""); }
                    e.stopPropagation();
                  }}
                  onKeyUp={(e) => e.stopPropagation()}
                  onMouseDown={(e) => e.stopPropagation()}
                  className="flex-1 min-w-0 h-7 px-2 text-xs rounded-md border bg-background outline-none focus:ring-1 focus:ring-ring"
                />
                <button
                  onClick={handleCreate}
                  disabled={!createName.trim()}
                  className="shrink-0 w-6 h-6 flex items-center justify-center rounded-md hover:bg-accent text-primary disabled:opacity-30 transition-colors"
                >
                  <Add16Regular className="w-3.5 h-3.5" />
                </button>
                <button
                  onClick={() => { setIsCreating(false); setCreateName(""); }}
                  className="shrink-0 w-6 h-6 flex items-center justify-center rounded-md hover:bg-accent text-muted-foreground transition-colors"
                >
                  <Dismiss16Regular className="w-3.5 h-3.5" />
                </button>
              </div>
            ) : (
              <button
                onClick={() => setIsCreating(true)}
                className="w-full flex items-center gap-1.5 px-2 py-1.5 text-xs rounded-md text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
              >
                <Add16Regular className="w-3.5 h-3.5" />
                新建标签
              </button>
            )}
          </div>
        </div>

        {/* Draggable divider */}
        <div className="relative shrink-0 w-px bg-border">
          <div
            onMouseDown={handleDividerMouseDown}
            className={cn(
              "absolute inset-y-0 -left-1 w-2.5 cursor-col-resize z-10 group/divider",
            )}
          >
            <div className={cn(
              "absolute inset-y-0 left-1 w-px transition-colors duration-150",
              isDragging ? "bg-primary" : "group-hover/divider:bg-primary/40",
            )} />
          </div>
        </div>

        {/* Right: items for selected tag */}
        <div className="flex-1 overflow-y-auto">
          {!selectedTag ? (
            <div className="flex flex-col items-center gap-2 py-12 px-4">
              <TagMultiple16Regular className="w-8 h-8 text-muted-foreground/30" />
              <p className="text-xs text-muted-foreground">请选择一个标签</p>
            </div>
          ) : isLoading ? (
            <div className="flex items-center justify-center py-12">
              <p className="text-xs text-muted-foreground animate-pulse">加载中…</p>
            </div>
          ) : sortedItems.length === 0 ? (
            <div className="flex flex-col items-center gap-2 py-12 px-4">
              <Tag16Regular className="w-8 h-8 text-muted-foreground/30" />
              <p className="text-xs text-muted-foreground">
                {searchQuery ? "没有匹配的条目" : "该标签下暂无条目"}
              </p>
            </div>
          ) : (
            <div>
              <div className="flex items-center gap-1 px-3 py-1.5 border-b border-border/50">
                <span className="text-[10px] text-muted-foreground/60 mr-auto">{sortedItems.length} 条</span>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <button
                      onClick={() => setSortMode("custom")}
                      className={cn(
                        "flex items-center gap-0.5 px-1.5 h-5 text-[10px] rounded transition-colors",
                        sortMode === "custom"
                          ? "bg-accent text-primary font-medium"
                          : "text-muted-foreground hover:text-foreground hover:bg-accent/50"
                      )}
                    >
                      <ReOrderDotsVertical16Filled className="w-3 h-3" />
                      自定义
                    </button>
                  </TooltipTrigger>
                  <TooltipContent>拖拽排序</TooltipContent>
                </Tooltip>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <button
                      onClick={() => setSortMode((m) => m === "time-desc" ? "time-asc" : "time-desc")}
                      className={cn(
                        "flex items-center gap-0.5 px-1.5 h-5 text-[10px] rounded transition-colors",
                        sortMode.startsWith("time")
                          ? "bg-accent text-primary font-medium"
                          : "text-muted-foreground hover:text-foreground hover:bg-accent/50"
                      )}
                    >
                      <Clock16Regular className="w-3 h-3" />
                      时间
                      {sortMode.startsWith("time") && (
                        sortMode === "time-desc"
                          ? <ArrowSortDown16Regular className="w-3 h-3" />
                          : <ArrowSortUp16Regular className="w-3 h-3" />
                      )}
                    </button>
                  </TooltipTrigger>
                  <TooltipContent>{sortMode === "time-desc" ? "最新在前" : sortMode === "time-asc" ? "最早在前" : "按时间排序"}</TooltipContent>
                </Tooltip>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <button
                      onClick={() => setSortMode((m) => m === "alpha-asc" ? "alpha-desc" : "alpha-asc")}
                      className={cn(
                        "flex items-center gap-0.5 px-1.5 h-5 text-[10px] rounded transition-colors",
                        sortMode.startsWith("alpha")
                          ? "bg-accent text-primary font-medium"
                          : "text-muted-foreground hover:text-foreground hover:bg-accent/50"
                      )}
                    >
                      <TextSortAscending16Regular className="w-3 h-3" />
                      字符
                      {sortMode.startsWith("alpha") && (
                        sortMode === "alpha-asc"
                          ? <ArrowSortUp16Regular className="w-3 h-3" />
                          : <ArrowSortDown16Regular className="w-3 h-3" />
                      )}
                    </button>
                  </TooltipTrigger>
                  <TooltipContent>{sortMode === "alpha-asc" ? "A → Z" : sortMode === "alpha-desc" ? "Z → A" : "按字符排序"}</TooltipContent>
                </Tooltip>
              </div>
              {sortMode === "custom" ? (
                <DndContext
                  sensors={sensors}
                  collisionDetection={closestCenter}
                  modifiers={[restrictToVerticalAxis, restrictToParentElement]}
                  onDragEnd={handleItemDragEnd}
                >
                  <SortableContext items={sortedItemIds} strategy={verticalListSortingStrategy}>
                    {sortedItems.map((item, i) => (
                      <SortableTagItemRow
                        key={item.id}
                        item={item}
                        timeFormat={timeFormat}
                        onPaste={() => handlePaste(item)}
                        onRemove={() => handleRemoveTagFromItem(item.id)}
                        isLast={i === sortedItems.length - 1}
                      />
                    ))}
                  </SortableContext>
                </DndContext>
              ) : (
                sortedItems.map((item, i) => (
                  <TagItemRow
                    key={item.id}
                    item={item}
                    timeFormat={timeFormat}
                    onPaste={() => handlePaste(item)}
                    onRemove={() => handleRemoveTagFromItem(item.id)}
                    isLast={i === sortedItems.length - 1}
                  />
                ))
              )}
            </div>
          )}
        </div>
      </div>

      {/* Tag context menu */}
      {ctxMenuTag && (
        <div
          ref={ctxMenuRef}
          className="fixed z-50 min-w-[120px] rounded-lg border bg-popover p-1 shadow-lg animate-in fade-in-0 zoom-in-95 duration-100"
          style={{ left: ctxMenuPos.x, top: ctxMenuPos.y }}
        >
          <div
            onClick={() => {
              setRenameTarget(ctxMenuTag);
              setRenameName(ctxMenuTag.name);
              setRenameOpen(true);
              setCtxMenuTag(null);
            }}
            className="flex items-center gap-2 px-2.5 py-1.5 text-xs rounded-md cursor-default hover:bg-accent hover:text-accent-foreground transition-colors"
          >
            <Edit16Regular className="w-3.5 h-3.5" />
            重命名
          </div>
          <div
            onClick={() => {
              setDeleteTarget(ctxMenuTag);
              setDeleteOpen(true);
              setCtxMenuTag(null);
            }}
            className="flex items-center gap-2 px-2.5 py-1.5 text-xs rounded-md cursor-default hover:bg-destructive/10 text-destructive transition-colors"
          >
            <Delete16Regular className="w-3.5 h-3.5" />
            删除
          </div>
        </div>
      )}

      {/* Rename tag dialog */}
      <Dialog open={renameOpen} onOpenChange={(o) => { setRenameOpen(o); if (!o) setRenameTarget(null); }}>
        <DialogContent showCloseButton={false}>
          <DialogHeader className="text-left"><DialogTitle>重命名标签</DialogTitle></DialogHeader>
          <Input
            placeholder="标签名称"
            value={renameName}
            onChange={(e) => setRenameName(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") handleRename(); }}
            autoFocus
          />
          <DialogFooter>
            <Button variant="outline" onClick={() => setRenameOpen(false)}>取消</Button>
            <Button onClick={handleRename} disabled={!renameName.trim()}>确定</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Delete tag dialog */}
      <Dialog open={deleteOpen} onOpenChange={(o) => { setDeleteOpen(o); if (!o) setDeleteTarget(null); }}>
        <DialogContent showCloseButton={false}>
          <DialogHeader className="text-left">
            <DialogTitle>删除标签</DialogTitle>
            <DialogDescription className="text-left">
              确定要删除标签"{deleteTarget?.name ?? ""}"吗？该标签将从所有条目中移除（条目本身不会被删除）。
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDeleteOpen(false)}>取消</Button>
            <Button variant="destructive" onClick={handleDelete} disabled={!deleteTarget}>删除</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
