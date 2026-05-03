import { useState, useEffect, useCallback, useRef } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { Speaker216Regular } from "@fluentui/react-icons";
import { useTtsSettings, type TtsEngine } from "@/stores/tts-settings";
import { speakWithEngine, stopSpeaking } from "@/lib/tts";

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

export function TtsTab() {
  const {
    enabled, setEnabled,
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
    showToolbarTts, setShowToolbarTts,
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
    <div className="space-y-4">
      {/* 总开关 */}
      <div className="rounded-lg border bg-card p-4">
        <h3 className="text-sm font-medium mb-3">语音朗读</h3>
        <p className="text-xs text-muted-foreground mb-4">
          开启后，翻译结果、OCR 结果等文本区域将显示朗读按钮，支持自然语音合成
        </p>
        <div className="flex items-center justify-between">
          <div className="space-y-0.5">
            <Label className="text-xs">启用语音朗读</Label>
            <p className="text-xs text-muted-foreground">
              开启后可对文本内容进行语音朗读
            </p>
          </div>
          <Switch checked={enabled} onCheckedChange={setEnabled} />
        </div>
        {enabled && (
          <>
            <div className="flex items-center justify-between pt-4 mt-1">
              <div className="space-y-0.5">
                <Label className="text-xs">朗读逐词高亮</Label>
                <p className="text-xs text-muted-foreground">
                  朗读时高亮显示当前读到的词
                </p>
              </div>
              <Switch checked={highlightWord} onCheckedChange={setHighlightWord} />
            </div>
            <div className="flex items-center justify-between pt-4 mt-1">
              <div className="space-y-0.5">
                <Label className="text-xs">条目工具栏朗读按钮</Label>
                <p className="text-xs text-muted-foreground">
                  鼠标悬浮在文本条目上时，工具栏显示朗读按钮
                </p>
              </div>
              <Switch checked={showToolbarTts} onCheckedChange={setShowToolbarTts} />
            </div>
          </>
        )}
      </div>

      {enabled && (
        <div className="rounded-lg border bg-card p-4">
          <h3 className="text-sm font-medium mb-3">朗读配置</h3>
          <p className="text-xs text-muted-foreground mb-4">
            配置语音朗读引擎、声源和语速，支持自然语音合成
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
      )}
    </div>
  );
}
