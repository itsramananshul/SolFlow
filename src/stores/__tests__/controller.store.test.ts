/**
 * `useControllerStore` connection-state machine coverage
 * (Phase C C.2 c66).
 *
 * The store is the editor's single source of truth for the
 * editor↔controller relationship. Bugs here ripple into the
 * RunModal mode selector ("controller-local stays available
 * after the controller goes down") and the settings modal
 * ("Connect button stays enabled after a successful connection
 * with no Disconnect"), so the state-machine deserves explicit
 * coverage.
 *
 * Strategy: inject a fake fetch via the global; we don't construct
 * a real `controllerClient` per call — the store does — so we
 * substitute `globalThis.fetch` for the duration of each test.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createPinia, setActivePinia } from 'pinia';
import { useControllerStore } from '../controller.store';
import { HOST_SPEC_MAJOR } from '@/runtime-host/types';

// localStorage shim — vitest's node env doesn't ship one.
beforeEach(() => {
  setActivePinia(createPinia());
  const mem = new Map<string, string>();
  vi.stubGlobal('localStorage', {
    getItem: (k: string) => mem.get(k) ?? null,
    setItem: (k: string, v: string) => mem.set(k, v),
    removeItem: (k: string) => mem.delete(k),
    clear: () => mem.clear(),
    key: () => null,
    length: 0,
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
});

function jsonOk(body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });
}

describe('useControllerStore — connection state machine', () => {
  it('starts idle, with empty URL', () => {
    const s = useControllerStore();
    expect(s.connection.kind).toBe('idle');
    expect(s.url).toBe('');
    expect(s.isConnected).toBe(false);
  });

  it('setUrl persists + invalidates connection back to idle', () => {
    const s = useControllerStore();
    s.setUrl('http://x.example');
    expect(s.url).toBe('http://x.example');
    expect(s.connection.kind).toBe('idle');
    expect(localStorage.getItem('solflow.controller.url')).toBe('http://x.example');
  });

  it('connect → connected on healthy response', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(
      jsonOk({ ok: true, controller_version: '0.1.0', host_spec_major: HOST_SPEC_MAJOR }),
    ));
    const s = useControllerStore();
    s.setUrl('http://x.example');
    await s.connect();
    expect(s.connection.kind).toBe('connected');
    if (s.connection.kind === 'connected') {
      expect(s.connection.health.controller_version).toBe('0.1.0');
    }
    expect(s.isConnected).toBe(true);
    // Auto-reconnect flag set so next reload re-tries.
    expect(localStorage.getItem('solflow.controller.auto_reconnect')).toBe('1');
  });

  it('connect → error{network} when fetch throws', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new TypeError('ECONNREFUSED')));
    const s = useControllerStore();
    s.setUrl('http://x.example');
    await s.connect();
    expect(s.connection.kind).toBe('error');
    if (s.connection.kind === 'error') {
      expect(s.connection.reason.kind).toBe('network');
    }
    expect(s.isConnected).toBe(false);
  });

  it('connect → error{version} on host-spec mismatch', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(
      jsonOk({ ok: true, controller_version: '99.0.0', host_spec_major: 999 }),
    ));
    const s = useControllerStore();
    s.setUrl('http://x.example');
    await s.connect();
    expect(s.connection.kind).toBe('error');
    if (s.connection.kind === 'error') {
      expect(s.connection.reason.kind).toBe('version');
      if (s.connection.reason.kind === 'version') {
        expect(s.connection.reason.controllerMajor).toBe(999);
        expect(s.connection.reason.editorMajor).toBe(HOST_SPEC_MAJOR);
      }
    }
  });

  it('connect → error{invalid_url} when URL is empty', async () => {
    const s = useControllerStore();
    s.setUrl(''); // explicit
    await s.connect();
    expect(s.connection.kind).toBe('error');
    if (s.connection.kind === 'error') {
      expect(s.connection.reason.kind).toBe('invalid_url');
    }
  });

  it('connect → error{invalid_url} when URL has no scheme', async () => {
    const s = useControllerStore();
    s.setUrl('127.0.0.1:3939');
    await s.connect();
    expect(s.connection.kind).toBe('error');
    if (s.connection.kind === 'error') {
      expect(s.connection.reason.kind).toBe('invalid_url');
    }
  });

  it('disconnect resets to idle + clears auto-reconnect', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(
      jsonOk({ ok: true, controller_version: '0.1.0', host_spec_major: HOST_SPEC_MAJOR }),
    ));
    const s = useControllerStore();
    s.setUrl('http://x.example');
    await s.connect();
    s.disconnect();
    expect(s.connection.kind).toBe('idle');
    expect(s.isConnected).toBe(false);
    expect(localStorage.getItem('solflow.controller.auto_reconnect')).toBe('0');
  });

  it('retry rebuilds connection (alias for connect)', async () => {
    let n = 0;
    vi.stubGlobal('fetch', vi.fn().mockImplementation(() => {
      n++;
      if (n === 1) return Promise.reject(new TypeError('first call'));
      return Promise.resolve(
        jsonOk({ ok: true, controller_version: '0.1.0', host_spec_major: HOST_SPEC_MAJOR }),
      );
    }));
    const s = useControllerStore();
    s.setUrl('http://x.example');
    await s.connect();
    expect(s.connection.kind).toBe('error');
    await s.retry();
    expect(s.connection.kind).toBe('connected');
  });

  it('tryReconnectOnMount no-ops when auto-reconnect was never set', () => {
    const fetchSpy = vi.fn().mockResolvedValue(jsonOk({}));
    vi.stubGlobal('fetch', fetchSpy);
    const s = useControllerStore();
    s.setUrl('http://x.example'); // setUrl clears auto-reconnect
    s.tryReconnectOnMount();
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('populates connectors after a successful connect (C.4)', async () => {
    const healthBody = {
      ok: true,
      controller_version: '0.1.0',
      host_spec_major: HOST_SPEC_MAJOR,
    };
    const connectorsBody = [
      {
        name: 'http',
        description: 'HTTP reference',
        version: '0.1.0',
        default_policy: {
          timeout_ms: 10_000,
          retry_attempts: 0,
          backoff_base_ms: 100,
          max_response_bytes: 1_048_576,
        },
      },
    ];
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation((input: RequestInfo | URL) => {
        const url = String(input);
        if (url.endsWith('/healthz')) return Promise.resolve(jsonOk(healthBody));
        if (url.endsWith('/connectors')) return Promise.resolve(jsonOk(connectorsBody));
        return Promise.reject(new Error('unexpected URL: ' + url));
      }),
    );
    const s = useControllerStore();
    s.setUrl('http://x.example');
    await s.connect();
    expect(s.connection.kind).toBe('connected');
    expect(s.connectors).toHaveLength(1);
    expect(s.connectors[0].name).toBe('http');
    s.disconnect();
    expect(s.connectors).toEqual([]);
  });

  it('connectors list degrades to empty when /connectors fails', async () => {
    const healthBody = {
      ok: true,
      controller_version: '0.1.0',
      host_spec_major: HOST_SPEC_MAJOR,
    };
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation((input: RequestInfo | URL) => {
        const url = String(input);
        if (url.endsWith('/healthz')) return Promise.resolve(jsonOk(healthBody));
        // Older controllers may return 404 for /connectors.
        if (url.endsWith('/connectors')) {
          return Promise.resolve(
            new Response('not found', { status: 404 }),
          );
        }
        return Promise.reject(new Error('unexpected URL: ' + url));
      }),
    );
    const s = useControllerStore();
    s.setUrl('http://x.example');
    await s.connect();
    expect(s.connection.kind).toBe('connected');
    expect(s.connectors).toEqual([]);
  });
});
