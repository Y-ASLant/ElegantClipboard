import { useState, useEffect, useCallback, useRef } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { Eye16Regular, EyeOff16Regular, Speaker216Regular } from "@fluentui/react-icons";
import { invoke } from "@tauri-apps/api/core";
import { useTranslateSettings, type TranslateProvider, type LanguageMode } from "@/stores/translate-settings";
import { useTtsSettings, type TtsEngine } from "@/stores/tts-settings";
import { PROVIDER_OPTIONS, LANGUAGES, translateText } from "@/lib/translate";
import { speakWithEngine, stopSpeaking } from "@/lib/tts";
import { logError } from "@/lib/logger";

/** KeyboardEvent.code 到快捷键名称的映射 */
const KEY_CODE_MAP: Record<string, string> = {
  Space: "Space", Tab: "Tab", Enter: "Enter", Backspace: "Backspace",
  Delete: "Delete", Escape: "Esc", Home: "Home", End: "End",
  PageUp: "PageUp", PageDown: "PageDown",
  ArrowUp: "Up", ArrowDown: "Down", ArrowLeft: "Left", ArrowRight: "Right",
  Backquote: "`",
};

export function TranslateTab() {
  const {
    enabled, setEnabled,
    recordTranslation, setRecordTranslation,
    provider, setProvider,
    languageMode, setLanguageMode,
    sourceLanguage, setSourceLanguage,
    targetLanguage, setTargetLanguage,
    deeplxEndpoint, setDeeplxEndpoint,
    googleApiKey, setGoogleApiKey,
    baiduAppId, setBaiduAppId,
    baiduSecretKey, setBaiduSecretKey,
    openaiEndpoint, setOpenaiEndpoint,
    openaiApiKey, setOpenaiApiKey,
    openaiModel, setOpenaiModel,
    proxyMode, setProxyMode,
    proxyUrl, setProxyUrl,
    translateSelectionEnabled, setTranslateSelectionEnabled,
    translateSelectionShortcut, setTranslateSelectionShortcut,
    loaded, loadSettings,
  } = useTranslateSettings();

  const [showGoogleKey, setShowGoogleKey] = useState(false);
  const [showBaiduKey, setShowBaiduKey] = useState(false);
  const [showOpenaiKey, setShowOpenaiKey] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; msg: string } | null>(null);
  const [tsRecording, setTsRecording] = useState(false);
  const [tsTempShortcut, setTsTempShortcut] = useState("");
  const [tsShortcutError, setTsShortcutError] = useState("");
  const [tsSaving, setTsSaving] = useState(false);

  // debounce helpers for text inputs
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const debounced = useCallback(<T extends (...args: string[]) => void>(fn: T, value: string) => {
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => fn(value), 300);
  }, []);

  useEffect(() => {
    if (!loaded) loadSettings();
  }, [loaded, loadSettings]);

  // 翻译选中文字快捷键录入
  const handleTsKeyDown = useCallback((e: KeyboardEvent) => {
    e.preventDefault();
    e.stopPropagation();
    const parts: string[] = [];
    if (e.ctrlKey) parts.push("Ctrl");
    if (e.altKey) parts.push("Alt");
    if (e.shiftKey) parts.push("Shift");
    if (e.metaKey) parts.push("Win");
    let key = "";
    if (e.code.startsWith("Key")) key = e.code.replace("Key", "");
    else if (e.code.startsWith("Digit")) key = e.code.replace("Digit", "");
    else if (e.code.startsWith("F") && !isNaN(Number(e.code.slice(1)))) key = e.code;
    else key = KEY_CODE_MAP[e.code] || "";
    if (key) { parts.push(key); setTsTempShortcut(parts.join("+")); setTsShortcutError(""); }
    else if (parts.length > 0) setTsTempShortcut(parts.join("+") + "+...");
  }, []);

  useEffect(() => {
    if (tsRecording) {
      window.addEventListener("keydown", handleTsKeyDown);
      return () => window.removeEventListener("keydown", handleTsKeyDown);
    }
  }, [tsRecording, handleTsKeyDown]);

  const saveTsShortcut = async () => {
    if (!tsTempShortcut || tsTempShortcut.includes("...")) {
      setTsShortcutError("请输入完整的快捷键"); return;
    }
    setTsSaving(true);
    try {
      await invoke("update_translate_selection_shortcut", { newShortcut: tsTempShortcut });
      setTranslateSelectionShortcut(tsTempShortcut);
      setTsRecording(false); setTsTempShortcut("");
    } catch (error) {
      setTsShortcutError(`保存失败: ${error}`);
    } finally { setTsSaving(false); }
  };

  const clearTsShortcut = async () => {
    setTsSaving(true);
    try {
      await invoke("update_translate_selection_shortcut", { newShortcut: "" });
      setTranslateSelectionShortcut(""); setTsTempShortcut(""); setTsRecording(false);
    } catch (error) { logError("清除翻译快捷键失败:", error); }
    finally { setTsSaving(false); }
  };

  const handleToggleTranslateSelection = async (value: boolean) => {
    setTranslateSelectionEnabled(value);
    // 开关变化时需重新注册/注销快捷键
    if (value && translateSelectionShortcut) {
      try { await invoke("update_translate_selection_shortcut", { newShortcut: translateSelectionShortcut }); } catch {}
    } else if (!value) {
      try { await invoke("update_translate_selection_shortcut", { newShortcut: "" }); } catch {}
    }
  };

  if (!loaded) return null;

  return (
    <div className="space-y-4">
      {/* 总开关 */}
      <div className="rounded-lg border bg-card p-4">
        <h3 className="text-sm font-medium mb-3">条目翻译</h3>
        <p className="text-xs text-muted-foreground mb-4">
          开启后，每个条目的工具栏和右键菜单中将出现翻译选项，翻译结果会显示在条目下方
        </p>
        <div className="flex items-center justify-between">
          <div className="space-y-0.5">
            <Label className="text-xs">启用条目翻译</Label>
            <p className="text-xs text-muted-foreground">
              开启后可对剪贴板文本条目进行翻译
            </p>
          </div>
          <Switch checked={enabled} onCheckedChange={async (value) => {
            setEnabled(value);
            // 总开关变化时同步翻译选中文字快捷键
            if (!value && translateSelectionShortcut) {
              try { await invoke("update_translate_selection_shortcut", { newShortcut: "" }); } catch {}
            } else if (value && translateSelectionEnabled && translateSelectionShortcut) {
              try { await invoke("update_translate_selection_shortcut", { newShortcut: translateSelectionShortcut }); } catch {}
            }
          }} />
        </div>
        {enabled && (
          <div className="flex items-center justify-between pt-4 mt-1">
            <div className="space-y-0.5">
              <Label className="text-xs">复制翻译时记录条目</Label>
              <p className="text-xs text-muted-foreground">
                开启后复制翻译结果时会作为新条目记录到剪贴板历史
              </p>
            </div>
            <Switch checked={recordTranslation} onCheckedChange={setRecordTranslation} />
          </div>
        )}
      </div>

      {enabled && (
        <>
          {/* 翻译渠道 */}
          <div className="rounded-lg border bg-card p-4">
            <h3 className="text-sm font-medium mb-3">翻译渠道</h3>
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <div className="space-y-0.5">
                  <Label className="text-xs">翻译服务</Label>
                  <p className="text-xs text-muted-foreground">选择用于翻译的服务提供者</p>
                </div>
                <Select value={provider} onValueChange={(v) => setProvider(v as TranslateProvider)}>
                  <SelectTrigger className="w-[180px] h-8 text-xs"><SelectValue /></SelectTrigger>
                  <SelectContent>
                    {PROVIDER_OPTIONS.map((opt) => (
                      <SelectItem key={opt.value} value={opt.value}>{opt.label}</SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              {/* Google API 配置 */}
              {provider === "google_api" && (
                <div className="space-y-1.5 pt-1">
                  <Label className="text-xs">Google API Key</Label>
                  <div className="relative">
                    <Input
                      className="h-8 text-xs pr-8"
                      type={showGoogleKey ? "text" : "password"}
                      placeholder="输入 Google Cloud API Key"
                      value={googleApiKey}
                      onChange={(e) => {
                        const v = e.target.value;
                        useTranslateSettings.setState({ googleApiKey: v });
                        debounced(setGoogleApiKey, v);
                      }}
                    />
                    <button
                      type="button"
                      className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground transition-colors"
                      onClick={() => setShowGoogleKey(!showGoogleKey)}
                    >
                      {showGoogleKey ? <EyeOff16Regular className="w-3.5 h-3.5" /> : <Eye16Regular className="w-3.5 h-3.5" />}
                    </button>
                  </div>
                  <p className="text-xs text-muted-foreground">
                    请前往{" "}
                    <a
                      className="text-primary hover:underline cursor-pointer"
                      onClick={() => {
                        import("@tauri-apps/plugin-opener").then(({ openUrl }) => {
                          openUrl("https://console.cloud.google.com/apis/credentials");
                        });
                      }}
                    >
                      Google Cloud Console
                    </a>
                    {" "}获取 API Key
                  </p>
                </div>
              )}

              {/* DeepLX 配置 */}
              {provider === "deeplx" && (
                <div className="space-y-1.5 pt-1">
                  <Label className="text-xs">请求地址</Label>
                  <Input
                    className="h-8 text-xs"
                    placeholder="http://127.0.0.1:1188/translate"
                    value={deeplxEndpoint}
                    onChange={(e) => {
                      const v = e.target.value;
                      useTranslateSettings.setState({ deeplxEndpoint: v });
                      debounced(setDeeplxEndpoint, v);
                    }}
                  />
                </div>
              )}

              {/* 百度翻译配置 */}
              {provider === "baidu" && (
                <div className="space-y-2 pt-1">
                  <div className="space-y-1.5">
                    <Label className="text-xs">百度翻译 APP ID</Label>
                    <Input
                      className="h-8 text-xs"
                      placeholder="输入 APP ID"
                      value={baiduAppId}
                      onChange={(e) => {
                        const v = e.target.value;
                        useTranslateSettings.setState({ baiduAppId: v });
                        debounced(setBaiduAppId, v);
                      }}
                    />
                  </div>
                  <div className="space-y-1.5">
                    <Label className="text-xs">百度翻译密钥</Label>
                    <div className="relative">
                      <Input
                        className="h-8 text-xs pr-8"
                        type={showBaiduKey ? "text" : "password"}
                        placeholder="输入密钥"
                        value={baiduSecretKey}
                        onChange={(e) => {
                          const v = e.target.value;
                          useTranslateSettings.setState({ baiduSecretKey: v });
                          debounced(setBaiduSecretKey, v);
                        }}
                      />
                      <button
                        type="button"
                        className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground transition-colors"
                        onClick={() => setShowBaiduKey(!showBaiduKey)}
                      >
                        {showBaiduKey ? <EyeOff16Regular className="w-3.5 h-3.5" /> : <Eye16Regular className="w-3.5 h-3.5" />}
                      </button>
                    </div>
                  </div>
                  <p className="text-xs text-muted-foreground">
                    请前往{" "}
                    <a
                      className="text-primary hover:underline cursor-pointer"
                      onClick={() => {
                        import("@tauri-apps/plugin-opener").then(({ openUrl }) => {
                          openUrl("https://fanyi-api.baidu.com/manage/developer");
                        });
                      }}
                    >
                      百度翻译开放平台
                    </a>
                    {" "}获取 APP ID 和密钥
                  </p>
                </div>
              )}

              {/* OpenAI 配置 */}
              {provider === "openai" && (
                <div className="space-y-2 pt-1">
                  <div className="space-y-1.5">
                    <Label className="text-xs">API 接口地址</Label>
                    <Input
                      className="h-8 text-xs"
                      placeholder="https://api.openai.com/v1"
                      value={openaiEndpoint}
                      onChange={(e) => {
                        const v = e.target.value;
                        useTranslateSettings.setState({ openaiEndpoint: v });
                        debounced(setOpenaiEndpoint, v);
                      }}
                    />
                    <p className="text-xs text-muted-foreground">
                      支持自定义接口，兼容 OpenAI API 格式
                    </p>
                  </div>
                  <div className="space-y-1.5">
                    <Label className="text-xs">API Key</Label>
                    <div className="relative">
                      <Input
                        className="h-8 text-xs pr-8"
                        type={showOpenaiKey ? "text" : "password"}
                        placeholder="输入 API Key"
                        value={openaiApiKey}
                        onChange={(e) => {
                          const v = e.target.value;
                          useTranslateSettings.setState({ openaiApiKey: v });
                          debounced(setOpenaiApiKey, v);
                        }}
                      />
                      <button
                        type="button"
                        className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground transition-colors"
                        onClick={() => setShowOpenaiKey(!showOpenaiKey)}
                      >
                        {showOpenaiKey ? <EyeOff16Regular className="w-3.5 h-3.5" /> : <Eye16Regular className="w-3.5 h-3.5" />}
                      </button>
                    </div>
                  </div>
                  <div className="space-y-1.5">
                    <Label className="text-xs">模型 ID</Label>
                    <Input
                      className="h-8 text-xs"
                      placeholder="gpt-5.2"
                      value={openaiModel}
                      onChange={(e) => {
                        const v = e.target.value;
                        useTranslateSettings.setState({ openaiModel: v });
                        debounced(setOpenaiModel, v);
                      }}
                    />
                  </div>
                </div>
              )}
            </div>

              {/* 网络代理 */}
              <div className="flex items-center justify-between pt-4">
                <div className="space-y-0.5">
                  <Label className="text-xs">网络代理</Label>
                  <p className="text-xs text-muted-foreground">翻译请求使用的代理设置</p>
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
                <Input
                  className="h-8 text-xs mt-2"
                  placeholder="http://127.0.0.1:7890 或 socks5://127.0.0.1:1080"
                  value={proxyUrl}
                  onChange={(e) => {
                    const v = e.target.value;
                    useTranslateSettings.setState({ proxyUrl: v });
                    debounced(setProxyUrl, v);
                  }}
                />
              )}

              {/* 测试按钮 */}
              <div className="flex items-center gap-3 pt-2">
                <Button
                  variant="outline"
                  size="sm"
                  className="h-7 text-xs"
                  disabled={testing}
                  onClick={async () => {
                    setTesting(true);
                    setTestResult(null);
                    try {
                      const result = await translateText("Hello");
                      setTestResult({ ok: true, msg: `连接成功：${result}` });
                    } catch (error) {
                      setTestResult({ ok: false, msg: String(error) });
                    } finally {
                      setTesting(false);
                    }
                  }}
                >
                  {testing ? "测试中…" : "测试连接"}
                </Button>
                {testResult && (
                  <span className={`text-xs ${testResult.ok ? "text-green-600 dark:text-green-400" : "text-destructive"}`}>
                    {testResult.msg}
                  </span>
                )}
              </div>
          </div>

          {/* 语言设置 */}
          <div className="rounded-lg border bg-card p-4">
            <h3 className="text-sm font-medium mb-3">语言设置</h3>
            <p className="text-xs text-muted-foreground mb-4">
              配置翻译的源语言和目标语言
            </p>
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <div className="space-y-0.5">
                  <Label className="text-xs">语言模式</Label>
                  <p className="text-xs text-muted-foreground">
                    {languageMode === "auto"
                      ? "自动检测：中文→英文，其他语言→中文"
                      : "手动指定源语言和目标语言"}
                  </p>
                </div>
                <Select value={languageMode} onValueChange={(v) => setLanguageMode(v as LanguageMode)}>
                  <SelectTrigger className="w-[130px] h-8 text-xs"><SelectValue /></SelectTrigger>
                  <SelectContent>
                    <SelectItem value="auto">自动检测</SelectItem>
                    <SelectItem value="manual">手动选择</SelectItem>
                  </SelectContent>
                </Select>
              </div>

              {languageMode === "manual" && (
                <div className="grid grid-cols-2 gap-2 pt-1">
                  <div className="space-y-1.5">
                    <Label className="text-xs">源语言</Label>
                    <Select value={sourceLanguage || "auto"} onValueChange={setSourceLanguage}>
                      <SelectTrigger className="h-8 text-xs"><SelectValue /></SelectTrigger>
                      <SelectContent>
                        <SelectItem value="auto">自动检测</SelectItem>
                        {LANGUAGES.map((lang) => (
                          <SelectItem key={lang.value} value={lang.value}>{lang.label}</SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </div>
                  <div className="space-y-1.5">
                    <Label className="text-xs">目标语言</Label>
                    <Select value={targetLanguage || "zh"} onValueChange={setTargetLanguage}>
                      <SelectTrigger className="h-8 text-xs"><SelectValue /></SelectTrigger>
                      <SelectContent>
                        {LANGUAGES.map((lang) => (
                          <SelectItem key={lang.value} value={lang.value}>{lang.label}</SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </div>
                </div>
              )}
            </div>
          </div>
          {/* 翻译选中文字 */}
          <div className="rounded-lg border bg-card p-4">
            <h3 className="text-sm font-medium mb-3">翻译选中文字</h3>
            <p className="text-xs text-muted-foreground mb-4">
              开启后，按下快捷键可自动获取当前选中的文字并翻译
            </p>
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">启用翻译选中文字</Label>
                <p className="text-xs text-muted-foreground">
                  通过全局快捷键翻译任意应用中选中的文字
                </p>
              </div>
              <Switch checked={translateSelectionEnabled} onCheckedChange={handleToggleTranslateSelection} />
            </div>

            {translateSelectionEnabled && (
              <div className="space-y-3 pt-4 mt-1 border-t">
                <Label className="text-xs">快捷键</Label>
                <div className="flex gap-2">
                  <Input
                    value={tsRecording ? tsTempShortcut || "按下快捷键..." : translateSelectionShortcut || "未设置"}
                    readOnly
                    className={`flex-1 h-8 text-sm bg-muted ${translateSelectionShortcut || tsRecording ? "font-mono" : ""}`}
                    onClick={() => {
                      if (!tsRecording) { setTsRecording(true); setTsTempShortcut(""); setTsShortcutError(""); }
                    }}
                  />
                  {tsRecording ? (
                    <div className="flex gap-1">
                      <Button variant="default" size="sm" className="h-8"
                        disabled={!tsTempShortcut || tsTempShortcut.includes("...") || tsSaving}
                        onClick={saveTsShortcut}
                      >保存</Button>
                      <Button variant="outline" size="sm" className="h-8"
                        onClick={() => { setTsRecording(false); setTsTempShortcut(""); setTsShortcutError(""); }}
                      >取消</Button>
                    </div>
                  ) : (
                    <div className="flex gap-1">
                      <Button variant="outline" size="sm" className="h-8"
                        onClick={() => { setTsRecording(true); setTsTempShortcut(""); setTsShortcutError(""); }}
                      >修改</Button>
                      {translateSelectionShortcut && (
                        <Button variant="ghost" size="sm" className="h-8 text-muted-foreground"
                          onClick={clearTsShortcut} disabled={tsSaving}
                        >清除</Button>
                      )}
                    </div>
                  )}
                </div>
                {tsShortcutError && (
                  <p className="text-xs text-destructive">{tsShortcutError}</p>
                )}
              </div>
            )}
          </div>

          {/* 语音朗读（TTS） */}
          <TtsSettingsCard />
        </>
      )}
    </div>
  );
}

/** 声源语言能力：en=仅英文，zh=仅中文，multi=多语言（中英都行） */
type VoiceLang = "en" | "zh" | "multi";

const EDGE_VOICE_OPTIONS: { value: string; label: string; lang: VoiceLang }[] = [
  // 中文声源
  { value: "zh-CN-XiaoxiaoNeural", label: "晓晓（中文女声·温柔）", lang: "zh" },
  { value: "zh-CN-YunxiNeural", label: "云希（中文男声·阳光）", lang: "zh" },
  { value: "zh-CN-YunjianNeural", label: "云健（中文男声·沉稳）", lang: "zh" },
  { value: "zh-CN-XiaoyiNeural", label: "晓伊（中文女声·活泼）", lang: "zh" },
  // 女声（字母序）
  { value: "en-US-AnaNeural", label: "Ana（美式女声·年轻）", lang: "en" },
  { value: "en-US-AriaNeural", label: "Aria（美式女声·自然）", lang: "en" },
  { value: "en-US-AvaMultilingualNeural", label: "Ava（美式女声·多语言）", lang: "multi" },
  { value: "en-US-JennyNeural", label: "Jenny（美式女声·亲切）", lang: "en" },
  { value: "en-GB-LibbyNeural", label: "Libby（英式女声·温暖）", lang: "en" },
  { value: "en-GB-MaisieNeural", label: "Maisie（英式女声·活泼）", lang: "en" },
  { value: "en-GB-SoniaNeural", label: "Sonia（英式女声·优雅）", lang: "en" },
  // 男声（字母序）
  { value: "en-US-AndrewMultilingualNeural", label: "Andrew（美式男声·多语言）", lang: "multi" },
  { value: "en-US-BrianMultilingualNeural", label: "Brian（美式男声·多语言）", lang: "multi" },
  { value: "en-US-ChristopherNeural", label: "Christopher（美式男声·清晰）", lang: "en" },
  { value: "en-US-GuyNeural", label: "Guy（美式男声·稳重）", lang: "en" },
  { value: "en-GB-RyanNeural", label: "Ryan（英式男声·绅士）", lang: "en" },
];

/** 统一模式可选声源（多语言 + 中文） */
const UNIFIED_VOICE_OPTIONS = EDGE_VOICE_OPTIONS.filter((v) => v.lang === "multi" || v.lang === "zh");
/** 英文可选声源（英文 + 多语言） */
const EN_VOICE_OPTIONS = EDGE_VOICE_OPTIONS.filter((v) => v.lang === "en" || v.lang === "multi");
/** 中文可选声源（中文 + 多语言） */
const ZH_VOICE_OPTIONS = EDGE_VOICE_OPTIONS.filter((v) => v.lang === "zh" || v.lang === "multi");

/** Edge TTS 语速选项 */
const EDGE_RATE_OPTIONS: { value: string; label: string }[] = [
  { value: "-50%", label: "0.5x 极慢" },
  { value: "-25%", label: "0.75x 较慢" },
  { value: "+0%", label: "1.0x 正常" },
  { value: "+25%", label: "1.25x 较快" },
  { value: "+50%", label: "1.5x 快速" },
  { value: "+100%", label: "2.0x 极快" },
];

function TtsSettingsCard() {
  const {
    engine, setEngine,
    edgeVoice, setEdgeVoice,
    edgeVoiceEn, setEdgeVoiceEn,
    edgeVoiceZh, setEdgeVoiceZh,
    edgeRate, setEdgeRate,
    edgeRateEn, setEdgeRateEn,
    edgeRateZh, setEdgeRateZh,
    splitVoice, setSplitVoice,
    proxyMode, setProxyMode,
    proxyUrl, setProxyUrl,
    highlightWord, setHighlightWord,
    loaded, loadSettings,
  } = useTtsSettings();

  const proxyTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const debouncedProxyUrl = useCallback((fn: (v: string) => void, value: string) => {
    if (proxyTimerRef.current) clearTimeout(proxyTimerRef.current);
    proxyTimerRef.current = setTimeout(() => fn(value), 300);
  }, []);

  const [testing, setTesting] = useState<"en" | "zh" | false>(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; msg: string } | null>(null);

  useEffect(() => {
    if (!loaded) loadSettings();
  }, [loaded, loadSettings]);

  const handleTest = async (lang: "en" | "zh") => {
    setTesting(lang);
    setTestResult(null);
    const testText = lang === "zh"
      ? "你好，这是一段中文语音朗读测试。"
      : "Hello, this is a test of the text to speech engine.";
    try {
      await new Promise<void>((resolve, reject) => {
        speakWithEngine(testText, engine, {
          onEnd: () => resolve(),
          onError: (err) => reject(new Error(err)),
        });
      });
      setTestResult({ ok: true, msg: "朗读成功" });
    } catch (error) {
      setTestResult({ ok: false, msg: String(error) });
    } finally {
      setTesting(false);
    }
  };

  if (!loaded) return null;

  return (
    <div className="rounded-lg border bg-card p-4">
      <h3 className="text-sm font-medium mb-3">语音朗读（TTS）</h3>
      <p className="text-xs text-muted-foreground mb-4">
        配置翻译结果的语音朗读引擎和声源，支持自然语音合成
      </p>
      <div className="space-y-3">
        {/* 引擎选择 */}
        <div className="flex items-center justify-between">
          <div className="space-y-0.5">
            <Label className="text-xs">朗读引擎</Label>
            <p className="text-xs text-muted-foreground">
              {engine === "edge" && "微软在线语音，免费、自然度高（推荐）"}
              {engine === "browser" && "浏览器内置语音，离线可用，声音较机械"}
            </p>
          </div>
          <Select value={engine} onValueChange={(v) => setEngine(v as TtsEngine)}>
            <SelectTrigger className="w-[180px] h-8 text-xs"><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="edge">Edge TTS（微软在线）</SelectItem>
              <SelectItem value="browser">浏览器内置</SelectItem>
            </SelectContent>
          </Select>
        </div>

        {/* 朗读逐词高亮 */}
        <div className="flex items-center justify-between">
          <div className="space-y-0.5">
            <Label className="text-xs">朗读逐词高亮</Label>
            <p className="text-xs text-muted-foreground">
              朗读时高亮显示当前读到的词
            </p>
          </div>
          <Switch checked={highlightWord} onCheckedChange={setHighlightWord} />
        </div>

        {/* Edge TTS 配置 */}
        {engine === "edge" && (
          <div className="space-y-3 pt-1">
            {/* 中英分开开关 */}
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">中英分开声源</Label>
                <p className="text-xs text-muted-foreground">
                  {splitVoice ? "英文和中文使用不同声源" : "中英文使用同一声源（仅显示多语言和中文声源）"}
                </p>
              </div>
              <Switch checked={splitVoice} onCheckedChange={setSplitVoice} />
            </div>

            {splitVoice ? (
              <>
                {/* 英文声源 */}
                <div className="flex items-center justify-between">
                  <Label className="text-xs">英文声源</Label>
                  <Select value={edgeVoiceEn} onValueChange={setEdgeVoiceEn}>
                    <SelectTrigger className="w-[220px] h-8 text-xs"><SelectValue /></SelectTrigger>
                    <SelectContent side="bottom" className="max-h-52">
                      {EN_VOICE_OPTIONS.map((opt) => (
                        <SelectItem key={opt.value} value={opt.value}>{opt.label}</SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
                {/* 中文声源 */}
                <div className="flex items-center justify-between">
                  <Label className="text-xs">中文声源</Label>
                  <Select value={edgeVoiceZh} onValueChange={setEdgeVoiceZh}>
                    <SelectTrigger className="w-[220px] h-8 text-xs"><SelectValue /></SelectTrigger>
                    <SelectContent side="bottom" className="max-h-52">
                      {ZH_VOICE_OPTIONS.map((opt) => (
                        <SelectItem key={opt.value} value={opt.value}>{opt.label}</SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              </>
            ) : (
              <div className="flex items-center justify-between">
                <Label className="text-xs">声源</Label>
                <Select value={edgeVoice} onValueChange={setEdgeVoice}>
                  <SelectTrigger className="w-[220px] h-8 text-xs"><SelectValue /></SelectTrigger>
                  <SelectContent side="bottom" className="max-h-52">
                    {UNIFIED_VOICE_OPTIONS.map((opt) => (
                      <SelectItem key={opt.value} value={opt.value}>{opt.label}</SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
            )}

            {splitVoice ? (
              <>
                <div className="flex items-center justify-between">
                  <Label className="text-xs">英文语速</Label>
                  <Select value={edgeRateEn} onValueChange={setEdgeRateEn}>
                    <SelectTrigger className="w-[130px] h-8 text-xs"><SelectValue /></SelectTrigger>
                    <SelectContent>
                      {EDGE_RATE_OPTIONS.map((opt) => (
                        <SelectItem key={opt.value} value={opt.value}>{opt.label}</SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
                <div className="flex items-center justify-between">
                  <Label className="text-xs">中文语速</Label>
                  <Select value={edgeRateZh} onValueChange={setEdgeRateZh}>
                    <SelectTrigger className="w-[130px] h-8 text-xs"><SelectValue /></SelectTrigger>
                    <SelectContent>
                      {EDGE_RATE_OPTIONS.map((opt) => (
                        <SelectItem key={opt.value} value={opt.value}>{opt.label}</SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              </>
            ) : (
              <div className="flex items-center justify-between">
                <Label className="text-xs">语速</Label>
                <Select value={edgeRate} onValueChange={setEdgeRate}>
                  <SelectTrigger className="w-[130px] h-8 text-xs"><SelectValue /></SelectTrigger>
                  <SelectContent>
                    {EDGE_RATE_OPTIONS.map((opt) => (
                      <SelectItem key={opt.value} value={opt.value}>{opt.label}</SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
            )}
          </div>
        )}

        {/* 网络代理（仅 Edge TTS） */}
        {engine === "edge" && (
          <div className="space-y-2 pt-1">
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">网络代理</Label>
                <p className="text-xs text-muted-foreground">Edge TTS 请求使用的代理设置</p>
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
              <Input
                className="h-8 text-xs"
                placeholder="http://127.0.0.1:7890 或 socks5://127.0.0.1:1080"
                value={proxyUrl}
                onChange={(e) => {
                  const v = e.target.value;
                  useTtsSettings.setState({ proxyUrl: v });
                  debouncedProxyUrl(setProxyUrl, v);
                }}
              />
            )}
          </div>
        )}

        {/* 测试按钮 */}
        <div className="flex items-center gap-2 pt-2 flex-wrap">
          <Button
            variant="outline"
            size="sm"
            className="h-7 text-xs"
            disabled={!!testing}
            onClick={() => handleTest("en")}
          >
            <Speaker216Regular className="w-3 h-3 mr-1" />
            {testing === "en" ? "朗读中…" : "英文测试"}
          </Button>
          <Button
            variant="outline"
            size="sm"
            className="h-7 text-xs"
            disabled={!!testing}
            onClick={() => handleTest("zh")}
          >
            <Speaker216Regular className="w-3 h-3 mr-1" />
            {testing === "zh" ? "朗读中…" : "中文测试"}
          </Button>
          {testing && (
            <Button
              variant="ghost"
              size="sm"
              className="h-7 text-xs"
              onClick={() => { stopSpeaking(); setTesting(false); }}
            >
              停止
            </Button>
          )}
          {testResult && (
            <span className={`text-xs ${testResult.ok ? "text-green-600 dark:text-green-400" : "text-destructive"}`}>
              {testResult.msg}
            </span>
          )}
        </div>
      </div>
    </div>
  );
}
