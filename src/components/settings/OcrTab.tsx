import { useState, useEffect, useCallback, useRef } from "react";
import { Eye16Regular, EyeOff16Regular } from "@fluentui/react-icons";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { logError } from "@/lib/logger";
import { useOcrSettings } from "@/stores/ocr-settings";
import { useTranslateSettings } from "@/stores/translate-settings";

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

/** 可复制的代码块 */
function CopyBlock({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const handleCopy = () => {
    navigator.clipboard.writeText(text);
    setCopied(true);
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => setCopied(false), 1500);
  };
  return (
    <div className="relative bg-muted rounded-md px-4 py-3 font-mono text-xs space-y-1 group/code">
      {text.split("\n").map((line, i) => <p key={i}>{line}</p>)}
      <button
        type="button"
        className="absolute top-2 right-2 px-2 py-1 rounded text-[11px] bg-background border opacity-0 group-hover/code:opacity-100 transition-opacity text-muted-foreground hover:text-foreground"
        onClick={handleCopy}
      >
        {copied ? "已复制" : "复制"}
      </button>
    </div>
  );
}

/** 可点击复制的行内代码 */
function CopyInlineCode({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const handleCopy = () => {
    navigator.clipboard.writeText(text);
    setCopied(true);
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => setCopied(false), 1500);
  };
  return (
    <code
      className="px-1.5 py-0.5 bg-muted rounded text-[11px] cursor-pointer hover:bg-muted/80 transition-colors"
      onClick={handleCopy}
      title="点击复制"
    >
      {copied ? "已复制 ✓" : text}
    </code>
  );
}

export function OcrTab() {
  const {
    enabled, setEnabled,
    recordOcrCopy, setRecordOcrCopy,
    autoCopy, setAutoCopy,
    autoTranslate, setAutoTranslate,
    provider, setProvider,
    accuracy, setAccuracy,
    shortcut, setShortcut,
    baiduApiKey, setBaiduApiKey,
    baiduSecretKey, setBaiduSecretKey,
    customApiUrl, setCustomApiUrl,
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
              选择 OCR 识别服务提供者
            </p>
            <div className="space-y-3">
              {/* 提供者选择 */}
              <div className="flex items-center justify-between">
                <div className="space-y-0.5">
                  <Label className="text-xs">识别服务</Label>
                  <p className="text-xs text-muted-foreground">
                    {provider === "baidu" ? "使用百度智能云 OCR API" : "使用自部署的 OCR 服务（PaddleOCR / RapidOCR 等）"}
                  </p>
                </div>
                <Select value={provider} onValueChange={(v) => setProvider(v as "baidu" | "custom")}>
                  <SelectTrigger className="w-[150px] h-8 text-xs"><SelectValue /></SelectTrigger>
                  <SelectContent>
                    <SelectItem value="baidu">百度 OCR</SelectItem>
                    <SelectItem value="custom">自定义 API</SelectItem>
                  </SelectContent>
                </Select>
              </div>

              {provider === "baidu" && (
                <>
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
                </>
              )}

              {provider === "custom" && (
                <>
                  <div className="space-y-1.5">
                    <Label className="text-xs">API 地址</Label>
                    <Input
                      className="h-8 text-xs"
                      placeholder="http://localhost:9003/ocr"
                      value={customApiUrl}
                      onChange={(e) => {
                        const v = e.target.value;
                        useOcrSettings.setState({ customApiUrl: v });
                        debounced(setCustomApiUrl, v);
                      }}
                    />
                  </div>
                  <div className="text-xs text-muted-foreground space-y-1.5">
                    <p>
                      推荐使用{" "}
                      <a
                        className="text-primary font-medium hover:underline cursor-pointer"
                        onClick={() => {
                          import("@tauri-apps/plugin-opener").then(({ openUrl }) => {
                            openUrl("https://rapidai.github.io/RapidOCRDocs/main/install_usage/rapidocr_api/usage/");
                          });
                        }}
                      >
                        RapidOCR API
                      </a>
                      ，安装简单、识别速度快、无需 GPU：
                    </p>
                    <CopyBlock text={"pip install rapidocr_api onnxruntime\nrapidocr_api"} />
                    <p>
                      启动后默认地址为{" "}
                      <CopyInlineCode text="http://localhost:9003/ocr" />
                    </p>
                    <p className="text-muted-foreground/70">如有 NVIDIA GPU 可将 onnxruntime 替换为 onnxruntime-gpu 以加速推理</p>
                    <p className="pt-1.5 text-muted-foreground/70">
                      也兼容{" "}
                      <a
                        className="text-primary/80 hover:underline cursor-pointer"
                        onClick={() => {
                          import("@tauri-apps/plugin-opener").then(({ openUrl }) => {
                            openUrl("https://github.com/PaddlePaddle/PaddleOCR");
                          });
                        }}
                      >
                        PaddleOCR
                      </a>
                      {" "}等其他 OCR 服务，只需接口返回包含识别文字的 JSON 即可。
                    </p>
                  </div>
                </>
              )}

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
