import { defineStore } from 'pinia';
import { ref } from 'vue';

export type SidebarTab =
  | 'palette'
  | 'outline'
  | 'blocks'
  | 'types'
  | 'imports'
  | 'policies';

export const useUIStore = defineStore('ui', () => {
  const sidebarTab = ref<SidebarTab>('palette');
  const drawerOpen = ref(false);
  const runOpen = ref(false);
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

  /**
   * A runtime error pinned to a source line, for the SOL source preview to
   * highlight (gutter marker + line decoration) and scroll to. `line` is
   * 1-based; `message` is the friendly explanation shown on hover. Cleared
   * when a run succeeds or the source changes.
   */
  const sourceError = ref<{ line: number; message: string; bumpId: number } | null>(null);
  let errBump = 0;
  function setSourceError(line: number, message: string) {
    errBump += 1;
    sourceError.value = { line, message, bumpId: errBump };
  }
  function clearSourceError() {
    sourceError.value = null;
  }

  /** Neutral request to scroll the SOL preview to a line (e.g. clicking a
   *  trace row), without the red error decoration. */
  const sourceFocus = ref<{ line: number; bumpId: number } | null>(null);
  let focusBump = 0;
  function focusSourceLine(line: number) {
    focusBump += 1;
    sourceFocus.value = { line, bumpId: focusBump };
  }

  function selectNode(id: string | null) {
    selectedNodeId.value = id;
  }

  function setHovered(id: string | null) {
    hoveredNodeId.value = id;
  }

  function setRunOpen(v: boolean) {
    runOpen.value = v;
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
    runOpen,
    setRunOpen,
    selectedNodeId,
    hoveredNodeId,
    focusRequest,
    selectNode,
    setHovered,
    toggleDrawer,
    setSidebarTab,
    requestFocus,
    clearFocusRequest,
    sourceError,
    setSourceError,
    clearSourceError,
    sourceFocus,
    focusSourceLine,
  };
});
