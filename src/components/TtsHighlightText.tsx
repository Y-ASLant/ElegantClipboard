import { memo } from "react";
import { useShallow } from "zustand/react/shallow";
import { useTtsPlayback } from "@/stores/tts-playback";
import { useTtsSettings } from "@/stores/tts-settings";

interface TtsHighlightTextProps {
  text: string;
}

/**
 * 朗读时逐词高亮的文本组件。
 * 当 tts-playback store 中的 sourceText 与 props.text 一致时，
 * 按当前 charOffset/charLength 将正在朗读的词高亮显示。
 * 受 TTS 设置中 highlightWord 开关控制。
 */
export const TtsHighlightText = memo(function TtsHighlightText({ text }: TtsHighlightTextProps) {
  const highlightEnabled = useTtsSettings((s) => s.highlightWord);
  const { isActive, charOffset, charLength } = useTtsPlayback(
    useShallow((s) => {
      const active = s.isPlaying && s.sourceText === text;
      return {
        isActive: active,
        charOffset: active ? s.charOffset : 0,
        charLength: active ? s.charLength : 0,
      };
    }),
  );

  if (!highlightEnabled || !isActive || charLength === 0) {
    return <>{text}</>;
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
