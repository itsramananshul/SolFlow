/**
 * Controller connection store.
 *
 * One source of truth for the editor's execution target and its
 * relationship with SolFlow controllers. SolFlow runs a workflow
 * against one of three targets:
 *
 *   - browser-sim:  the canonical SOL VM compiled to WASM, in this
 *                   browser. No controller. External Actions blocked.
 *   - local:        a controller on the user's own machine, e.g.
 *                   http://127.0.0.1:3939.
 *   - cloud:        a public HTTPS controller, e.g.
 *                   https://controller.example.com.
 *
 * The two controller endpoints are configured independently, each
 * with its own URL and live connection status, so the UI can show
 * both at once (connected / disconnected). The shared bearer token
 * is sent to whichever controller needs it.
 *
 * Everything persists to localStorage so a reload restores the same
 * target and URLs. On mount we probe both configured controllers so
 * their status is live without the user clicking anything.
 *
 * Back-compat: `url`, `connection`, `isConnected`, `getClient`,
 * `connect`, `disconnect`, and `connectors` resolve against the
 * ACTIVE controller (the cloud endpoint when the run target is
 * cloud, otherwise the local endpoint), so existing callers keep
 * working unchanged.
 */
import { computed, ref } from 'vue';
import { defineStore } from 'pinia';
import {
  ControllerClientErr,
  controllerClient,
  type ControllerClient,
} from '@/runtime-host/client';
import { opremClient, type OpremRunOutcome } from '@/runtime-host/opremClient';
import type { ConnectorMeta, Health } from '@/runtime-host/types';

/** Which controller endpoint a setting refers to. */
export type ControllerTarget = 'local' | 'cloud';
/** What a workflow run executes against. */
export type RunTarget = 'browser-sim' | 'local' | 'cloud';

const DEFAULT_LOCAL_URL = 'http://127.0.0.1:3939';

const STORAGE_KEY_LOCAL_URL = 'solflow.controller.local_url';
const STORAGE_KEY_CLOUD_URL = 'solflow.controller.cloud_url';
const STORAGE_KEY_RUN_TARGET = 'solflow.run.target';
/** Legacy single-URL key (pre run-targets); migrated into local. */
const STORAGE_KEY_LEGACY_URL = 'solflow.controller.url';
/**
 * Bearer token persisted alongside the URLs. Stored in plain
 * localStorage with the URL: the editor's "I trust this device"
 * model. Not appropriate for shared / kiosk environments.
 */
const STORAGE_KEY_TOKEN = 'solflow.controller.auth_token';

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
        | { kind: 'auth'; status: number; code: string; message: string }
        | { kind: 'unknown'; message: string };
    };

export const useControllerStore = defineStore('controller', () => {
  // ---- persistent settings ----
  const localUrl = ref<string>(loadStoredUrl(STORAGE_KEY_LOCAL_URL, DEFAULT_LOCAL_URL, true));
  const cloudUrl = ref<string>(loadStoredUrl(STORAGE_KEY_CLOUD_URL, '', false));
  const runTarget = ref<RunTarget>(loadStoredRunTarget());
  /** Bearer token sent on every controller request when non-empty. */
  const authToken = ref<string>(loadStoredToken());

  // ---- runtime state (per controller endpoint) ----
  const localConn = ref<ConnectionState>({ kind: 'idle' });
  const cloudConn = ref<ConnectionState>({ kind: 'idle' });
  /** Connector metadata reported by the active controller on connect. */
  const connectors = ref<ConnectorMeta[]>([]);

  /**
   * The controller endpoint the back-compat facades resolve to: the
   * cloud endpoint when running against cloud, otherwise local. When
   * the run target is browser-sim there is no controller in play, but
   * we still point at `local` so status panels have something to show.
   */
  const activeTarget = computed<ControllerTarget>(() =>
    runTarget.value === 'cloud' ? 'cloud' : 'local',
  );

  function urlFor(target: ControllerTarget): string {
    return target === 'cloud' ? cloudUrl.value : localUrl.value;
  }
  function connRef(target: ControllerTarget) {
    return target === 'cloud' ? cloudConn : localConn;
  }

  // ---- back-compat facades (active controller) ----
  const url = computed<string>(() => urlFor(activeTarget.value));
  const connection = computed<ConnectionState>(() => connRef(activeTarget.value).value);
  const isConnected = computed(() => connection.value.kind === 'connected');
  const controllerVersion = computed(() =>
    connection.value.kind === 'connected' ? connection.value.health.controller_version : null,
  );

  // ---- per-target status (for the settings UI) ----
  const localConnected = computed(() => localConn.value.kind === 'connected');
  const cloudConnected = computed(() => cloudConn.value.kind === 'connected');

  /** Build a fresh client for a target's URL + token. The client is
   *  stateless, so building on demand costs only URL normalization.
   *  Throws `ControllerClientErr` if the URL is invalid. */
  function getClient(target: ControllerTarget = activeTarget.value): ControllerClient {
    return controllerClient(urlFor(target), { authToken: authToken.value });
  }

  // ---- settings mutations ----
  function setLocalUrl(next: string): void {
    if (next === localUrl.value) return;
    localUrl.value = next;
    localConn.value = { kind: 'idle' };
    persist(STORAGE_KEY_LOCAL_URL, next);
  }

  function setCloudUrl(next: string): void {
    if (next === cloudUrl.value) return;
    cloudUrl.value = next;
    cloudConn.value = { kind: 'idle' };
    persist(STORAGE_KEY_CLOUD_URL, next);
  }

  /** Set the URL of the active controller endpoint (back-compat). */
  function setUrl(next: string): void {
    if (activeTarget.value === 'cloud') setCloudUrl(next);
    else setLocalUrl(next);
  }

  function setRunTarget(next: RunTarget): void {
    if (next === runTarget.value) return;
    runTarget.value = next;
    persist(STORAGE_KEY_RUN_TARGET, next);
  }

  function setAuthToken(next: string): void {
    if (next === authToken.value) return;
    authToken.value = next;
    try {
      if (next.length > 0) localStorage.setItem(STORAGE_KEY_TOKEN, next);
      else localStorage.removeItem(STORAGE_KEY_TOKEN);
    } catch {
      // value stays in-memory for this session
    }
    // A fixed token should let the user re-try a previously-rejected
    // controller without an extra "reset" step.
    for (const c of [localConn, cloudConn]) {
      if (c.value.kind === 'error' && c.value.reason.kind === 'auth') {
        c.value = { kind: 'idle' };
      }
    }
  }

  /**
   * Probe a controller's `/healthz`. Updates that endpoint's
   * connection state to `connected` or `error`. Safe to call
   * repeatedly; this is what the "Check" button calls.
   */
  async function checkHealth(target: ControllerTarget = activeTarget.value): Promise<void> {
    const conn = connRef(target);
    const targetUrl = urlFor(target).trim();
    if (!targetUrl) {
      conn.value = {
        kind: 'error',
        reason: { kind: 'invalid_url', message: 'controller URL is empty' },
      };
      return;
    }
    let client: ControllerClient;
    try {
      client = getClient(target);
    } catch (e) {
      conn.value = {
        kind: 'error',
        reason:
          e instanceof ControllerClientErr && e.payload.kind === 'invalid_url'
            ? { kind: 'invalid_url', message: e.payload.message }
            : { kind: 'unknown', message: errorMessage(e) },
      };
      return;
    }
    conn.value = { kind: 'connecting' };
    try {
      const health = await client.healthzChecked();
      // Connectors are best-effort; only the active controller's
      // connector list is surfaced in the UI.
      if (target === activeTarget.value) {
        try {
          connectors.value = await client.listConnectors({ timeoutMs: 3_000 });
        } catch {
          connectors.value = [];
        }
      }
      conn.value = { kind: 'connected', health, connectedAt: Date.now() };
    } catch (e) {
      conn.value = { kind: 'error', reason: connectionErrorFrom(e) };
      if (target === activeTarget.value) connectors.value = [];
    }
  }

  /** Back-compat: check the active controller. */
  function connect(): Promise<void> {
    return checkHealth(activeTarget.value);
  }

  /** Alias for clearer error-UI call sites. */
  function retry(target?: ControllerTarget): Promise<void> {
    return checkHealth(target ?? activeTarget.value);
  }

  function disconnect(target: ControllerTarget = activeTarget.value): void {
    connRef(target).value = { kind: 'idle' };
    if (target === activeTarget.value) connectors.value = [];
  }

  /**
   * Run a SOL workflow on the real OpenPrem controller protocol
   * (source-based POST /workflow + poll) at the active controller URL.
   * Independent of the `/healthz` path above.
   */
  function runOnOprem(
    source: string,
    workflow: string,
    signal?: AbortSignal,
  ): Promise<OpremRunOutcome> {
    const client = opremClient(url.value);
    return client.runWorkflow(source, workflow, { signal, overallTimeoutMs: 60_000 });
  }

  /**
   * Best-effort status probe on app mount. Checks both configured
   * controllers (any non-empty URL) so the UI shows live status
   * without the user clicking. Never blocks app boot.
   */
  function tryReconnectOnMount(): void {
    if (localUrl.value.trim().length > 0) void checkHealth('local');
    if (cloudUrl.value.trim().length > 0) void checkHealth('cloud');
  }

  return {
    // settings
    localUrl,
    cloudUrl,
    runTarget,
    authToken,
    // active-controller facades
    url,
    connection,
    isConnected,
    controllerVersion,
    connectors,
    activeTarget,
    // per-target status
    localConn,
    cloudConn,
    localConnected,
    cloudConnected,
    // mutations
    setLocalUrl,
    setCloudUrl,
    setUrl,
    setRunTarget,
    setAuthToken,
    // connection ops
    checkHealth,
    connect,
    retry,
    disconnect,
    getClient,
    runOnOprem,
    tryReconnectOnMount,
  };
});

// =============================================================
//  Helpers
// =============================================================

function loadStoredUrl(key: string, fallback: string, migrateLegacy: boolean): string {
  try {
    const v = localStorage.getItem(key);
    if (v !== null) return v;
    // One-time migration: the pre-run-targets build stored a single
    // controller URL. Fold it into the local endpoint.
    if (migrateLegacy) {
      const legacy = localStorage.getItem(STORAGE_KEY_LEGACY_URL);
      if (legacy !== null && legacy.trim().length > 0) {
        localStorage.setItem(key, legacy);
        return legacy;
      }
    }
    return fallback;
  } catch {
    return fallback;
  }
}

function loadStoredRunTarget(): RunTarget {
  try {
    const v = localStorage.getItem(STORAGE_KEY_RUN_TARGET);
    if (v === 'local' || v === 'cloud' || v === 'browser-sim') return v;
    return 'browser-sim';
  } catch {
    return 'browser-sim';
  }
}

function loadStoredToken(): string {
  try {
    return localStorage.getItem(STORAGE_KEY_TOKEN) ?? '';
  } catch {
    return '';
  }
}

function persist(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Ignore quota / disabled-storage failures.
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
    case 'auth':
      return { kind: 'auth', status: p.status, code: p.code, message: p.message };
    case 'invalid_url':
      return { kind: 'invalid_url', message: p.message };
    case 'aborted':
      return { kind: 'unknown', message: p.message };
  }
}
