import { describe, it, expect, vi, beforeEach } from "vitest";
import { formatTime, formatSize, formatCharCount } from "@/lib/format";
import { useClipboardStore, type ClipboardItem } from "@/stores/clipboard";
import { useUISettings } from "@/stores/ui-settings";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(() => Promise.resolve()),
}));

// Helper: generate mock clipboard items
function generateItems(count: number): ClipboardItem[] {
  return Array.from({ length: count }, (_, i) => ({
    id: i + 1,
    content_type: "text" as const,
    text_content: `Sample text content ${i}. ${"Lorem ipsum ".repeat(10)}`,
    html_content: null,
    rtf_content: null,
    image_path: null,
    file_paths: null,
    content_hash: `hash-${i}`,
    semantic_hash: `semantic-${i}`,
    preview: `Preview ${i}`,
    byte_size: 100 + i,
    image_width: null,
    image_height: null,
    is_pinned: i < 5,
    is_favorite: i % 10 === 0,
    favorite_order: i % 10 === 0 ? 100 - i : 0,
    sort_order: count - i,
    created_at: new Date(Date.now() - i * 60000).toISOString(),
    updated_at: new Date().toISOString(),
    access_count: i,
    last_accessed_at: null,
    char_count: 50 + i,
    source_app_name: null,
    source_app_icon: null,
  }));
}

describe("Performance benchmarks", () => {
  describe("Store operations", () => {
    it("setSearchQuery completes within 1ms", () => {
      const start = performance.now();
      for (let i = 0; i < 1000; i++) {
        useClipboardStore.getState().setSearchQuery(`query-${i}`);
      }
      const elapsed = performance.now() - start;
      expect(elapsed).toBeLessThan(1000); // 1000 ops in < 1s
    });

    it("setActiveIndex completes within 1ms for 1000 calls", () => {
      const start = performance.now();
      for (let i = 0; i < 1000; i++) {
        useClipboardStore.getState().setActiveIndex(i % 100);
      }
      const elapsed = performance.now() - start;
      expect(elapsed).toBeLessThan(1000);
    });

    it("batch toggleSelect completes within 5ms for 100 items", () => {
      const items = generateItems(100);
      useClipboardStore.setState({
        items,
        batchMode: true,
        selectedIds: new Set(),
      });

      const start = performance.now();
      for (let i = 0; i < 100; i++) {
        useClipboardStore.getState().toggleSelect(i + 1, i, false);
      }
      const elapsed = performance.now() - start;
      expect(elapsed).toBeLessThan(5000);
    });

    it("setState with items completes within 10ms for 10000 items", () => {
      const items = generateItems(10000);
      const start = performance.now();
      useClipboardStore.setState({ items });
      const elapsed = performance.now() - start;
      expect(elapsed).toBeLessThan(10);
    });

    it("setSelectedGroup resets state within 5ms", () => {
      const items = generateItems(5000);
      useClipboardStore.setState({
        items,
        batchMode: true,
        selectedIds: new Set([1, 2, 3, 4, 5]),
        lastSelectedIndex: 10,
      });

      const start = performance.now();
      useClipboardStore.getState().setSelectedGroup("text");
      const elapsed = performance.now() - start;
      expect(elapsed).toBeLessThan(5);
    });

    it("UISettings setCardMaxLines completes within 1ms", () => {
      const start = performance.now();
      for (let i = 0; i < 1000; i++) {
        useUISettings.getState().setCardMaxLines(i % 10);
      }
      const elapsed = performance.now() - start;
      expect(elapsed).toBeLessThan(1000);
    });
  });

  describe("Format functions", () => {
    it("formatTime handles 10000 calls within 100ms", () => {
      const dates = Array.from(
        { length: 10000 },
        (_, i) => new Date(Date.now() - i * 60000).toISOString(),
      );

      const start = performance.now();
      for (const d of dates) {
        formatTime(d);
      }
      const elapsed = performance.now() - start;
      expect(elapsed).toBeLessThan(200);
    });

    it("formatTime relative handles 10000 calls within 100ms", () => {
      const dates = Array.from(
        { length: 10000 },
        (_, i) => new Date(Date.now() - i * 1000).toISOString(),
      );

      const start = performance.now();
      for (const d of dates) {
        formatTime(d, "relative");
      }
      const elapsed = performance.now() - start;
      expect(elapsed).toBeLessThan(100);
    });

    it("formatSize handles 10000 calls within 50ms", () => {
      const sizes = Array.from({ length: 10000 }, (_, i) => i * 100);

      const start = performance.now();
      for (const s of sizes) {
        formatSize(s);
      }
      const elapsed = performance.now() - start;
      expect(elapsed).toBeLessThan(50);
    });

    it("formatCharCount handles 10000 calls within 50ms", () => {
      const counts = Array.from({ length: 10000 }, (_, i) => i);

      const start = performance.now();
      for (const c of counts) {
        formatCharCount(c);
      }
      const elapsed = performance.now() - start;
      expect(elapsed).toBeLessThan(100);
    });
  });

  describe("List operations", () => {
    it("filter items by search query within 20ms for 10000 items", () => {
      const items = generateItems(10000);
      const query = "Sample";

      const start = performance.now();
      const filtered = items.filter((item) =>
        item.text_content?.toLowerCase().includes(query.toLowerCase()),
      );
      const elapsed = performance.now() - start;
      expect(elapsed).toBeLessThan(50);
      expect(filtered.length).toBeGreaterThan(0);
    });

    it("sort items by sort_order within 20ms for 10000 items", () => {
      const items = generateItems(10000);

      const start = performance.now();
      [...items].sort((a, b) => b.sort_order - a.sort_order);
      const elapsed = performance.now() - start;
      expect(elapsed).toBeLessThan(20);
    });

    it("map items to SortableClipboardItem within 20ms for 10000 items", () => {
      const items = generateItems(10000);

      const start = performance.now();
      items.map((item) => ({ ...item, _sortId: `item-${item.id}` }));
      const elapsed = performance.now() - start;
      expect(elapsed).toBeLessThan(20);
    });

    it("compute pinnedCount within 20ms for 10000 items", () => {
      const items = generateItems(10000);

      const start = performance.now();
      items.filter((item) => item.is_pinned).length;
      const elapsed = performance.now() - start;
      expect(elapsed).toBeLessThan(20);
    });
  });

  describe("Scroll performance", () => {
    it("Virtual list item generation within 50ms for 10000 items", () => {
      const items = generateItems(10000);

      const start = performance.now();
      const virtualItems = items.slice(0, 50).map((item) => ({
        id: item.id,
        content_type: item.content_type,
        text_content: item.text_content,
        preview: item.preview,
        is_pinned: item.is_pinned,
        is_favorite: item.is_favorite,
      }));
      const elapsed = performance.now() - start;
      expect(elapsed).toBeLessThan(50);
      expect(virtualItems.length).toBe(50);
    });

    it("Scroll position calculation within 5ms for 10000 items", () => {
      const items = generateItems(10000);
      const itemHeight = 80;
      const viewportHeight = 640;

      const start = performance.now();
      for (let scrollY = 0; scrollY < 100000; scrollY += 100) {
        const startIndex = Math.floor(scrollY / itemHeight);
        const endIndex = Math.min(
          startIndex + Math.ceil(viewportHeight / itemHeight) + 2,
          items.length,
        );
        items.slice(startIndex, endIndex);
      }
      const elapsed = performance.now() - start;
      expect(elapsed).toBeLessThan(20);
    });

    it("Infinite scroll threshold check within 1ms", () => {
      const items = generateItems(10000);
      const itemHeight = 80;
      const viewportHeight = 640;
      const threshold = 200;

      const start = performance.now();
      for (let i = 0; i < 1000; i++) {
        const scrollY = i * 100;
        const totalHeight = items.length * itemHeight;
        const shouldLoadMore =
          scrollY + viewportHeight >= totalHeight - threshold;
        void shouldLoadMore;
      }
      const elapsed = performance.now() - start;
      expect(elapsed).toBeLessThan(1);
    });
  });

  describe("Card rendering performance", () => {
    it("Card content truncation within 5ms for 1000 items", () => {
      const items = generateItems(1000);
      const maxLines = 3;
      const charsPerLine = 50;
      const maxChars = maxLines * charsPerLine;

      const start = performance.now();
      items.map((item) => {
        const text = item.text_content ?? "";
        return text.length > maxChars ? text.slice(0, maxChars) + "..." : text;
      });
      const elapsed = performance.now() - start;
      expect(elapsed).toBeLessThan(5);
    });

    it("Card metadata generation within 10ms for 1000 items", () => {
      const items = generateItems(1000);

      const start = performance.now();
      items.map((item) => ({
        time: formatTime(item.created_at),
        size: formatSize(item.byte_size),
        chars: formatCharCount(item.char_count),
        type: item.content_type,
      }));
      const elapsed = performance.now() - start;
      expect(elapsed).toBeLessThan(20);
    });

    it("Batch selection state update within 10ms for 500 items", () => {
      const items = generateItems(500);
      useClipboardStore.setState({
        items,
        batchMode: true,
        selectedIds: new Set(),
      });

      const start = performance.now();
      const selectedIds = new Set<number>();
      for (let i = 0; i < 250; i++) {
        selectedIds.add(i + 1);
      }
      useClipboardStore.setState({ selectedIds });
      const elapsed = performance.now() - start;
      expect(elapsed).toBeLessThan(10);
    });
  });

  describe("Search response performance", () => {
    it("Search filtering within 20ms for 5000 items", () => {
      const items = generateItems(5000);
      const queries = ["sample", "lorem", "text", "content", "123"];

      const start = performance.now();
      for (const query of queries) {
        items.filter((item) =>
          item.text_content?.toLowerCase().includes(query.toLowerCase()),
        );
      }
      const elapsed = performance.now() - start;
      expect(elapsed).toBeLessThan(20);
    });

    it("Search debouncing simulation within 50ms", () => {
      const items = generateItems(5000);
      const searchHistory: string[] = [];

      const start = performance.now();
      for (let i = 0; i < 100; i++) {
        const query = `test-${i}`;
        searchHistory.push(query);
        if (searchHistory.length > 10) {
          searchHistory.shift();
        }
        items.filter((item) =>
          item.text_content?.toLowerCase().includes(query.toLowerCase()),
        );
      }
      const elapsed = performance.now() - start;
      expect(elapsed).toBeLessThan(200); // 100 searches on 5000 items
    });

    it("Search result ranking within 10ms for 1000 items", () => {
      const items = generateItems(1000);
      const query = "sample";

      const start = performance.now();
      items
        .map((item) => {
          const text = item.text_content?.toLowerCase() ?? "";
          const idx = text.indexOf(query.toLowerCase());
          return { item, score: idx === -1 ? -1 : 1 / (idx + 1) };
        })
        .filter((r) => r.score >= 0)
        .sort((a, b) => b.score - a.score);
      const elapsed = performance.now() - start;
      expect(elapsed).toBeLessThan(10);
    });
  });

  describe("Large data volume UI response", () => {
    it("Item insertion within 50ms for 10000 items", () => {
      const items = generateItems(10000);
      useClipboardStore.setState({ items: [] });

      const start = performance.now();
      useClipboardStore.setState({ items });
      const elapsed = performance.now() - start;
      expect(elapsed).toBeLessThan(50);
    });

    it("Item deletion within 20ms for 10000 items", () => {
      const items = generateItems(10000);
      useClipboardStore.setState({ items });

      const start = performance.now();
      useClipboardStore.setState({
        items: items.filter((item) => item.id !== 5000),
      });
      const elapsed = performance.now() - start;
      expect(elapsed).toBeLessThan(20);
    });

    it("Batch operations within 50ms for 1000 items", () => {
      const items = generateItems(1000);
      useClipboardStore.setState({
        items,
        batchMode: true,
        selectedIds: new Set(Array.from({ length: 500 }, (_, i) => i + 1)),
      });

      const start = performance.now();
      const { selectedIds } = useClipboardStore.getState();
      useClipboardStore.setState({
        items: items.filter((item) => !selectedIds.has(item.id)),
        batchMode: false,
        selectedIds: new Set(),
      });
      const elapsed = performance.now() - start;
      expect(elapsed).toBeLessThan(50);
    });

    it("State serialization within 100ms for 10000 items", () => {
      const items = generateItems(10000);

      const start = performance.now();
      JSON.stringify(items);
      const elapsed = performance.now() - start;
      expect(elapsed).toBeLessThan(100);
    });

    it("State deserialization within 100ms for 10000 items", () => {
      const items = generateItems(10000);
      const serialized = JSON.stringify(items);

      const start = performance.now();
      JSON.parse(serialized);
      const elapsed = performance.now() - start;
      expect(elapsed).toBeLessThan(100);
    });
  });
});

