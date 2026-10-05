import { useState, useEffect, useCallback, useRef } from "react";
import { Eye16Regular, EyeOff16Regular } from "@fluentui/react-icons";
import { invoke } from "@tauri-apps/api/core";
import { SettingsCard, SettingsCardHeader } from "@/components/settings/SettingSection";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { useTranslation } from "@/i18n";
import { logError } from "@/lib/logger";
import { getOperationErrorMessage, reportUserError } from "@/lib/operation-feedback";
import { KEY_CODE_MAP } from "@/lib/shortcut-helpers";
import { getProviderOptions, getLanguages, translateText } from "@/lib/translate";
import { useTranslateSettings, type TranslateProvider, type LanguageMode } from "@/stores/translate-settings";

export function TranslateTab() {
  const { t } = useTranslation();
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

  const timersRef = useRef<Record<string, ReturnType<typeof setTimeout>>>({});
  const pendingRef = useRef<Record<string, { fn: (v: string) => void; value: string }>>({});
  const debounced = useCallback((key: string, fn: (v: string) => void, value: string) => {
    if (timersRef.current[key]) clearTimeout(timersRef.current[key]);
    pendingRef.current[key] = { fn, value };
    timersRef.current[key] = setTimeout(() => {
      delete pendingRef.current[key];
      fn(value);
    }, 300);
  }, []);

  useEffect(() => {
    return () => {
      // flush 待写值，避免 unmount 时丢失未持久化的设置
      Object.values(pendingRef.current).forEach(({ fn, value }) => fn(value));
      Object.values(timersRef.current).forEach(clearTimeout);
    };
  }, []);

  useEffect(() => {
    if (!loaded) loadSettings();
  }, [loaded, loadSettings]);

  const handleTsKeyDown = useCallback((e: KeyboardEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") {
      setTsRecording(false); setTsTempShortcut(""); setTsShortcutError("");
      return;
    }
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
      setTsShortcutError(t("settings.translate.shortcutIncomplete")); return;
    }
    const hasModifier = tsTempShortcut.split("+").some((p) =>
      ["Ctrl", "Alt", "Shift", "Win"].includes(p.trim())
    );
    if (!hasModifier) {
      setTsShortcutError(t("settings.translate.shortcutNeedModifier")); return;
    }
    setTsSaving(true);
    try {
      await invoke("update_translate_selection_shortcut", { newShortcut: tsTempShortcut });
      setTranslateSelectionShortcut(tsTempShortcut);
      setTsRecording(false); setTsTempShortcut("");
    } catch (error) {
      logError("Failed to save translation shortcut", error);
      setTsShortcutError(t("settings.translate.shortcutSaveFailed", { error: getOperationErrorMessage(error) }));
    } finally { setTsSaving(false); }
  };

  const clearTsShortcut = async () => {
    setTsSaving(true);
    try {
      await invoke("update_translate_selection_shortcut", { newShortcut: "" });
      setTranslateSelectionShortcut(""); setTsTempShortcut(""); setTsRecording(false);
    } catch (error) { reportUserError(t("operationFeedback.userActions.updateShortcut"), error, "Failed to clear translation shortcut"); }
    finally { setTsSaving(false); }
  };

  const handleToggleTranslateSelection = async (value: boolean) => {
    setTranslateSelectionEnabled(value);
    if (value && translateSelectionShortcut) {
      try { await invoke("update_translate_selection_shortcut", { newShortcut: translateSelectionShortcut }); } catch (error) { reportUserError(t("operationFeedback.userActions.updateShortcut"), error); }
    } else if (!value) {
      try { await invoke("update_translate_selection_shortcut", { newShortcut: "" }); } catch (error) { reportUserError(t("operationFeedback.userActions.updateShortcut"), error); }
    }
  };

  if (!loaded) return null;

  return (
    <div className="space-y-3">
      {/* 总开关 */}
      <SettingsCard>
        <SettingsCardHeader
          title={t("settings.translate.entryTitle")}
          description={t("settings.translate.entryDesc")}
        />
        <div className="flex items-center justify-between">
          <div className="space-y-0.5">
            <Label className="text-xs">{t("settings.translate.enableEntry")}</Label>
            <p className="text-xs text-muted-foreground">{t("settings.translate.enableEntryDesc")}</p>
          </div>
          <Switch checked={enabled} onCheckedChange={async (value) => {
            setEnabled(value);
            if (!value && translateSelectionShortcut) {
              try { await invoke("update_translate_selection_shortcut", { newShortcut: "" }); } catch (error) { reportUserError(t("operationFeedback.userActions.updateShortcut"), error); }
            } else if (value && translateSelectionEnabled && translateSelectionShortcut) {
              try { await invoke("update_translate_selection_shortcut", { newShortcut: translateSelectionShortcut }); } catch (error) { reportUserError(t("operationFeedback.userActions.updateShortcut"), error); }
            }
          }} />
        </div>
        {enabled && (
          <div className="flex items-center justify-between pt-4 mt-1">
            <div className="space-y-0.5">
              <Label className="text-xs">{t("settings.translate.recordOnCopy")}</Label>
              <p className="text-xs text-muted-foreground">{t("settings.translate.recordOnCopyDesc")}</p>
            </div>
            <Switch checked={recordTranslation} onCheckedChange={setRecordTranslation} />
          </div>
        )}
      </SettingsCard>

      {enabled && (
        <>
          {/* 翻译渠道 */}
          <SettingsCard>
            <SettingsCardHeader title={t("settings.translate.providerTitle")} />
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <div className="space-y-0.5">
                  <Label className="text-xs">{t("settings.translate.provider")}</Label>
                  <p className="text-xs text-muted-foreground">{t("settings.translate.providerDesc")}</p>
                </div>
                <Select value={provider} onValueChange={(v) => setProvider(v as TranslateProvider)}>
                  <SelectTrigger className="w-[180px] h-8 text-xs"><SelectValue /></SelectTrigger>
                  <SelectContent>
                    {getProviderOptions().map((opt) => (
                      <SelectItem key={opt.value} value={opt.value}>{opt.label}</SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              {provider === "google_api" && (
                <div className="space-y-1.5 pt-1">
                  <Label className="text-xs">{t("settings.translate.googleApiKeyLabel")}</Label>
                  <div className="relative">
                    <Input className="h-8 text-xs pr-8" type={showGoogleKey ? "text" : "password"}
                      placeholder={t("settings.translate.googleApiKey")} value={googleApiKey}
                      onChange={(e) => { const v = e.target.value; useTranslateSettings.setState({ googleApiKey: v }); debounced("googleApiKey", setGoogleApiKey, v); }}
                    />
                    <button type="button" className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground transition-surface"
                      onClick={() => setShowGoogleKey(!showGoogleKey)}>
                      {showGoogleKey ? <EyeOff16Regular className="w-3.5 h-3.5" /> : <Eye16Regular className="w-3.5 h-3.5" />}
                    </button>
                  </div>
                  <p className="text-xs text-muted-foreground">
                    {t("settings.translate.visitBefore")}{" "}
                    <a className="text-primary hover:underline cursor-pointer"
                      onClick={() => import("@tauri-apps/plugin-opener").then(({ openUrl }) => openUrl("https://console.cloud.google.com/apis/credentials"))}>
                      Google Cloud Console
                    </a>
                    {" "}{t("settings.translate.linkGetGoogleKey")}
                  </p>
                </div>
              )}

              {provider === "deeplx" && (
                <div className="space-y-1.5 pt-1">
                  <Label className="text-xs">{t("settings.translate.requestUrl")}</Label>
                  <Input className="h-8 text-xs" placeholder={t("settings.translate.deeplxPlaceholder")} value={deeplxEndpoint}
                    onChange={(e) => { const v = e.target.value; useTranslateSettings.setState({ deeplxEndpoint: v }); debounced("deeplxEndpoint", setDeeplxEndpoint, v); }}
                  />
                </div>
              )}

              {provider === "baidu" && (
                <div className="space-y-2 pt-1">
                  <div className="space-y-1.5">
                    <Label className="text-xs">{t("settings.translate.baiduAppId")}</Label>
                    <Input className="h-8 text-xs" placeholder={t("settings.translate.baiduAppIdPlaceholder")} value={baiduAppId}
                      onChange={(e) => { const v = e.target.value; useTranslateSettings.setState({ baiduAppId: v }); debounced("baiduAppId", setBaiduAppId, v); }}
                    />
                  </div>
                  <div className="space-y-1.5">
                    <Label className="text-xs">{t("settings.translate.baiduSecret")}</Label>
                    <div className="relative">
                      <Input className="h-8 text-xs pr-8" type={showBaiduKey ? "text" : "password"}
                        placeholder={t("settings.translate.baiduSecretPlaceholder")} value={baiduSecretKey}
                        onChange={(e) => { const v = e.target.value; useTranslateSettings.setState({ baiduSecretKey: v }); debounced("baiduSecretKey", setBaiduSecretKey, v); }}
                      />
                      <button type="button" className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground transition-surface"
                        onClick={() => setShowBaiduKey(!showBaiduKey)}>
                        {showBaiduKey ? <EyeOff16Regular className="w-3.5 h-3.5" /> : <Eye16Regular className="w-3.5 h-3.5" />}
                      </button>
                    </div>
                  </div>
                  <p className="text-xs text-muted-foreground">
                    {t("settings.translate.visitBefore")}{" "}
                    <a className="text-primary hover:underline cursor-pointer"
                      onClick={() => import("@tauri-apps/plugin-opener").then(({ openUrl }) => openUrl("https://fanyi-api.baidu.com/manage/developer"))}>
                      {t("settings.translate.baiduPlatform")}
                    </a>
                    {" "}{t("settings.translate.linkGetBaiduKey")}
                  </p>
                </div>
              )}

              {provider === "openai" && (
                <div className="space-y-2 pt-1">
                  <div className="space-y-1.5">
                    <Label className="text-xs">{t("settings.translate.apiUrl")}</Label>
                    <Input className="h-8 text-xs" placeholder={t("settings.translate.openaiUrlPlaceholder")} value={openaiEndpoint}
                      onChange={(e) => { const v = e.target.value; useTranslateSettings.setState({ openaiEndpoint: v }); debounced("openaiEndpoint", setOpenaiEndpoint, v); }}
                    />
                    <p className="text-xs text-muted-foreground">{t("settings.translate.openaiFormat")}</p>
                  </div>
                  <div className="space-y-1.5">
                    <Label className="text-xs">{t("settings.translate.apiKeyLabel")}</Label>
                    <div className="relative">
                      <Input className="h-8 text-xs pr-8" type={showOpenaiKey ? "text" : "password"}
                        placeholder={t("settings.translate.openaiApiKey")} value={openaiApiKey}
                        onChange={(e) => { const v = e.target.value; useTranslateSettings.setState({ openaiApiKey: v }); debounced("openaiApiKey", setOpenaiApiKey, v); }}
                      />
                      <button type="button" className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground transition-surface"
                        onClick={() => setShowOpenaiKey(!showOpenaiKey)}>
                        {showOpenaiKey ? <EyeOff16Regular className="w-3.5 h-3.5" /> : <Eye16Regular className="w-3.5 h-3.5" />}
                      </button>
                    </div>
                  </div>
                  <div className="space-y-1.5">
                    <Label className="text-xs">{t("settings.translate.modelId")}</Label>
                    <Input className="h-8 text-xs" placeholder={t("settings.translate.openaiModelPlaceholder")} value={openaiModel}
                      onChange={(e) => { const v = e.target.value; useTranslateSettings.setState({ openaiModel: v }); debounced("openaiModel", setOpenaiModel, v); }}
                    />
                  </div>
                </div>
              )}

              {/* 网络代理 */}
              <div className="flex items-center justify-between pt-4">
                <div className="space-y-0.5">
                  <Label className="text-xs">{t("settings.translate.proxy")}</Label>
                  <p className="text-xs text-muted-foreground">{t("settings.translate.proxyDesc")}</p>
                </div>
                <Select value={proxyMode} onValueChange={(v) => setProxyMode(v as "system" | "none" | "custom")}>
                  <SelectTrigger className="w-[130px] h-8 text-xs"><SelectValue /></SelectTrigger>
                  <SelectContent>
                    <SelectItem value="system">{t("settings.translate.proxySystem")}</SelectItem>
                    <SelectItem value="none">{t("settings.translate.proxyNone")}</SelectItem>
                    <SelectItem value="custom">{t("settings.translate.proxyCustom")}</SelectItem>
                  </SelectContent>
                </Select>
              </div>
              {proxyMode === "custom" && (
                <Input className="h-8 text-xs mt-2" placeholder={t("settings.translate.proxyPlaceholder")}
                  value={proxyUrl}
                  onChange={(e) => { const v = e.target.value; useTranslateSettings.setState({ proxyUrl: v }); debounced("proxyUrl", setProxyUrl, v); }}
                />
              )}

              {/* 测试按钮 */}
              <div className="flex items-center gap-3 pt-2">
                <Button variant="outline" size="sm" className="h-7 text-xs" disabled={testing}
                  onClick={async () => {
                    setTesting(true); setTestResult(null);
                    try {
                      const result = await translateText("Hello");
                      setTestResult({ ok: true, msg: t("settings.translate.testSuccess", { result }) });
                    } catch (error) {
                      setTestResult({ ok: false, msg: error instanceof Error ? error.message : getOperationErrorMessage(error) });
                    } finally { setTesting(false); }
                  }}>
                  {testing ? t("settings.translate.testing") : t("settings.translate.testConnection")}
                </Button>
                {testResult && (
                  <span className={`text-xs ${testResult.ok ? "text-status-success" : "text-destructive"}`}>
                    {testResult.msg}
                  </span>
                )}
              </div>
            </div>
          </SettingsCard>

          {/* 语言设置 */}
          <SettingsCard>
            <SettingsCardHeader
              title={t("settings.translate.langTitle")}
              description={t("settings.translate.langDesc")}
            />
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <div className="space-y-0.5">
                  <Label className="text-xs">{t("settings.translate.langMode")}</Label>
                  <p className="text-xs text-muted-foreground">
                    {languageMode === "auto" ? t("settings.translate.langModeAutoDesc") : t("settings.translate.langModeManualDesc")}
                  </p>
                </div>
                <Select value={languageMode} onValueChange={(v) => setLanguageMode(v as LanguageMode)}>
                  <SelectTrigger className="w-[130px] h-8 text-xs"><SelectValue /></SelectTrigger>
                  <SelectContent>
                    <SelectItem value="auto">{t("settings.translate.langModeAuto")}</SelectItem>
                    <SelectItem value="manual">{t("settings.translate.langModeManual")}</SelectItem>
                  </SelectContent>
                </Select>
              </div>
              {languageMode === "manual" && (
                <div className="grid grid-cols-2 gap-2 pt-1">
                  <div className="space-y-1.5">
                    <Label className="text-xs">{t("settings.translate.sourceLanguage")}</Label>
                    <Select value={sourceLanguage || "auto"} onValueChange={setSourceLanguage}>
                      <SelectTrigger className="h-8 text-xs"><SelectValue /></SelectTrigger>
                      <SelectContent>
                        <SelectItem value="auto">{t("settings.translate.langModeAuto")}</SelectItem>
                        {getLanguages().map((lang) => (<SelectItem key={lang.value} value={lang.value}>{lang.label}</SelectItem>))}
                      </SelectContent>
                    </Select>
                  </div>
                  <div className="space-y-1.5">
                    <Label className="text-xs">{t("settings.translate.targetLanguage")}</Label>
                    <Select value={targetLanguage || "zh"} onValueChange={setTargetLanguage}>
                      <SelectTrigger className="h-8 text-xs"><SelectValue /></SelectTrigger>
                      <SelectContent>
                        {getLanguages().map((lang) => (<SelectItem key={lang.value} value={lang.value}>{lang.label}</SelectItem>))}
                      </SelectContent>
                    </Select>
                  </div>
                </div>
              )}
            </div>
          </SettingsCard>

          {/* 翻译选中文字 */}
          <SettingsCard>
            <SettingsCardHeader
              title={t("settings.translate.selectionActionTitle")}
              description={t("settings.translate.selectionActionDesc")}
            />
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-xs">{t("settings.translate.selectionActionEnable")}</Label>
                <p className="text-xs text-muted-foreground">{t("settings.translate.selectionActionEnableDesc")}</p>
              </div>
              <Switch checked={translateSelectionEnabled} onCheckedChange={handleToggleTranslateSelection} />
            </div>
            {translateSelectionEnabled && (
              <div className="space-y-3 pt-4 mt-1 border-t">
                <Label className="text-xs">{t("settings.translate.shortcut")}</Label>
                <div className="flex gap-2">
                  <Input
                    value={tsRecording ? tsTempShortcut || t("settings.shortcuts.pressShortcut") : translateSelectionShortcut || t("settings.translate.notSet")}
                    readOnly
                    className="flex-1 h-8 text-sm ui-font bg-muted"
                    onClick={() => { if (!tsRecording) { setTsRecording(true); setTsTempShortcut(""); setTsShortcutError(""); } }}
                  />
                  {tsRecording ? (
                    <div className="flex gap-1">
                      <Button variant="default" size="sm" className="h-8"
                        disabled={!tsTempShortcut || tsTempShortcut.includes("...") || tsSaving}
                        onClick={saveTsShortcut}>{t("common.save")}</Button>
                      <Button variant="outline" size="sm" className="h-8"
                        onClick={() => { setTsRecording(false); setTsTempShortcut(""); setTsShortcutError(""); }}>{t("common.cancel")}</Button>
                    </div>
                  ) : (
                    <div className="flex gap-1">
                      <Button variant="outline" size="sm" className="h-8"
                        onClick={() => { setTsRecording(true); setTsTempShortcut(""); setTsShortcutError(""); }}>{t("common.modify")}</Button>
                      {translateSelectionShortcut && (
                        <Button variant="ghost" size="sm" className="h-8 text-muted-foreground"
                          onClick={clearTsShortcut} disabled={tsSaving}>{t("common.clear")}</Button>
                      )}
                    </div>
                  )}
                </div>
                {tsShortcutError && <p className="text-xs text-destructive">{tsShortcutError}</p>}
              </div>
            )}
          </SettingsCard>
        </>
      )}
    </div>
  );
}
