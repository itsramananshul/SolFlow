/**
 * User-saved reusable blocks. Persists to localStorage; built-in
 * patterns live in src/graph/blocks.ts and are surfaced alongside
 * user blocks by the BlocksPanel.
 */

import { defineStore } from 'pinia';
import { computed, ref, watch } from 'vue';
import { nanoid } from 'nanoid';

import type { GraphEdge, GraphNode } from '@/graph/schema';
import type { SavedBlock } from '@/graph/blocks';

const STORAGE_KEY = 'solflow.blocks';

export const useBlocksStore = defineStore('blocks', () => {
  const userBlocks = ref<SavedBlock[]>([]);

  function bootstrap() {
    if (typeof localStorage === 'undefined') return;
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return;
    try {
      const parsed = JSON.parse(raw) as SavedBlock[];
      if (Array.isArray(parsed)) {
        userBlocks.value = parsed;
      }
    } catch {
      /* ignore corrupt storage */
    }
  }

  // Auto-persist on any change.
  watch(
    userBlocks,
    (next) => {
      if (typeof localStorage === 'undefined') return;
      try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
      } catch {
        /* quota / unavailable */
      }
    },
    { deep: true },
  );

  /**
   * Save a deep-cloned snapshot of the provided nodes + edges as a
   * new user block. Returns the saved block.
   */
  function save(
    name: string,
    description: string,
    nodes: GraphNode[],
    edges: GraphEdge[],
  ): SavedBlock {
    const cx = nodes.length > 0
      ? nodes.reduce((s, n) => s + n.position.x, 0) / nodes.length
      : 0;
    const cy = nodes.length > 0
      ? nodes.reduce((s, n) => s + n.position.y, 0) / nodes.length
      : 0;
    const block: SavedBlock = {
      id: nanoid(8),
      name: name.trim() || 'Untitled block',
      description: description.trim(),
      origin: 'user',
      nodes: JSON.parse(JSON.stringify(nodes)) as GraphNode[],
      edges: JSON.parse(JSON.stringify(edges)) as GraphEdge[],
      centroid: { x: cx, y: cy },
      createdAt: new Date().toISOString(),
    };
    userBlocks.value = [block, ...userBlocks.value];
    return block;
  }

  function rename(blockId: string, newName: string) {
    const b = userBlocks.value.find((x) => x.id === blockId);
    if (!b) return;
    b.name = newName.trim() || b.name;
  }

  function remove(blockId: string) {
    userBlocks.value = userBlocks.value.filter((b) => b.id !== blockId);
  }

  function findById(blockId: string): SavedBlock | undefined {
    return userBlocks.value.find((b) => b.id === blockId);
  }

  const count = computed(() => userBlocks.value.length);

  return {
    userBlocks,
    count,
    bootstrap,
    save,
    rename,
    remove,
    findById,
  };
});
