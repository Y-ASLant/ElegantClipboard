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
  CheckmarkCircle16Filled,
  Circle16Regular,
  ReOrderDotsVertical16Regular,
  Add16Regular,
  Translate16Regular,
  ArrowSync16Regular,
  Dismiss16Regular,
} from "@fluentui/react-icons";
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useShallow } from "zustand/react/shallow";
import {
  CardFooter,
  FileContent,
  VideoContent,
  getPreviewBounds,
  ImageCard,
} from "@/components/CardContentRenderers";
import {
  ActionToolbar,
  FileDetailsDialog,
  TagAssignSection,
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
import { focusWindowImmediately } from "@/hooks/useInputFocus";
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
import { useTagStore } from "@/stores/tags";
import { useUISettings } from "@/stores/ui-settings";
import { useTranslateSettings } from "@/stores/translate-settings";
import { translateText } from "@/lib/translate";

// ============ 类型定义 ============

interface ClipboardItemCardProps {
  item: ClipboardItem;
  index?: number;
  showBadge?: boolean;
  sortId?: string;
  isDragOverlay?: boolean;
}

const clipboardActions = () => useClipboardStore.getState();

let textPreviewLease = 0;
let textPreviewWanted = false;

export function acquireTextPreviewLease(): number {
  textPreviewLease += 1;
  textPreviewWanted = true;
  return textPreviewLease;
}

export function revokeTextPreviewLease(lease: number): void {
  if (textPreviewLease === lease) {
    textPreviewLease += 1;
    textPreviewWanted = false;
  }
}

export function isTextPreviewLeaseCurrent(lease: number): boolean {
  return textPreviewLease === lease;
}

export function isTextPreviewWanted(): boolean {
  return textPreviewWanted;
}

// ============ 全局 window-hidden 清理注册表 ============
// 替代每张卡片各自订阅 Tauri 事件，改为单一全局监听 + 回调注册
export const textPreviewCleanupCallbacks = new Set<() => void>();
let _windowHiddenListenerInit = false;

export function ensureWindowHiddenListener() {
  if (_windowHiddenListenerInit) return;
  _windowHiddenListenerInit = true;
  listen("window-hidden", () => {
    textPreviewCleanupCallbacks.forEach((cb) => cb());
  });
}

// ============ 标签弹出层 ============

function TagPopover({
  popoverRef,
  tags,
  itemTagIds,
  onToggle,
  onCreateAndAssign,
}: {
  popoverRef: React.RefObject<HTMLDivElement | null>;
  tags: { id: number; name: string }[];
  itemTagIds: Set<number>;
  onToggle: (tagId: number, isAssigned: boolean) => Promise<void>;
  onCreateAndAssign: (name: string) => Promise<void>;
}) {
  const [newName, setNewName] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    // auto-focus input when popover opens, enable OS keyboard focus
    const t = setTimeout(async () => {
      await focusWindowImmediately();
      inputRef.current?.focus();
    }, 50);
    return () => clearTimeout(t);
  }, []);

  const handleCreate = async () => {
    const trimmed = newName.trim();
    if (!trimmed) return;
    await onCreateAndAssign(trimmed);
    setNewName("");
  };

  return (
    <div
      ref={popoverRef}
      className="absolute right-1 top-0 z-30 w-[170px] rounded-lg border bg-popover shadow-lg animate-in fade-in-0 zoom-in-95 duration-100"
      onClick={(e) => e.stopPropagation()}
      onMouseDown={(e) => e.stopPropagation()}
    >
      {/* existing tags */}
      {tags.length > 0 && (
        <div className="max-h-36 overflow-y-auto p-1">
          {tags.map((t) => {
            const isAssigned = itemTagIds.has(t.id);
            return (
              <div
                key={t.id}
                onClick={() => onToggle(t.id, isAssigned)}
                className="flex items-center gap-2 px-2 py-1.5 text-xs rounded-md cursor-default hover:bg-accent hover:text-accent-foreground transition-colors duration-100"
              >
                <span className={cn(
                  "w-3.5 h-3.5 shrink-0 flex items-center justify-center rounded border text-[10px] transition-colors duration-100",
                  isAssigned
                    ? "bg-primary border-primary text-primary-foreground"
                    : "border-muted-foreground/30",
                )}>
                  {isAssigned && "✓"}
                </span>
                <span className="truncate">{t.name}</span>
              </div>
            );
          })}
        </div>
      )}
      {/* new tag input */}
      <div className={cn("px-1.5 py-1.5 flex items-center gap-1", tags.length > 0 && "border-t")}>
        <input
          ref={inputRef}
          type="text"
          placeholder="新建标签…"
          value={newName}
          onChange={(e) => setNewName(e.target.value)}
          onFocus={() => focusWindowImmediately()}
          onKeyDown={(e) => { if (e.key === "Enter") handleCreate(); e.stopPropagation(); }}
          onKeyUp={(e) => e.stopPropagation()}
          onMouseDown={(e) => e.stopPropagation()}
          className="flex-1 min-w-0 h-6 px-1.5 text-xs rounded-md border bg-background outline-none focus:ring-1 focus:ring-ring transition-shadow"
        />
        <button
          onClick={handleCreate}
          onMouseDown={(e) => e.stopPropagation()}
          disabled={!newName.trim()}
          className="shrink-0 w-5 h-5 flex items-center justify-center rounded-md hover:bg-accent text-primary disabled:opacity-30 transition-colors"
        >
          <Add16Regular className="w-3.5 h-3.5" />
        </button>
      </div>
    </div>
  );
}

// ============ 主卡片组件 ============

// 简化的 memo 比较，仅对比影响渲染的关键 props
const arePropsEqual = (
  prevProps: ClipboardItemCardProps,
  nextProps: ClipboardItemCardProps,
) => {
  if (prevProps.index !== nextProps.index) return false;
  if (prevProps.showBadge !== nextProps.showBadge) return false;
  if (prevProps.sortId !== nextProps.sortId) return false;
  if (prevProps.isDragOverlay !== nextProps.isDragOverlay) return false;

  // 对比关键 item 属性
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
  const batchMode = useClipboardStore((s) => s.batchMode);
  const isSelected = useClipboardStore((s) => s.selectedIds.has(item.id));
  const toggleSelect = useClipboardStore((s) => s.toggleSelect);
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
  const {
    cardMaxLines, showTime, showCharCount, showByteSize,
    showSourceApp, sourceAppDisplay, textPreviewEnabled,
    hoverPreviewDelay, previewPosition, sharpCorners, timeFormat,
  } = useUISettings(useShallow((s) => ({
    cardMaxLines: s.cardMaxLines,
    showTime: s.showTime,
    showCharCount: s.showCharCount,
    showByteSize: s.showByteSize,
    showSourceApp: s.showSourceApp,
    sourceAppDisplay: s.sourceAppDisplay,
    textPreviewEnabled: s.textPreviewEnabled,
    hoverPreviewDelay: s.hoverPreviewDelay,
    previewPosition: s.previewPosition,
    sharpCorners: s.sharpCorners,
    timeFormat: s.timeFormat,
  })));

  const [justPasted, setJustPasted] = useState(false);
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [fileListItems, setFileListItems] = useState<FileListItem[]>([]);
  const { tags, addTagToItem, removeTagFromItem, getItemTags, createTag } = useTagStore();
  const [itemTagIds, setItemTagIds] = useState<Set<number>>(new Set());
  const [tagPopoverOpen, setTagPopoverOpen] = useState(false);
  const tagPopoverRef = useRef<HTMLDivElement>(null);
  const translateEnabled = useTranslateSettings((s) => s.enabled);
  const recordTranslation = useTranslateSettings((s) => s.recordTranslation);
  const [translatedText, setTranslatedText] = useState<string | null>(null);
  const [translating, setTranslating] = useState(false);
  const [translateError, setTranslateError] = useState<string | null>(null);

  // close tag popover on click outside
  useEffect(() => {
    if (!tagPopoverOpen) return;
    const onPointerDown = (e: MouseEvent) => {
      if (tagPopoverRef.current && !tagPopoverRef.current.contains(e.target as Node)) {
        setTagPopoverOpen(false);
      }
    };
    document.addEventListener("mousedown", onPointerDown);
    return () => document.removeEventListener("mousedown", onPointerDown);
  }, [tagPopoverOpen]);

  const textPreviewTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const textPreviewVisibleRef = useRef(false);
  const textPreviewAnchorRef = useRef<HTMLDivElement | null>(null);
  const textPreviewHoveringRef = useRef(false);
  const textPreviewReqIdRef = useRef(0);
  const textPreviewLeaseRef = useRef<number | null>(null);
  const textScrollEmitRafRef = useRef<number | null>(null);
  const textScrollPendingDeltaRef = useRef(0);

  const filePaths = useMemo(
    () => (item.content_type === "files" || item.content_type === "video") ? parseFilePaths(item.file_paths) : [],
    [item.content_type, item.file_paths],
  );
  const filesInvalid =
    (item.content_type === "files" || item.content_type === "video") && item.files_valid === false;
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
    disabled: isDragOverlay || batchMode,
    // 保持拖拽动画干净利落
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

  const metaItems = useMemo(() => {
    const items: string[] = [];
    if (showTime) items.push(formatTime(item.created_at, timeFormat));
    if (showCharCount && item.char_count)
      items.push(formatCharCount(item.char_count));
    if (showByteSize) items.push(formatSize(item.byte_size));
    return items;
  }, [showTime, showCharCount, showByteSize, timeFormat, item.created_at, item.char_count, item.byte_size]);

  // ---- 事件处理 ----
  const clearTextPreviewTimer = useCallback(() => {
    if (textPreviewTimerRef.current) {
      clearTimeout(textPreviewTimerRef.current);
      textPreviewTimerRef.current = null;
    }
  }, []);

  const hideTextPreview = useCallback(() => {
    textPreviewReqIdRef.current += 1;
    const closingLease = textPreviewLeaseRef.current;
    if (closingLease !== null) {
      revokeTextPreviewLease(closingLease);
      textPreviewLeaseRef.current = null;
    }
    clearTextPreviewTimer();
    textPreviewHoveringRef.current = false;
    if (textScrollEmitRafRef.current !== null) {
      cancelAnimationFrame(textScrollEmitRafRef.current);
      textScrollEmitRafRef.current = null;
    }
    textScrollPendingDeltaRef.current = 0;
    if (closingLease !== null) {
      textPreviewVisibleRef.current = false;
      invoke("hide_text_preview", { token: closingLease }).catch((error) => {
        logError("Failed to hide text preview:", error);
      });
    } else if (textPreviewVisibleRef.current) {
      textPreviewVisibleRef.current = false;
      invoke("hide_text_preview").catch((error) => {
        logError("Failed to hide text preview:", error);
      });
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
    } catch (error) {
      logError("Failed to load full text content for preview:", error);
      return inlineText;
    }
  }, [isTextLikeContent, item.id, item.preview, item.text_content]);

  const showTextPreview = useCallback(async (reqId: number, lease: number) => {
    if (!textPreviewEnabled || !isTextLikeContent || !textPreviewAnchorRef.current) {
      return;
    }
    if (!textPreviewHoveringRef.current || reqId !== textPreviewReqIdRef.current || !isTextPreviewLeaseCurrent(lease)) return;
    const textContent = await resolveTextPreviewContent();
    if (!textContent) return;
    if (!textPreviewHoveringRef.current || reqId !== textPreviewReqIdRef.current || !isTextPreviewLeaseCurrent(lease)) return;

    const bounds = await getPreviewBounds(previewPosition, textPreviewAnchorRef.current);
    if (!textPreviewHoveringRef.current || reqId !== textPreviewReqIdRef.current || !isTextPreviewLeaseCurrent(lease)) return;
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
      invoke("hide_image_preview").catch((error) => {
        logError("Failed to hide image preview:", error);
      });
      const uiState = useUISettings.getState();
      await invoke("show_text_preview", {
        text: textContent,
        winX,
        winY,
        winWidth: winW,
        winHeight: winH,
        align,
        theme,
        sharpCorners,
        windowEffect: uiState.windowEffect,
        fontFamily: uiState.previewFont || null,
        fontSize: uiState.previewFontSize,
        token: lease,
      });
      if (!textPreviewHoveringRef.current || reqId !== textPreviewReqIdRef.current || !isTextPreviewLeaseCurrent(lease)) {
        textPreviewVisibleRef.current = false;
        if (!isTextPreviewWanted()) {
          invoke("hide_text_preview", { token: lease }).catch((error) => {
            logError("Failed to hide text preview after stale show:", error);
          });
        }
        return;
      }
      textPreviewVisibleRef.current = true;
    } catch (error) {
      textPreviewVisibleRef.current = false;
      logError("Failed to show text preview:", error);
    }
  }, [textPreviewEnabled, isTextLikeContent, previewPosition, resolveTextPreviewContent, sharpCorners]);

  const handleTextMouseEnter = useCallback(() => {
    if (!textPreviewEnabled || !isTextLikeContent || batchMode) return;
    textPreviewHoveringRef.current = true;
    textPreviewReqIdRef.current += 1;
    const reqId = textPreviewReqIdRef.current;
    const lease = acquireTextPreviewLease();
    textPreviewLeaseRef.current = lease;
    clearTextPreviewTimer();
    textPreviewTimerRef.current = setTimeout(() => {
      void showTextPreview(reqId, lease);
    }, hoverPreviewDelay);
  }, [textPreviewEnabled, isTextLikeContent, batchMode, clearTextPreviewTimer, showTextPreview, hoverPreviewDelay]);

  const handleTextMouseLeave = useCallback(() => {
    hideTextPreview();
  }, [hideTextPreview]);

  const handleTextWheel = useCallback((e: React.WheelEvent<HTMLDivElement>) => {
    // Ctrl+滚轮滚动文本预览，避免误触列表滚动
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
        emitTo("text-preview", "text-preview-scroll", { deltaY }).catch((error) => {
          textPreviewVisibleRef.current = false;
          logError("Failed to emit text preview scroll:", error);
        });
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

  // 主窗口隐藏时取消文本预览（通过全局清理注册表，避免每张卡片单独订阅 Tauri 事件）
  useEffect(() => {
    ensureWindowHiddenListener();
    textPreviewCleanupCallbacks.add(hideTextPreview);
    return () => { textPreviewCleanupCallbacks.delete(hideTextPreview); };
  }, [hideTextPreview]);

  const handlePaste = (e: React.MouseEvent) => {
    if (batchMode) {
      toggleSelect(item.id, index ?? 0, e.shiftKey);
      return;
    }
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
    // 图片从 image_path 保存，文件取第一个
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

  const handleEdit = async () => {
    try {
      await invoke("open_text_editor_window", { id: item.id });
    } catch (error) {
      logError("Failed to open editor:", error);
    }
  };

  const handleTranslate = async () => {
    if (translating) return;
    const text = item.text_content || item.preview || "";
    if (!text) return;
    setTranslating(true);
    setTranslateError(null);
    try {
      const result = await translateText(text);
      setTranslatedText(result);
    } catch (error) {
      setTranslateError(String(error));
    } finally {
      setTranslating(false);
    }
  };

  const handleCopyTranslation = async () => {
    if (!translatedText) return;
    try {
      await invoke("write_text_to_clipboard", { text: translatedText, record: recordTranslation });
    } catch (error) {
      logError("Failed to copy translation:", error);
    }
  };

  // ---- 卡片内容 ----

  const cardContent = (
    <div ref={setNodeRef} style={style} className="relative">
      <Card
        className={cn(
        "group relative cursor-pointer overflow-hidden shadow-none dark:shadow-[inset_0_0.5px_0_0_rgba(255,255,255,0.09),0_2px_8px_-1px_rgba(0,0,0,0.5)] hover:shadow-sm dark:hover:shadow-[inset_0_0.5px_0_0_rgba(255,255,255,0.12),0_4px_12px_-2px_rgba(0,0,0,0.6)] hover:border-primary/30 ring-1 ring-black/4 dark:ring-white/10",
          isDragOverlay && "shadow-lg border-primary cursor-grabbing",
          justPasted && "animate-paste-flash",
          isActive && "bg-accent shadow-sm",
          batchMode && isSelected && "bg-primary/5",
          batchMode && !isSelected && "opacity-90",
        )}
        onClick={handlePaste}
      >
        {!isDragging && !isDragOverlay && !batchMode && (
          <button
            ref={setActivatorNodeRef}
            {...attributes}
            {...listeners}
            type="button"
            data-drag-handle="true"
            onClick={(e) => e.stopPropagation()}
            className="absolute inset-y-0 left-0 z-10 w-5 flex items-center justify-center rounded-l-lg cursor-grab active:cursor-grabbing text-muted-foreground/0 hover:text-muted-foreground/70 hover:bg-muted/50 transition-colors duration-150"
            aria-label="拖拽排序"
            tabIndex={-1}
          >
            <ReOrderDotsVertical16Regular className="w-3.5 h-3.5" />
          </button>
        )}
        <div className="flex">
          <div className={cn(
            "flex items-center justify-center shrink-0 overflow-hidden border-r transition-all duration-200 ease-out",
            batchMode ? "w-8 border-border/30" : "w-0 border-transparent"
          )}>
            {isSelected
              ? <CheckmarkCircle16Filled className="w-4.5 h-4.5 text-primary" />
              : <Circle16Regular className="w-4.5 h-4.5 text-muted-foreground/30" />
            }
          </div>
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
          ) : item.content_type === "video" ? (
            <VideoContent
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
          ) : (item.content_type === "files") ? (
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
                className="clipboard-content leading-relaxed text-foreground/90 whitespace-pre-wrap break-all m-0"
                style={{
                  fontFamily: "var(--card-font-family)",
                  fontSize: "var(--card-font-size, 14px)",
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

          {!isDragging && !isDragOverlay && !batchMode && (
            <ActionToolbar
              item={item}
              onTogglePin={handleTogglePin}
              onToggleFavorite={handleToggleFavorite}
              onCopy={handleCopy}
              onDelete={handleDelete}
              onTranslate={translateEnabled && isTextLikeContent ? (e) => {
                e.stopPropagation();
                handleTranslate();
              } : undefined}
              onTag={(e) => {
                e.stopPropagation();
                if (!tagPopoverOpen) {
                  getItemTags(item.id).then((tagList) => {
                    setItemTagIds(new Set(tagList.map((t) => t.id)));
                  });
                }
                setTagPopoverOpen((o) => !o);
              }}
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
      {/* Tag popover rendered outside Card to avoid overflow-hidden clipping */}
      {tagPopoverOpen && (
        <TagPopover
          popoverRef={tagPopoverRef}
          tags={tags}
          itemTagIds={itemTagIds}
          onToggle={async (tagId, isAssigned) => {
            if (isAssigned) {
              await removeTagFromItem(item.id, tagId);
              setItemTagIds((prev) => { const next = new Set(prev); next.delete(tagId); return next; });
            } else {
              await addTagToItem(item.id, tagId);
              setItemTagIds((prev) => new Set([...prev, tagId]));
            }
          }}
          onCreateAndAssign={async (name) => {
            const tag = await createTag(name);
            if (tag) {
              await addTagToItem(item.id, tag.id);
              setItemTagIds((prev) => new Set([...prev, tag.id]));
            }
          }}
        />
      )}
      {/* 翻译结果区域 */}
      {(translating || translatedText || translateError) && (
        <div
          className="mt-1 rounded-lg border bg-muted/50 px-3 py-2 text-xs"
          onClick={(e) => e.stopPropagation()}
        >
          {translating && (
            <div className="flex items-center gap-1.5 text-muted-foreground">
              <ArrowSync16Regular className="w-3.5 h-3.5 animate-spin" />
              <span>翻译中…</span>
            </div>
          )}
          {translateError && !translating && (
            <div className="text-destructive">{translateError}</div>
          )}
          {translatedText && !translating && (
            <div className="relative group/translate">
              <pre
                className="whitespace-pre-wrap break-all text-foreground/90 leading-relaxed m-0"
                style={{
                  fontFamily: "var(--card-font-family)",
                  fontSize: "var(--card-font-size, 14px)",
                }}
              >
                {translatedText}
              </pre>
              <div className="absolute right-0 bottom-0 flex items-center gap-0.5 bg-background/90 rounded-md px-0.5 shadow-sm border opacity-0 group-hover/translate:opacity-100 transition-opacity">
                <Tooltip>
                  <TooltipTrigger asChild>
                    <button
                      onClick={handleCopyTranslation}
                      className="p-1 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors"
                    >
                      <Copy16Regular className="w-3.5 h-3.5" />
                    </button>
                  </TooltipTrigger>
                  <TooltipContent>复制翻译</TooltipContent>
                </Tooltip>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <button
                      onClick={() => { setTranslatedText(null); setTranslateError(null); }}
                      className="p-1 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors"
                    >
                      <Dismiss16Regular className="w-3.5 h-3.5" />
                    </button>
                  </TooltipTrigger>
                  <TooltipContent>关闭</TooltipContent>
                </Tooltip>
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );

  // 上下文菜单配置
  const contextMenuItems: ContextMenuItemConfig[] | null = (() => {
    if (isDragOverlay || batchMode) return null;
    // 文本类内容可编辑
    if (item.content_type === "text" || item.content_type === "html" || item.content_type === "rtf") {
      const items: ContextMenuItemConfig[] = [
        { icon: ClipboardPaste16Regular, label: "粘贴", onClick: () => pasteContent(item.id) },
        { icon: TextDescription16Regular, label: "粘贴为纯文本", onClick: () => pasteAsPlainText(item.id) },
        { icon: Copy16Regular, label: "复制", onClick: handleCopyCtxMenu },
        { icon: Edit16Regular, label: "编辑", onClick: handleEdit },
      ];
      if (translateEnabled) {
        items.push({ icon: Translate16Regular, label: translating ? "翻译中…" : "翻译", onClick: handleTranslate, disabled: translating, separator: true });
      }
      items.push({ icon: Delete16Regular, label: "删除", onClick: () => deleteItem(item.id), destructive: true, separator: !translateEnabled });
      return items;
    }
    if (item.content_type === "files" || item.content_type === "video") {
      return [
        { icon: ClipboardPaste16Regular, label: "粘贴", onClick: () => pasteContent(item.id) },
        { icon: TextDescription16Regular, label: "粘贴为路径", onClick: handlePasteAsPath },
        { icon: FolderOpen16Regular, label: "在资源管理器中显示", onClick: handleShowInExplorer, disabled: filesInvalid },
        { icon: ArrowDownload16Regular, label: "另存为", onClick: handleSaveAs, disabled: filesInvalid },
        { icon: Info16Regular, label: "查看详细信息", onClick: handleShowDetails, disabled: filesInvalid },
        { icon: Delete16Regular, label: "删除", onClick: () => deleteItem(item.id), destructive: true, separator: true },
      ];
    }
    if (item.content_type === "image" && item.image_path) {
      return [
        { icon: ClipboardPaste16Regular, label: "粘贴", onClick: () => pasteContent(item.id) },
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
        <ContextMenu onOpenChange={(open) => {
          if (open) {
            getItemTags(item.id).then((tagList) => {
              setItemTagIds(new Set(tagList.map((t) => t.id)));
            });
          }
        }}>
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
            {/* 标签管理 */}
            <TagAssignSection
              itemId={item.id}
              allTags={tags}
              itemTagIds={itemTagIds}
              onAddTag={async (itemId, tagId) => {
                await addTagToItem(itemId, tagId);
                setItemTagIds((prev) => new Set([...prev, tagId]));
              }}
              onRemoveTag={async (itemId, tagId) => {
                await removeTagFromItem(itemId, tagId);
                setItemTagIds((prev) => { const next = new Set(prev); next.delete(tagId); return next; });
              }}
            />
          </ContextMenuContent>
        </ContextMenu>
        {(item.content_type === "files" || item.content_type === "video") && (
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

