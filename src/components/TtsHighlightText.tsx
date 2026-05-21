import { memo, useRef } from "react";
import { useTtsPlayback } from "@/stores/tts-playback";
import { useTtsSettings } from "@/stores/tts-settings";

interface TtsHighlightTextProps {
  text: string;
  /** TTS 未激活时的回退渲染（如搜索高亮组件），不传则渲染纯文本 */
  fallback?: React.ReactNode;
}

interface TtsState {
  isActive: boolean;
  charOffset: number;
  charLength: number;
}

// 稳定的非活跃状态引用 — 避免每次 selector 创建新对象
const INACTIVE_STATE: TtsState = { isActive: false, charOffset: 0, charLength: 0 };

/**
 * 朗读时逐词高亮的文本组件。
 * 当 tts-playback store 中的 sourceText 与 props.text 一致时，
 * 按当前 charOffset/charLength 将正在朗读的词高亮显示。
 * 受 TTS 设置中 highlightWord 开关控制。
 *
 * 优化：使用稳定引用 + 手动比较避免所有卡片在每次播放进度更新时重渲染。
 * 非活跃卡片始终返回 INACTIVE_STATE 引用，zustand 的引用比较自动跳过更新。
 */
export const TtsHighlightText = memo(function TtsHighlightText({ text, fallback }: TtsHighlightTextProps) {
  const highlightEnabled = useTtsSettings((s) => s.highlightWord);

  // 使用 ref 缓存上次结果，仅当实际值变化时返回新对象
  const prevRef = useRef<TtsState>(INACTIVE_STATE);

  const state = useTtsPlayback((s): TtsState => {
    // 快速短路：不在播放时立即返回稳定值
    if (!s.isPlaying) return INACTIVE_STATE;

    const src = s.sourceText ?? "";
    // 长度守卫：如果长度差距过大，不可能是同一段文本
    const lenDiff = Math.abs(src.length - text.length);
    const maxLen = Math.max(src.length, text.length);
    if (lenDiff > 0 && lenDiff > maxLen * 0.5) return INACTIVE_STATE;

    const active = src === text || src.startsWith(text) || text.startsWith(src);
    if (!active) return INACTIVE_STATE;

    const next: TtsState = { isActive: true, charOffset: s.charOffset, charLength: s.charLength };
    // 手动浅比较，命中则返回旧引用避免重渲染
    const prev = prevRef.current;
    if (prev.isActive === next.isActive && prev.charOffset === next.charOffset && prev.charLength === next.charLength) {
      return prev;
    }
    prevRef.current = next;
    return next;
  });

  const { isActive, charOffset, charLength } = state;

  if (!highlightEnabled || !isActive || charLength === 0) {
    return <>{fallback ?? text}</>;
  }

  const before = text.slice(0, charOffset);
  const highlighted = text.slice(charOffset, charOffset + charLength);
  const after = text.slice(charOffset + charLength);

  return (
    <>
      {before}
      <mark className="tts-highlight">{highlighted}</mark>
      {after}
    </>
  );
});
