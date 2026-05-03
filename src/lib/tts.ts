/**
 * 文本朗读（TTS）模块
 * 支持两种引擎：Edge TTS（微软在线）、浏览器内置
 */

import { invoke } from "@tauri-apps/api/core";
import { useTtsSettings, type TtsEngine } from "@/stores/tts-settings";
import { ttsPlaybackStart, ttsPlaybackUpdate, ttsPlaybackStop } from "@/stores/tts-playback";
import { logError } from "@/lib/logger";

/** Edge TTS 后端返回结构 */
interface TtsEdgeResult {
  audio: string;
  boundaries: { offset_ms: number; duration_ms: number; text: string }[];
}

/** 映射后的词边界（含字符偏移） */
interface MappedBoundary {
  offset_ms: number;
  duration_ms: number;
  charOffset: number;
  charLength: number;
}

export type TtsAccent = "en-US" | "en-GB";

/** 当前正在播放的 Audio 对象 */
let currentAudio: HTMLAudioElement | null = null;
let currentSpeaking = false;
let trackingRafId: number | null = null;

/** 是否正在朗读 */
export function isSpeaking(): boolean {
  return currentSpeaking || speechSynthesis.speaking;
}

/** 停止当前朗读 */
export function stopSpeaking(): void {
  if (trackingRafId) {
    cancelAnimationFrame(trackingRafId);
    trackingRafId = null;
  }
  if (currentAudio) {
    currentAudio.pause();
    currentAudio.src = "";
    currentAudio = null;
  }
  speechSynthesis.cancel();
  currentSpeaking = false;
  ttsPlaybackStop();
}

/** 将后端返回的词边界映射到原文字符偏移 */
function mapBoundariesToText(
  text: string,
  boundaries: TtsEdgeResult["boundaries"],
): MappedBoundary[] {
  const result: MappedBoundary[] = [];
  let searchStart = 0;
  for (const b of boundaries) {
    const idx = text.indexOf(b.text, searchStart);
    if (idx >= 0) {
      result.push({
        offset_ms: b.offset_ms,
        duration_ms: b.duration_ms,
        charOffset: idx,
        charLength: b.text.length,
      });
      searchStart = idx + b.text.length;
    }
  }
  return result;
}

/** 播放 base64 编码的 MP3 音频并跟踪朗读位置 */
function playBase64AudioWithTracking(
  sourceText: string,
  base64: string,
  boundaries: MappedBoundary[],
  onEnd?: () => void,
  onError?: (error: string) => void,
): void {
  const audio = new Audio(`data:audio/mp3;base64,${base64}`);
  currentAudio = audio;
  currentSpeaking = true;
  ttsPlaybackStart(sourceText);

  const cleanup = () => {
    if (trackingRafId) { cancelAnimationFrame(trackingRafId); trackingRafId = null; }
    currentAudio = null;
    currentSpeaking = false;
    ttsPlaybackStop();
  };

  const trackPosition = () => {
    if (!currentAudio || currentAudio !== audio) return;
    const currentMs = audio.currentTime * 1000;
    for (let i = boundaries.length - 1; i >= 0; i--) {
      if (currentMs >= boundaries[i].offset_ms) {
        ttsPlaybackUpdate(boundaries[i].charOffset, boundaries[i].charLength);
        break;
      }
    }
    trackingRafId = requestAnimationFrame(trackPosition);
  };

  audio.onplay = () => {
    if (boundaries.length > 0) {
      trackingRafId = requestAnimationFrame(trackPosition);
    }
  };
  audio.onended = () => { cleanup(); onEnd?.(); };
  audio.onerror = () => { cleanup(); onError?.("音频播放失败"); };

  audio.play().catch((e) => { cleanup(); onError?.(String(e)); });
}

/** 检测文本是否包含中文（CJK 统一汉字） */
function isChinese(text: string): boolean {
  return /[\u4e00-\u9fff\u3400-\u4dbf]/.test(text);
}

/** 根据设置和文本语言解析应使用的 Edge 声源 */
function resolveEdgeVoice(text: string): string {
  const { edgeVoice, edgeVoiceEn, edgeVoiceZh, splitVoice } = useTtsSettings.getState();
  if (splitVoice) {
    return isChinese(text) ? edgeVoiceZh : edgeVoiceEn;
  }
  return edgeVoice;
}

/** 根据设置和文本语言解析应使用的 Edge 语速 */
function resolveEdgeRate(text: string): string {
  const { edgeRate, edgeRateEn, edgeRateZh, splitVoice } = useTtsSettings.getState();
  if (splitVoice) {
    return isChinese(text) ? edgeRateZh : edgeRateEn;
  }
  return edgeRate;
}

/** 使用 Edge TTS 引擎朗读 */
async function speakEdge(
  text: string,
  options?: { onEnd?: () => void; onError?: (error: string) => void },
): Promise<void> {
  const { proxyMode, proxyUrl } = useTtsSettings.getState();
  const voice = resolveEdgeVoice(text);
  const rate = resolveEdgeRate(text);

  try {
    currentSpeaking = true;
    const result = await invoke<TtsEdgeResult>("tts_speak_edge", {
      text,
      voice,
      rate,
      proxyMode,
      proxyUrl,
    });
    const mapped = mapBoundariesToText(text, result.boundaries);
    playBase64AudioWithTracking(text, result.audio, mapped, options?.onEnd, options?.onError);
  } catch (error) {
    currentSpeaking = false;
    logError("Edge TTS 失败:", error);
    options?.onError?.(String(error));
  }
}

/** 使用浏览器内置 SpeechSynthesis 朗读 */
function speakBrowser(
  text: string,
  options?: { onEnd?: () => void; onError?: (error: string) => void },
): void {
  const { browserAccent } = useTtsSettings.getState();

  const utterance = new SpeechSynthesisUtterance(text);
  const voices = speechSynthesis.getVoices().filter((v) => v.lang.startsWith("en-"));
  const voice = voices.find((v) => v.lang === browserAccent)
    ?? voices.find((v) => v.lang.startsWith(browserAccent))
    ?? voices[0];
  if (voice) {
    utterance.voice = voice;
    utterance.lang = voice.lang;
  } else {
    utterance.lang = browserAccent;
  }

  currentSpeaking = true;
  ttsPlaybackStart(text);

  utterance.addEventListener("boundary", (e: SpeechSynthesisEvent) => {
    if (e.name === "word") {
      const len = (e as any).charLength || 1;
      ttsPlaybackUpdate(e.charIndex, len);
    }
  });
  utterance.onend = () => {
    currentSpeaking = false;
    ttsPlaybackStop();
    options?.onEnd?.();
  };
  utterance.onerror = (e) => {
    currentSpeaking = false;
    ttsPlaybackStop();
    options?.onError?.(e.error);
  };
  speechSynthesis.speak(utterance);
}

/** 统一朗读入口 —— 根据设置选择引擎 */
export function speak(
  text: string,
  _accent: TtsAccent,
  options?: {
    rate?: number;
    pitch?: number;
    onEnd?: () => void;
    onError?: (error: string) => void;
  },
): void {
  stopSpeaking();
  if (!text.trim()) return;

  const { engine } = useTtsSettings.getState();
  switch (engine) {
    case "edge":
      speakEdge(text, options);
      break;
    case "browser":
      speakBrowser(text, options);
      break;
    default:
      speakEdge(text, options);
  }
}

/** 直接用指定引擎朗读 */
export function speakWithEngine(
  text: string,
  engine: TtsEngine,
  options?: { onEnd?: () => void; onError?: (error: string) => void },
): void {
  stopSpeaking();
  if (!text.trim()) return;

  switch (engine) {
    case "edge":
      speakEdge(text, options);
      break;
    case "browser":
      speakBrowser(text, options);
      break;
  }
}

/** 获取可用口音选项（仅浏览器引擎使用） */
export const ACCENT_OPTIONS: { value: TtsAccent; label: string }[] = [
  { value: "en-US", label: "美式英语" },
  { value: "en-GB", label: "英式英语" },
];
