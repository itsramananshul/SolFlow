/**
 * Shared z-index counter for floating IDE panels (Run, Trace, ...).
 * Clicking a panel calls `nextPanelZ()` and applies the result so it rises
 * above the others — a single source of truth keeps stacking deterministic.
 * Starts above `--sf-z-modal` (100).
 */
let topZ = 100;

export function nextPanelZ(): number {
  topZ += 1;
  return topZ;
}
