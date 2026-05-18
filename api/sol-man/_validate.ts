/**
 * Defensive validator for LLM-generated GeneratedGraphSpec payloads.
 *
 * Even with a strict system prompt, models do produce malformed JSON
 * — missing fields, wrong types, references to non-existent ids, etc.
 * This module rejects unsafe payloads BEFORE they hit the client and
 * surfaces a single clear error message.
 *
 * Run-once on the server side. The client receives only validated
 * shapes (and trusts them at that point).
 */

import type {
  GeneratedEdge,
  GeneratedGraphSpec,
  GeneratedNode,
} from '../../src/sol-man/types';

const ALLOWED_KINDS = new Set([
  'trigger',
  'let',
  'assign',
  'print',
  'return',
  'branch',
  'while',
  'forEach',
  'call',
]);

const ALLOWED_TRIGGER_KINDS = new Set([
  'manual',
  'webhook',
  'timer',
  'event',
  'http',
]);

const ALLOWED_PRIMS = new Set(['int', 'float', 'bool', 'str']);

const ALLOWED_HTTP = new Set(['GET', 'POST', 'PUT', 'PATCH', 'DELETE']);

export class SpecValidationError extends Error {}

function isObject(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && !Array.isArray(v);
}

function asString(v: unknown, path: string): string {
  if (typeof v !== 'string') {
    throw new SpecValidationError(`${path}: expected string, got ${typeof v}`);
  }
  return v;
}

function optionalString(v: unknown, path: string): string | undefined {
  if (v === undefined || v === null) return undefined;
  if (typeof v !== 'string') {
    throw new SpecValidationError(`${path}: expected string, got ${typeof v}`);
  }
  return v;
}

function optionalBool(v: unknown, path: string): boolean | undefined {
  if (v === undefined || v === null) return undefined;
  if (typeof v !== 'boolean') {
    throw new SpecValidationError(`${path}: expected boolean, got ${typeof v}`);
  }
  return v;
}

function validateNode(raw: unknown, idx: number): GeneratedNode {
  if (!isObject(raw)) {
    throw new SpecValidationError(`nodes[${idx}]: must be an object`);
  }
  const id = asString(raw.id, `nodes[${idx}].id`);
  const kind = asString(raw.kind, `nodes[${idx}].kind`);
  if (!ALLOWED_KINDS.has(kind)) {
    throw new SpecValidationError(
      `nodes[${idx}].kind: "${kind}" is not an allowed node kind`,
    );
  }
  const node: GeneratedNode = {
    id,
    kind: kind as GeneratedNode['kind'],
  };
  // Per-kind optional fields — accept all known ones, ignore unknown.
  if (raw.triggerKind !== undefined) {
    const tk = asString(raw.triggerKind, `nodes[${idx}].triggerKind`);
    if (!ALLOWED_TRIGGER_KINDS.has(tk)) {
      throw new SpecValidationError(
        `nodes[${idx}].triggerKind: "${tk}" is not an allowed trigger kind`,
      );
    }
    node.triggerKind = tk as GeneratedNode['triggerKind'];
  }
  if (raw.varType !== undefined) {
    const t = asString(raw.varType, `nodes[${idx}].varType`);
    if (!ALLOWED_PRIMS.has(t)) {
      throw new SpecValidationError(
        `nodes[${idx}].varType: "${t}" is not a primitive type`,
      );
    }
    node.varType = t as GeneratedNode['varType'];
  }
  if (raw.iteratorType !== undefined) {
    const t = asString(raw.iteratorType, `nodes[${idx}].iteratorType`);
    if (!ALLOWED_PRIMS.has(t)) {
      throw new SpecValidationError(
        `nodes[${idx}].iteratorType: "${t}" is not a primitive type`,
      );
    }
    node.iteratorType = t as GeneratedNode['iteratorType'];
  }
  if (raw.httpMethod !== undefined) {
    const m = asString(raw.httpMethod, `nodes[${idx}].httpMethod`).toUpperCase();
    if (!ALLOWED_HTTP.has(m)) {
      throw new SpecValidationError(
        `nodes[${idx}].httpMethod: "${m}" is not an allowed HTTP method`,
      );
    }
    node.httpMethod = m as GeneratedNode['httpMethod'];
  }
  node.label = optionalString(raw.label, `nodes[${idx}].label`);
  node.eventName = optionalString(raw.eventName, `nodes[${idx}].eventName`);
  node.samplePayload = optionalString(raw.samplePayload, `nodes[${idx}].samplePayload`);
  node.webhookPath = optionalString(raw.webhookPath, `nodes[${idx}].webhookPath`);
  node.cronExpr = optionalString(raw.cronExpr, `nodes[${idx}].cronExpr`);
  node.httpPath = optionalString(raw.httpPath, `nodes[${idx}].httpPath`);
  node.varName = optionalString(raw.varName, `nodes[${idx}].varName`);
  node.value = optionalString(raw.value, `nodes[${idx}].value`);
  node.cond = optionalString(raw.cond, `nodes[${idx}].cond`);
  node.hasElse = optionalBool(raw.hasElse, `nodes[${idx}].hasElse`);
  node.hasValue = optionalBool(raw.hasValue, `nodes[${idx}].hasValue`);
  node.iteratorName = optionalString(raw.iteratorName, `nodes[${idx}].iteratorName`);
  node.callTarget = optionalString(raw.callTarget, `nodes[${idx}].callTarget`);
  return node;
}

function validateEdge(
  raw: unknown,
  idx: number,
  nodeIds: Set<string>,
): GeneratedEdge {
  if (!isObject(raw)) {
    throw new SpecValidationError(`edges[${idx}]: must be an object`);
  }
  const from = asString(raw.from, `edges[${idx}].from`);
  const to = asString(raw.to, `edges[${idx}].to`);
  if (!nodeIds.has(from)) {
    throw new SpecValidationError(
      `edges[${idx}].from: "${from}" does not match any node id`,
    );
  }
  if (!nodeIds.has(to)) {
    throw new SpecValidationError(
      `edges[${idx}].to: "${to}" does not match any node id`,
    );
  }
  const fromPort = optionalString(raw.fromPort, `edges[${idx}].fromPort`);
  const toPort = optionalString(raw.toPort, `edges[${idx}].toPort`);
  const kindRaw = optionalString(raw.kind, `edges[${idx}].kind`);
  if (kindRaw !== undefined && kindRaw !== 'control' && kindRaw !== 'data') {
    throw new SpecValidationError(
      `edges[${idx}].kind: "${kindRaw}" must be control or data`,
    );
  }
  return {
    from,
    to,
    fromPort,
    toPort,
    kind: (kindRaw as 'control' | 'data' | undefined) ?? 'control',
  };
}

export function validateSpec(raw: unknown): GeneratedGraphSpec {
  if (!isObject(raw)) {
    throw new SpecValidationError('Response root must be an object');
  }
  if (!isObject(raw.meta)) {
    throw new SpecValidationError('meta: must be an object');
  }
  const name = asString(raw.meta.name, 'meta.name');
  const description = asString(raw.meta.description, 'meta.description');

  if (!Array.isArray(raw.nodes)) {
    throw new SpecValidationError('nodes: must be an array');
  }
  const nodes: GeneratedNode[] = raw.nodes.map((n, i) => validateNode(n, i));
  // Reject duplicate ids — they'd corrupt the id-map on translation.
  const seen = new Set<string>();
  for (const n of nodes) {
    if (seen.has(n.id)) {
      throw new SpecValidationError(`duplicate node id: "${n.id}"`);
    }
    seen.add(n.id);
  }
  // Require at least one trigger as entry; the client emitter accepts
  // a `start` fallback too, but Sol Man's contract is event-driven.
  if (!nodes.some((n) => n.kind === 'trigger')) {
    throw new SpecValidationError(
      'spec must include at least one node with kind "trigger" as entry point',
    );
  }

  if (!Array.isArray(raw.edges)) {
    throw new SpecValidationError('edges: must be an array');
  }
  const nodeIds = new Set(nodes.map((n) => n.id));
  const edges = raw.edges.map((e, i) => validateEdge(e, i, nodeIds));

  // Frames + notes (both optional).
  let frames: GeneratedGraphSpec['frames'];
  if (raw.frames !== undefined) {
    if (!Array.isArray(raw.frames)) {
      throw new SpecValidationError('frames: must be an array when present');
    }
    frames = raw.frames.map((f, i) => {
      if (!isObject(f)) {
        throw new SpecValidationError(`frames[${i}]: must be an object`);
      }
      const title = asString(f.title, `frames[${i}].title`);
      if (!Array.isArray(f.nodeIds)) {
        throw new SpecValidationError(`frames[${i}].nodeIds: must be an array`);
      }
      const ids = f.nodeIds.map((id, j) => {
        const s = asString(id, `frames[${i}].nodeIds[${j}]`);
        if (!nodeIds.has(s)) {
          throw new SpecValidationError(
            `frames[${i}].nodeIds[${j}]: "${s}" does not match any node`,
          );
        }
        return s;
      });
      return { title, nodeIds: ids };
    });
  }

  let notes: GeneratedGraphSpec['notes'];
  if (raw.notes !== undefined) {
    if (!Array.isArray(raw.notes)) {
      throw new SpecValidationError('notes: must be an array when present');
    }
    notes = raw.notes.map((n, i) => {
      if (!isObject(n)) {
        throw new SpecValidationError(`notes[${i}]: must be an object`);
      }
      return { text: asString(n.text, `notes[${i}].text`) };
    });
  }

  let assumptions: string[] | undefined;
  if (raw.assumptions !== undefined) {
    if (!Array.isArray(raw.assumptions)) {
      throw new SpecValidationError('assumptions: must be an array of strings');
    }
    assumptions = raw.assumptions.map((s, i) =>
      asString(s, `assumptions[${i}]`),
    );
  }

  return {
    meta: { name, description },
    nodes,
    edges,
    frames,
    notes,
    assumptions,
  };
}
