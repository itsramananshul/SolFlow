/**
 * POST /api/sol-man/generate
 *
 * Provider-agnostic LLM call. Resolves which provider to use from the
 * environment (see _providers.ts) and invokes its native API. Returns
 * a validated GeneratedGraphSpec the client renders into a real
 * workflow graph.
 *
 * Honesty contract:
 *   - No provider configured → 503 { configMissing: true, availableProviders }
 *   - Malformed LLM output → 502 with the specific parse/validation error
 *   - Upstream API error → 502 with the provider's own message
 *   - No fake responses, no fallback templates, no demo mode
 *
 * Configuration via environment variables (set in Vercel project env
 * or .env.local for `vercel dev`):
 *
 *   ANTHROPIC_API_KEY   — Anthropic Claude       (default: claude-sonnet-4-6)
 *   OPENAI_API_KEY      — OpenAI                  (default: gpt-4o)
 *   GEMINI_API_KEY      — Google Gemini           (default: gemini-2.0-flash)
 *   GROK_API_KEY        — xAI Grok                (default: grok-3)
 *   SOL_MAN_API_KEY     ─┐
 *   SOL_MAN_API_BASE    ─┴─ generic OpenAI-compatible (OpenRouter, etc.)
 *                          requires SOL_MAN_PROVIDER=openai-compatible
 *                          and SOL_MAN_MODEL
 *
 *   SOL_MAN_PROVIDER    — optional explicit selector
 *                          (anthropic|openai|gemini|grok|openai-compatible)
 *   SOL_MAN_MODEL       — optional override of the provider's default
 *
 * Set any ONE of the provider API keys and Sol Man works. The first
 * key found in the order above wins (auto-detection).
 */

import type { VercelRequest, VercelResponse } from '@vercel/node';
import type {
  GenerateRequestBody,
  GenerateResponseBody,
} from '../../src/sol-man/types';
// .js extensions required: package.json has "type": "module" so Vercel
// ships these as ESM, and Node's ESM resolver rejects extensionless
// relative imports at runtime (ERR_MODULE_NOT_FOUND).
import { SYSTEM_PROMPT } from './_prompt.js';
import { SpecValidationError, validateSpec } from './_validate.js';
import { providerSummaries, resolveProvider } from './_providers.js';

export default async function handler(req: VercelRequest, res: VercelResponse) {
  if (req.method !== 'POST') {
    return send(res, 405, {
      ok: false,
      error: 'method not allowed — POST required',
    });
  }

  const body = req.body as GenerateRequestBody | undefined;
  const prompt = (body?.prompt ?? '').toString().trim();
  if (prompt.length === 0) {
    return send(res, 400, {
      ok: false,
      error: 'prompt is empty — describe the workflow you want',
    });
  }
  if (prompt.length > 4000) {
    return send(res, 400, {
      ok: false,
      error: 'prompt is too long (max 4000 characters)',
    });
  }

  // Pull optional inline (BYO-key) config from the request body so the
  // user's browser-stored provider settings take precedence over any
  // server env vars. Never echo the key back; never log it.
  const inline = body?.config;

  const resolved = resolveProvider(inline ?? null);
  if (!resolved) {
    return send(res, 503, {
      ok: false,
      error:
        'No LLM provider is configured. Open Sol Man settings (gear icon) and enter an API key from any supported provider — or set one of the provider env vars on this deployment.',
      configMissing: true,
      availableProviders: providerSummaries(),
    });
  }
  if (!resolved.model) {
    return send(res, 400, {
      ok: false,
      error: `${resolved.provider.name} needs a model name. Open Sol Man settings and set a Model value (or override the default via SOL_MAN_MODEL on the server).`,
    });
  }

  let llmResult;
  try {
    llmResult = await resolved.provider.call({
      systemPrompt: SYSTEM_PROMPT,
      userPrompt: prompt,
      apiKey: resolved.apiKey,
      model: resolved.model,
      baseUrl: resolved.baseUrl,
    });
  } catch (e) {
    return send(res, 502, {
      ok: false,
      error: `LLM provider (${resolved.provider.name}) failed: ${(e as Error).message}`,
    });
  }

  const text = llmResult.text;
  if (!text || text.length === 0) {
    return send(res, 502, {
      ok: false,
      error: `${resolved.provider.name} returned an empty response`,
    });
  }

  // Some models still wrap JSON in ```json fences despite the prompt
  // saying not to. Strip if present so the parser doesn't choke.
  const stripped = stripCodeFences(text);

  let raw: unknown;
  try {
    raw = JSON.parse(stripped);
  } catch (e) {
    return send(res, 502, {
      ok: false,
      error: `${resolved.provider.name} returned non-JSON output: ${(e as Error).message}. Raw start: ${stripped.slice(0, 200)}`,
    });
  }

  let spec;
  try {
    spec = validateSpec(raw);
  } catch (e) {
    if (e instanceof SpecValidationError) {
      return send(res, 502, {
        ok: false,
        error: `${resolved.provider.name} output failed validation: ${e.message}`,
      });
    }
    return send(res, 500, {
      ok: false,
      error: `Unexpected validation error: ${(e as Error).message}`,
    });
  }

  return send(res, 200, {
    ok: true,
    spec,
    model: resolved.model,
    provider: { id: resolved.provider.id, name: resolved.provider.name },
    usage: llmResult.usage,
  });
}

function send(
  res: VercelResponse,
  status: number,
  body: GenerateResponseBody,
): void {
  res.status(status).json(body);
}

function stripCodeFences(text: string): string {
  let s = text.trim();
  if (s.startsWith('```')) {
    const firstNL = s.indexOf('\n');
    if (firstNL !== -1) s = s.slice(firstNL + 1);
    if (s.endsWith('```')) s = s.slice(0, -3);
    s = s.trim();
  }
  return s;
}
