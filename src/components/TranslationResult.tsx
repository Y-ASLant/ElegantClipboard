// 翻译结果区域组件（从 ClipboardItemCard.tsx 提取）

import {
  Copy16Regular,
  ArrowSync16Regular,
  Dismiss16Regular,
} from "@fluentui/react-icons";
import { TtsButton } from "@/components/TtsButton";
import { TtsHighlightText } from "@/components/TtsHighlightText";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

interface TranslationResultProps {
  translating: boolean;
  translatedText: string | null;
  translateError: string | null;
  onCopy: () => void;
  onDismiss: () => void;
}

export function TranslationResult({
  translating,
  translatedText,
  translateError,
  onCopy,
  onDismiss,
}: TranslationResultProps) {
  if (!translating && !translatedText && !translateError) return null;

  return (
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
            <TtsHighlightText text={translatedText} />
          </pre>
          <div className="absolute right-0 bottom-0 flex items-center gap-0.5 bg-background/90 rounded-md px-0.5 shadow-sm border opacity-0 group-hover/translate:opacity-100 transition-opacity">
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  onClick={onCopy}
                  className="p-1 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors"
                >
                  <Copy16Regular className="w-3.5 h-3.5" />
                </button>
              </TooltipTrigger>
              <TooltipContent>复制翻译</TooltipContent>
            </Tooltip>
            <TtsButton text={translatedText || ""} />
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  onClick={onDismiss}
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
  );
}
