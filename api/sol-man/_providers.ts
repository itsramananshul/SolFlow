/**
 * Sol Man LLM provider registry.
 *
 * Each provider implements a single `call()` function that takes a
 * system prompt + user prompt + API key + model name and returns the
 * text response with usage telemetry. The generate.ts handler
 * resolves a provider based on environment variables and invokes it.
 *
 * Three native providers (Anthropic / OpenAI / Gemini), one
 * convenience wrapper (Grok — OpenAI-compatible at api.x.ai), and one
 * generic OpenAI-compatible adapter for everything else (OpenRouter,
 * Together, local Ollama, custom endpoints).
 *
 * Adding a provider: append to PROVIDERS with a call() impl. No
 * changes to generate.ts needed.
 */

interface ProviderCallOptions {
  systemPrompt: string;
  userPrompt: string;
  apiKey: string;
  model: string;
  /** Only used by openai-compatible; native providers ignore. */
  baseUrl?: string;
}

interface ProviderCallResult {
  text: string;
  usage?: { inputTokens: number; outputTokens: number };
}

export interface ProviderConfig {
  id: string;
  name: string;
  /** Env var that holds the API key for this provider. */
  envKey: string;
  /** Optional secondary env var (e.g. base URL for openai-compatible). */
  envBase?: string;
  /** Sensible default model name; overridable via SOL_MAN_MODEL. */
  defaultModel: string;
  /** True when picking this provider requires an explicit
   *  SOL_MAN_PROVIDER (i.e. it won't auto-detect from env). */
  explicitOnly?: boolean;
  /** Make a request and return parsed text + usage. */
  call(opts: ProviderCallOptions): Promise<ProviderCallResult>;
}

const MAX_TOKENS = 4096;

// =============================================================
//  Helpers
// =============================================================

async function safeText(r: Response): Promise<string> {
  try {
    return await r.text();
  } catch {
    return '';
  }
}

function throwIfBad(providerName: string, r: Response, text: string): void {
  if (r.ok) return;
  throw new Error(`${providerName} ${r.status}: ${text || r.statusText}`);
}

// =============================================================
//  Anthropic — native Messages API
// =============================================================

interface AnthropicBlock { type: 'text'; text: string }
interface AnthropicResponse {
  content?: AnthropicBlock[];
  usage?: { input_tokens?: number; output_tokens?: number };
  error?: { type?: string; message?: string };
}

const anthropic: ProviderConfig = {
  id: 'anthropic',
  name: 'Anthropic Claude',
  envKey: 'ANTHROPIC_API_KEY',
  defaultModel: 'claude-sonnet-4-6',
  async call({ systemPrompt, userPrompt, apiKey, model }) {
    const r = await fetch('https://api.anthropic.com/v1/messages', {
      method: 'POST',
      headers: {
        'content-type': 'application/json',
        'x-api-key': apiKey,
        'anthropic-version': '2023-06-01',
      },
      body: JSON.stringify({
        model,
        max_tokens: MAX_TOKENS,
        system: systemPrompt,
        messages: [{ role: 'user', content: userPrompt }],
      }),
    });
    const raw = await safeText(r);
    throwIfBad(this.name, r, raw);
    const data = JSON.parse(raw) as AnthropicResponse;
    if (data.error) {
      throw new Error(data.error.message ?? data.error.type ?? 'unknown');
    }
    const text = (data.content ?? [])
      .filter((c): c is AnthropicBlock => c.type === 'text')
      .map((c) => c.text)
      .join('')
      .trim();
    return {
      text,
      usage: data.usage
        ? {
            inputTokens: data.usage.input_tokens ?? 0,
            outputTokens: data.usage.output_tokens ?? 0,
          }
        : undefined,
    };
  },
};

// =============================================================
//  OpenAI — Chat Completions API
//  Also reused by the OpenAI-compatible / Grok adapters.
// =============================================================

interface OpenAIChatChoice {
  message?: { content?: string };
}
interface OpenAIChatResponse {
  choices?: OpenAIChatChoice[];
  usage?: { prompt_tokens?: number; completion_tokens?: number };
  error?: { type?: string; message?: string };
}

async function callOpenAICompatible(opts: {
  baseUrl: string;
  apiKey: string;
  model: string;
  systemPrompt: string;
  userPrompt: string;
  providerName: string;
  /** Extra request headers — OpenRouter likes a referer + title for
   *  rate-limit attribution; harmless on plain OpenAI. */
  extraHeaders?: Record<string, string>;
}): Promise<ProviderCallResult> {
  const url = `${opts.baseUrl.replace(/\/$/, '')}/chat/completions`;
  const r = await fetch(url, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      authorization: `Bearer ${opts.apiKey}`,
      ...(opts.extraHeaders ?? {}),
    },
    body: JSON.stringify({
      model: opts.model,
      max_tokens: MAX_TOKENS,
      messages: [
        { role: 'system', content: opts.systemPrompt },
        { role: 'user', content: opts.userPrompt },
      ],
    }),
  });
  const raw = await safeText(r);
  throwIfBad(opts.providerName, r, raw);
  const data = JSON.parse(raw) as OpenAIChatResponse;
  if (data.error) {
    throw new Error(data.error.message ?? data.error.type ?? 'unknown');
  }
  const text = (data.choices?.[0]?.message?.content ?? '').trim();
  return {
    text,
    usage: data.usage
      ? {
          inputTokens: data.usage.prompt_tokens ?? 0,
          outputTokens: data.usage.completion_tokens ?? 0,
        }
      : undefined,
  };
}

const openai: ProviderConfig = {
  id: 'openai',
  name: 'OpenAI',
  envKey: 'OPENAI_API_KEY',
  defaultModel: 'gpt-4o',
  call(opts) {
    return callOpenAICompatible({
      baseUrl: 'https://api.openai.com/v1',
      apiKey: opts.apiKey,
      model: opts.model,
      systemPrompt: opts.systemPrompt,
      userPrompt: opts.userPrompt,
      providerName: 'OpenAI',
    });
  },
};

// =============================================================
//  Google Gemini — Generative Language API
// =============================================================

interface GeminiPart { text?: string }
interface GeminiCandidate { content?: { parts?: GeminiPart[] } }
interface GeminiResponse {
  candidates?: GeminiCandidate[];
  usageMetadata?: {
    promptTokenCount?: number;
    candidatesTokenCount?: number;
  };
  error?: { message?: string; status?: string };
}

const gemini: ProviderConfig = {
  id: 'gemini',
  name: 'Google Gemini',
  envKey: 'GEMINI_API_KEY',
  defaultModel: 'gemini-2.0-flash',
  async call({ systemPrompt, userPrompt, apiKey, model }) {
    const url = `https://generativelanguage.googleapis.com/v1beta/models/${encodeURIComponent(model)}:generateContent?key=${encodeURIComponent(apiKey)}`;
    const r = await fetch(url, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        systemInstruction: { parts: [{ text: systemPrompt }] },
        contents: [{ role: 'user', parts: [{ text: userPrompt }] }],
        generationConfig: { maxOutputTokens: MAX_TOKENS },
      }),
    });
    const raw = await safeText(r);
    throwIfBad(this.name, r, raw);
    const data = JSON.parse(raw) as GeminiResponse;
    if (data.error) {
      throw new Error(data.error.message ?? data.error.status ?? 'unknown');
    }
    const text = (data.candidates?.[0]?.content?.parts ?? [])
      .map((p) => p.text ?? '')
      .join('')
      .trim();
    return {
      text,
      usage: data.usageMetadata
        ? {
            inputTokens: data.usageMetadata.promptTokenCount ?? 0,
            outputTokens: data.usageMetadata.candidatesTokenCount ?? 0,
          }
        : undefined,
    };
  },
};

// =============================================================
//  xAI Grok — OpenAI-compatible API at api.x.ai
// =============================================================

const grok: ProviderConfig = {
  id: 'grok',
  name: 'xAI Grok',
  envKey: 'GROK_API_KEY',
  defaultModel: 'grok-3',
  call(opts) {
    return callOpenAICompatible({
      baseUrl: 'https://api.x.ai/v1',
      apiKey: opts.apiKey,
      model: opts.model,
      systemPrompt: opts.systemPrompt,
      userPrompt: opts.userPrompt,
      providerName: 'xAI Grok',
    });
  },
};

// =============================================================
//  OpenRouter — aggregator with a HUGE free-tier model list.
//  OpenAI-compatible API at openrouter.ai/api/v1. Adding it as a
//  first-class provider so users get free models out of the box.
// =============================================================

const openrouter: ProviderConfig = {
  id: 'openrouter',
  name: 'OpenRouter',
  envKey: 'OPENROUTER_API_KEY',
  // Default to a strong FREE model so users with a fresh OpenRouter
  // key get a working workflow without paying. The :free suffix
  // routes the request through OpenRouter's free quota for that
  // model. Override via SOL_MAN_MODEL or the modal's Model field.
  defaultModel: 'meta-llama/llama-3.3-70b-instruct:free',
  call(opts) {
    return callOpenAICompatible({
      baseUrl: 'https://openrouter.ai/api/v1',
      apiKey: opts.apiKey,
      model: opts.model,
      systemPrompt: opts.systemPrompt,
      userPrompt: opts.userPrompt,
      providerName: 'OpenRouter',
      // OpenRouter encourages an HTTP-Referer + X-Title for rate-limit
      // attribution and dashboard analytics. These don't expose user
      // data — they identify the calling app.
      extraHeaders: {
        'HTTP-Referer': 'https://solflow.app',
        'X-Title': 'SolFlow',
      },
    });
  },
};

// =============================================================
//  Generic OpenAI-compatible — for OpenRouter, Together, Ollama,
//  Anyscale, vLLM, anything that speaks /v1/chat/completions.
// =============================================================

const openaiCompatible: ProviderConfig = {
  id: 'openai-compatible',
  name: 'OpenAI-compatible (custom)',
  envKey: 'SOL_MAN_API_KEY',
  envBase: 'SOL_MAN_API_BASE',
  defaultModel: '',
  explicitOnly: true,
  call({ systemPrompt, userPrompt, apiKey, model, baseUrl }) {
    if (!baseUrl) {
      return Promise.reject(
        new Error(
          'openai-compatible provider requires SOL_MAN_API_BASE (e.g. https://openrouter.ai/api/v1)',
        ),
      );
    }
    if (!model) {
      return Promise.reject(
        new Error(
          'openai-compatible provider requires SOL_MAN_MODEL (e.g. anthropic/claude-3.5-sonnet for OpenRouter)',
        ),
      );
    }
    return callOpenAICompatible({
      baseUrl,
      apiKey,
      model,
      systemPrompt,
      userPrompt,
      providerName: 'OpenAI-compatible',
    });
  },
};

// =============================================================
//  Registry
// =============================================================

// Order matters for auto-detection — first one with a key wins.
// Anthropic first because the user originally pointed Sol Man at
// Claude; downstream order is provider popularity.
const PROVIDER_LIST: ProviderConfig[] = [
  anthropic,
  openai,
  gemini,
  grok,
  openrouter,
  openaiCompatible,
];

export const PROVIDERS: Record<string, ProviderConfig> = Object.fromEntries(
  PROVIDER_LIST.map((p) => [p.id, p]),
);

/**
 * Lightweight summary of available providers — surfaced to the client
 * when configMissing so the modal can render a clean config screen
 * instead of a string blob.
 */
export interface ProviderSummary {
  id: string;
  name: string;
  envKey: string;
  envBase?: string;
  defaultModel: string;
}
export function providerSummaries(): ProviderSummary[] {
  return PROVIDER_LIST.map((p) => ({
    id: p.id,
    name: p.name,
    envKey: p.envKey,
    envBase: p.envBase,
    defaultModel: p.defaultModel,
  }));
}

export interface ResolvedProvider {
  provider: ProviderConfig;
  apiKey: string;
  model: string;
  baseUrl?: string;
}

/**
 * Resolve which provider to call.
 *
 * Priority (each step takes the first complete match):
 *   1. Request-body config (BYO-key path) — user-supplied provider/
 *      key from the SolFlow UI. Highest priority because the user
 *      explicitly set it in their browser.
 *   2. Explicit SOL_MAN_PROVIDER env var (deployer-pinned provider).
 *   3. Auto-detect: scan known providers for the first with a set
 *      key env var. openai-compatible is skipped here because it
 *      needs explicit selection.
 *   4. None set → return null so the caller can surface
 *      configMissing with the full list of options.
 */
export function resolveProvider(
  inline?: {
    providerId?: string;
    apiKey?: string;
    model?: string;
    baseUrl?: string;
  } | null,
): ResolvedProvider | null {
  // 1. Inline (BYO-key) wins when complete.
  if (inline && inline.providerId && inline.apiKey) {
    const p = PROVIDERS[inline.providerId.trim().toLowerCase()];
    if (!p) return null;
    const model =
      (inline.model && inline.model.trim()) || p.defaultModel || '';
    const baseUrl =
      (inline.baseUrl && inline.baseUrl.trim()) ||
      (p.envBase ? process.env[p.envBase] : undefined);
    return { provider: p, apiKey: inline.apiKey, model, baseUrl };
  }

  // 2. Explicit env-var-pinned provider.
  const explicit = process.env.SOL_MAN_PROVIDER?.trim().toLowerCase();
  const modelOverride = process.env.SOL_MAN_MODEL?.trim();
  if (explicit) {
    const p = PROVIDERS[explicit];
    if (!p) return null;
    const apiKey = process.env[p.envKey];
    if (!apiKey) return null;
    const baseUrl = p.envBase ? process.env[p.envBase] : undefined;
    const model = modelOverride || p.defaultModel;
    return { provider: p, apiKey, model, baseUrl };
  }

  // 3. Auto-detect from env keys.
  for (const p of PROVIDER_LIST) {
    if (p.explicitOnly) continue;
    const apiKey = process.env[p.envKey];
    if (apiKey) {
      const baseUrl = p.envBase ? process.env[p.envBase] : undefined;
      const model = modelOverride || p.defaultModel;
      return { provider: p, apiKey, model, baseUrl };
    }
  }
  return null;
}
