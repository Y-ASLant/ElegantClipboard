// ============ Text Preview Utilities ============

export const TEXT_PREVIEW_CACHE_MAX_ITEMS = 180;
export const TEXT_PREVIEW_MIN_W = 360;
export const TEXT_PREVIEW_MAX_W = 900;
export const TEXT_PREVIEW_MIN_H = 130;
export const TEXT_PREVIEW_MAX_H = 560;
export const TEXT_PREVIEW_CHAR_WIDTH = 7.6;
export const TEXT_PREVIEW_HORIZONTAL_PADDING = 44;
export const TEXT_PREVIEW_MIN_CHARS_PER_LINE = 24;
export const TEXT_PREVIEW_SAMPLE_MAX_CHARS = 24_000;
export const TEXT_PREVIEW_SAMPLE_MAX_LINES = 900;

export interface ClipboardItemDetail {
  id: number;
  text_content: string | null;
  preview: string | null;
}

export interface TextPreviewSample {
  longestVisualCols: number;
  lineColumns: number[];
  processedCodeUnits: number;
  truncated: boolean;
}

function isWideCodePoint(codePoint: number): boolean {
  return (
    (codePoint >= 0x2E80 && codePoint <= 0xA4CF)
    || (codePoint >= 0xAC00 && codePoint <= 0xD7A3)
    || (codePoint >= 0xF900 && codePoint <= 0xFAFF)
    || (codePoint >= 0xFE10 && codePoint <= 0xFE6F)
    || (codePoint >= 0xFF00 && codePoint <= 0xFFEF)
  );
}

export function sampleTextPreview(text: string): TextPreviewSample {
  const lineColumns: number[] = [];
  let longestVisualCols = 1;
  let currentLineCols = 0;
  let processedCodeUnits = 0;
  let lineCount = 1;
  let truncated = false;
  let endsWithLineBreak = false;

  const finalizeLine = () => {
    const cols = Math.max(1, currentLineCols);
    lineColumns.push(cols);
    longestVisualCols = Math.max(longestVisualCols, cols);
    currentLineCols = 0;
  };

  for (let i = 0; i < text.length; i += 1) {
    const code = text.charCodeAt(i);
    processedCodeUnits += 1;

    if (code === 0x0D || code === 0x0A) {
      if (code === 0x0D && i + 1 < text.length && text.charCodeAt(i + 1) === 0x0A) {
        i += 1;
        processedCodeUnits += 1;
      }
      finalizeLine();
      lineCount += 1;
      endsWithLineBreak = true;
    } else {
      endsWithLineBreak = false;
      const codePoint = text.codePointAt(i);
      if (codePoint !== undefined) {
        currentLineCols += isWideCodePoint(codePoint) ? 2 : 1;
        if (codePoint > 0xFFFF) {
          i += 1;
          processedCodeUnits += 1;
        }
      }
    }

    if (processedCodeUnits >= TEXT_PREVIEW_SAMPLE_MAX_CHARS || lineCount > TEXT_PREVIEW_SAMPLE_MAX_LINES) {
      truncated = true;
      break;
    }
  }

  if (currentLineCols > 0 || lineColumns.length === 0 || endsWithLineBreak) {
    finalizeLine();
  }

  return {
    longestVisualCols,
    lineColumns,
    processedCodeUnits,
    truncated,
  };
}

// ============ Text Preview Content Cache (LRU) ============

const textPreviewContentCache = new Map<number, string>();

export function getCachedTextPreviewContent(id: number): string | undefined {
  const cached = textPreviewContentCache.get(id);
  if (cached === undefined) {
    return undefined;
  }
  textPreviewContentCache.delete(id);
  textPreviewContentCache.set(id, cached);
  return cached;
}

export function setCachedTextPreviewContent(id: number, text: string): void {
  textPreviewContentCache.set(id, text);
  if (textPreviewContentCache.size <= TEXT_PREVIEW_CACHE_MAX_ITEMS) {
    return;
  }
  const oldestKey = textPreviewContentCache.keys().next().value;
  if (oldestKey !== undefined) {
    textPreviewContentCache.delete(oldestKey);
  }
}
