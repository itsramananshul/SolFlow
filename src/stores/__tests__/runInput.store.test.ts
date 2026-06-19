/**
 * `useRunInputStore` + `usesPayload` coverage — test-payload UX.
 */
import { beforeEach, describe, expect, it } from 'vitest';
import { createPinia, setActivePinia } from 'pinia';
import { useRunInputStore, usesPayload } from '../runInput.store';

beforeEach(() => setActivePinia(createPinia()));

describe('usesPayload', () => {
  it('detects payload-like free names', () => {
    expect(usesPayload('workflow "w" { return payload.total; }')).toBe(true);
    expect(usesPayload('let x: int = event.id;')).toBe(true);
    expect(usesPayload('input')).toBe(true);
    expect(usesPayload('return request.body;')).toBe(true);
  });
  it('does not false-positive on similar words', () => {
    expect(usesPayload('workflow "w" { let total: int = 1; return total; }')).toBe(false);
    expect(usesPayload('let payloadX: int = 1;')).toBe(false);
    expect(usesPayload('')).toBe(false);
  });
});

describe('useRunInputStore', () => {
  it('seeds + resets to the sample payload', () => {
    const s = useRunInputStore();
    s.loadSamplePayload('{ "total": 1200 }');
    expect(s.payloadText).toBe('{ "total": 1200 }');
    expect(s.hasSamplePayload).toBe(true);
    s.setPayload('{ "total": 5 }');
    expect(s.payloadText).toBe('{ "total": 5 }');
    s.resetToSample();
    expect(s.payloadText).toBe('{ "total": 1200 }');
  });

  it('clears the payload for samples with none', () => {
    const s = useRunInputStore();
    s.loadSamplePayload('{ "a": 1 }');
    s.loadSamplePayload(undefined);
    expect(s.payloadText).toBe('');
    expect(s.hasSamplePayload).toBe(false);
  });

  it('validates JSON; empty is valid (binds nothing)', () => {
    const s = useRunInputStore();
    s.setPayload('');
    expect(s.validity.ok).toBe(true);
    expect(s.injectable).toBe('');
    s.setPayload('{ not json }');
    expect(s.validity.ok).toBe(false);
    expect(s.injectable).toBe('');
    s.setPayload('{ "total": 1200 }');
    expect(s.validity.ok).toBe(true);
    expect(s.injectable).toBe('{ "total": 1200 }');
  });
});
