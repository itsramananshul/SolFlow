/**
 * Sol Man API client. Single function — wraps fetch(/api/sol-man/generate)
 * with strict typing on both ends.
 */

import type {
  GenerateRequestBody,
  GenerateResponseBody,
} from './types';

export async function callSolMan(prompt: string): Promise<GenerateResponseBody> {
  let resp: Response;
  try {
    resp = await fetch('/api/sol-man/generate', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ prompt } satisfies GenerateRequestBody),
    });
  } catch (e) {
    return {
      ok: false,
      error: `Network error reaching Sol Man: ${(e as Error).message}. Are you running 'vercel dev'? Vite alone doesn't serve /api routes.`,
    };
  }
  let body: unknown;
  try {
    body = await resp.json();
  } catch {
    return {
      ok: false,
      error: `Sol Man returned a non-JSON response (HTTP ${resp.status})`,
    };
  }
  // Trust the server validation; if the shape is off, surface as error.
  if (typeof body !== 'object' || body === null) {
    return {
      ok: false,
      error: 'Sol Man returned a malformed response',
    };
  }
  return body as GenerateResponseBody;
}
