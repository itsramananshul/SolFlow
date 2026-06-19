import type { SolWorkflow } from '@/graph/schema';
import { buildSmokeTest } from './smokeTest';
import { buildHello } from './hello';
import { buildMonitor } from './monitor';
import { buildOrchestration } from './orchestration';
import { buildPayments } from './payments';
import { buildEnterprise } from './enterprise';
import { buildCapabilityDemo } from './capabilityDemo';
import { buildWebhookOrder } from './webhookOrder';

export interface Sample {
  id: string;
  name: string;
  description: string;
  /**
   * True when the whole program lives in the workflow body, so it runs
   * end to end with no providers. False when it relies on helper
   * functions or external capabilities the runtime does not execute on
   * its own (calls to those fail with a clear runtime error). The menu
   * surfaces this so a structure demo is not mistaken for a runnable
   * program.
   */
  runnable: boolean;
  /**
   * True when the program makes an external capability `call(...)` and so
   * runs end to end only on a controller with a matching provider
   * registered. In Browser Simulation it is intentionally blocked. Mutually
   * exclusive with `runnable` (which means browser-sim standalone).
   */
  requiresProvider?: boolean;
  /**
   * True when the workflow reads event data (`payload`) from a trigger or
   * webhook, so a manual run needs a test payload. Such a sample ships a
   * default example payload in `samplePayload` and is badged "Needs test
   * payload"; the Run panel prefills the payload editor with it.
   */
  requiresPayload?: boolean;
  /** Default example payload (JSON string) for a `requiresPayload` sample. */
  samplePayload?: string;
  build: () => SolWorkflow;
}

export const SAMPLES: Sample[] = [
  {
    id: 'smoke-test',
    name: 'Smoke Test',
    description: 'Self-contained: prints and returns a value. Runs end to end in Browser Simulation and through a controller.',
    runnable: true,
    build: buildSmokeTest,
  },
  {
    id: 'hello',
    name: 'Hello Person',
    description: 'Runs: a struct and a helper function (print_person) the workflow calls to print its fields.',
    runnable: true,
    build: buildHello,
  },
  {
    id: 'monitor',
    name: 'System Monitor',
    description: 'Runs: a while loop over nodes plus a helper (assess) that checks each against a limit and alerts.',
    runnable: true,
    build: buildMonitor,
  },
  {
    id: 'orchestration',
    name: 'Service Orchestration',
    description: 'Runs: enum, struct, and helper functions (start_service / stop_service) the workflow drives.',
    runnable: true,
    build: buildOrchestration,
  },
  {
    id: 'payments',
    name: 'Payment Processing',
    description: 'Runs: a full helper chain (process_transaction calls evaluate_payment, returns a PaymentStatus enum, then deploys on approval).',
    runnable: true,
    build: buildPayments,
  },
  {
    id: 'capability-demo',
    name: 'Capability Call',
    description: 'Calls an external provider: call("demo.add", { a: 20, b: 22 }). Runs for real on a controller with the demo provider registered; blocked in Browser Simulation.',
    runnable: false,
    requiresProvider: true,
    build: buildCapabilityDemo,
  },
  {
    id: 'webhook-order',
    name: 'Webhook Order',
    description: 'Reads payload.total from a webhook/trigger event. Needs a test payload to run manually; ships one ({ "total": 1200 }) prefilled in the Run panel.',
    runnable: false,
    requiresPayload: true,
    samplePayload: '{\n  "total": 1200\n}',
    build: buildWebhookOrder,
  },
  {
    id: 'enterprise',
    name: 'Order Processing (large)',
    description: 'Large layout demo: triggers, framed regions, loops, and branches. Built for readability; its graph is not fully wired for execution yet.',
    runnable: false,
    build: buildEnterprise,
  },
];
