import { memo, useMemo } from "react";
import { useClipboardStore } from "@/stores/clipboard";

interface HighlightTextProps {
  text: string;
}

/**
 * Renders text with search query matches highlighted.
 * Reads searchQuery directly from the clipboard store.
 */
export const HighlightText = memo(function HighlightText({ text }: HighlightTextProps) {
  const searchQuery = useClipboardStore((s) => s.searchQuery);

  const parts = useMemo(() => {
    if (!searchQuery || searchQuery.trim().length === 0) return null;

    // Escape regex special characters
    const escaped = searchQuery.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const regex = new RegExp(`(${escaped})`, "gi");
    return text.split(regex);
  }, [text, searchQuery]);

  if (!parts) return <>{text}</>;

  return (
    <>
      {parts.map((part, i) =>
        i % 2 === 1 ? (
          <mark key={i} className="search-highlight">
            {part}
          </mark>
        ) : (
          part
        ),
      )}
    </>
  );
});
