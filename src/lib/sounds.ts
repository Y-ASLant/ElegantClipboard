// Lightweight sound feedback using synthesized Web Audio tones
// No external audio files needed

import { useUISettings } from "@/stores/ui-settings";

let _ctx: AudioContext | null = null;
function ctx(): AudioContext {
  if (!_ctx) _ctx = new AudioContext();
  if (_ctx.state === "suspended") _ctx.resume();
  return _ctx;
}

function playTone(freq: number, duration: number, volume = 0.15) {
  try {
    const ac = ctx();
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
  } catch { /* audio not available */ }
}

export function playCopySound() {
  if (!useUISettings.getState().copySound) return;
  playTone(880, 0.08);
  setTimeout(() => playTone(1100, 0.08), 60);
}

export function playPasteSound() {
  if (!useUISettings.getState().pasteSound) return;
  playTone(660, 0.1);
}
