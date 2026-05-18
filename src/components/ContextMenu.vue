<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue';

export interface ContextMenuItem {
  label: string;
  shortcut?: string;
  danger?: boolean;
  disabled?: boolean;
  action: () => void;
}

const props = defineProps<{
  open: boolean;
  x: number;
  y: number;
  items: ContextMenuItem[];
}>();
const emit = defineEmits<{ (e: 'close'): void }>();

const menuRef = ref<HTMLDivElement | null>(null);

function onClickOutside(e: MouseEvent) {
  if (!menuRef.value) return;
  if (!menuRef.value.contains(e.target as Node)) emit('close');
}

function onEsc(e: KeyboardEvent) {
  if (e.key === 'Escape') emit('close');
}

onMounted(() => {
  document.addEventListener('mousedown', onClickOutside);
  document.addEventListener('keydown', onEsc);
});
onBeforeUnmount(() => {
  document.removeEventListener('mousedown', onClickOutside);
  document.removeEventListener('keydown', onEsc);
});

function runItem(it: ContextMenuItem) {
  if (it.disabled) return;
  it.action();
  emit('close');
}

// Adjust position so menu stays inside viewport.
const adjusted = ref({ x: 0, y: 0 });
watch(
  () => [props.x, props.y, props.open] as const,
  () => {
    if (!props.open) return;
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    const w = 200;
    const h = Math.min(props.items.length * 28 + 8, 400);
    adjusted.value = {
      x: Math.min(props.x, vw - w - 8),
      y: Math.min(props.y, vh - h - 8),
    };
  },
  { immediate: true },
);
</script>

<template>
  <Teleport to="body">
    <div
      v-if="open"
      ref="menuRef"
      class="ctx-menu"
      :style="{ left: adjusted.x + 'px', top: adjusted.y + 'px' }"
      @click.stop
    >
      <button
        v-for="(it, i) in items"
        :key="i"
        class="ctx-item"
        :class="{ danger: it.danger, disabled: it.disabled }"
        :disabled="it.disabled"
        @click="runItem(it)"
      >
        <span>{{ it.label }}</span>
        <span v-if="it.shortcut" class="shortcut">{{ it.shortcut }}</span>
      </button>
    </div>
  </Teleport>
</template>

<style scoped>
.ctx-menu {
  position: fixed;
  z-index: var(--sf-z-popover);
  background: var(--sf-bg-2);
  border: 1px solid var(--sf-border-strong);
  border-radius: var(--sf-radius-md);
  box-shadow: var(--sf-shadow-3);
  padding: 4px;
  min-width: 180px;
  font-size: 0.75rem;
}
.ctx-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  background: transparent;
  border: none;
  border-radius: var(--sf-radius-sm);
  padding: 5px 10px;
  text-align: left;
  cursor: pointer;
  color: var(--sf-text-1);
  font-family: inherit;
  font-size: 0.75rem;
}
.ctx-item:hover:not(.disabled) {
  background: var(--sf-bg-3);
  color: var(--sf-text-0);
}
.ctx-item.danger {
  color: var(--sf-text-1);
}
.ctx-item.danger:hover:not(.disabled) {
  background: rgba(255, 77, 79, 0.12);
  color: var(--sf-error);
}
.ctx-item.disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
.shortcut {
  font-family: var(--sf-font-mono);
  font-size: 0.625rem;
  color: var(--sf-text-3);
  margin-left: 16px;
}
</style>
