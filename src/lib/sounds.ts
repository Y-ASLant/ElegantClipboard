// 轻量级音效反馈（Web Audio 合成音，无需外部音频文件）

import { logError } from "@/lib/logger";
import { useUISettings } from "@/stores/ui-settings";

// 立即创建并预热 AudioContext（WebView2 无 autoplay 限制）
let _ctx: AudioContext | null = null;
let _audioFailureLogged = false;

function logAudioFailureOnce(message: string, error: unknown): void {
  if (_audioFailureLogged) return;
  _audioFailureLogged = true;
  logError(message, error);
}

function getCtx(): AudioContext {
  if (!_ctx) {
    _ctx = new AudioContext();
    // 预热：播放静音音调，激活音频管线
    if (_ctx.state === "suspended") {
      _ctx.resume().catch((error) => {
        logAudioFailureOnce("Failed to resume AudioContext:", error);
      });
    }
    const osc = _ctx.createOscillator();
    const gain = _ctx.createGain();
    gain.gain.value = 0;
    osc.connect(gain);
    gain.connect(_ctx.destination);
    osc.start();
    osc.stop(_ctx.currentTime + 0.01);
  }
  return _ctx;
}

// 模块加载时立即初始化
if (typeof window !== "undefined") getCtx();

function playTone(freq: number, duration: number, volume = 0.15) {
  try {
    const ac = getCtx();
    const osc = ac.createOscillator();
    const gain = ac.createGain();
    osc.type = "sine";
    osc.frequency.value = freq;
    gain.gain.setValueAtTime(volume, ac.currentTime);
    gain.gain.exponentialRampToValueAtTime(0.001, ac.currentTime + duration);
    osc.connect(gain);
    gain.connect(ac.destination);
    osc.start();
    osc.stop(ac.currentTime + duration);
  } catch (error) {
    // 运行环境不支持音频时仅记录一次，避免高频音效路径刷屏。
    logAudioFailureOnce("Audio unavailable, skip sound effect:", error);
  }
}

export function playCopySound(timing: "immediate" | "after_success") {
  const s = useUISettings.getState();
  if (!s.copySound || s.copySoundTiming !== timing) return;
  playTone(880, 0.08);
  setTimeout(() => playTone(1100, 0.08), 60);
}

export function playPasteSound(timing: "immediate" | "after_success") {
  const s = useUISettings.getState();
  if (!s.pasteSound || s.pasteSoundTiming !== timing) return;
  playTone(660, 0.1);
}
