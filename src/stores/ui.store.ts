import { defineStore } from 'pinia';
import { ref } from 'vue';

export type SidebarTab = 'palette' | 'types' | 'imports' | 'policies';

export const useUIStore = defineStore('ui', () => {
  const sidebarTab = ref<SidebarTab>('palette');
  const drawerOpen = ref(false);
  const selectedNodeId = ref<string | null>(null);
  // Hovered node id — used by Canvas to highlight connected edges.
  const hoveredNodeId = ref<string | null>(null);

  /**
   * One-shot focus request. Any component (diagnostics, search palette,
   * outline) can ask the Canvas to pan-and-zoom to a specific node by
   * calling `requestFocus(nodeId)`. The Canvas watches `focusRequest`
   * and performs the actual setCenter + selection, then clears it.
   *
   * `bumpId` exists so a back-to-back request for the SAME node still
   * fires — without it, the watch wouldn't trigger when the value
   * doesn't change.
   */
  const focusRequest = ref<{ nodeId: string; bumpId: number } | null>(null);
  let bump = 0;
  function requestFocus(nodeId: string) {
    bump += 1;
    focusRequest.value = { nodeId, bumpId: bump };
  }
  function clearFocusRequest() {
    focusRequest.value = null;
  }

  function selectNode(id: string | null) {
    selectedNodeId.value = id;
  }

  function setHovered(id: string | null) {
    hoveredNodeId.value = id;
  }

  function toggleDrawer() {
    drawerOpen.value = !drawerOpen.value;
  }

  function setSidebarTab(tab: SidebarTab) {
    sidebarTab.value = tab;
  }

  return {
    sidebarTab,
    drawerOpen,
    selectedNodeId,
    hoveredNodeId,
    focusRequest,
    selectNode,
    setHovered,
    toggleDrawer,
    setSidebarTab,
    requestFocus,
    clearFocusRequest,
  };
});
