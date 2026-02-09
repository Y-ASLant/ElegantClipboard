import { memo, useEffect, useState, useRef } from "react";
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
} from "@fluentui/react-icons";
import { invoke } from "@tauri-apps/api/core";
import { CardFooter, ImageCard, FileContent } from "@/components/CardContentRenderers";
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
import { cn } from "@/lib/utils";
import { useClipboardStore, ClipboardItem } from "@/stores/clipboard";
import { useUISettings } from "@/stores/ui-settings";

// ============ Types ============

interface FileListItem {
  name: string;
  path: string;
  isDir: boolean;
  exists: boolean;
}

interface ClipboardItemCardProps {
  item: ClipboardItem;
  index?: number;
  sortId?: string;
  isDragOverlay?: boolean;
}

const clipboardActions = () => useClipboardStore.getState();

// ============ File Details Dialog ============

interface FileDetailsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  fileListItems: FileListItem[];
}

const FileDetailsDialog = ({ open, onOpenChange, fileListItems }: FileDetailsDialogProps) => (
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
                : "bg-red-50 dark:bg-red-950/30 border-red-200 dark:border-red-900"
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
              <p className={cn("text-sm font-medium truncate", !file.exists && "text-red-500")}>
                {file.name}
                {!file.exists && <span className="ml-1 text-xs font-normal">(已失效)</span>}
              </p>
              <p className="text-xs text-muted-foreground truncate mt-0.5">{file.path}</p>
            </div>
          </div>
        ))}
      </div>
      {fileListItems.some((f) => !f.exists) && (
        <p className="text-xs text-red-500 mt-2">部分文件已被移动或删除，无法粘贴</p>
      )}
    </DialogContent>
  </Dialog>
);

// ============ Action Toolbar ============

interface ActionToolbarProps {
  item: ClipboardItem;
  onTogglePin: (e: React.MouseEvent) => void;
  onToggleFavorite: (e: React.MouseEvent) => void;
  onCopy: (e: React.MouseEvent) => void;
  onDelete: (e: React.MouseEvent) => void;
}

const ActionToolbar = ({ item, onTogglePin, onToggleFavorite, onCopy, onDelete }: ActionToolbarProps) => (
  <div
    className="absolute right-1 top-1 flex items-center gap-0.5 bg-background/95 rounded-md px-0.5 shadow-sm border opacity-0 group-hover:opacity-100 transition-opacity"
    data-drag-ignore="true"
  >
    <Tooltip>
      <TooltipTrigger asChild>
        <Button variant="ghost" size="icon" onClick={onTogglePin} className="h-6 w-6">
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
        <Button variant="ghost" size="icon" onClick={onToggleFavorite} className="h-6 w-6">
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
        <Button variant="ghost" size="icon" onClick={onCopy} className="h-6 w-6">
          <Copy16Regular className="w-3.5 h-3.5" />
        </Button>
      </TooltipTrigger>
      <TooltipContent>复制</TooltipContent>
    </Tooltip>
    <Tooltip>
      <TooltipTrigger asChild>
        <Button variant="ghost" size="icon" onClick={onDelete} className="h-6 w-6 hover:text-destructive">
          <Delete16Regular className="w-3.5 h-3.5" />
        </Button>
      </TooltipTrigger>
      <TooltipContent>删除</TooltipContent>
    </Tooltip>
  </div>
);

// ============ Main Card Component ============

export const ClipboardItemCard = memo(
  function ClipboardItemCard({ item, index, sortId, isDragOverlay = false }: ClipboardItemCardProps) {
    const { togglePin, toggleFavorite, deleteItem, copyToClipboard, pasteContent } = clipboardActions();
    const cardMaxLines = useUISettings((s) => s.cardMaxLines);
    const showTime = useUISettings((s) => s.showTime);
    const showCharCount = useUISettings((s) => s.showCharCount);
    const showByteSize = useUISettings((s) => s.showByteSize);

    const [justDropped, setJustDropped] = useState(false);
    const [detailsOpen, setDetailsOpen] = useState(false);
    const [fileListItems, setFileListItems] = useState<FileListItem[]>([]);
    const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
    const hasDraggedRef = useRef(false);

    const filesInvalid = item.content_type === "files" && item.files_valid === false;
    const filePaths = item.content_type === "files" ? parseFilePaths(item.file_paths) : [];

    const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
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
      transition: transition || "transform 200ms ease",
      opacity: isDragging ? 0 : 1,
      cursor: isDragging ? "grabbing" : "pointer",
      zIndex: isDragging ? 1000 : "auto",
    };

    const config = contentTypeConfig[item.content_type] || contentTypeConfig.text;

    const metaItems: string[] = [];
    if (showTime) metaItems.push(formatTime(item.created_at));
    if (showCharCount && item.text_content) metaItems.push(formatCharCount(item.text_content));
    if (showByteSize) metaItems.push(formatSize(item.byte_size));

    // ---- Event handlers ----

    const handlePaste = () => {
      if (!isDragging && !isDragOverlay) pasteContent(item.id);
    };
    const handleCopy = (e: React.MouseEvent) => { e.stopPropagation(); copyToClipboard(item.id); };
    const handleTogglePin = (e: React.MouseEvent) => { e.stopPropagation(); togglePin(item.id); };
    const handleToggleFavorite = (e: React.MouseEvent) => { e.stopPropagation(); toggleFavorite(item.id); };
    const handleDelete = (e: React.MouseEvent) => { e.stopPropagation(); deleteItem(item.id); };

    const handleShowInExplorer = async () => {
      if (filePaths.length > 0) {
        try { await invoke("show_in_explorer", { path: filePaths[0] }); }
        catch (error) { console.error("Failed to show in explorer:", error); }
      }
    };

    const handlePasteAsPath = async () => {
      try { await invoke("paste_as_path", { id: item.id }); }
      catch (error) { console.error("Failed to paste as path:", error); }
    };

    const handleShowDetails = async () => {
      if (filePaths.length === 0) return;
      try {
        const checkResult = await invoke<Record<string, { exists: boolean; is_dir: boolean }>>(
          "check_files_exist",
          { paths: filePaths }
        );
        const items: FileListItem[] = filePaths.map((path) => {
          const name = getFileNameFromPath(path);
          const info = checkResult[path] ?? { exists: false, is_dir: false };
          return { name, path, isDir: info.is_dir, exists: info.exists };
        });
        setFileListItems(items);
        setDetailsOpen(true);
      } catch (error) {
        console.error("Failed to get file details:", error);
      }
    };

    // ---- Card content ----

    const cardContent = (
      <div ref={setNodeRef} style={style} {...attributes} {...listeners}>
        <Card
          className={cn(
            "group relative cursor-pointer overflow-hidden hover:shadow-md hover:border-primary/30",
            isDragOverlay && "shadow-lg border-primary cursor-grabbing",
            justDropped && "animate-scale-bounce"
          )}
          onClick={handlePaste}
        >
          <div className="flex">
            {item.content_type === "image" && item.image_path ? (
              <ImageCard
                image_path={item.image_path}
                metaItems={metaItems}
                index={index}
                isDragOverlay={isDragOverlay}
              />
            ) : item.content_type === "files" ? (
              <FileContent
                filePaths={filePaths}
                filesInvalid={filesInvalid}
                preview={item.preview}
                metaItems={metaItems}
                index={index}
                isDragOverlay={isDragOverlay}
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
                  {item.preview || item.text_content || `[${config.label}]`}
                </pre>
                <CardFooter metaItems={metaItems} index={index} isDragOverlay={isDragOverlay} />
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

    // File items get a context menu wrapper
    if (item.content_type === "files" && !isDragOverlay) {
      return (
        <>
          <ContextMenu>
            <ContextMenuTrigger asChild>{cardContent}</ContextMenuTrigger>
            <ContextMenuContent className="w-48">
              <ContextMenuItem onClick={handlePaste}>
                <ClipboardPaste16Regular className="mr-2 h-4 w-4" />
                <span>粘贴</span>
              </ContextMenuItem>
              <ContextMenuItem onClick={handlePasteAsPath}>
                <TextDescription16Regular className="mr-2 h-4 w-4" />
                <span>粘贴为路径</span>
              </ContextMenuItem>
              <ContextMenuItem onClick={handleShowInExplorer} disabled={filesInvalid}>
                <FolderOpen16Regular className="mr-2 h-4 w-4" />
                <span>在资源管理器中显示</span>
              </ContextMenuItem>
              <ContextMenuItem onClick={handleShowDetails} disabled={filesInvalid}>
                <Info16Regular className="mr-2 h-4 w-4" />
                <span>查看详细信息</span>
              </ContextMenuItem>
              <ContextMenuSeparator />
              <ContextMenuItem onClick={() => deleteItem(item.id)} className="text-destructive focus:text-destructive">
                <Delete16Regular className="mr-2 h-4 w-4" />
                <span>删除</span>
              </ContextMenuItem>
            </ContextMenuContent>
          </ContextMenu>
          <FileDetailsDialog open={detailsOpen} onOpenChange={setDetailsOpen} fileListItems={fileListItems} />
        </>
      );
    }

    return cardContent;
  },
  (prevProps, nextProps) => {
    return (
      prevProps.index === nextProps.index &&
      prevProps.sortId === nextProps.sortId &&
      prevProps.isDragOverlay === nextProps.isDragOverlay &&
      prevProps.item.id === nextProps.item.id &&
      prevProps.item.is_pinned === nextProps.item.is_pinned &&
      prevProps.item.is_favorite === nextProps.item.is_favorite &&
      prevProps.item.content_type === nextProps.item.content_type &&
      prevProps.item.created_at === nextProps.item.created_at &&
      prevProps.item.byte_size === nextProps.item.byte_size &&
      prevProps.item.image_path === nextProps.item.image_path &&
      prevProps.item.image_width === nextProps.item.image_width &&
      prevProps.item.image_height === nextProps.item.image_height &&
      prevProps.item.files_valid === nextProps.item.files_valid &&
      prevProps.item.text_content === nextProps.item.text_content &&
      prevProps.item.preview === nextProps.item.preview &&
      prevProps.item.file_paths === nextProps.item.file_paths
    );
  }
);
