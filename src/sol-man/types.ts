/**
 * Sol Man — shared types between the client and the serverless API.
 *
 * The LLM emits a `GeneratedGraphSpec` — a structured workflow
 * description that uses SolFlow's existing node vocabulary. The
 * client validates it, runs auto-layout, and converts it into real
 * GraphNodes / GraphEdges via the same `createNode` factory the
 * editor uses. This means anything Sol Man produces is editable in
 * the exact same surfaces as a hand-built workflow.
 *
 * Graph remains source of truth: the LLM never emits SOL source;
 * SOL is generated FROM the graph by the existing emitter.
 */

/** Subset of NodeKind that Sol Man is allowed to emit. */
export type GeneratedNodeKind =
  | 'trigger'
  | 'let'
  | 'assign'
  | 'print'
  | 'return'
  | 'branch'
  | 'while'
  | 'forEach'
  | 'call';

export type GeneratedTriggerKind =
  | 'manual'
  | 'webhook'
  | 'timer'
  | 'event'
  | 'http';

export type GeneratedPrimitive = 'int' | 'float' | 'bool' | 'str';

export interface GeneratedNode {
  /** LLM-assigned id, mapped to a real nanoid at apply-time. */
  id: string;
  kind: GeneratedNodeKind;
  /** Optional human-friendly label override (unused by most kinds). */
  label?: string;

  // Trigger ---------------------------------------------------------
  triggerKind?: GeneratedTriggerKind;
  eventName?: string;
  samplePayload?: string;
  webhookPath?: string;
  cronExpr?: string;
  httpMethod?: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';
  httpPath?: string;

  // Variable bindings -----------------------------------------------
  varName?: string;
  varType?: GeneratedPrimitive;

  // Inline expression for the node's primary data input — interpreted
  // by graph.emit + interpret unchanged. Maps to:
  //   let/assign/print → 'value' port
  //   return           → 'value' port (when hasValue)
  //   while            → 'cond' port
  //   forEach          → 'array' port
  value?: string;

  // Branch ---------------------------------------------------------
  cond?: string;
  hasElse?: boolean;

  // Return ----------------------------------------------------------
  hasValue?: boolean;

  // forEach ---------------------------------------------------------
  iteratorName?: string;
  iteratorType?: GeneratedPrimitive;

  // Call ------------------------------------------------------------
  /** Function name to call. Sol Man may invent stubs; we surface as a
   *  warning in the assumptions list. */
  callTarget?: string;
}

export interface GeneratedEdge {
  from: string;
  to: string;
  /**
   * Source port id. Defaults to 'next' (control). For branch arms use
   * 'then' / 'else' / 'after'; for loop body use 'body' / 'after';
   * for data wires name the actual data output port.
   */
  fromPort?: string;
  /** Target port id. Defaults to 'prev' (control). */
  toPort?: string;
  /** Edge kind. Defaults to 'control'. */
  kind?: 'control' | 'data';
}

export interface GeneratedFrame {
  /** Display title shown above the frame on the canvas. */
  title: string;
  /** LLM ids of nodes that should sit inside this frame. */
  nodeIds: string[];
}

export interface GeneratedNote {
  text: string;
}

export interface GeneratedGraphSpec {
  /** Workflow-level metadata. */
  meta: {
    name: string;
    description: string;
  };
  nodes: GeneratedNode[];
  edges: GeneratedEdge[];
  frames?: GeneratedFrame[];
  notes?: GeneratedNote[];
  /** Plain-English notes about decisions the LLM made. Surfaced in
   *  the modal preview so users can sanity-check. */
  assumptions?: string[];
}

// =============================================================
//  API contract
// =============================================================

/**
 * Optional per-request provider config. When present, OVERRIDES any
 * server-side env-var configuration. This is the BYO-key path —
 * the user supplies their own provider/key in the SolFlow UI and we
 * proxy it to the LLM on each request without persisting it.
 *
 * Falls back to env vars when this field is absent or incomplete,
 * so self-hosted deployments with a shared key still work.
 */
export interface InlineProviderConfig {
  providerId: string;
  apiKey: string;
  model?: string;
  baseUrl?: string;
}

export interface GenerateRequestBody {
  /** Free-form prompt from the user. */
  prompt: string;
  /** Optional BYO-key config; takes priority over server env vars. */
  config?: InlineProviderConfig;
}

/**
 * Lightweight summary of one provider — shipped to the client when
 * configMissing is true so the modal can render a structured "set
 * one of these keys" panel.
 */
export interface ProviderSummary {
  id: string;
  name: string;
  envKey: string;
  envBase?: string;
  defaultModel: string;
}

/**
 * Discriminated failure category. Surfaces in the modal so we can
 * render specific guidance + decide whether to auto-retry.
 *
 * Reliability hardening pass: replaces the old single-field error
 * with a structured envelope so "Sol Man returned a non-JSON
 * response (HTTP 504)" becomes something the UI can actually
 * route on (gateway timeout → retry; validation failure → tweak
 * prompt; config missing → settings panel; etc.).
 */
export type GenerateErrorKind =
  /** No provider configured server-side AND nothing in BYO config. */
  | 'config_missing'
  /** User input rejected (empty / too long / bad shape). */
  | 'bad_request'
  /** Provider rejected the request — wrong key, auth, model not found, etc. */
  | 'provider_error'
  /** Provider returned 5xx OR our outer wrapper hit Vercel's gateway
   *  timeout. Auto-retry candidate (idempotent). */
  | 'gateway_timeout'
  /** Provider returned an empty completion. */
  | 'empty_response'
  /** Provider returned text we couldn't parse + couldn't repair into
   *  valid JSON. */
  | 'invalid_json'
  /** Parsed JSON but the validator (schema/structure/ids) rejected it. */
  | 'validation_failed'
  /** Network or fetch failure on the client side (CORS, offline, etc.). */
  | 'network'
  /** Generic unknown — last-resort bucket. */
  | 'unknown';

/** Which lifecycle stage the failure was detected at. */
export type GenerateStage =
  | 'request_validation'
  | 'provider_resolution'
  | 'provider_call'
  | 'json_extraction'
  | 'spec_validation'
  | 'unknown';

export type GenerateResponseBody =
  | {
      ok: true;
      spec: GeneratedGraphSpec;
      model: string;
      provider?: { id: string; name: string };
      usage?: {
        inputTokens: number;
        outputTokens: number;
      };
      /** Number of total LLM round-trips made (1 normal, 2 if the
       *  strict-retry kicked in). Surfaced so we can show "Recovered
       *  after retry" in the UI. */
      attempts?: number;
      /** True when the JSON-repair layer modified the raw provider
       *  output. Surfaced as a soft warning. */
      repairApplied?: boolean;
    }
  | {
      ok: false;
      error: string;
      /** Discriminated cause; defaults to 'unknown' on older
       *  payloads so legacy parsing keeps working. */
      kind?: GenerateErrorKind;
      /** Lifecycle stage the failure occurred at. */
      stage?: GenerateStage;
      /** How many round-trips we made before giving up. */
      attempts?: number;
      /** Whether the failure is worth re-trying — gateway_timeout,
       *  empty_response, transient provider_error are all true.
       *  invalid_json / validation_failed are typically true after
       *  ONE auto-retry; afterwards the modal prompts the user. */
      retryable?: boolean;
      /** Optional extra diagnostic detail — provider name, raw
       *  excerpt, repair pass log. Never includes API keys. */
      details?: {
        provider?: string;
        model?: string;
        httpStatus?: number;
        rawExcerpt?: string;
        repairLog?: string[];
      };
      /** True when the failure is configuration (missing key, etc.) so
       *  the client can show a "set up your provider" hint. */
      configMissing?: boolean;
      /** When configMissing, the full list of providers SolFlow knows
       *  about. The modal renders this as a checklist. */
      availableProviders?: ProviderSummary[];
    };
