import { memo, useCallback, useEffect, useState, useRef, useMemo } from "react";
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
  Translate16Regular,
} from "@fluentui/react-icons";
import { invoke } from "@tauri-apps/api/core";
import { useShallow } from "zustand/react/shallow";
import {
  CardFooter,
  FileContent,
  ImageCard,
} from "@/components/CardContentRenderers";
import {
  ActionToolbar,
  type FileListItem,
  type ContextMenuItemConfig,
} from "@/components/CardSubComponents";
import { TagPopover } from "@/components/clipboard/TagPopover";
import { ClipboardContextMenu } from "@/components/ClipboardContextMenu";
import { HighlightText } from "@/components/HighlightText";
import { type ClipboardItemDetail } from "@/components/text-preview";
import { TranslationResult } from "@/components/TranslationResult";
import { TtsHighlightText } from "@/components/TtsHighlightText";
import { Card } from "@/components/ui/card";
import { VideoContent } from "@/components/VideoCard";
import { useSortable, CSS } from "@/hooks/useSortableList";
import { useTextPreview } from "@/hooks/useTextPreview";
import { cachedCheckFilesExist } from "@/lib/file-check-cache";
import {
  contentTypeConfig,
  formatTime,
  formatCharCount,
  formatSize,
  getFileNameFromPath,
  getLogicalContentType,
  parseFilePaths,
} from "@/lib/format";
import { logError } from "@/lib/logger";
import { translateText } from "@/lib/translate";
import { speak, stopSpeaking, isSpeaking } from "@/lib/tts";
import { cn } from "@/lib/utils";
import { useClipboardStore, ClipboardItem } from "@/stores/clipboard";
import { useTagStore } from "@/stores/tags";
import { useTranslateSettings } from "@/stores/translate-settings";
import { useTtsSettings } from "@/stores/tts-settings";
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

// 文本预览租约管理和清理回调已提取到 useTextPreview hook
export {
  acquireTextPreviewLease,
  revokeTextPreviewLease,
  isTextPreviewLeaseCurrent,
  isTextPreviewWanted,
  textPreviewCleanupCallbacks,
  ensureWindowHiddenListener,
} from "@/hooks/useTextPreview";

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
    showSourceApp, sourceAppDisplay, timeFormat,
  } = useUISettings(useShallow((s) => ({
    cardMaxLines: s.cardMaxLines,
    showTime: s.showTime,
    showCharCount: s.showCharCount,
    showByteSize: s.showByteSize,
    showSourceApp: s.showSourceApp,
    sourceAppDisplay: s.sourceAppDisplay,
    timeFormat: s.timeFormat,
  })));

  const [justPasted, setJustPasted] = useState(false);
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [fileListItems, setFileListItems] = useState<FileListItem[]>([]);
  const [localTags, setLocalTags] = useState<{ id: number; name: string }[]>([]);
  const [itemTagIds, setItemTagIds] = useState<Set<number>>(new Set());
  const [tagPopoverOpen, setTagPopoverOpen] = useState(false);
  const tagPopoverRef = useRef<HTMLDivElement>(null);
  const translateEnabled = useTranslateSettings((s) => s.enabled);
  const ttsToolbarEnabled = useTtsSettings((s) => s.enabled && s.showToolbarTts);
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

  const style = useMemo<React.CSSProperties>(() => ({
    transform: CSS.Transform.toString(transform),
    transition: transition || undefined,
    opacity: isDragging ? 0 : 1,
    cursor: isDragging ? "grabbing" : "pointer",
    zIndex: isDragging ? 1000 : "auto",
  }), [transform, transition, isDragging]);

  const config = contentTypeConfig[item.content_type] || contentTypeConfig.text;

  const metaItems = useMemo(() => {
    const items: string[] = [];
    if (showTime) items.push(formatTime(item.created_at, timeFormat));
    if (showCharCount && item.char_count)
      items.push(formatCharCount(item.char_count));
    if (showByteSize) items.push(formatSize(item.byte_size));
    return items;
  }, [showTime, showCharCount, showByteSize, timeFormat, item.created_at, item.char_count, item.byte_size]);

  const preStyle = useMemo<React.CSSProperties>(() => ({
    fontFamily: "var(--card-font-family)",
    fontSize: "var(--card-font-size, 14px)",
    display: "-webkit-box",
    WebkitLineClamp: cardMaxLines,
    WebkitBoxOrient: "vertical",
    overflow: "hidden",
  }), [cardMaxLines]);

  // ---- 文本预览（hook） ----
  const {
    textPreviewAnchorRef,
    handleTextMouseEnter,
    handleTextMouseLeave,
    handleTextWheel,
    hideTextPreview,
  } = useTextPreview({
    itemId: item.id,
    textContent: item.text_content,
    preview: item.preview,
    isTextLikeContent,
    isDragging,
  });

  // ---- 事件处理 ----
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
  const handleCopy = useCallback((e: React.MouseEvent) => {
    e.stopPropagation();
    copyToClipboard(item.id);
  }, [item.id, copyToClipboard]);
  const handleCopyCtxMenu = useCallback(() => copyToClipboard(item.id), [item.id, copyToClipboard]);
  const handleTogglePin = useCallback((e: React.MouseEvent) => {
    e.stopPropagation();
    togglePin(item.id);
  }, [item.id, togglePin]);
  const handleToggleFavorite = useCallback((e: React.MouseEvent) => {
    e.stopPropagation();
    toggleFavorite(item.id);
  }, [item.id, toggleFavorite]);
  const handleDelete = useCallback((e: React.MouseEvent) => {
    e.stopPropagation();
    deleteItem(item.id);
  }, [item.id, deleteItem]);

  const handleShowInExplorer = useCallback(async () => {
    if (filePaths.length > 0) {
      try {
        await invoke("show_in_explorer", { path: filePaths[0] });
      } catch (error) {
        logError("Failed to show in explorer:", error);
      }
    }
  }, [filePaths]);

  const handlePasteAsPath = useCallback(async () => {
    try {
      await invoke("paste_as_path", { id: item.id });
    } catch (error) {
      logError("Failed to paste as path:", error);
    }
  }, [item.id]);

  const handleShowDetails = useCallback(async () => {
    if (filePaths.length === 0) return;
    try {
      const checkResult = await cachedCheckFilesExist(filePaths);
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
  }, [filePaths]);

  const handleSaveAs = useCallback(async () => {
    // 图片从 image_path 保存，文件取第一个
    const sourcePath =
      item.content_type === "image" ? item.image_path : filePaths[0];
    if (!sourcePath) return;
    try {
      await invoke("save_file_as", { sourcePath });
    } catch (error) {
      logError("Failed to save file:", error);
    }
  }, [item.content_type, item.image_path, filePaths]);

  const handleShowImageInExplorer = useCallback(async () => {
    if (!item.image_path) return;
    try {
      await invoke("show_in_explorer", { path: item.image_path });
    } catch (error) {
      logError("Failed to show in explorer:", error);
    }
  }, [item.image_path]);

  const handleEdit = useCallback(async () => {
    try {
      await invoke("open_text_editor_window", { id: item.id });
    } catch (error) {
      logError("Failed to open editor:", error);
    }
  }, [item.id]);

  const handleTranslate = useCallback(async () => {
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
  }, [translating, item.text_content, item.preview]);

  const handleCopyTranslation = useCallback(async () => {
    if (!translatedText) return;
    try {
      await invoke("write_text_to_clipboard", { text: translatedText, record: useTranslateSettings.getState().recordTranslation });
    } catch (error) {
      logError("Failed to copy translation:", error);
    }
  }, [translatedText]);

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
                style={preStyle}
              >
                {ttsToolbarEnabled ? (
                  <TtsHighlightText
                    text={item.preview || item.text_content || `[${config.label}]`}
                    fallback={<HighlightText text={item.preview || item.text_content || `[${config.label}]`} />}
                  />
                ) : (
                  <HighlightText text={item.preview || item.text_content || `[${config.label}]`} />
                )}
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
              onTts={ttsToolbarEnabled && isTextLikeContent && getLogicalContentType(item) === "text" ? (e) => {
                e.stopPropagation();
                if (isSpeaking()) { stopSpeaking(); return; }
                (async () => {
                  let txt = item.text_content || "";
                  if (!txt) {
                    try {
                      const detail = await invoke<ClipboardItemDetail | null>("get_clipboard_item", { id: item.id });
                      txt = detail?.text_content || detail?.preview || item.preview || "";
                    } catch { /* fallback to preview */ txt = item.preview || ""; }
                  }
                  if (txt.trim()) speak(txt, "en-US");
                })();
              } : undefined}
              onTranslate={translateEnabled && isTextLikeContent && getLogicalContentType(item) === "text" ? (e) => {
                e.stopPropagation();
                handleTranslate();
              } : undefined}
              onTag={(e) => {
                e.stopPropagation();
                if (!tagPopoverOpen) {
                  const tagStore = useTagStore.getState();
                  // 如果标签列表为空（未打开过标签管理），先从后端加载
                  const loadAndSet = (tags: { id: number; name: string }[]) => {
                    setLocalTags(tags);
                    tagStore.getItemTags(item.id).then((tagList) => {
                      setItemTagIds(new Set(tagList.map((t) => t.id)));
                    });
                  };
                  if (tagStore.tags.length === 0) {
                    tagStore.fetchTags().then(() => {
                      loadAndSet(useTagStore.getState().tags);
                    });
                  } else {
                    loadAndSet(tagStore.tags);
                  }
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
          tags={localTags}
          itemTagIds={itemTagIds}
          onToggle={async (tagId, isAssigned) => {
            const tagStore = useTagStore.getState();
            if (isAssigned) {
              await tagStore.removeTagFromItem(item.id, tagId);
              setItemTagIds((prev) => { const next = new Set(prev); next.delete(tagId); return next; });
            } else {
              await tagStore.addTagToItem(item.id, tagId);
              setItemTagIds((prev) => new Set([...prev, tagId]));
            }
          }}
          onCreateAndAssign={async (name) => {
            const tagStore = useTagStore.getState();
            const tag = await tagStore.createTag(name);
            if (tag) {
              await tagStore.addTagToItem(item.id, tag.id);
              setItemTagIds((prev) => new Set([...prev, tag.id]));
            }
          }}
        />
      )}
      {/* 翻译结果区域 */}
      <TranslationResult
        translating={translating}
        translatedText={translatedText}
        translateError={translateError}
        onCopy={handleCopyTranslation}
        onDismiss={() => { setTranslatedText(null); setTranslateError(null); }}
      />
    </div>
  );

  // 上下文菜单配置（仅在依赖变化时重新计算）
  const contextMenuItems = useMemo<ContextMenuItemConfig[] | null>(() => {
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
  }, [isDragOverlay, batchMode, item.content_type, item.id, item.image_path, filesInvalid, translateEnabled, translating]);

  if (contextMenuItems) {
    return (
      <ClipboardContextMenu
        contextMenuItems={contextMenuItems}
        itemId={item.id}
        contentType={item.content_type}
        localTags={localTags}
        itemTagIds={itemTagIds}
        setLocalTags={setLocalTags}
        setItemTagIds={setItemTagIds}
        detailsOpen={detailsOpen}
        setDetailsOpen={setDetailsOpen}
        fileListItems={fileListItems}
      >
        {cardContent}
      </ClipboardContextMenu>
    );
  }

  return cardContent;
}, arePropsEqual);

