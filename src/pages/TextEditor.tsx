import { useState, useEffect, useRef } from "react";
import { Edit16Filled } from "@fluentui/react-icons";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { WindowTitleBar } from "@/components/WindowTitleBar";
import { initTheme } from "@/lib/theme-applier";
import { cn } from "@/lib/utils";

export function TextEditor() {
  const [text, setText] = useState("");
  const [originalText, setOriginalText] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [themeReady, setThemeReady] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const params = new URLSearchParams(window.location.search);
  const id = Number(params.get("id"));

  // Load theme then show window
  useEffect(() => {
    initTheme().then(async () => {
      const win = getCurrentWindow();
      document.body.getBoundingClientRect();
      await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
      win.show();
      win.setFocus();
      await new Promise((r) => requestAnimationFrame(r));
      setThemeReady(true);
    });
  }, []);

  // Load item content
  useEffect(() => {
    if (!id) return;
    invoke<{ text_content: string | null }>("get_clipboard_item", { id }).then(
      (item) => {
        const content = item?.text_content ?? "";
        setText(content);
        setOriginalText(content);
        setLoading(false);
      },
    );
  }, [id]);

  // ESC to close
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        getCurrentWindow().close();
      }
      // Ctrl+S to save
      if (e.ctrlKey && e.key === "s") {
        e.preventDefault();
        handleSave();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [text, originalText]);

  const hasChanges = text !== originalText;

  const handleSave = async () => {
    if (!hasChanges || saving) return;
    setSaving(true);
    try {
      const deleted = await invoke<boolean>("update_text_content", { id, newText: text });
      if (deleted) {
        getCurrentWindow().close();
        return;
      }
      setOriginalText(text);
    } catch (error) {
      console.error("Failed to save:", error);
    } finally {
      setSaving(false);
    }
  };

  const handleSaveAndClose = async () => {
    if (saving) return;
    if (hasChanges) {
      setSaving(true);
      try {
        await invoke<boolean>("update_text_content", { id, newText: text });
      } catch (error) {
        console.error("Failed to save:", error);
        setSaving(false);
        return;
      }
    }
    getCurrentWindow().close();
  };

  return (
    <div
      className={cn(
        "h-screen flex flex-col bg-muted/40 overflow-hidden p-3 gap-3",
        !themeReady && "[&_*]:!transition-none",
      )}
    >
      <WindowTitleBar
        icon={<Edit16Filled className="w-5 h-5 text-muted-foreground" />}
        title="编辑文本"
        extra={hasChanges ? <span className="text-xs text-amber-500">● 未保存</span> : undefined}
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
            className="flex-1 w-full resize-none border-0 bg-transparent p-4 text-sm leading-relaxed font-mono focus:outline-none placeholder:text-muted-foreground"
            placeholder="无内容"
            spellCheck={false}
            autoFocus
          />
        )}
      </Card>

      {/* Footer */}
      <Card className="shrink-0">
        <div className="h-11 flex items-center justify-between px-4">
          <span className="text-xs text-muted-foreground">
            {text.length} 字符 · {new Blob([text]).size} 字节
          </span>
          <div className="flex gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={() => getCurrentWindow().close()}
            >
              取消
            </Button>
            <Button
              size="sm"
              onClick={handleSaveAndClose}
              disabled={saving}
            >
              {saving ? "保存中..." : hasChanges ? "保存并关闭" : "关闭"}
            </Button>
          </div>
        </div>
      </Card>
    </div>
  );
}
