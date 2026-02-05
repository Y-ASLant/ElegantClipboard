import { memo } from "react";
import { useClipboardStore, ClipboardItem } from "@/stores/clipboard";
import { useUISettings } from "@/stores/ui-settings";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import {
  Pin16Regular,
  Pin16Filled,
  Star16Regular,
  Star16Filled,
  Delete16Regular,
  Copy16Regular,
  Image16Regular,
} from "@fluentui/react-icons";

interface ClipboardItemCardProps {
  item: ClipboardItem;
  index?: number;
}

const contentTypeConfig: Record<string, { label: string }> = {
  text: { label: "文本" },
  html: { label: "HTML" },
  rtf: { label: "RTF" },
  image: { label: "图片" },
  files: { label: "文件" },
};

function formatTime(dateStr: string): string {
  const date = new Date(dateStr);
  const now = new Date();
  const isToday = date.toDateString() === now.toDateString();
  
  const hours = date.getHours().toString().padStart(2, '0');
  const minutes = date.getMinutes().toString().padStart(2, '0');
  const time = `${hours}:${minutes}`;
  
  if (isToday) {
    return `今天 ${time}`;
  }
  
  const yesterday = new Date(now);
  yesterday.setDate(yesterday.getDate() - 1);
  if (date.toDateString() === yesterday.toDateString()) {
    return `昨天 ${time}`;
  }
  
  const month = (date.getMonth() + 1).toString().padStart(2, '0');
  const day = date.getDate().toString().padStart(2, '0');
  return `${month}-${day} ${time}`;
}

function formatCharCount(text: string | null): string {
  if (!text) return "0 字符";
  const count = text.length;
  if (count >= 10000) {
    return `${(count / 10000).toFixed(1)}万 字符`;
  }
  return `${count.toLocaleString()} 字符`;
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

// Use selectors for stable references to avoid unnecessary re-renders
const clipboardActions = () => useClipboardStore.getState();

export const ClipboardItemCard = memo(function ClipboardItemCard({ item, index }: ClipboardItemCardProps) {
  // Get actions via getState() to avoid subscribing to store changes
  const { togglePin, toggleFavorite, deleteItem, copyToClipboard, pasteContent } = clipboardActions();
  // Subscribe only to specific UI settings using shallow comparison
  const cardMaxLines = useUISettings((s) => s.cardMaxLines);
  const showTime = useUISettings((s) => s.showTime);
  const showCharCount = useUISettings((s) => s.showCharCount);
  const showByteSize = useUISettings((s) => s.showByteSize);

  const config = contentTypeConfig[item.content_type] || contentTypeConfig.text;
  
  // Build meta info array
  const metaItems: string[] = [];
  if (showTime) metaItems.push(formatTime(item.created_at));
  if (showCharCount && item.text_content) metaItems.push(formatCharCount(item.text_content));
  if (showByteSize) metaItems.push(formatSize(item.byte_size));

  // Click card to paste directly
  const handlePaste = () => {
    pasteContent(item.id);
  };

  const handleCopy = (e: React.MouseEvent) => {
    e.stopPropagation();
    copyToClipboard(item.id);
  };

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

  return (
    <Card
      className={cn(
        "group relative cursor-pointer transition-all duration-150 overflow-hidden",
        "hover:shadow-md hover:border-primary/30"
      )}
      onClick={handlePaste}
    >
      <div className="flex">
        {/* Content - Different layout for images */}
        {item.content_type === "image" && item.image_path ? (
          <div className="flex-1 min-w-0 px-3 py-2.5">
            {/* Image Thumbnail - Base64 inline for instant display */}
            <div className="relative w-full h-20 rounded overflow-hidden bg-muted/30 flex items-center justify-center">
              {item.preview?.startsWith("data:image") ? (
                <img
                  src={item.preview}
                  alt="Preview"
                  className="w-full h-full object-contain"
                />
              ) : (
                <Image16Regular className="w-8 h-8 text-muted-foreground/40" />
              )}
            </div>
            
            {/* Meta Info */}
            {metaItems.length > 0 && (
              <div className="flex items-center gap-1.5 text-xs text-muted-foreground mt-1.5">
                {metaItems.map((info, i) => (
                  <span key={i} className="flex items-center gap-1.5">
                    {i > 0 && <span className="text-muted-foreground/50">·</span>}
                    {info}
                  </span>
                ))}
              </div>
            )}
          </div>
        ) : (
          <div className="flex-1 min-w-0 px-3 py-2.5">
            {/* Preview Text */}
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
            
            {/* Meta Info */}
            {metaItems.length > 0 && (
              <div className="flex items-center gap-1.5 text-xs text-muted-foreground mt-1.5">
                {metaItems.map((info, i) => (
                  <span key={i} className="flex items-center gap-1.5">
                    {i > 0 && <span className="text-muted-foreground/50">·</span>}
                    {info}
                  </span>
                ))}
              </div>
            )}
          </div>
        )}

        {/* Actions - show on hover using CSS */}
        <div className="absolute right-1 top-1 flex items-center gap-0.5 bg-background/95 rounded-md px-0.5 shadow-sm border opacity-0 group-hover:opacity-100 transition-opacity">
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                onClick={handleTogglePin}
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
                onClick={handleToggleFavorite}
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
                onClick={handleCopy}
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
                onClick={handleDelete}
                className="h-6 w-6 hover:text-destructive"
              >
                <Delete16Regular className="w-3.5 h-3.5" />
              </Button>
            </TooltipTrigger>
            <TooltipContent>删除</TooltipContent>
          </Tooltip>
        </div>

        {/* Pinned Badge - hide on hover when action buttons show */}
        {item.is_pinned && (
          <div className="absolute right-2 top-2 inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-medium bg-blue-50 text-blue-500 border border-blue-100 dark:bg-blue-950 dark:text-blue-400 dark:border-blue-900 opacity-100 group-hover:opacity-0 transition-opacity">
            已置顶
          </div>
        )}

        {/* Index Badge */}
        {index !== undefined && (
          <div className="absolute right-2 bottom-2 min-w-5 h-5 px-1.5 rounded-full bg-primary/10 flex items-center justify-center">
            <span className="text-[10px] font-semibold text-primary">{index + 1}</span>
          </div>
        )}
      </div>
    </Card>
  );
}, (prevProps, nextProps) => {
  // Custom comparison: only re-render if item data or index actually changed
  return (
    prevProps.index === nextProps.index &&
    prevProps.item.id === nextProps.item.id &&
    prevProps.item.is_pinned === nextProps.item.is_pinned &&
    prevProps.item.is_favorite === nextProps.item.is_favorite &&
    prevProps.item.preview === nextProps.item.preview &&
    prevProps.item.content_type === nextProps.item.content_type &&
    prevProps.item.created_at === nextProps.item.created_at &&
    prevProps.item.byte_size === nextProps.item.byte_size &&
    prevProps.item.text_content === nextProps.item.text_content &&
    prevProps.item.image_path === nextProps.item.image_path
  );
});
