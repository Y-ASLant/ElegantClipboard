import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { logError } from "@/lib/logger";
import { cn } from "@/lib/utils";

export interface ShortcutSettings {
  shortcut: string;
  winv_replacement: boolean;
}

interface ShortcutsTabProps {
  settings: ShortcutSettings;
  onSettingsChange: (settings: ShortcutSettings) => void;
}

export function ShortcutsTab({
  settings,
  onSettingsChange,
}: ShortcutsTabProps) {
  const [winvLoading, setWinvLoading] = useState(false);
  const [winvError, setWinvError] = useState("");
  const [winvConfirmDialogOpen, setWinvConfirmDialogOpen] = useState(false);
  const [winvPendingAction, setWinvPendingAction] = useState<
    "enable" | "disable" | null
  >(null);

  // Shortcut editing state
  const [shortcutDialogOpen, setShortcutDialogOpen] = useState(false);
  const [recordingShortcut, setRecordingShortcut] = useState(false);
  const [tempShortcut, setTempShortcut] = useState("");
  const [shortcutError, setShortcutError] = useState("");

  // Handle keyboard event for shortcut recording
  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    e.preventDefault();
    e.stopPropagation();

    const parts: string[] = [];

    // Modifiers
    if (e.ctrlKey) parts.push("Ctrl");
    if (e.altKey) parts.push("Alt");
    if (e.shiftKey) parts.push("Shift");
    if (e.metaKey) parts.push("Win");

    // Key
    let key = "";
    if (e.code.startsWith("Key")) {
      key = e.code.replace("Key", "");
    } else if (e.code.startsWith("Digit")) {
      key = e.code.replace("Digit", "");
    } else if (e.code.startsWith("F") && !isNaN(Number(e.code.slice(1)))) {
      key = e.code; // F1-F12
    } else {
      const keyMap: Record<string, string> = {
        Space: "Space",
        Tab: "Tab",
        Enter: "Enter",
        Backspace: "Backspace",
        Delete: "Delete",
        Escape: "Esc",
        Home: "Home",
        End: "End",
        PageUp: "PageUp",
        PageDown: "PageDown",
        ArrowUp: "Up",
        ArrowDown: "Down",
        ArrowLeft: "Left",
        ArrowRight: "Right",
        Backquote: "`",
      };
      key = keyMap[e.code] || "";
    }

    if (key && parts.length > 0) {
      parts.push(key);
      setTempShortcut(parts.join("+"));
      setShortcutError("");
    } else if (!key && parts.length > 0) {
      // Only modifiers pressed, show hint
      setTempShortcut(parts.join("+") + "+...");
    } else if (key && parts.length === 0) {
      setShortcutError("请至少使用一个修饰键 (Ctrl/Alt/Shift/Win)");
    }
  }, []);

  // Start/stop recording
  useEffect(() => {
    if (recordingShortcut) {
      window.addEventListener("keydown", handleKeyDown);
      return () => window.removeEventListener("keydown", handleKeyDown);
    }
  }, [recordingShortcut, handleKeyDown]);

  const startRecording = () => {
    setRecordingShortcut(true);
    setTempShortcut("");
    setShortcutError("");
  };

  const cancelRecording = () => {
    setRecordingShortcut(false);
    setTempShortcut("");
    setShortcutError("");
    setShortcutDialogOpen(false);
  };

  const saveShortcut = async () => {
    if (!tempShortcut || tempShortcut.includes("...")) {
      setShortcutError("请输入完整的快捷键");
      return;
    }

    try {
      await invoke("update_shortcut", { newShortcut: tempShortcut });
      await invoke("set_setting", {
        key: "global_shortcut",
        value: tempShortcut,
      });
      onSettingsChange({ ...settings, shortcut: tempShortcut });
      setShortcutDialogOpen(false);
      setRecordingShortcut(false);
      setTempShortcut("");
    } catch (error) {
      setShortcutError(`保存失败: ${error}`);
    }
  };

  // Execute Win+V toggle after user confirms
  const executeWinvToggle = async () => {
    if (!winvPendingAction) return;

    setWinvConfirmDialogOpen(false);
    setWinvLoading(true);
    setWinvError("");

    try {
      if (winvPendingAction === "enable") {
        await invoke("enable_winv_replacement");
        onSettingsChange({ ...settings, winv_replacement: true });
      } else {
        await invoke("disable_winv_replacement");
        onSettingsChange({ ...settings, winv_replacement: false });
      }
    } catch (error) {
      logError("Failed to toggle Win+V replacement:", error);
      setWinvError(String(error));
    } finally {
      setWinvLoading(false);
      setWinvPendingAction(null);
    }
  };

  return (
    <>
      <div className="space-y-4">
        {/* Custom Shortcut Card */}
        <div className="rounded-lg border bg-card p-4">
          <h3 className="text-sm font-medium mb-3">自定义快捷键</h3>
          <p className="text-xs text-muted-foreground mb-4">
            自定义呼出剪贴板的快捷键
          </p>
          <div
            className={cn(
              "space-y-2",
              settings.winv_replacement && "opacity-50",
            )}
          >
            <Label className="text-xs">呼出快捷键</Label>
            <div className="flex gap-2">
              <Input
                value={settings.shortcut}
                readOnly
                className="flex-1 h-8 text-sm font-mono bg-muted"
              />
              <Button
                variant="outline"
                size="sm"
                className="h-8"
                onClick={() => setShortcutDialogOpen(true)}
                disabled={settings.winv_replacement}
              >
                修改
              </Button>
            </div>
            <p className="text-xs text-muted-foreground">
              {settings.winv_replacement
                ? "已启用 Win+V，自定义快捷键已禁用"
                : "点击修改按钮自定义快捷键"}
            </p>
          </div>
        </div>

        {/* Win+V Card */}
        <div className="rounded-lg border bg-card p-4">
          <h3 className="text-sm font-medium mb-3">使用 Win+V</h3>
          <p className="text-xs text-muted-foreground mb-4">
            用 Win+V 替代系统剪贴板
          </p>
          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">启用 Win+V</Label>
                <p className="text-xs text-muted-foreground">
                  替代系统剪贴板（将禁用自定义快捷键）
                </p>
              </div>
              <Switch
                checked={settings.winv_replacement}
                disabled={winvLoading}
                onCheckedChange={(checked) => {
                  // Open confirmation dialog
                  setWinvPendingAction(checked ? "enable" : "disable");
                  setWinvConfirmDialogOpen(true);
                }}
              />
            </div>
            {winvLoading && (
              <p className="text-xs text-muted-foreground">
                正在修改系统设置，请稍候...
              </p>
            )}
            {winvError && (
              <p className="text-xs text-destructive">{winvError}</p>
            )}
            <p className="text-xs text-amber-500">
              注意：此操作会修改注册表并重启 Windows 资源管理器
            </p>
          </div>
        </div>

        {/* Current Active Card */}
        <div className="rounded-lg border bg-card p-4">
          <h3 className="text-sm font-medium mb-3">当前生效</h3>
          <p className="text-xs text-muted-foreground mb-4">
            {settings.winv_replacement
              ? "使用 Win+V 呼出剪贴板"
              : `使用 ${settings.shortcut} 呼出剪贴板`}
          </p>
          <div className="flex items-center justify-between py-2 px-3 rounded-md bg-primary/10 border border-primary/20">
            <span className="text-sm font-medium">呼出/隐藏窗口</span>
            <kbd className="pointer-events-none inline-flex h-6 select-none items-center gap-1 rounded border bg-background px-2 font-mono text-xs font-medium">
              {settings.winv_replacement ? "Win+V" : settings.shortcut}
            </kbd>
          </div>
          <p className="text-xs text-muted-foreground mt-2">
            注：自定义快捷键和 Win+V 只能二选一，不能同时生效
          </p>
        </div>
      </div>

      {/* Shortcut Edit Dialog */}
      <Dialog
        open={shortcutDialogOpen}
        onOpenChange={(open) => {
          if (!open) cancelRecording();
          else setShortcutDialogOpen(open);
        }}
      >
        <DialogContent showCloseButton={false}>
          <DialogHeader>
            <DialogTitle>修改快捷键</DialogTitle>
            <DialogDescription>
              按下新的快捷键组合来设置呼出剪贴板的快捷键
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4 py-4">
            <div
              className={cn(
                "h-16 flex items-center justify-center rounded-md border-2 border-dashed transition-colors",
                recordingShortcut
                  ? "border-primary bg-primary/5"
                  : "border-muted",
              )}
              onClick={startRecording}
            >
              {recordingShortcut ? (
                <span className={cn("text-lg font-medium", tempShortcut && "font-mono")}>
                  {tempShortcut || "按下快捷键..."}
                </span>
              ) : (
                <span className="text-sm text-muted-foreground">
                  点击此处开始录入快捷键
                </span>
              )}
            </div>

            {shortcutError && (
              <p className="text-sm text-destructive">{shortcutError}</p>
            )}

            <p className="text-xs text-muted-foreground">
              快捷键必须包含至少一个修饰键 (Ctrl / Alt / Shift / Win)
              加一个普通按键
            </p>
          </div>

          <DialogFooter className="flex justify-between sm:justify-between">
            <Button
              variant="ghost"
              onClick={() => {
                setTempShortcut("Alt+C");
                setRecordingShortcut(false);
                setShortcutError("");
              }}
              className="text-muted-foreground"
            >
              重置为默认
            </Button>
            <div className="flex gap-2">
              <Button variant="outline" onClick={cancelRecording}>
                取消
              </Button>
              <Button
                onClick={saveShortcut}
                disabled={!tempShortcut || tempShortcut.includes("...")}
              >
                保存
              </Button>
            </div>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Win+V Confirmation Dialog */}
      <Dialog
        open={winvConfirmDialogOpen}
        onOpenChange={(open) => {
          if (!open) {
            setWinvConfirmDialogOpen(false);
            setWinvPendingAction(null);
          }
        }}
      >
        <DialogContent className="max-w-[400px]" showCloseButton={false}>
          <DialogHeader>
            <DialogTitle>
              {winvPendingAction === "enable" ? "启用 Win+V" : "禁用 Win+V"}
            </DialogTitle>
            <DialogDescription>
              {winvPendingAction === "enable"
                ? "启用 Win+V 需要修改注册表并重启 Windows 资源管理器，桌面会短暂刷新。"
                : "禁用 Win+V 需要恢复注册表并重启 Windows 资源管理器，桌面会短暂刷新。"}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => {
                setWinvConfirmDialogOpen(false);
                setWinvPendingAction(null);
              }}
            >
              取消
            </Button>
            <Button onClick={executeWinvToggle}>确定</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

