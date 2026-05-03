import { useState, useEffect, useRef, useCallback, useMemo } from "react";
import { ScanText16Regular, Copy16Regular, Translate16Regular, Edit16Regular } from "@fluentui/react-icons";
import { TtsButtonLarge } from "@/components/TtsButton";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import Linkify from "linkify-react";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { WindowTitleBar } from "@/components/WindowTitleBar";
import { logError } from "@/lib/logger";
import { initTheme } from "@/lib/theme-applier";
import { translateText } from "@/lib/translate";
import { useTranslateSettings } from "@/stores/translate-settings";
import { useOcrSettings } from "@/stores/ocr-settings";
import { useTtsSettings } from "@/stores/tts-settings";
import { cn } from "@/lib/utils";
import { openUrl } from "@tauri-apps/plugin-opener";
import { TtsHighlightText } from "@/components/TtsHighlightText";
import { useTtsPlayback } from "@/stores/tts-playback";

export function OcrResult() {
  const [text, setText] = useState("");
  const [themeReady, setThemeReady] = useState(false);
  const [copied, setCopied] = useState(false);
  const [translating, setTranslating] = useState(false);
  const [translatedText, setTranslatedText] = useState("");
  const [translateError, setTranslateError] = useState("");
  const [translatedCopied, setTranslatedCopied] = useState(false);
  const [ocrRecognizing, setOcrRecognizing] = useState(true);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const ocrTextRef = useRef<HTMLDivElement>(null);
  const translationRef = useRef<HTMLDivElement>(null);
  const [editing, setEditing] = useState(false);

  // Linkify 配置：URL 渲染为蓝色下划线，Ctrl+点击打开
  const linkifyOptions = useMemo(() => ({
    render: ({
      attributes,
      content,
    }: {
      tagName: string;
      attributes: Record<string, any>;
      content: string;
    }) => {
      const href = attributes.href as string;
      return (
        <span
          className="text-primary underline underline-offset-2 decoration-primary/60 hover:decoration-primary cursor-pointer"
          title={`Ctrl+\u70b9\u51fb\u6253\u5f00\u94fe\u63a5`}
          onMouseDown={(e: React.MouseEvent) => {
            if (e.ctrlKey) {
              e.preventDefault();
              e.stopPropagation();
              openUrl(href).catch((err) => logError("打开链接失败:", err));
            }
          }}
        >
          {content}
        </span>
      );
    },
  }), []);

  const translateEnabled = useTranslateSettings((s) => s.enabled);
  const translateLoaded = useTranslateSettings((s) => s.loaded);
  const recordTranslation = useTranslateSettings((s) => s.recordTranslation);
  const ocrRecordCopy = useOcrSettings((s) => s.recordOcrCopy);
  const ocrLoaded = useOcrSettings((s) => s.loaded);
  const ttsHighlight = useTtsSettings((s) => s.highlightWord);
  const ttsPlayingOcr = useTtsPlayback((s) => s.isPlaying && s.sourceText === text) && ttsHighlight;

  const ttsLoaded = useTtsSettings((s) => s.loaded);

  // 加载设置
  useEffect(() => {
    if (!translateLoaded) useTranslateSettings.getState().loadSettings();
    if (!ocrLoaded) useOcrSettings.getState().loadSettings();
    if (!ttsLoaded) useTtsSettings.getState().loadSettings();
  }, [translateLoaded, ocrLoaded, ttsLoaded]);

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

  // 挂载后从 Rust 获取暂存文本（兜底：防止窗口刚创建时 emit 事件丢失）
  useEffect(() => {
    invoke<string>("get_pending_ocr_text").then((t) => {
      if (t) {
        setText(t);
        setOcrRecognizing(false);
        // 自动复制
        const { autoCopy, recordOcrCopy: record } = useOcrSettings.getState();
        if (autoCopy && t.trim()) {
          invoke("write_text_to_clipboard", { text: t, record }).catch(() => {});
        }
        // 自动翻译
        const { autoTranslate } = useOcrSettings.getState();
        const { enabled: transEnabled } = useTranslateSettings.getState();
        if (autoTranslate && transEnabled && t.trim()) {
          setTranslating(true);
          translateText(t).then((r) => {
            setTranslatedText(r);
          }).catch((e) => {
            setTranslateError(String(e));
          }).finally(() => {
            setTranslating(false);
          });
        }
      }
    }).catch(() => {});
  }, []);

  // 监听文本更新事件（窗口已存在时复用）
  useEffect(() => {
    const unlisten = listen<string>("ocr-result-update", (event) => {
      if (event.payload === "") {
        // 空文本 = 开始识别
        setText("");
        setOcrRecognizing(true);
        setEditing(false);
      } else {
        const newText = event.payload;
        setText(newText);
        setOcrRecognizing(false);
        // 识别完成后自动复制
        const { autoCopy, recordOcrCopy: record } = useOcrSettings.getState();
        if (autoCopy && newText.trim()) {
          invoke("write_text_to_clipboard", { text: newText, record }).catch(() => {});
        }
        // 识别完成后自动翻译
        const { autoTranslate } = useOcrSettings.getState();
        const { enabled: transEnabled } = useTranslateSettings.getState();
        if (autoTranslate && transEnabled && newText.trim()) {
          setTranslating(true);
          translateText(newText).then((r) => {
            setTranslatedText(r);
          }).catch((e) => {
            setTranslateError(String(e));
          }).finally(() => {
            setTranslating(false);
          });
          return; // 跳过下面的重置
        }
      }
      setTranslatedText("");
      setTranslateError("");
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


  const handleCopy = useCallback(async () => {
    try {
      await invoke("write_text_to_clipboard", {
        text,
        record: ocrRecordCopy,
      });
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch (error) {
      logError("复制失败:", error);
    }
  }, [text, ocrRecordCopy]);

  const handleTranslate = useCallback(async () => {
    if (!text.trim() || translating) return;
    setTranslating(true);
    setTranslateError("");
    try {
      const result = await translateText(text);
      setTranslatedText(result);
    } catch (error) {
      setTranslateError(String(error));
    } finally {
      setTranslating(false);
    }
  }, [text, translating]);

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


  return (
    <div
      className={cn(
        "h-screen flex flex-col bg-muted/40 overflow-hidden p-3 gap-3",
        !themeReady && "**:transition-none!",
      )}
    >
      <WindowTitleBar
        icon={<ScanText16Regular className="w-5 h-5 text-muted-foreground" />}
        title="OCR识别结果"
      />

      {/* 识别结果 */}
      <Card className="flex-1 overflow-hidden flex flex-col min-h-0">
        <div className="flex items-center justify-between px-4 pt-3 pb-1">
          <span className="text-xs font-medium text-muted-foreground">{ocrRecognizing ? "正在识别中..." : "识别结果"}</span>
          <div className="flex items-center gap-1">
            {!ocrRecognizing && text.trim() && (
              <Button
                variant="ghost"
                size="sm"
                className="h-6 px-2 text-xs"
                onClick={() => {
                  setEditing((v) => !v);
                  if (!editing) {
                    requestAnimationFrame(() => textareaRef.current?.focus());
                  }
                }}
              >
                <Edit16Regular className="w-3 h-3 mr-1" />
                {editing ? "完成" : "编辑"}
              </Button>
            )}
            <Button
              variant="ghost"
              size="sm"
              className="h-6 px-2 text-xs"
              onClick={handleCopy}
              disabled={!text.trim()}
            >
              <Copy16Regular className="w-3 h-3 mr-1" />
              {copied ? "已复制" : "复制"}
            </Button>
            <TtsButtonLarge
              disabled={!text.trim() || ocrRecognizing}
              getTextFn={() => {
                const sel = window.getSelection();
                if (sel && sel.toString().trim() && ocrTextRef.current?.contains(sel.anchorNode)) {
                  return sel.toString();
                }
                if (editing && textareaRef.current) {
                  const { selectionStart, selectionEnd, value } = textareaRef.current;
                  if (selectionStart !== selectionEnd) return value.slice(selectionStart, selectionEnd);
                }
                return text;
              }}
            />
          </div>
        </div>
        {editing ? (
          <textarea
            ref={textareaRef}
            value={text}
            onChange={(e) => setText(e.target.value)}
            className="flex-1 w-full resize-none border-0 bg-transparent px-4 pb-3 text-sm leading-relaxed font-mono focus:outline-none placeholder:text-muted-foreground"
            placeholder="编辑识别结果..."
            spellCheck={false}
            autoFocus
          />
        ) : (
          <div ref={ocrTextRef} className="flex-1 overflow-auto px-4 pb-3 text-sm leading-relaxed font-mono whitespace-pre-wrap break-words select-text cursor-text">
            {text ? (
              ttsPlayingOcr ? <TtsHighlightText text={text} /> : <Linkify options={linkifyOptions}>{text}</Linkify>
            ) : (
              <span className="text-muted-foreground">
                {ocrRecognizing ? "正在识别中..." : "等待识别结果..."}
              </span>
            )}
          </div>
        )}
      </Card>

      {/* 翻译结果 */}
      {translatedText && (
        <Card className="shrink-0 max-h-[200px] overflow-hidden flex flex-col">
          <div className="flex items-center justify-between px-4 pt-3 pb-1">
            <span className="text-xs font-medium text-muted-foreground">翻译结果</span>
            <div className="flex items-center gap-1">
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
            </div>
          </div>
          <div ref={translationRef} className="flex-1 overflow-auto px-4 pb-3 select-text">
            <p className="text-sm leading-relaxed whitespace-pre-wrap cursor-text"><TtsHighlightText text={translatedText} /></p>
          </div>
        </Card>
      )}

      {translateError && (
        <p className="text-xs text-destructive px-1">{translateError}</p>
      )}

      {/* 底部操作栏 */}
      <Card className="shrink-0">
        <div className="flex items-center justify-between px-4 py-2">
          <span className="text-xs text-muted-foreground">
            {text.length} 字符
          </span>
          <div className="flex gap-2 items-center">
            {!translateEnabled && (
              <span className="text-xs text-destructive">
                翻译功能未开启，请在设置 → 条目翻译中启用
              </span>
            )}
            <Button
              variant="outline"
              size="sm"
              onClick={handleTranslate}
              disabled={!translateEnabled || translating || ocrRecognizing || !text.trim()}
            >
              <Translate16Regular className="w-4 h-4 mr-1" />
              {translating ? "翻译中..." : "翻译"}
            </Button>
          </div>
        </div>
      </Card>
    </div>
  );
}
