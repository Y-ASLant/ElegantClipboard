import { describe, it, expect, afterEach } from "vitest";
import type { ClipboardItem } from "@/stores/clipboard";
import { clearMasonryHeightCacheForTest } from "./masonry-height-cache";
import {
  buildMasonrySection,
  distributeToMasonryColumns,
  estimateClipboardItemHeight,
  findPlacedItemByIndex,
  getVisiblePlacedItems,
  masonryColumnWidth,
} from "./masonry-layout";

function makeItem(id: number, content_type: ClipboardItem["content_type"]): ClipboardItem {
  return {
    id,
    content_type,
    text_content: null,
    html_content: null,
    rtf_content: null,
    image_path: null,
    file_paths: null,
    content_hash: `hash-${id}`,
    preview: "preview",
    byte_size: 0,
    image_width: null,
    image_height: null,
    is_pinned: false,
    is_favorite: false,
    favorite_order: 0,
    sort_order: id,
    created_at: "",
    updated_at: "",
    access_count: 0,
    last_accessed_at: null,
    char_count: null,
    source_app_name: null,
    source_app_icon: null,
    group_id: null,
  };
}

describe("masonry-layout", () => {
  afterEach(() => {
    clearMasonryHeightCacheForTest();
  });

  it("distributes items across columns by estimated height", () => {
    const items = [
      makeItem(1, "text"),
      makeItem(2, "image"),
      makeItem(3, "text"),
      makeItem(4, "image"),
    ];
    const estimate = (item: ClipboardItem) =>
      estimateClipboardItemHeight(item, 3, 512, false);

    const columns = distributeToMasonryColumns(items, 0, estimate, 2);
    expect(columns).toHaveLength(2);
    expect(columns.flat()).toHaveLength(4);
    expect(columns.flat().map((entry) => entry.index).sort((a, b) => a - b)).toEqual([0, 1, 2, 3]);
  });

  it("preserves global start index", () => {
    const items = [makeItem(10, "text"), makeItem(11, "files")];
    const columns = distributeToMasonryColumns(items, 5, () => 100, 2);
    expect(columns.flat().map((entry) => entry.index)).toEqual([5, 6]);
  });

  it("buildMasonrySection assigns shortest column in order", () => {
    const items = [makeItem(1, "text"), makeItem(2, "text"), makeItem(3, "text")];
    const layout = buildMasonrySection(items, 0, () => 100, 8, 2);
    expect(layout.placed.map((entry) => entry.column)).toEqual([0, 1, 0]);
    expect(layout.totalHeight).toBe(208);
  });

  it("getVisiblePlacedItems filters by scroll window", () => {
    const layout = buildMasonrySection(
      [makeItem(1, "text"), makeItem(2, "text"), makeItem(3, "text")],
      0,
      (item) => (item.id === 2 ? 200 : 100),
      0,
      1,
    );
    const visible = getVisiblePlacedItems(layout.placed, 90, 120, 0);
    expect(visible.map((entry) => entry.index)).toEqual([0, 1]);
  });

  it("findPlacedItemByIndex resolves pinned and main sections", () => {
    const pinned = buildMasonrySection([makeItem(1, "text")], 0, () => 80, 0, 2);
    const main = buildMasonrySection([makeItem(2, "text")], 1, () => 120, 0, 2);
    const pinnedBlockHeight = pinned.totalHeight + 17;

    expect(findPlacedItemByIndex(pinned, main, pinnedBlockHeight, 0)?.absoluteTop).toBe(0);
    expect(findPlacedItemByIndex(pinned, main, pinnedBlockHeight, 1)?.absoluteTop).toBe(
      pinnedBlockHeight,
    );
  });

  it("masonryColumnWidth subtracts horizontal inset and column gap", () => {
    expect(masonryColumnWidth(400)).toBe(188);
  });
});
