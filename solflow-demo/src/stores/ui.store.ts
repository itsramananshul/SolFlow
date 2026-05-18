import { defineStore } from 'pinia';
import { ref } from 'vue';

export type SidebarTab = 'palette' | 'types' | 'imports';

export const useUIStore = defineStore('ui', () => {
  const sidebarTab = ref<SidebarTab>('palette');
  const drawerOpen = ref(false);
  const selectedNodeId = ref<string | null>(null);

  function selectNode(id: string | null) {
    selectedNodeId.value = id;
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
    selectNode,
    toggleDrawer,
    setSidebarTab,
  };
});
