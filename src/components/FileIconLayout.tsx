// 文件/视频图标+文件名共享布局组件

import { memo } from "react";
import type { FluentIcon } from "@fluentui/react-icons";
import {
  Folder16Regular,
  Warning16Regular,
} from "@fluentui/react-icons";
import { CardFooter } from "@/components/CardContentRenderers";
import { HighlightText } from "@/components/HighlightText";
import { getFileNameFromPath } from "@/lib/format";
import { cn } from "@/lib/utils";

type ColorScheme = "blue" | "purple";

const colorMap: Record<ColorScheme, { bg: string; icon: string }> = {
  blue: { bg: "bg-blue-50 dark:bg-blue-950", icon: "text-blue-500" },
  purple: { bg: "bg-purple-50 dark:bg-purple-950", icon: "text-purple-500" },
};

interface FileIconLayoutProps {
  filePaths: string[];
  filesInvalid: boolean;
  preview: string | null;
  metaItems: string[];
  index?: number;
  showBadge?: boolean;
  isDragOverlay?: boolean;
  sourceAppName?: string | null;
  sourceAppIcon?: string | null;
  colorScheme: ColorScheme;
  singleIcon: FluentIcon;
  multiLabel: string;
}

export const FileIconLayout = memo(function FileIconLayout({
  filePaths,
  filesInvalid,
  preview,
  metaItems,
  index,
  showBadge,
  isDragOverlay,
  sourceAppName,
  sourceAppIcon,
  colorScheme,
  singleIcon: SingleIcon,
  multiLabel,
}: FileIconLayoutProps) {
  const isMultiple = filePaths.length > 1;
  const colors = colorMap[colorScheme];

  return (
    <div className="flex-1 min-w-0 px-3 py-2.5">
      <div className="flex items-start gap-2.5">
        <div
          className={cn(
            "shrink-0 w-10 h-10 rounded-lg flex items-center justify-center",
            filesInvalid ? "bg-red-50 dark:bg-red-950" : colors.bg,
          )}
        >
          {filesInvalid ? (
            <Warning16Regular className="w-5 h-5 text-red-500" />
          ) : isMultiple ? (
            <Folder16Regular className={cn("w-5 h-5", colors.icon)} />
          ) : (
            <SingleIcon className={cn("w-5 h-5", colors.icon)} />
          )}
        </div>
        <div className="flex-1 min-w-0">
          {isMultiple ? (
            <>
              <p
                className={cn(
                  "text-sm font-medium",
                  filesInvalid ? "text-red-500 line-through" : "text-foreground",
                )}
              >
                {filePaths.length} {multiLabel}
                {filesInvalid && (
                  <span className="ml-1.5 text-xs font-normal">(已失效)</span>
                )}
              </p>
              <p
                className={cn(
                  "text-xs truncate mt-0.5",
                  filesInvalid ? "text-red-400 line-through" : "text-muted-foreground",
                )}
              >
                <HighlightText
                  text={
                    filePaths
                      .map((p) => getFileNameFromPath(p))
                      .slice(0, 3)
                      .join(", ") + (filePaths.length > 3 ? "..." : "")
                  }
                />
              </p>
            </>
          ) : (
            <>
              <p
                className={cn(
                  "text-sm font-medium truncate",
                  filesInvalid ? "text-red-500 line-through" : "text-foreground",
                )}
              >
                <HighlightText
                  text={getFileNameFromPath(filePaths[0] || preview || "")}
                />
                {filesInvalid && (
                  <span className="ml-1.5 text-xs font-normal">(已失效)</span>
                )}
              </p>
              <p
                className={cn(
                  "text-xs truncate mt-0.5",
                  filesInvalid
                    ? "text-red-400 line-through"
                    : "text-muted-foreground",
                )}
              >
                <HighlightText text={filePaths[0] || preview || ""} />
              </p>
            </>
          )}
        </div>
      </div>
      <CardFooter
        metaItems={metaItems}
        index={index}
        showBadge={showBadge}
        isDragOverlay={isDragOverlay}
        sourceAppName={sourceAppName}
        sourceAppIcon={sourceAppIcon}
      />
    </div>
  );
});
