import { invoke } from "@tauri-apps/api/core";
import { useTranslateSettings, type TranslateProvider } from "@/stores/translate-settings";
import { logError } from "@/lib/logger";

// 语言列表
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

// 检测是否为中文（包含至少1个CJK统一表意文字）
function isChinese(text: string): boolean {
  const sample = text.slice(0, 200);
  let cjkCount = 0;
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
    if (/\p{L}/u.test(ch)) totalLetters++;
  }
  return totalLetters > 0 && cjkCount / totalLetters > 0.3;
}

// 根据自动检测确定目标语言
function resolveLanguages(text: string): { from: string; to: string } {
  const settings = useTranslateSettings.getState();
  if (settings.languageMode === "manual" && settings.sourceLanguage && settings.targetLanguage) {
    return { from: settings.sourceLanguage, to: settings.targetLanguage };
  }
  // 自动模式：中文→英文，其他→中文
  if (isChinese(text)) {
    return { from: "zh", to: "en" };
  }
  return { from: "auto", to: "zh" };
}

// 构建代理参数
function getProxyArgs(): { proxyMode: string; proxyUrl: string } {
  const { proxyMode, proxyUrl } = useTranslateSettings.getState();
  return { proxyMode, proxyUrl };
}

// 主翻译函数
export async function translateText(text: string): Promise<string> {
  const settings = useTranslateSettings.getState();
  if (!settings.enabled) throw new Error("翻译功能未启用");

  const { from, to } = resolveLanguages(text);
  const proxy = getProxyArgs();

  try {
    const result = await invoke<string>("translate_text", {
      text,
      from,
      to,
      provider: settings.provider,
      proxyMode: proxy.proxyMode,
      proxyUrl: proxy.proxyUrl,
      deeplxEndpoint: settings.deeplxEndpoint || null,
      googleApiKey: settings.googleApiKey || null,
      baiduAppId: settings.baiduAppId || null,
      baiduSecretKey: settings.baiduSecretKey || null,
      openaiEndpoint: settings.openaiEndpoint || null,
      openaiApiKey: settings.openaiApiKey || null,
      openaiModel: settings.openaiModel || null,
    });
    return result;
  } catch (error) {
    logError("翻译失败:", error);
    throw error;
  }
}

// 提供者显示名称
export const PROVIDER_OPTIONS: { value: TranslateProvider; label: string; needsConfig: boolean }[] = [
  { value: "microsoft", label: "微软翻译（免费）", needsConfig: false },
  { value: "google_free", label: "谷歌翻译（免费）", needsConfig: false },
  { value: "google_api", label: "谷歌翻译（API）", needsConfig: true },
  { value: "baidu", label: "百度翻译（API）", needsConfig: true },
  { value: "deeplx", label: "DeepLX", needsConfig: true },
  { value: "openai", label: "OpenAI / AI", needsConfig: true },
];
