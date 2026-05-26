import { useState, useEffect, useCallback } from "react";
import { Translate16Regular, Copy16Regular } from "@fluentui/react-icons";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { WindowTitleBar } from "@/components/WindowTitleBar";
import { logError } from "@/lib/logger";
import { initTheme } from "@/lib/theme-applier";
import { translateText } from "@/lib/translate";
import { cn } from "@/lib/utils";
import { useTranslateSettings } from "@/stores/translate-settings";

export function TranslateResult() {
  const [text, setText] = useState("");
  const [themeReady, setThemeReady] = useState(false);
  const [copied, setCopied] = useState(false);
  const [translating, setTranslating] = useState(false);
  const [translatedText, setTranslatedText] = useState("");
  const [translateError, setTranslateError] = useState("");
  const [translatedCopied, setTranslatedCopied] = useState(false);

  const recordTranslation = useTranslateSettings((s) => s.recordTranslation);
  const translateLoaded = useTranslateSettings((s) => s.loaded);

  useEffect(() => {
    if (!translateLoaded) useTranslateSettings.getState().loadSettings();
  }, [translateLoaded]);

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

  useEffect(() => {
    invoke<string>("get_pending_translate_text").then((t) => {
      if (t) { setText(t); doTranslate(t); }
    }).catch(() => {});
  }, [doTranslate]);

  useEffect(() => {
    const unlisten = listen<string>("translate-result-update", (event) => {
      setText(event.payload);
      setTranslatedText("");
      setTranslateError("");
      doTranslate(event.payload);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, [doTranslate]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") getCurrentWindow().close();
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  const handleCopy = useCallback(async () => {
    try {
      await invoke("write_text_to_clipboard", { text, record: false });
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch (error) { logError("复制失败:", error); }
  }, [text]);

  const handleCopyTranslation = useCallback(async () => {
    try {
      await invoke("write_text_to_clipboard", { text: translatedText, record: recordTranslation });
      setTranslatedCopied(true);
      setTimeout(() => setTranslatedCopied(false), 1500);
    } catch (error) { logError("复制翻译结果失败:", error); }
  }, [translatedText, recordTranslation]);

  return (
    <div className={cn("h-screen flex flex-col bg-muted/40 overflow-hidden p-3 gap-3", !themeReady && "**:transition-none!")}>
      <WindowTitleBar
        icon={<Translate16Regular className="w-5 h-5 text-muted-foreground" />}
        title="翻译选中文字"
      />

      {/* 原文 */}
      <Card className="flex-1 overflow-hidden flex flex-col min-h-0">
        <div className="flex items-center justify-between px-4 pt-3 pb-1">
          <span className="text-xs font-medium text-muted-foreground">原文</span>
          <Button variant="ghost" size="sm" className="h-6 px-2 text-xs" onClick={handleCopy}>
            <Copy16Regular className="w-3 h-3 mr-1" />
            {copied ? "已复制" : "复制"}
          </Button>
        </div>
        <textarea
          value={text}
          readOnly
          className="flex-1 w-full resize-none border-0 bg-transparent px-4 pb-3 text-sm leading-relaxed font-mono focus:outline-none placeholder:text-muted-foreground"
          placeholder="等待选中文字..."
          spellCheck={false}
        />
      </Card>

      {/* 翻译结果 */}
      <Card className="flex-1 overflow-hidden flex flex-col min-h-0">
        <div className="flex items-center justify-between px-4 pt-3 pb-1">
          <span className="text-xs font-medium text-muted-foreground">
            {translating ? "翻译中..." : "翻译结果"}
          </span>
          {translatedText && (
            <Button variant="ghost" size="sm" className="h-6 px-2 text-xs" onClick={handleCopyTranslation}>
              <Copy16Regular className="w-3 h-3 mr-1" />
              {translatedCopied ? "已复制" : "复制"}
            </Button>
          )}
        </div>
        <div className="flex-1 overflow-auto px-4 pb-3">
          {translating && <p className="text-sm text-muted-foreground">正在翻译...</p>}
          {translatedText && <p className="text-sm leading-relaxed whitespace-pre-wrap cursor-text select-text">{translatedText}</p>}
          {translateError && <p className="text-sm text-destructive">{translateError}</p>}
        </div>
      </Card>

      {/* 底部操作栏 */}
      <Card className="shrink-0">
        <div className="h-11 flex items-center justify-between px-4">
          <span className="text-xs text-muted-foreground">{text.length} 字符</span>
          <Button variant="outline" size="sm"
            onClick={() => doTranslate(text)}
            disabled={translating || !text.trim()}>
            <Translate16Regular className="w-4 h-4 mr-1" />
            {translating ? "翻译中..." : "重新翻译"}
          </Button>
        </div>
      </Card>
    </div>
  );
}
