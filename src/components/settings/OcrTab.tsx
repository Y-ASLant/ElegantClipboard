import { useState, useEffect, useCallback, useRef } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { Eye16Regular, EyeOff16Regular } from "@fluentui/react-icons";
import { invoke } from "@tauri-apps/api/core";
import { useOcrSettings } from "@/stores/ocr-settings";
import { useTranslateSettings } from "@/stores/translate-settings";
import { logError } from "@/lib/logger";

/** KeyboardEvent.code 到快捷键名称的映射 */
const KEY_CODE_MAP: Record<string, string> = {
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

export function OcrTab() {
  const {
    enabled, setEnabled,
    recordOcrCopy, setRecordOcrCopy,
    autoCopy, setAutoCopy,
    autoTranslate, setAutoTranslate,
    accuracy, setAccuracy,
    shortcut, setShortcut,
    baiduApiKey, setBaiduApiKey,
    baiduSecretKey, setBaiduSecretKey,
    proxyMode, setProxyMode,
    proxyUrl, setProxyUrl,
    loaded, loadSettings,
  } = useOcrSettings();

  const translateEnabled = useTranslateSettings((s) => s.enabled);

  const [showSecretKey, setShowSecretKey] = useState(false);
  const [recording, setRecording] = useState(false);
  const [tempShortcut, setTempShortcut] = useState("");
  const [shortcutError, setShortcutError] = useState("");
  const [shortcutSaving, setShortcutSaving] = useState(false);

  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const debounced = useCallback(<T extends (...args: string[]) => void>(fn: T, value: string) => {
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => fn(value), 300);
  }, []);

  useEffect(() => {
    if (!loaded) loadSettings();
  }, [loaded, loadSettings]);

  // 录入快捷键
  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    e.preventDefault();
    e.stopPropagation();

    const parts: string[] = [];
    if (e.ctrlKey) parts.push("Ctrl");
    if (e.altKey) parts.push("Alt");
    if (e.shiftKey) parts.push("Shift");
    if (e.metaKey) parts.push("Win");

    let key = "";
    if (e.code.startsWith("Key")) {
      key = e.code.replace("Key", "");
    } else if (e.code.startsWith("Digit")) {
      key = e.code.replace("Digit", "");
    } else if (e.code.startsWith("F") && !isNaN(Number(e.code.slice(1)))) {
      key = e.code;
    } else {
      key = KEY_CODE_MAP[e.code] || "";
    }

    if (key) {
      parts.push(key);
      setTempShortcut(parts.join("+"));
      setShortcutError("");
    } else if (parts.length > 0) {
      setTempShortcut(parts.join("+") + "+...");
    }
  }, []);

  useEffect(() => {
    if (recording) {
      window.addEventListener("keydown", handleKeyDown);
      return () => window.removeEventListener("keydown", handleKeyDown);
    }
  }, [recording, handleKeyDown]);

  const saveShortcut = async () => {
    if (!tempShortcut || tempShortcut.includes("...")) {
      setShortcutError("请输入完整的快捷键");
      return;
    }
    setShortcutSaving(true);
    try {
      await invoke("update_ocr_shortcut", { newShortcut: tempShortcut });
      setShortcut(tempShortcut);
      setRecording(false);
      setTempShortcut("");
    } catch (error) {
      setShortcutError(`保存失败: ${error}`);
    } finally {
      setShortcutSaving(false);
    }
  };

  const clearShortcut = async () => {
    setShortcutSaving(true);
    try {
      await invoke("update_ocr_shortcut", { newShortcut: "" });
      setShortcut("");
      setTempShortcut("");
      setRecording(false);
    } catch (error) {
      logError("清除 OCR 快捷键失败:", error);
    } finally {
      setShortcutSaving(false);
    }
  };

  const handleToggleEnabled = async (value: boolean) => {
    setEnabled(value);
    try {
      await invoke("ocr_toggle_enabled", { enabled: value });
    } catch (error) {
      logError("切换 OCR 状态失败:", error);
    }
  };

  if (!loaded) return null;

  return (
    <div className="space-y-4">
      {/* 总开关 */}
      <div className="rounded-lg border bg-card p-4">
        <h3 className="text-sm font-medium mb-3">OCR识别</h3>
        <p className="text-xs text-muted-foreground mb-4">
          开启后，按下快捷键可截取屏幕区域并识别其中的文字
        </p>
        <div className="flex items-center justify-between">
          <div className="space-y-0.5">
            <Label className="text-xs">启用 OCR识别</Label>
            <p className="text-xs text-muted-foreground">
              开启后可通过快捷键触发屏幕截图文字识别
            </p>
          </div>
          <Switch checked={enabled} onCheckedChange={handleToggleEnabled} />
        </div>
        {enabled && (
          <>
            <div className="flex items-center justify-between pt-4 mt-1">
              <div className="space-y-0.5">
                <Label className="text-xs">复制识别结果时记录条目</Label>
                <p className="text-xs text-muted-foreground">
                  开启后复制识别结果时会作为新条目记录到剪贴板历史
                </p>
              </div>
              <Switch checked={recordOcrCopy} onCheckedChange={setRecordOcrCopy} />
            </div>
            <div className="flex items-center justify-between pt-4 mt-1">
              <div className="space-y-0.5">
                <Label className="text-xs">识别完成后自动复制</Label>
                <p className="text-xs text-muted-foreground">
                  识别结果出来后自动复制到剪贴板
                </p>
              </div>
              <Switch checked={autoCopy} onCheckedChange={setAutoCopy} />
            </div>
            <div className="flex items-center justify-between pt-4 mt-1">
              <div className="space-y-0.5">
                <Label className="text-xs">识别完成后自动翻译</Label>
                <p className={`text-xs ${translateEnabled ? "text-muted-foreground" : "text-destructive"}`}>
                  {translateEnabled ? "识别结果出来后自动调用翻译" : "需先在「条目翻译」中开启翻译功能"}
                </p>
              </div>
              <Switch
                checked={autoTranslate}
                disabled={!translateEnabled}
                onCheckedChange={setAutoTranslate}
              />
            </div>
          </>
        )}
      </div>

      {enabled && (
        <>
          {/* 快捷键 */}
          <div className="rounded-lg border bg-card p-4">
            <h3 className="text-sm font-medium mb-3">截图快捷键</h3>
            <p className="text-xs text-muted-foreground mb-4">
              设置触发 OCR 截图识别的全局快捷键
            </p>
            <div className="space-y-3">
              <div className="flex gap-2">
                <Input
                  value={recording ? tempShortcut || "按下快捷键..." : shortcut || "未设置"}
                  readOnly
                  className={`flex-1 h-8 text-sm bg-muted ${shortcut || recording ? "font-mono" : ""}`}
                  onClick={() => {
                    if (!recording) {
                      setRecording(true);
                      setTempShortcut("");
                      setShortcutError("");
                    }
                  }}
                />
                {recording ? (
                  <div className="flex gap-1">
                    <Button
                      variant="default"
                      size="sm"
                      className="h-8"
                      disabled={!tempShortcut || tempShortcut.includes("...") || shortcutSaving}
                      onClick={saveShortcut}
                    >
                      保存
                    </Button>
                    <Button
                      variant="outline"
                      size="sm"
                      className="h-8"
                      onClick={() => {
                        setRecording(false);
                        setTempShortcut("");
                        setShortcutError("");
                      }}
                    >
                      取消
                    </Button>
                  </div>
                ) : (
                  <div className="flex gap-1">
                    <Button
                      variant="outline"
                      size="sm"
                      className="h-8"
                      onClick={() => {
                        setRecording(true);
                        setTempShortcut("");
                        setShortcutError("");
                      }}
                    >
                      修改
                    </Button>
                    {shortcut && (
                      <Button
                        variant="ghost"
                        size="sm"
                        className="h-8 text-muted-foreground"
                        onClick={clearShortcut}
                        disabled={shortcutSaving}
                      >
                        清除
                      </Button>
                    )}
                  </div>
                )}
              </div>
              {shortcutError && (
                <p className="text-xs text-destructive">{shortcutError}</p>
              )}
              <p className="text-xs text-muted-foreground">
                按下快捷键后将截取屏幕，拖拽选择区域后自动识别文字
              </p>
            </div>
          </div>

          {/* 识别接口 */}
          <div className="rounded-lg border bg-card p-4">
            <h3 className="text-sm font-medium mb-3">识别接口</h3>
            <p className="text-xs text-muted-foreground mb-4">
              配置百度 OCR 识别接口参数
            </p>
            <div className="space-y-3">
              {/* 识别精度 */}
              <div className="flex items-center justify-between">
                <div className="space-y-0.5">
                  <Label className="text-xs">识别精度</Label>
                  <p className="text-xs text-muted-foreground">
                    {accuracy === "high" ? "精度更高，速度较慢" : "速度更快，精度略低"}
                  </p>
                </div>
                <Select value={accuracy} onValueChange={(v) => setAccuracy(v as "high" | "standard")}>
                  <SelectTrigger className="w-[150px] h-8 text-xs"><SelectValue /></SelectTrigger>
                  <SelectContent>
                    <SelectItem value="high">高精度版</SelectItem>
                    <SelectItem value="standard">标准版（更快）</SelectItem>
                  </SelectContent>
                </Select>
              </div>
              <div className="space-y-1.5">
                <Label className="text-xs">API Key</Label>
                <Input
                  className="h-8 text-xs"
                  placeholder="输入百度 OCR API Key"
                  value={baiduApiKey}
                  onChange={(e) => {
                    const v = e.target.value;
                    useOcrSettings.setState({ baiduApiKey: v });
                    debounced(setBaiduApiKey, v);
                  }}
                />
              </div>
              <div className="space-y-1.5">
                <Label className="text-xs">Secret Key</Label>
                <div className="relative">
                  <Input
                    className="h-8 text-xs pr-8"
                    type={showSecretKey ? "text" : "password"}
                    placeholder="输入百度 OCR Secret Key"
                    value={baiduSecretKey}
                    onChange={(e) => {
                      const v = e.target.value;
                      useOcrSettings.setState({ baiduSecretKey: v });
                      debounced(setBaiduSecretKey, v);
                    }}
                  />
                  <button
                    type="button"
                    className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground transition-colors"
                    onClick={() => setShowSecretKey(!showSecretKey)}
                  >
                    {showSecretKey ? <EyeOff16Regular className="w-3.5 h-3.5" /> : <Eye16Regular className="w-3.5 h-3.5" />}
                  </button>
                </div>
              </div>
              <p className="text-xs text-muted-foreground">
                请前往{" "}
                <a
                  className="text-primary hover:underline cursor-pointer"
                  onClick={() => {
                    import("@tauri-apps/plugin-opener").then(({ openUrl }) => {
                      openUrl("https://console.bce.baidu.com/ai/#/ai/ocr/overview/index");
                    });
                  }}
                >
                  百度智能云 OCR 控制台
                </a>
                {" "}获取 API Key 和 Secret Key
              </p>

              {/* 网络代理 */}
              <div className="flex items-center justify-between pt-4">
                <div className="space-y-0.5">
                  <Label className="text-xs">网络代理</Label>
                  <p className="text-xs text-muted-foreground">OCR 请求使用的代理设置</p>
                </div>
                <Select value={proxyMode} onValueChange={(v) => setProxyMode(v as "system" | "none" | "custom")}>
                  <SelectTrigger className="w-[130px] h-8 text-xs"><SelectValue /></SelectTrigger>
                  <SelectContent>
                    <SelectItem value="system">系统代理</SelectItem>
                    <SelectItem value="none">不使用代理</SelectItem>
                    <SelectItem value="custom">自定义代理</SelectItem>
                  </SelectContent>
                </Select>
              </div>
              {proxyMode === "custom" && (
                <div
                  className="mt-2"
                  ref={(el) => {
                    if (el) el.scrollIntoView({ behavior: "smooth", block: "nearest" });
                  }}
                >
                  <Input
                    className="h-8 text-xs"
                    placeholder="http://127.0.0.1:7890 或 socks5://127.0.0.1:1080"
                    value={proxyUrl}
                    onChange={(e) => {
                      const v = e.target.value;
                      useOcrSettings.setState({ proxyUrl: v });
                      debounced(setProxyUrl, v);
                    }}
                  />
                </div>
              )}
            </div>
          </div>
        </>
      )}
    </div>
  );
}
