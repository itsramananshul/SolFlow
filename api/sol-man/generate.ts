/**
 * POST /api/sol-man/generate
 *
 * Server-side LLM call. Takes a user prompt, returns a validated
 * GeneratedGraphSpec the client can render.
 *
 * Provider: Anthropic Claude (REST API, no SDK dependency). Env vars:
 *   ANTHROPIC_API_KEY  — required
 *   SOL_MAN_MODEL      — optional, defaults to claude-sonnet-4-6
 *
 * Honesty contract:
 *   - If the API key is missing, returns 503 with configMissing:true.
 *     The client surfaces this verbatim — no fallback templates, no
 *     fake responses, no demo mode.
 *   - If the LLM returns invalid JSON or fails schema validation,
 *     we return the error message so the user knows what happened.
 *   - All responses are GenerateResponseBody-shaped.
 */

import type { VercelRequest, VercelResponse } from '@vercel/node';
import type {
  GenerateRequestBody,
  GenerateResponseBody,
} from '../../src/sol-man/types';
import { SYSTEM_PROMPT } from './_prompt';
import { SpecValidationError, validateSpec } from './_validate';

const DEFAULT_MODEL = 'claude-sonnet-4-6';
const ANTHROPIC_VERSION = '2023-06-01';

interface AnthropicTextBlock {
  type: 'text';
  text: string;
}
interface AnthropicResponse {
  content?: AnthropicTextBlock[];
  usage?: { input_tokens?: number; output_tokens?: number };
  error?: { type?: string; message?: string };
}

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

  const apiKey = process.env.ANTHROPIC_API_KEY;
  if (!apiKey) {
    return send(res, 503, {
      ok: false,
      error:
        'Sol Man is not configured on this deployment. Set ANTHROPIC_API_KEY to enable workflow generation.',
      configMissing: true,
    });
  }

  const model = process.env.SOL_MAN_MODEL || DEFAULT_MODEL;

  // Anthropic Messages API — straight REST, no SDK. Sized 60s in
  // vercel.json's `functions` config; matches the longest reasonable
  // generation latency.
  let llmResp: Response;
  try {
    llmResp = await fetch('https://api.anthropic.com/v1/messages', {
      method: 'POST',
      headers: {
        'content-type': 'application/json',
        'x-api-key': apiKey,
        'anthropic-version': ANTHROPIC_VERSION,
      },
      body: JSON.stringify({
        model,
        max_tokens: 4096,
        system: SYSTEM_PROMPT,
        messages: [{ role: 'user', content: prompt }],
      }),
    });
  } catch (e) {
    return send(res, 502, {
      ok: false,
      error: `Failed to reach the LLM provider: ${(e as Error).message}`,
    });
  }

  if (!llmResp.ok) {
    const text = await safeText(llmResp);
    return send(res, llmResp.status, {
      ok: false,
      error: `LLM provider returned ${llmResp.status}: ${text || llmResp.statusText}`,
    });
  }

  let parsedResponse: AnthropicResponse;
  try {
    parsedResponse = (await llmResp.json()) as AnthropicResponse;
  } catch {
    return send(res, 502, {
      ok: false,
      error: 'LLM provider returned an unparseable response',
    });
  }

  if (parsedResponse.error) {
    return send(res, 502, {
      ok: false,
      error: `LLM provider error: ${parsedResponse.error.message ?? parsedResponse.error.type ?? 'unknown'}`,
    });
  }

  const text =
    parsedResponse.content
      ?.filter((c): c is AnthropicTextBlock => c.type === 'text')
      .map((c) => c.text)
      .join('')
      .trim() ?? '';

  if (text.length === 0) {
    return send(res, 502, {
      ok: false,
      error: 'LLM returned an empty response',
    });
  }

  // Defensive: some models still wrap output in ```json fences even
  // when told not to. Strip if present so the parser doesn't choke.
  const stripped = stripCodeFences(text);

  let raw: unknown;
  try {
    raw = JSON.parse(stripped);
  } catch (e) {
    return send(res, 502, {
      ok: false,
      error: `LLM returned non-JSON output: ${(e as Error).message}. Raw start: ${stripped.slice(0, 200)}`,
    });
  }

  let spec;
  try {
    spec = validateSpec(raw);
  } catch (e) {
    if (e instanceof SpecValidationError) {
      return send(res, 502, {
        ok: false,
        error: `LLM output failed validation: ${e.message}`,
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
    model,
    usage: parsedResponse.usage
      ? {
          inputTokens: parsedResponse.usage.input_tokens ?? 0,
          outputTokens: parsedResponse.usage.output_tokens ?? 0,
        }
      : undefined,
  });
}

function send(
  res: VercelResponse,
  status: number,
  body: GenerateResponseBody,
): void {
  res.status(status).json(body);
}

async function safeText(r: Response): Promise<string> {
  try {
    return await r.text();
  } catch {
    return '';
  }
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
