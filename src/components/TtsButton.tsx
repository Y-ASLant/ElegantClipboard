import { useState, useCallback, useEffect } from "react";
import { Speaker216Regular, DismissCircle16Regular } from "@fluentui/react-icons";
import { speak, stopSpeaking, isSpeaking } from "@/lib/tts";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useTtsSettings } from "@/stores/tts-settings";

interface TtsButtonProps {
  /** 要朗读的文本 */
  text: string;
  /** 按钮尺寸样式，默认与翻译区域按钮一致 */
  className?: string;
  /** 图标尺寸 class */
  iconClassName?: string;
}

/**
 * 语音朗读按钮（小图标版，用于卡片翻译区域）
 * 引擎和声源由设置页统一控制，点击即朗读/停止
 */
export function TtsButton({
  text,
  className = "p-1 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors",
  iconClassName = "w-3.5 h-3.5",
}: TtsButtonProps) {
  const ttsEnabled = useTtsSettings((s) => s.enabled);
  const [speaking, setSpeaking] = useState(false);

  useEffect(() => {
    return () => { if (isSpeaking()) stopSpeaking(); };
  }, []);

  const handleClick = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      if (speaking) {
        stopSpeaking();
        setSpeaking(false);
        return;
      }
      setSpeaking(true);
      speak(text, "en-US", {
        onEnd: () => setSpeaking(false),
        onError: () => setSpeaking(false),
      });
    },
    [text, speaking],
  );

  if (!ttsEnabled) return null;

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button onClick={handleClick} className={className}>
          {speaking ? (
            <DismissCircle16Regular className={iconClassName} />
          ) : (
            <Speaker216Regular className={iconClassName} />
          )}
        </button>
      </TooltipTrigger>
      <TooltipContent>{speaking ? "停止朗读" : "朗读"}</TooltipContent>
    </Tooltip>
  );
}

/** 带文字标签的朗读按钮（用于 OCR、翻译选中文字页面） */
export function TtsButtonLarge({
  text,
  getTextFn,
  disabled = false,
}: {
  text?: string;
  /** 动态获取朗读文本的函数（优先于 text），可用于"有选中读选中，否则读全部" */
  getTextFn?: () => string;
  disabled?: boolean;
}) {
  const ttsEnabled = useTtsSettings((s) => s.enabled);
  const [speaking, setSpeaking] = useState(false);

  useEffect(() => {
    return () => { if (isSpeaking()) stopSpeaking(); };
  }, []);

  const handleClick = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      if (speaking) {
        stopSpeaking();
        setSpeaking(false);
        return;
      }
      const toSpeak = getTextFn ? getTextFn() : (text ?? "");
      if (!toSpeak.trim()) return;
      setSpeaking(true);
      speak(toSpeak, "en-US", {
        onEnd: () => setSpeaking(false),
        onError: () => setSpeaking(false),
      });
    },
    [text, getTextFn, speaking],
  );

  if (!ttsEnabled) return null;

  return (
    <button
      onMouseDown={(e) => e.preventDefault()}
      onClick={handleClick}
      disabled={disabled}
      className="inline-flex items-center justify-center gap-1 rounded-md text-xs font-medium h-6 px-2 hover:bg-accent hover:text-accent-foreground transition-colors disabled:opacity-50 disabled:pointer-events-none"
    >
      {speaking ? (
        <DismissCircle16Regular className="w-3 h-3" />
      ) : (
        <Speaker216Regular className="w-3 h-3" />
      )}
      {speaking ? "停止" : "朗读"}
    </button>
  );
}
