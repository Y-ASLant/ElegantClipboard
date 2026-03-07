import { Fragment, memo, useCallback, useEffect, useState, useRef, useMemo } from "react";
import {
  Pin16Filled,
  Delete16Regular,
  Copy16Regular,
  FolderOpen16Regular,
  Info16Regular,
  TextDescription16Regular,
  ClipboardPaste16Regular,
  ArrowDownload16Regular,
  Edit16Regular,
} from "@fluentui/react-icons";
import { invoke } from "@tauri-apps/api/core";
import { emitTo } from "@tauri-apps/api/event";
import {
  CardFooter,
  FileContent,
  getPreviewBounds,
  ImageCard,
} from "@/components/CardContentRenderers";
import {
  ActionToolbar,
  FileDetailsDialog,
  MoveToGroupSection,
  type FileListItem,
  type ContextMenuItemConfig,
} from "@/components/CardSubComponents";
import { HighlightText } from "@/components/HighlightText";
import {
  type ClipboardItemDetail,
  sampleTextPreview,
  getCachedTextPreviewContent,
  setCachedTextPreviewContent,
  TEXT_PREVIEW_MIN_W,
  TEXT_PREVIEW_MAX_W,
  TEXT_PREVIEW_MIN_H,
  TEXT_PREVIEW_MAX_H,
  TEXT_PREVIEW_CHAR_WIDTH,
  TEXT_PREVIEW_HORIZONTAL_PADDING,
  TEXT_PREVIEW_MIN_CHARS_PER_LINE,
} from "@/components/text-preview";
import { Card } from "@/components/ui/card";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
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

interface ClipboardItemCardProps {
  item: ClipboardItem;
  index?: number;
  showBadge?: boolean;
  sortId?: string;
  isDragOverlay?: boolean;
}

const clipboardActions = () => useClipboardStore.getState();
const fileValidityCache = new Map<string, boolean>();

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
  const textPreviewEnabled = useUISettings((s) => s.textPreviewEnabled);
  const hoverPreviewDelay = useUISettings((s) => s.hoverPreviewDelay);
  const previewPosition = useUISettings((s) => s.previewPosition);
  const sharpCorners = useUISettings((s) => s.sharpCorners);

  const [justPasted, setJustPasted] = useState(false);
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [fileListItems, setFileListItems] = useState<FileListItem[]>([]);
  const { groups, moveItemToGroup } = useGroupStore();
  const selectedGroupId = useClipboardStore((s) => s.selectedGroupId);
  const textPreviewTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const textPreviewVisibleRef = useRef(false);
  const textPreviewAnchorRef = useRef<HTMLDivElement | null>(null);
  const textPreviewHoveringRef = useRef(false);
  const textPreviewReqIdRef = useRef(0);
  const textScrollEmitRafRef = useRef<number | null>(null);
  const textScrollPendingDeltaRef = useRef(0);

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
  const isTextLikeContent =
    item.content_type === "text" || item.content_type === "html" || item.content_type === "rtf";

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
    // Keep reorder motion crisp and avoid bounce-like release feel.
    transition: {
      duration: 120,
      easing: "ease-out",
    },
  });

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
  const clearTextPreviewTimer = useCallback(() => {
    if (textPreviewTimerRef.current) {
      clearTimeout(textPreviewTimerRef.current);
      textPreviewTimerRef.current = null;
    }
  }, []);

  const hideTextPreview = useCallback(() => {
    clearTextPreviewTimer();
    textPreviewHoveringRef.current = false;
    if (textScrollEmitRafRef.current !== null) {
      cancelAnimationFrame(textScrollEmitRafRef.current);
      textScrollEmitRafRef.current = null;
    }
    textScrollPendingDeltaRef.current = 0;
    if (textPreviewVisibleRef.current) {
      textPreviewVisibleRef.current = false;
      invoke("hide_text_preview").catch(() => {});
    }
  }, [clearTextPreviewTimer]);

  const resolveTextPreviewContent = useCallback(async (): Promise<string> => {
    const inlineText = item.text_content || item.preview || "";
    if (!isTextLikeContent) return "";
    if (item.text_content) return item.text_content;
    const cached = getCachedTextPreviewContent(item.id);
    if (cached) return cached;
    try {
      const detail = await invoke<ClipboardItemDetail | null>("get_clipboard_item", { id: item.id });
      const resolved = detail?.text_content || detail?.preview || inlineText;
      if (resolved) {
        setCachedTextPreviewContent(item.id, resolved);
      }
      return resolved;
    } catch {
      return inlineText;
    }
  }, [isTextLikeContent, item.id, item.preview, item.text_content]);

  const showTextPreview = useCallback(async () => {
    if (!textPreviewEnabled || !isTextLikeContent || !textPreviewAnchorRef.current) {
      return;
    }

    const reqId = ++textPreviewReqIdRef.current;
    const textContent = await resolveTextPreviewContent();
    if (!textContent) return;
    if (!textPreviewHoveringRef.current || reqId !== textPreviewReqIdRef.current) return;

    const bounds = await getPreviewBounds(previewPosition, textPreviewAnchorRef.current);
    if (!textPreviewHoveringRef.current || reqId !== textPreviewReqIdRef.current) return;
    const availableCssW = Math.max(260, Math.floor(bounds.maxW / bounds.scale));
    const availableCssH = Math.max(140, Math.floor(bounds.maxH / bounds.scale));
    const sampled = sampleTextPreview(textContent);
    const desiredWidth = sampled.longestVisualCols * TEXT_PREVIEW_CHAR_WIDTH + TEXT_PREVIEW_HORIZONTAL_PADDING;
    const windowCssW = Math.min(
      availableCssW,
      Math.min(TEXT_PREVIEW_MAX_W, Math.max(TEXT_PREVIEW_MIN_W, desiredWidth)),
    );
    const charsPerLine = Math.max(
      TEXT_PREVIEW_MIN_CHARS_PER_LINE,
      Math.floor((windowCssW - 30) / TEXT_PREVIEW_CHAR_WIDTH),
    );
    const sampledWrappedLines = sampled.lineColumns.reduce((sum, lineCols) => {
      return sum + Math.max(1, Math.ceil(lineCols / charsPerLine));
    }, 0);
    let estimatedLines = sampledWrappedLines;
    if (sampled.truncated && sampled.processedCodeUnits < textContent.length) {
      const remaining = textContent.length - sampled.processedCodeUnits;
      const linesPerCodeUnit = sampledWrappedLines / Math.max(1, sampled.processedCodeUnits);
      estimatedLines += Math.max(1, Math.ceil(remaining * linesPerCodeUnit));
    }
    const estimatedCssH = Math.min(
      TEXT_PREVIEW_MAX_H,
      Math.max(TEXT_PREVIEW_MIN_H, estimatedLines * 21 + 40),
    );
    const windowCssH = Math.min(availableCssH, estimatedCssH);
    const winW = Math.max(1, Math.round(windowCssW * bounds.scale));
    const winH = Math.max(1, Math.round(windowCssH * bounds.scale));
    const winX = bounds.side === "left" ? bounds.anchorX - winW : bounds.anchorX;
    const centeredY = Math.round(bounds.cardCenterY - winH / 2);
    const winY = Math.max(bounds.monY, Math.min(centeredY, bounds.monBottom - winH));
    const align = bounds.side === "left" ? "right" : "left";
    const theme =
      document.documentElement.classList.contains("dark") ? "dark" : "light";

    try {
      invoke("hide_image_preview").catch(() => {});
      await invoke("show_text_preview", {
        text: textContent,
        winX,
        winY,
        winWidth: winW,
        winHeight: winH,
        align,
        theme,
        sharpCorners,
      });
      textPreviewVisibleRef.current = true;
    } catch (error) {
      textPreviewVisibleRef.current = false;
      logError("Failed to show text preview:", error);
    }
  }, [textPreviewEnabled, isTextLikeContent, previewPosition, resolveTextPreviewContent, sharpCorners]);

  const handleTextMouseEnter = useCallback(() => {
    if (!textPreviewEnabled || !isTextLikeContent) return;
    textPreviewHoveringRef.current = true;
    clearTextPreviewTimer();
    textPreviewTimerRef.current = setTimeout(showTextPreview, hoverPreviewDelay);
  }, [textPreviewEnabled, isTextLikeContent, clearTextPreviewTimer, showTextPreview, hoverPreviewDelay]);

  const handleTextMouseLeave = useCallback(() => {
    textPreviewHoveringRef.current = false;
    textPreviewReqIdRef.current += 1;
    hideTextPreview();
  }, [hideTextPreview]);

  const handleTextWheel = useCallback((e: React.WheelEvent<HTMLDivElement>) => {
    // Reuse Ctrl+wheel gesture for text preview scrolling to avoid accidental list scrolling.
    if (!e.ctrlKey || !textPreviewVisibleRef.current) return;
    e.preventDefault();
    e.stopPropagation();
    textScrollPendingDeltaRef.current += e.deltaY;

    if (textScrollEmitRafRef.current === null) {
      textScrollEmitRafRef.current = requestAnimationFrame(() => {
        textScrollEmitRafRef.current = null;
        const deltaY = textScrollPendingDeltaRef.current;
        textScrollPendingDeltaRef.current = 0;
        if (deltaY === 0 || !textPreviewVisibleRef.current) return;
        emitTo("text-preview", "text-preview-scroll", { deltaY }).catch(() => {});
      });
    }
  }, []);

  useEffect(() => {
    if (!textPreviewEnabled || !isTextLikeContent) {
      hideTextPreview();
    }
  }, [textPreviewEnabled, isTextLikeContent, hideTextPreview]);

  useEffect(() => {
    if (isDragging) {
      hideTextPreview();
    }
  }, [isDragging, hideTextPreview]);

  useEffect(() => {
    return () => {
      hideTextPreview();
    };
  }, [hideTextPreview]);

  const handlePaste = () => {
    if (!isDragging && !isDragOverlay) {
      hideTextPreview();
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
        "group relative cursor-pointer overflow-hidden shadow-none dark:shadow-[inset_0_0.5px_0_0_rgba(255,255,255,0.09),0_2px_8px_-1px_rgba(0,0,0,0.5)] hover:shadow-sm dark:hover:shadow-[inset_0_0.5px_0_0_rgba(255,255,255,0.12),0_4px_12px_-2px_rgba(0,0,0,0.6)] hover:border-primary/30 ring-1 ring-black/4 dark:ring-white/10",
          isDragOverlay && "shadow-lg border-primary cursor-grabbing",
          // justDropped animation removed for cleaner drag-drop feel
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
              aria-label="拖拽区域"
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
              aria-label="拖拽区域"
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
                className="pointer-events-none absolute inset-y-0 z-6 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity duration-150"
                style={{ left: dragHandleWidth, right: dragHandleWidth }}
              >
                <div className="absolute inset-y-1 left-0 border-l border-dashed border-border/50" />
                <div className="absolute inset-y-1 right-0 border-r border-dashed border-border/50" />
                <div className="rounded border border-dashed border-border/60 bg-background/80 px-2 py-1 text-center">
                  <div className="text-[10px] leading-none text-muted-foreground">粘贴、预览触发区域</div>
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
            <div
              ref={textPreviewAnchorRef}
              className="flex-1 min-w-0 px-3 py-2.5"
              onMouseEnter={handleTextMouseEnter}
              onMouseLeave={handleTextMouseLeave}
              onWheel={handleTextWheel}
            >
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

