/**
 * Host-embed bridge — active ONLY when the page is loaded with `?embed=1`.
 *
 * Lets a parent window (the OpenPrem Platform) drive the embedded editor over
 * `postMessage` without changing any normal SolFlow behavior. Outside embed
 * mode this module does nothing.
 *
 * Message contract:
 *   Parent  -> SolFlow:  solflow:load {source}, solflow:getSource, solflow:clear
 *   SolFlow -> Parent:   solflow:ready,
 *                        solflow:loaded,
 *                        solflow:source {source},
 *                        solflow:changed {source},
 *                        solflow:error {error}
 */
import { watch } from 'vue';
import type { useGraphStore } from '@/stores/graph.store';

type GraphStore = ReturnType<typeof useGraphStore>;

/** True when the editor is embedded (the host passed `?embed=1`). */
export function isEmbed(): boolean {
  try {
    return new URLSearchParams(window.location.search).get('embed') === '1';
  } catch {
    return false;
  }
}

export function setupEmbedBridge(graph: GraphStore): void {
  if (!isEmbed()) return;

  const post = (msg: Record<string, unknown>, origin = '*'): void => {
    if (window.parent && window.parent !== window) {
      window.parent.postMessage(msg, origin);
    }
  };

  const currentSource = (): string => {
    const e = graph.emitted as unknown as { source?: string } | undefined;
    return e && typeof e.source === 'string' ? e.source : '';
  };

  window.addEventListener('message', async (e: MessageEvent) => {
    const data = e.data as { type?: unknown; source?: unknown } | null;
    if (!data || typeof data !== 'object' || typeof data.type !== 'string') return;
    if (!data.type.startsWith('solflow:')) return;
    // Reply to whoever asked. SOL source is workflow content, not a secret;
    // we only ever act on the defined message types.
    const replyOrigin = e.origin && e.origin !== 'null' ? e.origin : '*';

    switch (data.type) {
      case 'solflow:load': {
        const source = typeof data.source === 'string' ? data.source : '';
        try {
          const res = await graph.importFromSource(source);
          if (res && res.ok === false) {
            post({ type: 'solflow:error', error: 'import failed', report: res.report }, replyOrigin);
          } else {
            post({ type: 'solflow:loaded' }, replyOrigin);
          }
        } catch (err) {
          post({ type: 'solflow:error', error: String((err as Error)?.message ?? err) }, replyOrigin);
        }
        break;
      }
      case 'solflow:getSource':
        post({ type: 'solflow:source', source: currentSource() }, replyOrigin);
        break;
      case 'solflow:clear':
        try {
          await graph.importFromSource('workflow "untitled" {\n}\n');
        } catch {
          /* ignore */
        }
        post({ type: 'solflow:loaded' }, replyOrigin);
        break;
    }
  });

  // Optional: notify the host when the edited source changes (debounced).
  let timer: ReturnType<typeof setTimeout> | null = null;
  let last = '';
  watch(
    () => currentSource(),
    (src) => {
      if (src === last) return;
      last = src;
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => post({ type: 'solflow:changed', source: src }), 400);
    },
  );

  // Announce readiness so the host can send the first workflow.
  post({ type: 'solflow:ready' });
}
