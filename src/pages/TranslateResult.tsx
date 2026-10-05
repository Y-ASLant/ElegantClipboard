import { useState, useEffect, useCallback, useRef } from "react";
import { Translate16Regular, Copy16Regular } from "@fluentui/react-icons";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { WindowTitleBar } from "@/components/WindowTitleBar";
import { useTranslation } from "@/i18n";
import { logError } from "@/lib/logger";
import { runClipboardOperation, reportUserError } from "@/lib/operation-feedback";
import { initTheme } from "@/lib/theme-applier";
import { translateText } from "@/lib/translate";
import { cn } from "@/lib/utils";
import { useTranslateSettings } from "@/stores/translate-settings";

export function TranslateResult() {
  const { t } = useTranslation();
  const [text, setText] = useState("");
  const [themeReady, setThemeReady] = useState(false);
  const [copied, setCopied] = useState(false);
  const [translating, setTranslating] = useState(false);
  const [translatedText, setTranslatedText] = useState("");
  const [translateError, setTranslateError] = useState("");
  const [translatedCopied, setTranslatedCopied] = useState(false);
  const [isPinned, setIsPinned] = useState(false);

  const recordTranslation = useTranslateSettings((s) => s.recordTranslation);
  const translateLoaded = useTranslateSettings((s) => s.loaded);
  const requestIdRef = useRef(0);
  const copyRequestRef = useRef(0);
  const copyTimerRef = useRef<number | undefined>(undefined);
  useEffect(() => {
    copyRequestRef.current++;
    setCopied(false);
    setTranslatedCopied(false);
    return () => {
      copyRequestRef.current++;
      window.clearTimeout(copyTimerRef.current);
    };
  }, [text, translatedText]);

  useEffect(() => () => { requestIdRef.current++; }, []);

  useEffect(() => {
    if (!translateLoaded) useTranslateSettings.getState().loadSettings();
  }, [translateLoaded]);

  useEffect(() => {
    void initTheme();
    setThemeReady(true);
  }, []);

  const doTranslate = useCallback(async (sourceText: string) => {
    if (!sourceText.trim()) return;
    const reqId = ++requestIdRef.current;
    setTranslating(true);
    setTranslateError("");
    setTranslatedText("");
    try {
      const result = await translateText(sourceText);
      if (reqId !== requestIdRef.current) return; // 过期请求，丢弃结果
      setTranslatedText(result);
    } catch (error) {
      if (reqId !== requestIdRef.current) return;
      setTranslateError(error instanceof Error ? error.message : t("operationFeedback.unknownReason"));
    } finally {
      if (reqId === requestIdRef.current) setTranslating(false);
    }
  }, [t]);

  useEffect(() => {
    invoke<boolean>("is_translate_window_pinned")
      .then(setIsPinned)
      .catch((error) => logError("Failed to load translate pin state:", error));
  }, []);

  useEffect(() => {
    const load = async () => {
      if (!useTranslateSettings.getState().loaded) {
        await useTranslateSettings.getState().loadSettings();
      }
      try {
        const pending = await invoke<string>("get_pending_translate_text");
        if (pending) {
          setText(pending);
          doTranslate(pending);
        }
      } catch (error) {
        logError("Failed to load pending translate text:", error);
      }
    };
    load();
  }, [doTranslate]);

  useEffect(() => {
    const unlisten = listen<string>("translate-result-update", (event) => {
      setText(event.payload);
      setTranslatedText("");
      setTranslateError("");
      setIsPinned(false);
      invoke("set_translate_window_pinned", { pinned: false }).catch((error) => {
        logError("Failed to reset translate pin state:", error);
      });
      doTranslate(event.payload);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, [doTranslate]);

  useEffect(() => {
    void invoke("translate_window_ready").catch((error) => {
      logError("Failed to confirm translate window readiness:", error);
    });
  }, []);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !isPinned) {
        void getCurrentWindow().close().catch((error) => reportUserError(t("common.close"), error, "Failed to close translate window"));
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isPinned, t]);

  const togglePinned = useCallback(async () => {
    const next = !isPinned;
    try {
      await invoke("set_translate_window_pinned", { pinned: next });
      setIsPinned(next);
    } catch (error) {
      reportUserError(t("toolbar.pinWindow"), error, "Failed to toggle translate pin state");
    }
  }, [isPinned, t]);

  const handleCopy = async (translation = false) => {
    const request = ++copyRequestRef.current;
    window.clearTimeout(copyTimerRef.current);
    setCopied(false);
    setTranslatedCopied(false);
    const result = await runClipboardOperation(translation ? "copyTranslation" : "copyText", {
      text: translation ? translatedText : text,
      record: translation ? recordTranslation : false,
    });
    if (result.status !== "success" || request !== copyRequestRef.current) return;
    if (translation) setTranslatedCopied(true);
    else setCopied(true);
    copyTimerRef.current = window.setTimeout(() => {
      if (request !== copyRequestRef.current) return;
      setCopied(false);
      setTranslatedCopied(false);
    }, 1500);
  };

  return (
    <div className={cn("h-screen flex flex-col bg-page-shell overflow-hidden p-3 gap-3", !themeReady ? "opacity-0 **:transition-none!" : "opacity-100")}>
      <WindowTitleBar
        icon={<Translate16Regular className="w-5 h-5 text-muted-foreground" />}
        title={t("translateResult.title")}
        pinControl={{
          pinned: isPinned,
          onToggle: togglePinned,
          pinLabel: t("toolbar.pinWindow"),
          unpinLabel: t("toolbar.unpinWindow"),
        }}
      />

      {/* 原文 */}
      <Card className="flex-1 overflow-hidden flex flex-col min-h-0">
        <div className="flex items-center justify-between px-4 pt-3 pb-1">
          <span className="text-xs font-medium text-muted-foreground">{t("translateResult.original")}</span>
          <Button variant="ghost" size="sm" className="h-6 px-2 text-xs" onClick={() => handleCopy()}>
            <Copy16Regular className="w-3 h-3 mr-1" />
            {copied ? t("translateResult.copied") : t("translateResult.copy")}
          </Button>
          {copied && <span className="sr-only" role="status">{t("operationFeedback.copySucceeded")}</span>}
        </div>
        <textarea
          value={text}
          readOnly
          className="flex-1 w-full resize-none border-0 bg-transparent px-4 pb-3 text-sm leading-relaxed ui-font focus:outline-none placeholder:text-muted-foreground"
          placeholder={t("translateResult.waitingText")}
          spellCheck={false}
        />
      </Card>

      {/* 翻译结果 */}
      <Card className="flex-1 overflow-hidden flex flex-col min-h-0">
        <div className="flex items-center justify-between px-4 pt-3 pb-1">
          <span className="text-xs font-medium text-muted-foreground">
            {translating ? t("translateResult.translating") : t("translateResult.result")}
          </span>
          {translatedText && (
            <Button variant="ghost" size="sm" className="h-6 px-2 text-xs" onClick={() => handleCopy(true)}>
              <Copy16Regular className="w-3 h-3 mr-1" />
              {translatedCopied ? t("translateResult.copied") : t("translateResult.copy")}
            </Button>
          )}
          {translatedCopied && <span className="sr-only" role="status">{t("operationFeedback.copySucceeded")}</span>}
        </div>
        <div className="flex-1 overflow-auto px-4 pb-3">
          {translating && <p className="text-sm text-muted-foreground">{t("translateResult.translatingProgress")}</p>}
          {translatedText && (
            <p className="text-sm leading-relaxed whitespace-pre-wrap cursor-text select-text ui-font">{translatedText}</p>
          )}
          {translateError && <p className="text-sm text-destructive">{translateError}</p>}
        </div>
        <div className="shrink-0 flex items-center justify-between px-4 py-2 border-t">
          <span className="text-xs text-muted-foreground">{t("translateResult.charCount", { count: text.length })}</span>
          <Button
            variant="ghost"
            size="sm"
            className="h-7 px-2 text-xs text-primary hover:text-primary hover:bg-primary-subtle"
            onClick={() => doTranslate(text)}
            disabled={translating || !text.trim()}
          >
            <Translate16Regular className="w-3.5 h-3.5" />
            {translating ? t("translateResult.translating") : t("translateResult.retranslate")}
          </Button>
        </div>
      </Card>
    </div>
  );
}
