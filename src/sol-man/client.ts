/**
 * Sol Man API client. Single function — wraps fetch(/api/sol-man/generate)
 * with strict typing on both ends.
 */

import type {
  GenerateRequestBody,
  GenerateResponseBody,
  InlineProviderConfig,
} from './types';

export async function callSolMan(
  prompt: string,
  config?: InlineProviderConfig | null,
): Promise<GenerateResponseBody> {
  const request: GenerateRequestBody = config
    ? { prompt, config }
    : { prompt };
  let resp: Response;
  try {
    resp = await fetch('/api/sol-man/generate', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(request),
    });
  } catch (e) {
    return {
      ok: false,
      error: `Network error reaching Sol Man: ${(e as Error).message}. Are you running 'vercel dev'? Vite alone doesn't serve /api routes.`,
    };
  }
  let parsed: unknown;
  try {
    parsed = await resp.json();
  } catch {
    return {
      ok: false,
      error: `Sol Man returned a non-JSON response (HTTP ${resp.status})`,
    };
  }
  if (typeof parsed !== 'object' || parsed === null) {
    return {
      ok: false,
      error: 'Sol Man returned a malformed response',
    };
  }
  return parsed as GenerateResponseBody;
}
