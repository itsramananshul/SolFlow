/**
 * Shared floating-panel behavior for the Run and Trace IDE windows.
 *
 * Gives a panel: fixed left/top positioning, drag-from-handle, hard
 * viewport clamping (the WHOLE panel stays inside on open, drag, resize,
 * and content-height change), a session-remembered position, recenter, and
 * a deterministic z-index with click-to-front. Drag bails on interactive
 * targets so buttons/tabs stay clickable and panes keep their own scroll.
 */
import { computed, nextTick, onBeforeUnmount, ref, watch, type Ref } from 'vue';
import { nextPanelZ } from './panelZ';

const EDGE = 8; // gap kept between the panel and every viewport edge

export type Placement = 'center' | 'right' | 'left';

export interface DraggablePanel {
  /** Bind to the panel root element (`ref="panelRef"`). */
  panelRef: Ref<HTMLElement | null>;
  /** Inline style: fixed position + z-index. */
  panelStyle: Ref<Record<string, string>>;
  dragging: Ref<boolean>;
  /** `@pointerdown` on the drag handle / header. */
  onHeaderPointerDown: (e: PointerEvent) => void;
  /** Recenter button / double-click handler. */
  recenter: () => void;
  /** Bring this panel above the others (call on panel pointerdown). */
  bringToFront: () => void;
  /** Call when the panel becomes visible (open) to (re)place it. */
  open: () => void;
}

export function useDraggablePanel(
  id: string,
  opts: { width: number; placement?: Placement } = { width: 640 },
): DraggablePanel {
  const PANEL_W = opts.width;
  const placement = opts.placement ?? 'center';
  const POS_KEY = `solflow.panel.${id}.pos`;

  const panelRef = ref<HTMLElement | null>(null);
  const pos = ref<{ x: number; y: number } | null>(null);
  const z = ref<number>(nextPanelZ());
  const dragging = ref(false);
  let dragOffset = { x: 0, y: 0 };

  function panelWidth(): number {
    return Math.min(PANEL_W, window.innerWidth - 2 * EDGE);
  }
  function panelSize(): { w: number; h: number } {
    const el = panelRef.value;
    if (el) return { w: el.offsetWidth, h: el.offsetHeight };
    return { w: panelWidth(), h: Math.min(window.innerHeight - 2 * EDGE, 560) };
  }

  function clampPos(x: number, y: number): { x: number; y: number } {
    const { w, h } = panelSize();
    const maxX = Math.max(EDGE, window.innerWidth - w - EDGE);
    const maxY = Math.max(EDGE, window.innerHeight - h - EDGE);
    return {
      x: Math.min(Math.max(x, EDGE), maxX),
      y: Math.min(Math.max(y, EDGE), maxY),
    };
  }

  function defaultPos(): { x: number; y: number } {
    const { w } = panelSize();
    let x: number;
    if (placement === 'right') x = window.innerWidth - w - EDGE - 8;
    else if (placement === 'left') x = EDGE + 8;
    else x = Math.round((window.innerWidth - w) / 2);
    const y = Math.round(window.innerHeight * 0.08);
    return clampPos(x, y);
  }

  function reclamp() {
    if (pos.value) pos.value = clampPos(pos.value.x, pos.value.y);
  }

  function open() {
    let next: { x: number; y: number } | null = null;
    try {
      const raw = sessionStorage.getItem(POS_KEY);
      if (raw) {
        const p = JSON.parse(raw);
        if (typeof p?.x === 'number' && typeof p?.y === 'number') next = { x: p.x, y: p.y };
      }
    } catch { /* ignore */ }
    pos.value = next ?? defaultPos();
    bringToFront();
    void nextTick(() => reclamp());
  }

  function recenter() {
    pos.value = defaultPos();
    persist();
  }

  function persist() {
    if (pos.value) {
      try { sessionStorage.setItem(POS_KEY, JSON.stringify(pos.value)); } catch { /* ignore */ }
    }
  }

  function bringToFront() {
    z.value = nextPanelZ();
  }

  const panelStyle = computed<Record<string, string>>(() => {
    const p = pos.value;
    if (!p) return {} as Record<string, string>;
    const s: Record<string, string> = {
      position: 'fixed',
      left: `${p.x}px`,
      top: `${p.y}px`,
      margin: '0',
      zIndex: String(z.value),
    };
    return s;
  });

  function onHeaderPointerDown(e: PointerEvent) {
    const el = e.target as HTMLElement;
    if (el.closest('button, a, input, select, textarea, .target-toggle, .no-drag')) return;
    if (e.button !== 0) return;
    bringToFront();
    dragging.value = true;
    const cur = pos.value ?? defaultPos();
    dragOffset = { x: e.clientX - cur.x, y: e.clientY - cur.y };
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
    document.body.style.userSelect = 'none';
  }
  function onMove(e: PointerEvent) {
    if (!dragging.value) return;
    pos.value = clampPos(e.clientX - dragOffset.x, e.clientY - dragOffset.y);
  }
  function onUp() {
    if (!dragging.value) return;
    dragging.value = false;
    window.removeEventListener('pointermove', onMove);
    window.removeEventListener('pointerup', onUp);
    document.body.style.userSelect = '';
    persist();
  }

  function onResize() { reclamp(); }
  window.addEventListener('resize', onResize);

  // Re-clamp when the panel's content height changes (e.g. a tall trace).
  let ro: ResizeObserver | null = null;
  watch(panelRef, (el) => {
    ro?.disconnect();
    if (el && typeof ResizeObserver !== 'undefined') {
      ro = new ResizeObserver(() => reclamp());
      ro.observe(el);
    }
  });

  onBeforeUnmount(() => {
    window.removeEventListener('resize', onResize);
    window.removeEventListener('pointermove', onMove);
    window.removeEventListener('pointerup', onUp);
    ro?.disconnect();
  });

  return { panelRef, panelStyle, dragging, onHeaderPointerDown, recenter, bringToFront, open };
}
