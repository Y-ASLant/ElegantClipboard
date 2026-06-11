import { invoke } from "@tauri-apps/api/core";
import { logError } from "@/lib/logger";
import { useTranslateSettings, type TranslateProvider } from "@/stores/translate-settings";

export const LANGUAGES = [
  { value: "zh", label: "中文" },
  { value: "en", label: "英语" },
  { value: "ja", label: "日语" },
  { value: "ko", label: "韩语" },
  { value: "th", label: "泰语" },
  { value: "fr", label: "法语" },
  { value: "de", label: "德语" },
  { value: "ru", label: "俄语" },
  { value: "vi", label: "越南语" },
  { value: "es", label: "西班牙语" },
  { value: "pt", label: "葡萄牙语" },
  { value: "ar", label: "阿拉伯语" },
  { value: "it", label: "意大利语" },
];

function isChinese(text: string): boolean {
  const sample = text.slice(0, 200);
  let cjkCount = 0;
  let kanaCount = 0;
  let totalLetters = 0;
  for (const ch of sample) {
    const code = ch.codePointAt(0) ?? 0;
    if (
      (code >= 0x4e00 && code <= 0x9fff) ||
      (code >= 0x3400 && code <= 0x4dbf) ||
      (code >= 0x20000 && code <= 0x2a6df)
    ) {
      cjkCount++;
    }
    // 日文假名（平假名 + 片假名）→ 非中文
    if (
      (code >= 0x3040 && code <= 0x309f) ||
      (code >= 0x30a0 && code <= 0x30ff)
    ) {
      kanaCount++;
    }
    if (/\p{L}/u.test(ch)) totalLetters++;
  }
  // 含日文假名则判定为日文，非中文
  if (kanaCount > 0) return false;
  return totalLetters > 0 && cjkCount / totalLetters > 0.3;
}

function resolveLanguages(text: string): { from: string; to: string } {
  const settings = useTranslateSettings.getState();
  if (settings.languageMode === "manual" && settings.sourceLanguage && settings.targetLanguage) {
    return { from: settings.sourceLanguage, to: settings.targetLanguage };
  }
  if (isChinese(text)) {
    return { from: "zh", to: "en" };
  }
  return { from: "auto", to: "zh" };
}

export async function translateText(text: string): Promise<string> {
  const settings = useTranslateSettings.getState();
  if (!settings.enabled) throw new Error("翻译功能未启用");

  const { from, to } = resolveLanguages(text);

  try {
    return await invoke<string>("translate_text", {
      text,
      from,
      to,
      provider: settings.provider,
      proxyMode: settings.proxyMode,
      proxyUrl: settings.proxyUrl,
      deeplxEndpoint: settings.deeplxEndpoint || null,
      googleApiKey: settings.googleApiKey || null,
      baiduAppId: settings.baiduAppId || null,
      baiduSecretKey: settings.baiduSecretKey || null,
      openaiEndpoint: settings.openaiEndpoint || null,
      openaiApiKey: settings.openaiApiKey || null,
      openaiModel: settings.openaiModel || null,
    });
  } catch (error) {
    logError("翻译失败:", error);
    throw error;
  }
}

export const PROVIDER_OPTIONS: { value: TranslateProvider; label: string; needsConfig: boolean }[] = [
  { value: "microsoft", label: "微软翻译（免费）", needsConfig: false },
  { value: "google_free", label: "谷歌翻译（免费）", needsConfig: false },
  { value: "google_api", label: "谷歌翻译（API）", needsConfig: true },
  { value: "baidu", label: "百度翻译（API）", needsConfig: true },
  { value: "deeplx", label: "DeepLX", needsConfig: true },
  { value: "openai", label: "OpenAI / AI", needsConfig: true },
];
