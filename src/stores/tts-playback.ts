import { create } from "zustand";

export interface TtsPlaybackState {
  /** 当前正在朗读的原文，null 表示未在朗读 */
  sourceText: string | null;
  /** 当前朗读到的字符偏移 */
  charOffset: number;
  /** 当前朗读词的字符长度 */
  charLength: number;
  /** 是否正在播放 */
  isPlaying: boolean;
}

export const useTtsPlayback = create<TtsPlaybackState>(() => ({
  sourceText: null,
  charOffset: 0,
  charLength: 0,
  isPlaying: false,
}));

/** 开始朗读跟踪 */
export function ttsPlaybackStart(text: string) {
  useTtsPlayback.setState({ sourceText: text, charOffset: 0, charLength: 0, isPlaying: true });
}

/** 更新当前朗读位置 */
export function ttsPlaybackUpdate(charOffset: number, charLength: number) {
  useTtsPlayback.setState({ charOffset, charLength });
}

/** 停止朗读跟踪 */
export function ttsPlaybackStop() {
  useTtsPlayback.setState({ sourceText: null, charOffset: 0, charLength: 0, isPlaying: false });
}
