import type { SolWorkflow } from '@/graph/schema';
import { buildHello } from './hello';
import { buildMonitor } from './monitor';
import { buildOrchestration } from './orchestration';
import { buildPayments } from './payments';
import { buildEnterprise } from './enterprise';

export interface Sample {
  id: string;
  name: string;
  description: string;
  build: () => SolWorkflow;
}

export const SAMPLES: Sample[] = [
  {
    id: 'hello',
    name: 'Hello Person',
    description: 'Smallest meaningful sample — struct + multi-function call.',
    build: buildHello,
  },
  {
    id: 'monitor',
    name: 'System Monitor',
    description: 'While loop + struct + helper call. Modelled on jj_comp.sol.',
    build: buildMonitor,
  },
  {
    id: 'orchestration',
    name: 'Service Orchestration',
    description: 'Full orchestration pattern: imports, enum, struct, multiple helpers. Modelled on s1.sol.',
    build: buildOrchestration,
  },
  {
    id: 'payments',
    name: 'Payment Processing',
    description: 'Payment-domain orchestration: imports, enum, struct, evaluator + dispatcher. Modelled on s2.sol.',
    build: buildPayments,
  },
  {
    id: 'enterprise',
    name: 'Order Processing (large)',
    description: '40+ nodes across 5 framed regions, triggers, loops, branches, helper calls — readability stress test.',
    build: buildEnterprise,
  },
];
