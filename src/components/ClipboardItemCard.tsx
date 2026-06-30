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
  Translate16Regular,
  CheckmarkCircle16Filled,
  Circle16Regular,
} from "@fluentui/react-icons";
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { useShallow } from "zustand/react/shallow";
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
import { useTranslation } from "@/i18n";
import {
  contentTypeConfig,
  formatTime,
  formatCharCount,
  formatSize,
  getFileNameFromPath,
  parseFilePaths,
} from "@/lib/format";
import { createLeaseManager } from "@/lib/lease-manager";
import { logError } from "@/lib/logger";
import { translateText } from "@/lib/translate";
import { cn } from "@/lib/utils";
import { useClipboardStore, ClipboardItem } from "@/stores/clipboard";
import { useGroupStore } from "@/stores/groups";
import { useTranslateSettings } from "@/stores/translate-settings";
import { useUISettings } from "@/stores/ui-settings";

// ============ 类型定义 ============

interface ClipboardItemCardProps {
  item: ClipboardItem;
  index?: number;
  showBadge?: boolean;
  sortId?: string;
  isDragOverlay?: boolean;
}

const clipboardActions = () => useClipboardStore.getState();

// LRU 缓存：文件有效性检查结果，上限 500 条
const FILE_VALIDITY_CACHE_MAX = 500;
const fileValidityCache = new Map<string, boolean>();
function setFileValidityCache(key: string, value: boolean) {
  if (fileValidityCache.size >= FILE_VALIDITY_CACHE_MAX) {
    // 删除最早的条目
    const firstKey = fileValidityCache.keys().next().value;
    if (firstKey !== undefined) fileValidityCache.delete(firstKey);
  }
  fileValidityCache.set(key, value);
}
const textPreviewLM = createLeaseManager("allocate_text_preview_lease");

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
  const { t } = useTranslation();
  // 每张卡片自行订阅 activeIndex，只有选中态变化的卡片才重渲染
  const isActiveIndex = useClipboardStore(
    (s) => index !== undefined && index >= 0 && s.activeIndex === index,
  );
  const batchMode = useClipboardStore((s) => s.batchMode);
  const isSelected = useClipboardStore((s) => s.selectedIds.has(item.id));
  const toggleSelect = useClipboardStore((s) => s.toggleSelect);
  const keyboardNavEnabled = useUISettings((s) => s.keyboardNavigation);
  const isActive = isActiveIndex && keyboardNavEnabled;

  // 合并高频变化的UI设置为单次订阅，避免17个独立selector的开销
  const uiSettings = useUISettings(
    useShallow((s) => ({
      cardMaxLines: s.cardMaxLines,
      showTime: s.showTime,
      showCharCount: s.showCharCount,
      showByteSize: s.showByteSize,
      showSourceApp: s.showSourceApp,
      sourceAppDisplay: s.sourceAppDisplay,
      showDragAreaIndicator: s.showDragAreaIndicator,
      textPreviewEnabled: s.textPreviewEnabled,
      hoverPreviewDelay: s.hoverPreviewDelay,
      previewPosition: s.previewPosition,
      sharpCorners: s.sharpCorners,
      timeFormat: s.timeFormat,
    })),
  );
  const {
    cardMaxLines,
    showTime,
    showCharCount,
    showByteSize,
    showSourceApp,
    sourceAppDisplay,
    showDragAreaIndicator,
    textPreviewEnabled,
    hoverPreviewDelay,
    previewPosition,
    sharpCorners,
    timeFormat,
  } = uiSettings;
  const {
    togglePin,
    toggleFavorite,
    deleteItem,
    copyToClipboard,
    pasteContent,
    pasteAsPlainText,
  } = clipboardActions();

  const translateEnabled = useTranslateSettings((s) => s.enabled);
  const [translateStatus, setTranslateStatus] = useState<"idle" | "loading" | "done" | "error">("idle");
  const [translatedText, setTranslatedText] = useState("");

  const [justPasted, setJustPasted] = useState(false);
  const [justCopied, setJustCopied] = useState(false);
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [fileListItems, setFileListItems] = useState<FileListItem[]>([]);
  const { groups, moveItemToGroup } = useGroupStore(
    useShallow((s) => ({ groups: s.groups, moveItemToGroup: s.moveItemToGroup })),
  );
  const selectedGroupId = useClipboardStore((s) => s.selectedGroupId);
  const textPreviewTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const textPreviewVisibleRef = useRef(false);
  const textPreviewAnchorRef = useRef<HTMLDivElement | null>(null);
  const textPreviewHoveringRef = useRef(false);
  const textPreviewReqIdRef = useRef(0);
  const textPreviewLeaseRef = useRef<number | null>(null);
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
        setFileValidityCache(cacheKey, allExist);
        if (!cancelled) setRuntimeFilesValid(allExist);
      })
      .catch((error) => {
        logError("Failed to check files exist:", error);
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
    cursor: isDragging ? "move" : "pointer",
    zIndex: isDragging ? 1000 : "auto",
  };

  const config = contentTypeConfig[item.content_type] || contentTypeConfig.text;
  const dragHandleWidth = "clamp(40px, 14%, 72px)";


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
      textPreviewLM.revoke(closingLease);
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
    if (!textPreviewHoveringRef.current || reqId !== textPreviewReqIdRef.current || !textPreviewLM.isCurrent(lease)) return;
    const textContent = await resolveTextPreviewContent();
    if (!textContent) return;
    if (!textPreviewHoveringRef.current || reqId !== textPreviewReqIdRef.current || !textPreviewLM.isCurrent(lease)) return;

    const bounds = await getPreviewBounds(previewPosition, textPreviewAnchorRef.current);
    if (!textPreviewHoveringRef.current || reqId !== textPreviewReqIdRef.current || !textPreviewLM.isCurrent(lease)) return;
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
      if (!textPreviewHoveringRef.current || reqId !== textPreviewReqIdRef.current || !textPreviewLM.isCurrent(lease)) {
        textPreviewVisibleRef.current = false;
        if (!textPreviewLM.isWanted()) {
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
    clearTextPreviewTimer();
    void (async () => {
      const lease = await textPreviewLM.acquire();
      // 异步分配期间用户可能已离开或触发新悬停，重新校验后再装定时器
      if (!textPreviewHoveringRef.current || reqId !== textPreviewReqIdRef.current) {
        textPreviewLM.revoke(lease);
        return;
      }
      textPreviewLeaseRef.current = lease;
      textPreviewTimerRef.current = setTimeout(() => {
        void showTextPreview(reqId, lease);
      }, hoverPreviewDelay);
    })();
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

  // 主窗口隐藏时取消文本预览计时器
  useEffect(() => {
    const unlisten = listen("window-hidden", () => {
      hideTextPreview();
    });
    return () => { unlisten.then((fn) => fn()); };
  }, [hideTextPreview]);

  const triggerTranslate = useCallback(async (forceRedo = false) => {
    let shouldTranslate = true;
    setTranslateStatus((prev) => {
      if (prev !== "idle" && !forceRedo) { shouldTranslate = false; return "idle"; }
      return "loading";
    });
    setTranslatedText("");
    if (!shouldTranslate) return;
    try {
      const text = await resolveTextPreviewContent();
      if (!text.trim()) { setTranslateStatus("idle"); return; }
      const result = await translateText(text);
      setTranslatedText(result);
      setTranslateStatus("done");
    } catch (error) {
      setTranslatedText(String(error));
      setTranslateStatus("error");
    }
  }, [resolveTextPreviewContent]);

  const handleTranslateClick = useCallback((e: React.MouseEvent) => {
    e.stopPropagation();
    triggerTranslate();
  }, [triggerTranslate]);

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
    setJustCopied(true);
    setTimeout(() => setJustCopied(false), 700);
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

  // ---- 卡片内容 ----

  const cardContent = (
    <div ref={setNodeRef} style={style}>
      <Card
        className={cn(
        "group relative cursor-pointer overflow-hidden shadow-sm hover:shadow-md hover:border-primary/30 ring-1 ring-black/4 dark:ring-white/10",
          isDragOverlay && "shadow-lg border-primary cursor-move",
          justPasted && "animate-paste-flash",
          isActive && "bg-accent shadow-sm",
          batchMode && isSelected && "bg-primary/5",
          batchMode && !isSelected && "opacity-90",
        )}
        onClick={handlePaste}
      >
        {justPasted && <div className="paste-flash-overlay" />}
        {justCopied && (
          <div className="copy-success-overlay">
            <CheckmarkCircle16Filled className="copy-success-icon w-8 h-8 text-primary" />
          </div>
        )}
        {!isDragging && !isDragOverlay && !batchMode && (
          <>
            <button
              ref={setActivatorNodeRef}
              {...attributes}
              {...listeners}
              type="button"
              data-drag-handle="true"
              onClick={(e) => e.stopPropagation()}
              className={cn(
                "absolute inset-y-0 left-0 z-10 flex items-center justify-center rounded-l-lg cursor-move",
                showDragAreaIndicator
                  ? "border-r border-dashed border-primary/40 bg-primary/15 text-primary opacity-0 group-hover:opacity-90 transition-[opacity,colors] duration-150 hover:bg-primary/25 hover:text-primary"
                  : "border-r border-transparent bg-transparent text-transparent opacity-0",
              )}
              style={{ width: dragHandleWidth }}
              aria-label={t("clipboard.card.dragAreaAria")}
              tabIndex={showDragAreaIndicator ? 0 : -1}
            >
              <span
                aria-hidden
                className="pointer-events-none text-[10px] leading-tight text-center text-primary/80"
              >
                {t("clipboard.card.dragArea")}
              </span>
            </button>

            <button
              {...attributes}
              {...listeners}
              type="button"
              data-drag-handle="true"
              onClick={(e) => e.stopPropagation()}
              className={cn(
                "absolute inset-y-0 right-0 z-10 flex items-center justify-center rounded-r-lg cursor-move",
                showDragAreaIndicator
                  ? "border-l border-dashed border-primary/40 bg-primary/15 text-primary opacity-0 group-hover:opacity-90 transition-[opacity,colors] duration-150 hover:bg-primary/25 hover:text-primary"
                  : "border-l border-transparent bg-transparent text-transparent opacity-0",
              )}
              style={{ width: dragHandleWidth }}
              aria-label={t("clipboard.card.dragAreaAria")}
              tabIndex={showDragAreaIndicator ? 0 : -1}
            >
              <span
                aria-hidden
                className="pointer-events-none text-[10px] leading-tight text-center text-primary/80"
              >
                {t("clipboard.card.dragArea")}
              </span>
            </button>

            {showDragAreaIndicator && (
              <div
                aria-hidden
                className="pointer-events-none absolute inset-y-0 z-6 flex items-center justify-center bg-amber-500/12 opacity-0 group-hover:opacity-100 transition-opacity duration-150"
                style={{ left: dragHandleWidth, right: dragHandleWidth }}
              >
                <div className="text-center">
                  <div className="text-[10px] leading-none text-amber-700/80 dark:text-amber-300/80">{t("clipboard.card.pasteZoneHint")}</div>
                  <div className="mt-0.5 text-[10px] leading-none text-amber-700/80 dark:text-amber-300/80">{t("clipboard.card.clickToPaste")}</div>
                  <div className="mt-0.5 text-[10px] leading-none text-amber-700/80 dark:text-amber-300/80">{t("clipboard.card.disableInSettings")}</div>
                </div>
              </div>
            )}
          </>
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
              onTranslate={translateEnabled && isTextLikeContent ? handleTranslateClick : undefined}
              translateActive={translateStatus !== "idle"}
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
      {translateStatus !== "idle" && (
        <div
          className="mt-1 rounded-md border bg-muted/40 px-3 py-2"
          onClick={(e) => e.stopPropagation()}
        >
          {translateStatus === "loading" && (
            <span className="text-xs text-muted-foreground">{t("clipboard.card.translating")}</span>
          )}
          {(translateStatus === "done" || translateStatus === "error") && (
            <p className={cn(
              "text-sm leading-relaxed whitespace-pre-wrap select-text cursor-text",
              translateStatus === "error" && "text-destructive",
            )}>{translatedText}</p>
          )}
        </div>
      )}
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
    if (isDragOverlay || batchMode) return null;
    // 文本类内容（text/html/rtf）可编辑
    if (item.content_type === "text" || item.content_type === "html" || item.content_type === "rtf") {
      return [
        { icon: ClipboardPaste16Regular, label: t("clipboard.contextMenu.paste"), onClick: () => pasteContent(item.id) },
        { icon: TextDescription16Regular, label: t("clipboard.contextMenu.pastePlainText"), onClick: () => pasteAsPlainText(item.id) },
        { icon: Copy16Regular, label: t("clipboard.contextMenu.copy"), onClick: handleCopyCtxMenu },
        ...(translateEnabled ? [{ icon: Translate16Regular, label: t("clipboard.contextMenu.translate"), onClick: () => triggerTranslate(true) }] : []),
        { icon: Edit16Regular, label: t("clipboard.contextMenu.edit"), onClick: handleEdit },
        { icon: Delete16Regular, label: t("clipboard.contextMenu.delete"), onClick: () => deleteItem(item.id), destructive: true, separator: true },
      ];
    }
    if (item.content_type === "files") {
      return [
        { icon: ClipboardPaste16Regular, label: t("clipboard.contextMenu.paste"), onClick: () => pasteContent(item.id) },
        { icon: TextDescription16Regular, label: t("clipboard.contextMenu.pasteAsPath"), onClick: handlePasteAsPath },
        { icon: FolderOpen16Regular, label: t("clipboard.contextMenu.showInExplorer"), onClick: handleShowInExplorer, disabled: filesInvalid },
        { icon: ArrowDownload16Regular, label: t("clipboard.contextMenu.saveAs"), onClick: handleSaveAs, disabled: filesInvalid },
        { icon: Info16Regular, label: t("clipboard.contextMenu.viewDetails"), onClick: handleShowDetails, disabled: filesInvalid },
        { icon: Delete16Regular, label: t("clipboard.contextMenu.delete"), onClick: () => deleteItem(item.id), destructive: true, separator: true },
      ];
    }
    if (item.content_type === "image" && item.image_path) {
      return [
        { icon: ClipboardPaste16Regular, label: t("clipboard.contextMenu.paste"), onClick: () => pasteContent(item.id) },
        { icon: Copy16Regular, label: t("clipboard.contextMenu.copy"), onClick: handleCopyCtxMenu },
        { icon: FolderOpen16Regular, label: t("clipboard.contextMenu.showInExplorer"), onClick: handleShowImageInExplorer },
        { icon: ArrowDownload16Regular, label: t("clipboard.contextMenu.saveAs"), onClick: handleSaveAs },
        { icon: Delete16Regular, label: t("clipboard.contextMenu.delete"), onClick: () => deleteItem(item.id), destructive: true, separator: true },
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

