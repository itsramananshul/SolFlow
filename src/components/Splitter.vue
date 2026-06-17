<script setup lang="ts">
/**
 * Draggable splitter / resize bar.
 *
 * Drives a reactive size (px or fraction) via pointer events. The
 * parent layout consumes the size and applies it to grid tracks or
 * flex children. Splitters never read DOM dimensions themselves —
 * delta math is in screen pixels, the parent maps to whatever space
 * makes sense for its layout.
 *
 * Orientation:
 *   'vertical'   = a 1-cell-wide vertical bar that drags horizontally
 *                  (resizes left/right panels)
 *   'horizontal' = a 1-cell-tall horizontal bar that drags vertically
 *                  (resizes top/bottom panels)
 *
 * Behaviors:
 *   - Pointer capture so dragging works even when the cursor leaves
 *     the splitter element.
 *   - user-select: none body-class added during drag so highlight
 *     ghosting doesn't appear.
 *   - col-resize / row-resize cursor.
 *   - Double-click resets to the supplied `defaultSize`.
 */
import { ref } from 'vue';

const props = defineProps<{
  orientation: 'vertical' | 'horizontal';
  /** Current size (px or fraction — parent decides the unit). */
  size: number;
  /** Minimum allowed size. */
  min: number;
  /** Maximum allowed size. */
  max: number;
  /** Returned by double-click to reset. */
  defaultSize: number;
  /**
   * When true, treat size as a fraction 0..1 instead of px. The parent
   * still receives raw drag deltas in CSS px — it's our job to convert
   * via `containerPx`. Required iff size mode is fraction.
   */
  fraction?: boolean;
  /** Container pixel size, used to convert px deltas to fractions. */
  containerPx?: number;
}>();

const emit = defineEmits<{
  (e: 'update:size', value: number): void;
}>();

const dragging = ref(false);
let startCursor = 0;
let startSize = 0;

function onPointerDown(e: PointerEvent) {
  // Only primary-button presses (left mouse / touch).
  if (e.button !== 0 && e.pointerType === 'mouse') return;
  e.preventDefault();
  dragging.value = true;
  startCursor = props.orientation === 'vertical' ? e.clientX : e.clientY;
  startSize = props.size;
  (e.target as Element).setPointerCapture(e.pointerId);
  document.body.classList.add('sf-splitter-drag');
  document.body.style.cursor =
    props.orientation === 'vertical' ? 'col-resize' : 'row-resize';
}

function onPointerMove(e: PointerEvent) {
  if (!dragging.value) return;
  const cursor = props.orientation === 'vertical' ? e.clientX : e.clientY;
  const dxPx = cursor - startCursor;
  let next: number;
  if (props.fraction && props.containerPx && props.containerPx > 0) {
    next = startSize + dxPx / props.containerPx;
  } else {
    next = startSize + dxPx;
  }
  next = Math.max(props.min, Math.min(props.max, next));
  emit('update:size', next);
}

function endDrag(e: PointerEvent) {
  if (!dragging.value) return;
  dragging.value = false;
  try {
    (e.target as Element).releasePointerCapture(e.pointerId);
  } catch {
    /* may have already been released */
  }
  document.body.classList.remove('sf-splitter-drag');
  document.body.style.cursor = '';
}

function onDoubleClick() {
  emit('update:size', props.defaultSize);
}
</script>

<template>
  <div
    :class="['sf-splitter', orientation, { dragging }]"
    role="separator"
    :aria-orientation="orientation === 'vertical' ? 'vertical' : 'horizontal'"
    @pointerdown="onPointerDown"
    @pointermove="onPointerMove"
    @pointerup="endDrag"
    @pointercancel="endDrag"
    @dblclick="onDoubleClick"
  >
    <div class="grip" />
  </div>
</template>

<style scoped>
/*
 * Subtle but discoverable: a 1px hairline matching the surrounding
 * borders, with a wider invisible hit area (8px) so the cursor can
 * grab the splitter without precision aiming. On hover/drag we add a
 * light accent tint so the splitter "lights up" as the user finds it.
 */
.sf-splitter {
  position: relative;
  flex-shrink: 0;
  background: transparent;
  transition: background 0.12s ease;
  touch-action: none;
}
.sf-splitter.vertical {
  /* 10px hit area centered on a 1px hairline — wide enough to grab
     comfortably on a touchpad without precision aiming. */
  width: 10px;
  margin-left: -4px;
  margin-right: -5px;
  cursor: col-resize;
  z-index: 2;
}
.sf-splitter.horizontal {
  height: 10px;
  margin-top: -4px;
  margin-bottom: -5px;
  cursor: row-resize;
  z-index: 2;
}
.sf-splitter .grip {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
}
.sf-splitter.vertical .grip::after {
  content: '';
  width: 1px;
  height: 100%;
  background: var(--sf-border);
  transition: background 0.12s ease;
}
.sf-splitter.horizontal .grip::after {
  content: '';
  width: 100%;
  height: 1px;
  background: var(--sf-border);
  transition: background 0.12s ease;
}
.sf-splitter:hover .grip::after,
.sf-splitter.dragging .grip::after {
  background: var(--sf-accent);
}
.sf-splitter:hover {
  background: rgba(108, 92, 231, 0.05);
}
.sf-splitter.dragging {
  background: rgba(108, 92, 231, 0.1);
}
</style>
