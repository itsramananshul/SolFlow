import type { SolWorkflow } from '@/graph/schema';
import { buildSmokeTest } from './smokeTest';
import { buildHello } from './hello';
import { buildMonitor } from './monitor';
import { buildOrchestration } from './orchestration';
import { buildPayments } from './payments';
import { buildEnterprise } from './enterprise';

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
    description: 'Structure demo: struct + a helper function call. The runtime runs only the workflow body, so the helper call does not execute.',
    runnable: false,
    build: buildHello,
  },
  {
    id: 'monitor',
    name: 'System Monitor',
    description: 'Structure demo: while loop + struct + a helper call. The helper is not executed by the runtime.',
    runnable: false,
    build: buildMonitor,
  },
  {
    id: 'orchestration',
    name: 'Service Orchestration',
    description: 'Structure demo: imports, enum, struct, and multiple helpers. Needs capability providers and uses helper calls, so it does not run standalone.',
    runnable: false,
    build: buildOrchestration,
  },
  {
    id: 'payments',
    name: 'Payment Processing',
    description: 'Structure demo: imports, enum, struct, evaluator + dispatcher. Needs capability providers and uses helper calls, so it does not run standalone.',
    runnable: false,
    build: buildPayments,
  },
  {
    id: 'enterprise',
    name: 'Order Processing (large)',
    description: 'Large layout demo: triggers, loops, branches, framed regions, and helper calls. For readability, not standalone execution.',
    runnable: false,
    build: buildEnterprise,
  },
];
