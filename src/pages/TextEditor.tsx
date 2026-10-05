import { useState, useEffect, useRef } from "react";
import { Edit16Filled } from "@fluentui/react-icons";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { WindowTitleBar } from "@/components/WindowTitleBar";
import { useTranslation } from "@/i18n";
import { logError } from "@/lib/logger";
import { reportUserError, runClipboardOperation } from "@/lib/operation-feedback";
import { initTheme } from "@/lib/theme-applier";
import { cn } from "@/lib/utils";

export function TextEditor() {
  const { t } = useTranslation();
  const [text, setText] = useState("");
  const [originalText, setOriginalText] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [themeReady, setThemeReady] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const params = new URLSearchParams(window.location.search);
  const id = Number(params.get("id"));

  // 加载主题后显示窗口，并确认后端可将此 WebView 视为可用。
  useEffect(() => {
    void initTheme()
      .then(() => {
        const win = getCurrentWindow();
        requestAnimationFrame(() => {
          void (async () => {
            try {
              await win.show();
              await win.setFocus();
              await invoke("managed_window_ready");
            } catch (error) {
              logError("Failed to initialize editor window:", error);
            } finally {
              setThemeReady(true);
            }
          })();
        });
      })
      .catch((error) => logError("Failed to initialize editor theme:", error));
  }, []);

  // 加载条目内容
  useEffect(() => {
    if (!id) {
      setLoading(false);
      reportUserError(t("operationFeedback.userActions.loadContent"), { code: "invalid_content", detail: "Invalid editor item id" });
      return;
    }
    let active = true;
    void invoke<{ text_content: string | null } | null>("get_clipboard_item", { id }).then((item) => {
      if (!active) return;
      if (!item) {
        reportUserError(t("operationFeedback.userActions.loadContent"), { code: "item_not_found", detail: `Editor item ${id} does not exist` });
        return;
      }
      const content = item.text_content ?? "";
      setText(content);
      setOriginalText(content);
    }).catch((error: unknown) => {
      if (active) reportUserError(t("operationFeedback.userActions.loadContent"), error, "Failed to load editor content");
    }).finally(() => {
      if (active) setLoading(false);
    });
    return () => { active = false; };
  }, [id]);

  // ESC 关闭
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        void closeEditor();
      }
      // Ctrl+S 保存
      if (e.ctrlKey && e.key === "s") {
        e.preventDefault();
        handleSave();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [text, originalText, saving, loading, t]);

  const hasChanges = text !== originalText;

  const closeEditor = async () => {
    try {
      await getCurrentWindow().close();
    } catch (error) {
      reportUserError(t("operationFeedback.userActions.closeWindow"), error, "Failed to close text editor");
    }
  };

  const handleSave = async (): Promise<boolean> => {
    if (saving || loading) return false;
    if (!hasChanges) return true;
    setSaving(true);
    const savedText = text;
    try {
      const result = await runClipboardOperation<boolean>("editText", { id, newText: savedText });
      if (result.status !== "success") return false;
      setOriginalText(savedText);
      if (result.value) await closeEditor();
      return !result.value;
    } finally {
      setSaving(false);
    }
  };

  const handleSaveAndClose = async () => {
    if (await handleSave()) await closeEditor();
  };

  return (
    <div
      className={cn(
        "h-screen flex flex-col bg-page-shell overflow-hidden p-3 gap-3",
        !themeReady && "**:transition-none!",
      )}
    >
      <WindowTitleBar
        icon={<Edit16Filled className="w-5 h-5 text-muted-foreground" />}
        title={t("textEditor.title")}
        extra={hasChanges ? <span className="text-xs text-status-warning">{t("textEditor.unsaved")}</span> : undefined}
      />

      {/* Editor Area */}
      <Card className="flex-1 overflow-hidden flex flex-col">
        {loading ? (
          <div className="flex-1 flex items-center justify-center">
            <div className="w-6 h-6 border-2 border-primary border-t-transparent rounded-full animate-spin" />
          </div>
        ) : (
          <textarea
            ref={textareaRef}
            value={text}
            onChange={(e) => setText(e.target.value)}
            className="clipboard-content flex-1 w-full resize-none border-0 bg-transparent p-4 leading-relaxed focus:outline-none placeholder:text-muted-foreground"
            placeholder={t("textEditor.noContent")}
            spellCheck={false}
            autoFocus
          />
        )}
      </Card>

      {/* Footer */}
      <Card className="shrink-0">
        <div className="h-11 flex items-center justify-between px-4">
          <span className="text-xs text-muted-foreground">
            {t("textEditor.charAndBytes", { chars: text.length, bytes: new Blob([text]).size })}
          </span>
          <div className="flex gap-2">
            <Button
              size="sm"
              onClick={handleSaveAndClose}
              disabled={saving}
            >
              {saving ? t("textEditor.saving") : hasChanges ? t("textEditor.saveAndClose") : t("textEditor.close")}
            </Button>
          </div>
        </div>
      </Card>
    </div>
  );
}

