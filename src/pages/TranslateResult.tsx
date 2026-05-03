import { useState, useEffect, useRef, useCallback } from "react";
import { Translate16Regular, Copy16Regular } from "@fluentui/react-icons";
import { TtsButtonLarge } from "@/components/TtsButton";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { WindowTitleBar } from "@/components/WindowTitleBar";
import { logError } from "@/lib/logger";
import { initTheme } from "@/lib/theme-applier";
import { translateText } from "@/lib/translate";
import { useTranslateSettings } from "@/stores/translate-settings";
import { useTtsSettings } from "@/stores/tts-settings";
import { cn } from "@/lib/utils";
import { TtsHighlightText } from "@/components/TtsHighlightText";
import { useTtsPlayback } from "@/stores/tts-playback";

export function TranslateResult() {
  const [text, setText] = useState("");
  const [themeReady, setThemeReady] = useState(false);
  const [copied, setCopied] = useState(false);
  const [translating, setTranslating] = useState(false);
  const [translatedText, setTranslatedText] = useState("");
  const [translateError, setTranslateError] = useState("");
  const [translatedCopied, setTranslatedCopied] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const translationRef = useRef<HTMLDivElement>(null);

  const recordTranslation = useTranslateSettings((s) => s.recordTranslation);
  const translateLoaded = useTranslateSettings((s) => s.loaded);
  const ttsLoaded = useTtsSettings((s) => s.loaded);
  const ttsHighlight = useTtsSettings((s) => s.highlightWord);
  const ttsPlayingOriginal = useTtsPlayback((s) => s.isPlaying && s.sourceText === text) && ttsHighlight;

  // 加载设置
  useEffect(() => {
    if (!translateLoaded) useTranslateSettings.getState().loadSettings();
    if (!ttsLoaded) useTtsSettings.getState().loadSettings();
  }, [translateLoaded, ttsLoaded]);

  // 加载主题后显示窗口
  useEffect(() => {
    initTheme().then(async () => {
      const win = getCurrentWindow();
      document.body.getBoundingClientRect();
      await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
      await new Promise((r) => setTimeout(r, 30));
      win.show();
      win.setFocus();
      await new Promise((r) => requestAnimationFrame(r));
      setThemeReady(true);
    });
  }, []);

  // 挂载后从 Rust 获取暂存文本
  useEffect(() => {
    invoke<string>("get_pending_translate_text").then((t) => {
      if (t) {
        setText(t);
        doTranslate(t);
      }
    }).catch(() => {});
  }, []);

  // 监听文本更新事件（窗口已存在时复用）
  useEffect(() => {
    const unlisten = listen<string>("translate-result-update", (event) => {
      setText(event.payload);
      setTranslatedText("");
      setTranslateError("");
      doTranslate(event.payload);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // ESC 关闭
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        getCurrentWindow().close();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  // 自动翻译
  const doTranslate = useCallback(async (sourceText: string) => {
    if (!sourceText.trim()) return;
    setTranslating(true);
    setTranslateError("");
    setTranslatedText("");
    try {
      const result = await translateText(sourceText);
      setTranslatedText(result);
    } catch (error) {
      setTranslateError(String(error));
    } finally {
      setTranslating(false);
    }
  }, []);

  const handleCopy = useCallback(async () => {
    try {
      await invoke("write_text_to_clipboard", {
        text,
        record: false,
      });
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch (error) {
      logError("复制失败:", error);
    }
  }, [text]);

  const handleCopyTranslation = useCallback(async () => {
    try {
      await invoke("write_text_to_clipboard", {
        text: translatedText,
        record: recordTranslation,
      });
      setTranslatedCopied(true);
      setTimeout(() => setTranslatedCopied(false), 1500);
    } catch (error) {
      logError("复制翻译结果失败:", error);
    }
  }, [translatedText, recordTranslation]);

  const handleRetranslate = useCallback(async () => {
    if (!text.trim() || translating) return;
    doTranslate(text);
  }, [text, translating, doTranslate]);

  return (
    <div
      className={cn(
        "h-screen flex flex-col bg-muted/40 overflow-hidden p-3 gap-3",
        !themeReady && "**:transition-none!",
      )}
    >
      <WindowTitleBar
        icon={<Translate16Regular className="w-5 h-5 text-muted-foreground" />}
        title="翻译选中文字"
      />

      {/* 原文 */}
      <Card className="flex-1 overflow-hidden flex flex-col min-h-0">
        <div className="flex items-center justify-between px-4 pt-3 pb-1">
          <span className="text-xs font-medium text-muted-foreground">原文</span>
          <div className="flex items-center gap-1">
            <Button
              variant="ghost"
              size="sm"
              className="h-6 px-2 text-xs"
              onClick={handleCopy}
            >
              <Copy16Regular className="w-3 h-3 mr-1" />
              {copied ? "已复制" : "复制"}
            </Button>
            <TtsButtonLarge
              disabled={!text.trim()}
              getTextFn={() => {
                if (textareaRef.current) {
                  const { selectionStart, selectionEnd, value } = textareaRef.current;
                  if (selectionStart !== selectionEnd) return value.slice(selectionStart, selectionEnd);
                }
                return text;
              }}
            />
          </div>
        </div>
        {ttsPlayingOriginal ? (
          <div className="flex-1 w-full overflow-auto px-4 pb-3 text-sm leading-relaxed font-mono whitespace-pre-wrap break-words select-text cursor-text">
            <TtsHighlightText text={text} />
          </div>
        ) : (
          <textarea
            ref={textareaRef}
            value={text}
            onChange={(e) => setText(e.target.value)}
            className="flex-1 w-full resize-none border-0 bg-transparent px-4 pb-3 text-sm leading-relaxed font-mono focus:outline-none placeholder:text-muted-foreground"
            placeholder="等待选中文字..."
            spellCheck={false}
            readOnly
          />
        )}
      </Card>

      {/* 翻译结果 */}
      <Card className="flex-1 overflow-hidden flex flex-col min-h-0">
        <div className="flex items-center justify-between px-4 pt-3 pb-1">
          <span className="text-xs font-medium text-muted-foreground">
            {translating ? "翻译中..." : "翻译结果"}
          </span>
          <div className="flex items-center gap-1">
            {translatedText && (
              <>
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-6 px-2 text-xs"
                  onClick={handleCopyTranslation}
                >
                  <Copy16Regular className="w-3 h-3 mr-1" />
                  {translatedCopied ? "已复制" : "复制"}
                </Button>
                <TtsButtonLarge
                  getTextFn={() => {
                    const sel = window.getSelection();
                    if (sel && sel.toString().trim() && translationRef.current?.contains(sel.anchorNode)) {
                      return sel.toString();
                    }
                    return translatedText;
                  }}
                />
              </>
            )}
          </div>
        </div>
        <div ref={translationRef} className="flex-1 overflow-auto px-4 pb-3">
          {translating && (
            <p className="text-sm text-muted-foreground">正在翻译...</p>
          )}
          {translatedText && (
            <p className="text-sm leading-relaxed whitespace-pre-wrap cursor-text select-text"><TtsHighlightText text={translatedText} /></p>
          )}
          {translateError && (
            <p className="text-sm text-destructive">{translateError}</p>
          )}
        </div>
      </Card>

      {/* 底部操作栏 */}
      <Card className="shrink-0">
        <div className="h-11 flex items-center justify-between px-4">
          <span className="text-xs text-muted-foreground">
            {text.length} 字符
          </span>
          <div className="flex gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={handleRetranslate}
              disabled={translating || !text.trim()}
            >
              <Translate16Regular className="w-4 h-4 mr-1" />
              {translating ? "翻译中..." : "重新翻译"}
            </Button>
          </div>
        </div>
      </Card>
    </div>
  );
}
