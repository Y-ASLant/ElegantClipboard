// 共享的标签加载逻辑，避免 ClipboardItemCard 和 ClipboardContextMenu 重复实现

import { useTagStore } from "@/stores/tags";

export interface TagLoadResult {
  tags: { id: number; name: string }[];
  itemTagIds: Set<number>;
}

/**
 * 加载标签列表和指定条目的标签分配关系。
 * 如果 tagStore 尚未加载标签，会先 fetchTags。
 */
export async function loadTagsForItem(itemId: number): Promise<TagLoadResult> {
  const tagStore = useTagStore.getState();

  if (tagStore.tags.length === 0) {
    await tagStore.fetchTags();
  }

  const tags = useTagStore.getState().tags;
  const tagList = await tagStore.getItemTags(itemId);
  const itemTagIds = new Set(tagList.map((t) => t.id));

  return { tags, itemTagIds };
}
