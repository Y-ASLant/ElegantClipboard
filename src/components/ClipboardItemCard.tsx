import { Fragment, memo, useEffect, useState, useRef, useMemo } from "react";
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
  FolderOpen16Regular,
  Info16Regular,
  TextDescription16Regular,
  ClipboardPaste16Regular,
  ArrowDownload16Regular,
  Edit16Regular,
  ChevronDown16Regular,
} from "@fluentui/react-icons";
import { invoke } from "@tauri-apps/api/core";
import {
  CardFooter,
  ImageCard,
  FileContent,
} from "@/components/CardContentRenderers";
import { HighlightText } from "@/components/HighlightText";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
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
import { useSortable, CSS } from "@/hooks/useSortableList";
import {
  contentTypeConfig,
  formatTime,
  formatCharCount,
  formatSize,
  getFileNameFromPath,
  parseFilePaths,
} from "@/lib/format";
import { logError } from "@/lib/logger";
import { cn } from "@/lib/utils";
import { useClipboardStore, ClipboardItem } from "@/stores/clipboard";
import { useGroupStore } from "@/stores/groups";
import { useUISettings } from "@/stores/ui-settings";

// ============ Types ============

interface FileListItem {
  name: string;
  path: string;
  isDir: boolean;
  exists: boolean;
}

interface ContextMenuItemConfig {
  icon: React.ComponentType<{ className?: string }>;
  label: string;
  onClick: () => void;
  disabled?: boolean;
  destructive?: boolean;
  separator?: boolean;
}

interface ClipboardItemCardProps {
  item: ClipboardItem;
  index?: number;
  showBadge?: boolean;
  sortId?: string;
  isDragOverlay?: boolean;
}

const clipboardActions = () => useClipboardStore.getState();
const fileValidityCache = new Map<string, boolean>();

// ============ File Details Dialog ============

interface FileDetailsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  fileListItems: FileListItem[];
}

const FileDetailsDialog = ({
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
            <div className="flex-shrink-0 mt-0.5">
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

function MoveToGroupSection({
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

const ActionToolbar = ({
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

// ============ Main Card Component ============

// Simplified memoization - only compare essential props that affect rendering
const arePropsEqual = (
  prevProps: ClipboardItemCardProps,
  nextProps: ClipboardItemCardProps,
) => {
  if (prevProps.index !== nextProps.index) return false;
  if (prevProps.showBadge !== nextProps.showBadge) return false;
  if (prevProps.sortId !== nextProps.sortId) return false;
  if (prevProps.isDragOverlay !== nextProps.isDragOverlay) return false;

  // Compare essential item properties
  const item = prevProps.item;
  const nextItem = nextProps.item;

  return (
    item.id === nextItem.id &&
    item.is_pinned === nextItem.is_pinned &&
    item.is_favorite === nextItem.is_favorite &&
    item.content_type === nextItem.content_type &&
    item.created_at === nextItem.created_at &&
    item.byte_size === nextItem.byte_size &&
    item.char_count === nextItem.char_count &&
    item.image_path === nextItem.image_path &&
    item.files_valid === nextItem.files_valid &&
    item.preview === nextItem.preview &&
    item.source_app_name === nextItem.source_app_name &&
    item.source_app_icon === nextItem.source_app_icon
  );
};

export const ClipboardItemCard = memo(function ClipboardItemCard({
  item,
  index,
  showBadge,
  sortId,
  isDragOverlay = false,
}: ClipboardItemCardProps) {
  // 每张卡片自行订阅 activeIndex，只有选中态变化的卡片才重渲染
  const isActiveIndex = useClipboardStore(
    (s) => index !== undefined && index >= 0 && s.activeIndex === index,
  );
  const keyboardNavEnabled = useUISettings((s) => s.keyboardNavigation);
  const isActive = isActiveIndex && keyboardNavEnabled;
  const {
    togglePin,
    toggleFavorite,
    deleteItem,
    copyToClipboard,
    pasteContent,
    pasteAsPlainText,
  } = clipboardActions();
  const cardMaxLines = useUISettings((s) => s.cardMaxLines);
  const showTime = useUISettings((s) => s.showTime);
  const showCharCount = useUISettings((s) => s.showCharCount);
  const showByteSize = useUISettings((s) => s.showByteSize);
  const showSourceApp = useUISettings((s) => s.showSourceApp);
  const sourceAppDisplay = useUISettings((s) => s.sourceAppDisplay);
  const showDragAreaIndicator = useUISettings((s) => s.showDragAreaIndicator);

  const [justDropped, setJustDropped] = useState(false);
  const [justPasted, setJustPasted] = useState(false);
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [fileListItems, setFileListItems] = useState<FileListItem[]>([]);
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const hasDraggedRef = useRef(false);
  const { groups, moveItemToGroup } = useGroupStore();
  const selectedGroupId = useClipboardStore((s) => s.selectedGroupId);

  const filePaths = useMemo(
    () => item.content_type === "files" ? parseFilePaths(item.file_paths) : [],
    [item.content_type, item.file_paths],
  );
  const [runtimeFilesValid, setRuntimeFilesValid] = useState<
    boolean | undefined
  >(undefined);

  useEffect(() => {
    if (item.content_type !== "files") {
      setRuntimeFilesValid(undefined);
      return;
    }

    if (item.files_valid !== undefined) {
      setRuntimeFilesValid(item.files_valid);
      return;
    }

    if (filePaths.length === 0) {
      setRuntimeFilesValid(false);
      return;
    }

    const cacheKey = item.file_paths ?? filePaths.join("\n");
    const cached = fileValidityCache.get(cacheKey);
    if (cached !== undefined) {
      setRuntimeFilesValid(cached);
      return;
    }

    let cancelled = false;
    invoke<Record<string, { exists: boolean; is_dir: boolean }>>(
      "check_files_exist",
      { paths: filePaths },
    )
      .then((checkResult) => {
        const allExist = filePaths.every((path) => checkResult[path]?.exists);
        fileValidityCache.set(cacheKey, allExist);
        if (!cancelled) setRuntimeFilesValid(allExist);
      })
      .catch(() => {
        if (!cancelled) setRuntimeFilesValid(undefined);
      });

    return () => {
      cancelled = true;
    };
  }, [item.content_type, item.files_valid, item.file_paths, filePaths]);

  const effectiveFilesValid = item.files_valid ?? runtimeFilesValid;
  const filesInvalid =
    item.content_type === "files" && effectiveFilesValid === false;

  const {
    attributes,
    listeners,
    setNodeRef,
    setActivatorNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({
    id: sortId || `item-${item.id}`,
    disabled: isDragOverlay,
  });

  // Bounce animation after drag (skip initial mount)
  useEffect(() => {
    if (isDragging) {
      hasDraggedRef.current = true;
    } else if (hasDraggedRef.current && !isDragOverlay) {
      setJustDropped(true);
      if (timeoutRef.current) clearTimeout(timeoutRef.current);
      timeoutRef.current = setTimeout(() => setJustDropped(false), 300);
    }
    return () => {
      if (timeoutRef.current) clearTimeout(timeoutRef.current);
    };
  }, [isDragging, isDragOverlay]);

  const style: React.CSSProperties = {
    transform: CSS.Transform.toString(transform),
    transition: transition || undefined,
    opacity: isDragging ? 0 : 1,
    cursor: isDragging ? "grabbing" : "pointer",
    zIndex: isDragging ? 1000 : "auto",
  };

  const config = contentTypeConfig[item.content_type] || contentTypeConfig.text;
  const dragHandleWidth = "clamp(40px, 14%, 72px)";

  const timeFormat = useUISettings((s) => s.timeFormat);

  const metaItems = useMemo(() => {
    const items: string[] = [];
    if (showTime) items.push(formatTime(item.created_at, timeFormat));
    if (showCharCount && item.char_count)
      items.push(formatCharCount(item.char_count));
    if (showByteSize) items.push(formatSize(item.byte_size));
    return items;
  }, [showTime, showCharCount, showByteSize, timeFormat, item.created_at, item.char_count, item.byte_size]);

  // ---- Event handlers ----

  const handlePaste = () => {
    if (!isDragging && !isDragOverlay) {
      pasteContent(item.id);
      setJustPasted(true);
      setTimeout(() => setJustPasted(false), 300);
    }
  };
  const handleCopy = (e: React.MouseEvent) => {
    e.stopPropagation();
    copyToClipboard(item.id);
  };
  const handleCopyCtxMenu = () => copyToClipboard(item.id);
  const handleTogglePin = (e: React.MouseEvent) => {
    e.stopPropagation();
    togglePin(item.id);
  };
  const handleToggleFavorite = (e: React.MouseEvent) => {
    e.stopPropagation();
    toggleFavorite(item.id);
  };
  const handleDelete = (e: React.MouseEvent) => {
    e.stopPropagation();
    deleteItem(item.id);
  };

  const handleShowInExplorer = async () => {
    if (filePaths.length > 0) {
      try {
        await invoke("show_in_explorer", { path: filePaths[0] });
      } catch (error) {
        logError("Failed to show in explorer:", error);
      }
    }
  };

  const handlePasteAsPath = async () => {
    try {
      await invoke("paste_as_path", { id: item.id });
    } catch (error) {
      logError("Failed to paste as path:", error);
    }
  };

  const handleShowDetails = async () => {
    if (filePaths.length === 0) return;
    try {
      const checkResult = await invoke<
        Record<string, { exists: boolean; is_dir: boolean }>
      >("check_files_exist", { paths: filePaths });
      const items: FileListItem[] = filePaths.map((path) => {
        const name = getFileNameFromPath(path);
        const info = checkResult[path] ?? { exists: false, is_dir: false };
        return { name, path, isDir: info.is_dir, exists: info.exists };
      });
      setFileListItems(items);
      setDetailsOpen(true);
    } catch (error) {
      logError("Failed to get file details:", error);
    }
  };

  const handleSaveAs = async () => {
    // For images: save from image_path; for files: save the first file
    const sourcePath =
      item.content_type === "image" ? item.image_path : filePaths[0];
    if (!sourcePath) return;
    try {
      await invoke("save_file_as", { sourcePath });
    } catch (error) {
      logError("Failed to save file:", error);
    }
  };

  const handleShowImageInExplorer = async () => {
    if (!item.image_path) return;
    try {
      await invoke("show_in_explorer", { path: item.image_path });
    } catch (error) {
      logError("Failed to show in explorer:", error);
    }
  };

  // ---- Card content ----

  const cardContent = (
    <div ref={setNodeRef} style={style}>
      <Card
        className={cn(
        "group relative cursor-pointer overflow-hidden shadow-none dark:shadow-[inset_0_0.5px_0_0_rgba(255,255,255,0.09),0_2px_8px_-1px_rgba(0,0,0,0.5)] hover:shadow-sm dark:hover:shadow-[inset_0_0.5px_0_0_rgba(255,255,255,0.12),0_4px_12px_-2px_rgba(0,0,0,0.6)] hover:border-primary/30 ring-1 ring-black/[0.04] dark:ring-white/[0.1]",
          isDragOverlay && "shadow-lg border-primary cursor-grabbing",
          justDropped && "animate-scale-bounce",
          justPasted && "animate-paste-flash",
          isActive && "bg-accent shadow-sm",
        )}
        onClick={handlePaste}
      >
        {!isDragging && !isDragOverlay && (
          <>
            <button
              ref={setActivatorNodeRef}
              {...attributes}
              {...listeners}
              type="button"
              data-drag-handle="true"
              onClick={(e) => e.stopPropagation()}
              className={cn(
                "absolute inset-y-0 left-0 z-10 flex items-center justify-center rounded-l-lg cursor-grab active:cursor-grabbing",
                showDragAreaIndicator
                  ? "border-r border-dashed border-border/50 bg-background/20 text-muted-foreground/85 opacity-0 group-hover:opacity-55 transition-[opacity,colors] duration-150 hover:bg-background/35 hover:text-foreground"
                  : "border-r border-transparent bg-transparent text-transparent opacity-0",
              )}
              style={{ width: dragHandleWidth }}
              title="左侧拖拽区域"
              aria-label="左侧拖拽区域"
              tabIndex={showDragAreaIndicator ? 0 : -1}
            >
              <span
                aria-hidden
                className="pointer-events-none text-[10px] leading-tight text-center text-muted-foreground"
              >
                拖拽区域
              </span>
            </button>

            <button
              {...attributes}
              {...listeners}
              type="button"
              data-drag-handle="true"
              onClick={(e) => e.stopPropagation()}
              className={cn(
                "absolute inset-y-0 right-0 z-10 flex items-center justify-center rounded-r-lg cursor-grab active:cursor-grabbing",
                showDragAreaIndicator
                  ? "border-l border-dashed border-border/50 bg-background/20 text-muted-foreground/85 opacity-0 group-hover:opacity-55 transition-[opacity,colors] duration-150 hover:bg-background/35 hover:text-foreground"
                  : "border-l border-transparent bg-transparent text-transparent opacity-0",
              )}
              style={{ width: dragHandleWidth }}
              title="右侧拖拽区域"
              aria-label="右侧拖拽区域"
              tabIndex={showDragAreaIndicator ? 0 : -1}
            >
              <span
                aria-hidden
                className="pointer-events-none text-[10px] leading-tight text-center text-muted-foreground"
              >
                拖拽区域
              </span>
            </button>

            {showDragAreaIndicator && (
              <div
                aria-hidden
                className="pointer-events-none absolute inset-y-0 z-[6] flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity duration-150"
                style={{ left: dragHandleWidth, right: dragHandleWidth }}
              >
                <div className="absolute inset-y-1 left-0 border-l border-dashed border-border/50" />
                <div className="absolute inset-y-1 right-0 border-r border-dashed border-border/50" />
                <div className="rounded border border-dashed border-border/60 bg-background/80 px-2 py-1 text-center">
                  <div className="text-[10px] leading-none text-muted-foreground">中间粘贴区域</div>
                  <div className="mt-0.5 text-[10px] leading-none text-muted-foreground/90">点击卡片可粘贴</div>
                </div>
              </div>
            )}
          </>
        )}
        <div className="flex">
          {item.content_type === "image" && item.image_path ? (
            <ImageCard
              image_path={item.image_path}
              metaItems={metaItems}
              index={index}
              showBadge={showBadge}
              isDragOverlay={isDragOverlay}
              sourceAppName={showSourceApp && sourceAppDisplay !== "icon" ? item.source_app_name : undefined}
              sourceAppIcon={showSourceApp && sourceAppDisplay !== "name" ? item.source_app_icon : undefined}
            />
          ) : item.content_type === "files" ? (
            <FileContent
              filePaths={filePaths}
              filesInvalid={filesInvalid}
              preview={item.preview}
              metaItems={metaItems}
              index={index}
              showBadge={showBadge}
              isDragOverlay={isDragOverlay}
              sourceAppName={showSourceApp && sourceAppDisplay !== "icon" ? item.source_app_name : undefined}
              sourceAppIcon={showSourceApp && sourceAppDisplay !== "name" ? item.source_app_icon : undefined}
            />
          ) : (
            <div className="flex-1 min-w-0 px-3 py-2.5">
              <pre
                className="clipboard-content text-sm leading-relaxed text-foreground/90 whitespace-pre-wrap break-all m-0"
                style={{
                  display: "-webkit-box",
                  WebkitLineClamp: cardMaxLines,
                  WebkitBoxOrient: "vertical",
                  overflow: "hidden",
                }}
              >
                <HighlightText text={item.preview || item.text_content || `[${config.label}]`} />
              </pre>
              <CardFooter
                metaItems={metaItems}
                index={index}
                showBadge={showBadge}
                isDragOverlay={isDragOverlay}
                sourceAppName={showSourceApp && sourceAppDisplay !== "icon" ? item.source_app_name : undefined}
                sourceAppIcon={showSourceApp && sourceAppDisplay !== "name" ? item.source_app_icon : undefined}
              />
            </div>
          )}

          {!isDragging && !isDragOverlay && (
            <ActionToolbar
              item={item}
              onTogglePin={handleTogglePin}
              onToggleFavorite={handleToggleFavorite}
              onCopy={handleCopy}
              onDelete={handleDelete}
            />
          )}

          {/* Pin indicator badge */}
          {item.is_pinned && !isDragging && !isDragOverlay && (
            <>
              <div className="absolute -right-6 -top-6 w-12 h-12 rotate-45 bg-primary opacity-100 group-hover:opacity-0 transition-opacity" />
              <div className="absolute right-0.5 top-0.5 opacity-100 group-hover:opacity-0 transition-opacity">
                <Pin16Filled className="w-3 h-3 text-primary-foreground" />
              </div>
            </>
          )}
        </div>
      </Card>
    </div>
  );

  const handleEdit = async () => {
    try {
      await invoke("open_text_editor_window", { id: item.id });
    } catch (error) {
      logError("Failed to open editor:", error);
    }
  };

  // 上下文菜单配置
  const contextMenuItems: ContextMenuItemConfig[] | null = (() => {
    if (isDragOverlay) return null;
    // 文本类内容（text/html/rtf）可编辑
    if (item.content_type === "text" || item.content_type === "html" || item.content_type === "rtf") {
      return [
        { icon: ClipboardPaste16Regular, label: "粘贴", onClick: handlePaste },
        { icon: TextDescription16Regular, label: "粘贴为纯文本", onClick: () => pasteAsPlainText(item.id) },
        { icon: Copy16Regular, label: "复制", onClick: handleCopyCtxMenu },
        { icon: Edit16Regular, label: "编辑", onClick: handleEdit },
        { icon: Delete16Regular, label: "删除", onClick: () => deleteItem(item.id), destructive: true, separator: true },
      ];
    }
    if (item.content_type === "files") {
      return [
        { icon: ClipboardPaste16Regular, label: "粘贴", onClick: handlePaste },
        { icon: TextDescription16Regular, label: "粘贴为路径", onClick: handlePasteAsPath },
        { icon: FolderOpen16Regular, label: "在资源管理器中显示", onClick: handleShowInExplorer, disabled: filesInvalid },
        { icon: ArrowDownload16Regular, label: "另存为", onClick: handleSaveAs, disabled: filesInvalid },
        { icon: Info16Regular, label: "查看详细信息", onClick: handleShowDetails, disabled: filesInvalid },
        { icon: Delete16Regular, label: "删除", onClick: () => deleteItem(item.id), destructive: true, separator: true },
      ];
    }
    if (item.content_type === "image" && item.image_path) {
      return [
        { icon: ClipboardPaste16Regular, label: "粘贴", onClick: handlePaste },
        { icon: Copy16Regular, label: "复制", onClick: handleCopyCtxMenu },
        { icon: FolderOpen16Regular, label: "在资源管理器中显示", onClick: handleShowImageInExplorer },
        { icon: ArrowDownload16Regular, label: "另存为", onClick: handleSaveAs },
        { icon: Delete16Regular, label: "删除", onClick: () => deleteItem(item.id), destructive: true, separator: true },
      ];
    }
    return null;
  })();

  if (contextMenuItems) {
    return (
      <>
        <ContextMenu>
          <ContextMenuTrigger asChild>{cardContent}</ContextMenuTrigger>
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
            {/* 分组内联折叠（排除当前分组，显示可移动的目标分组）*/}
            <MoveToGroupSection
              itemId={item.id}
              groups={groups}
              selectedGroupId={selectedGroupId}
              moveItemToGroup={moveItemToGroup}
            />
          </ContextMenuContent>
        </ContextMenu>
        {item.content_type === "files" && (
          <FileDetailsDialog
            open={detailsOpen}
            onOpenChange={setDetailsOpen}
            fileListItems={fileListItems}
          />
        )}
      </>
    );
  }

  return cardContent;
}, arePropsEqual);

