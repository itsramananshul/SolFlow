/**
 * Controller connection store (Phase C C.2 c62).
 *
 * One source of truth for the editor's relationship with a SolFlow
 * controller. ControllerSettingsModal manages this; RunModal reads
 * it to decide whether controller-local execution mode is available.
 *
 * State machine (linear, no surprises):
 *
 *    idle ──setUrl──> idle
 *    idle ──connect──> connecting ──ok──> connected
 *                              └──err──> error
 *    connected ──disconnect──> idle
 *    connected ──connect──> connecting  (refresh)
 *    error ──retry──> connecting
 *
 * URL + last-known-good connection are persisted to localStorage so
 * a page reload reconnects without ceremony (silent reconnect on
 * mount; user sees "connecting" briefly then "connected").
 *
 * The `ControllerClient` instance is cached and rebuilt whenever
 * the URL changes — the client itself is stateless so the cost
 * is just URL normalization.
 */
import { computed, ref } from 'vue';
import { defineStore } from 'pinia';
import {
  ControllerClientErr,
  controllerClient,
  type ControllerClient,
} from '@/runtime-host/client';
import type { Health } from '@/runtime-host/types';

const STORAGE_KEY_URL = 'solflow.controller.url';
const STORAGE_KEY_AUTO = 'solflow.controller.auto_reconnect';

export type ConnectionState =
  | { kind: 'idle' }
  | { kind: 'connecting' }
  | {
      kind: 'connected';
      health: Health;
      connectedAt: number;
    }
  | {
      kind: 'error';
      /** Discriminated error from the client layer. */
      reason:
        | { kind: 'network'; message: string }
        | { kind: 'timeout'; message: string }
        | { kind: 'http'; status: number; message: string; code?: string }
        | { kind: 'decode'; message: string }
        | { kind: 'version'; controllerMajor: number; editorMajor: number; message: string }
        | { kind: 'invalid_url'; message: string }
        | { kind: 'unknown'; message: string };
    };

export const useControllerStore = defineStore('controller', () => {
  // ---- persistent state ----
  const url = ref<string>(loadStoredUrl());
  /** Did the last successful connection come from this URL? Used to
   *  decide whether to auto-reconnect on app mount. */
  const autoReconnect = ref<boolean>(loadAutoReconnectFlag());

  // ---- runtime state ----
  const connection = ref<ConnectionState>({ kind: 'idle' });

  let cachedClient: ControllerClient | null = null;
  let cachedFor: string | null = null;

  /** Build / re-use a `ControllerClient` for the current URL.
   *  Throws if the URL is invalid (e.g. missing scheme). */
  function getClient(): ControllerClient {
    if (cachedClient && cachedFor === url.value) return cachedClient;
    cachedClient = controllerClient(url.value);
    cachedFor = url.value;
    return cachedClient;
  }

  /** True when controller-local execution is usable. */
  const isConnected = computed(() => connection.value.kind === 'connected');

  /** The connected controller's reported version, if any. */
  const controllerVersion = computed(() =>
    connection.value.kind === 'connected' ? connection.value.health.controller_version : null,
  );

  function setUrl(next: string): void {
    if (next === url.value) return;
    url.value = next;
    cachedClient = null;
    cachedFor = null;
    try {
      localStorage.setItem(STORAGE_KEY_URL, next);
    } catch {
      // Ignore quota / disabled-storage failures — value stays
      // in-memory for this session.
    }
    // URL change invalidates connection. Drop to idle; user must
    // explicitly re-connect. (Don't auto-fire connect() here —
    // that would be surprising for typing in the URL field.)
    connection.value = { kind: 'idle' };
    autoReconnect.value = false;
    try {
      localStorage.setItem(STORAGE_KEY_AUTO, '0');
    } catch {
      // ignore
    }
  }

  /** Attempt a healthz call. Updates `connection` to either
   *  `connected` or `error`. Safe to call repeatedly. */
  async function connect(): Promise<void> {
    if (!url.value.trim()) {
      connection.value = {
        kind: 'error',
        reason: { kind: 'invalid_url', message: 'controller URL is empty' },
      };
      return;
    }
    let client: ControllerClient;
    try {
      client = getClient();
    } catch (e) {
      if (e instanceof ControllerClientErr && e.payload.kind === 'decode') {
        connection.value = {
          kind: 'error',
          reason: { kind: 'invalid_url', message: e.payload.message },
        };
      } else {
        connection.value = {
          kind: 'error',
          reason: { kind: 'unknown', message: errorMessage(e) },
        };
      }
      return;
    }
    connection.value = { kind: 'connecting' };
    try {
      const health = await client.healthzChecked();
      connection.value = {
        kind: 'connected',
        health,
        connectedAt: Date.now(),
      };
      autoReconnect.value = true;
      try {
        localStorage.setItem(STORAGE_KEY_AUTO, '1');
      } catch {
        // ignore
      }
    } catch (e) {
      connection.value = { kind: 'error', reason: connectionErrorFrom(e) };
    }
  }

  function disconnect(): void {
    connection.value = { kind: 'idle' };
    autoReconnect.value = false;
    try {
      localStorage.setItem(STORAGE_KEY_AUTO, '0');
    } catch {
      // ignore
    }
  }

  /** Alias for `connect()` — clearer at call sites in error UIs. */
  function retry(): Promise<void> {
    return connect();
  }

  /**
   * Best-effort silent reconnect on app mount. Only attempts when
   * `autoReconnect` is set (i.e. the URL previously succeeded);
   * never blocks app boot.
   */
  function tryReconnectOnMount(): void {
    if (autoReconnect.value && url.value.trim().length > 0) {
      // Fire-and-forget; the modal subscribes to `connection`.
      void connect();
    }
  }

  return {
    url,
    autoReconnect,
    connection,
    isConnected,
    controllerVersion,
    setUrl,
    connect,
    disconnect,
    retry,
    getClient,
    tryReconnectOnMount,
  };
});

// =============================================================
//  Helpers
// =============================================================

function loadStoredUrl(): string {
  try {
    return localStorage.getItem(STORAGE_KEY_URL) ?? '';
  } catch {
    return '';
  }
}

function loadAutoReconnectFlag(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY_AUTO) === '1';
  } catch {
    return false;
  }
}

function errorMessage(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

function connectionErrorFrom(e: unknown): Extract<ConnectionState, { kind: 'error' }>['reason'] {
  if (!(e instanceof ControllerClientErr)) {
    return { kind: 'unknown', message: errorMessage(e) };
  }
  const p = e.payload;
  switch (p.kind) {
    case 'network':
      return { kind: 'network', message: p.message };
    case 'timeout':
      return { kind: 'timeout', message: p.message };
    case 'http':
      return { kind: 'http', status: p.status, message: p.message, code: p.code };
    case 'decode':
      return { kind: 'decode', message: p.message };
    case 'version':
      return {
        kind: 'version',
        controllerMajor: p.controllerMajor,
        editorMajor: p.editorMajor,
        message: p.message,
      };
    case 'aborted':
      // Treat user-initiated abort during health-check as just
      // "back to idle" — but we wrap as unknown so the UX shows a
      // generic message instead of dangling on "connecting".
      return { kind: 'unknown', message: p.message };
  }
}
