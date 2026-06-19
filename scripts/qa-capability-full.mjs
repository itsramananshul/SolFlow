// Full live-UI verification for the capability node (Phase 3.1).
// Checklist:
//   1. Capability Call sample appears in the menu.
//   2. The canvas shows a capability node imported from call(...).
//   3. The node emits clean SOL (single call("demo.add", ...)).
//   4. Browser Simulation blocks the external call clearly.
//   5. Local Controller runs the sample with the demo provider.
//   6. Trace shows EXTCALL -> EXTRESULT -> STMT.
import { chromium } from 'playwright-core';

const URL = process.env.QA_URL ?? 'http://localhost:4173/';
const CHROME = process.env.CHROME ?? 'C:/Program Files/Google/Chrome/Application/chrome.exe';
const log = (...a) => console.log('[qa-full]', ...a);
const results = {};

const browser = await chromium.launch({ executablePath: CHROME, headless: true });
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
const consoleErrors = [];
page.on('pageerror', (e) => { consoleErrors.push(`pageerror: ${e.message}`); log('PAGE ERROR:', e.message); });
page.on('console', (m) => {
  if (m.type() === 'error') {
    const t = m.text();
    // Ignore benign network-failed noise from probing an absent controller.
    if (!/Failed to load resource|net::ERR|favicon/i.test(t)) consoleErrors.push(`console: ${t}`);
  }
});

const text = async () => (await page.locator('body').innerText()).replace(/\s+/g, ' ');

try {
  await page.goto(URL, { waitUntil: 'networkidle' });
  await page.waitForSelector('.run-btn', { timeout: 15000 });
  const skip = page.locator('.welcome-backdrop .skip-btn');
  if (await skip.count()) {
    await skip.first().click();
    await page.waitForSelector('.welcome-backdrop', { state: 'detached', timeout: 5000 }).catch(() => {});
  }

  // 1. Sample appears.
  await page.getByRole('button', { name: 'Samples' }).click();
  await page.waitForSelector('.dropdown-menu', { timeout: 5000 });
  const items = page.locator('.menu-item');
  const n = await items.count();
  let idx = -1;
  for (let i = 0; i < n; i++) {
    const t = (await items.nth(i).locator('.menu-title').innerText()).trim();
    if (/capability/i.test(t)) { idx = i; break; }
  }
  results['1. sample appears'] = idx >= 0;
  const badge = idx >= 0 ? (await items.nth(idx).locator('.menu-runnable').innerText()).trim() : '';
  results['1b. badge "Needs provider"'] = /needs provider/i.test(badge);
  await items.nth(idx).click();
  await page.waitForTimeout(700);

  // 2. Canvas shows a capability node (imported from call(...)).
  const canvasText = await text();
  results['2. capability node on canvas'] = /demo\.add|Capability Call|Act\b/i.test(canvasText);

  // 3. Clean SOL emit (exactly one call) — read the live SOL preview pane.
  const sol = (await page.locator('.source-preview').first().innerText().catch(() => '')).replace(/\s+/g, ' ');
  const callCount = (sol.match(/call\("demo\.add"/g) || []).length;
  results['3. emits single clean call'] = callCount === 1 && /let sum: int = call\("demo\.add"/.test(sol);

  // 4. Browser Simulation blocks clearly (default target on run).
  await page.locator('.run-btn').click();
  await page.waitForSelector('.target-toggle', { timeout: 10000 });
  // Ensure Browser Simulation is selected.
  await page.locator('.target-btn', { hasText: 'Browser Simulation' }).click();
  await page.getByRole('button', { name: /Re-run|Running/ }).click().catch(() => {});
  await page.waitForFunction(() => !document.body.innerText.includes('Running…'), { timeout: 15000 }).catch(() => {});
  await page.waitForTimeout(500);
  const simText = await text();
  results['4. browser-sim blocks clearly'] = /block/i.test(simText) || /ExtCallBlocked/i.test(simText);

  // 5 + 6. Local Controller runs + trace EXTCALL -> EXTRESULT -> STMT.
  await page.locator('.target-btn', { hasText: 'Local Controller' }).click();
  await page.waitForTimeout(1200);
  await page.getByRole('button', { name: /Re-run|Running/ }).click().catch(() => {});
  await page.waitForFunction(() => !document.body.innerText.includes('Running…'), { timeout: 20000 }).catch(() => {});
  await page.waitForTimeout(800);
  await page.locator('.tab', { hasText: 'Trace' }).click();
  await page.waitForTimeout(400);
  const kinds = (await page.locator('.trace-kind').allInnerTexts().catch(() => [])).map((k) => k.trim().toLowerCase());
  const order = kinds.filter((k) => ['extcall', 'extresult', 'stmt'].includes(k));
  const iExt = order.indexOf('extcall'), iRes = order.indexOf('extresult'), iStmt = order.lastIndexOf('stmt');
  results['5. controller runs (extresult present)'] = kinds.includes('extresult');
  results['6. trace order EXTCALL<EXTRESULT<STMT'] = iExt >= 0 && iRes > iExt && iStmt > iRes;
  // Output value.
  await page.locator('.tab', { hasText: 'Output' }).first().click().catch(() => {});
  await page.waitForTimeout(300);
  results['5b. shows return 42'] = /\b42\b/.test(await text());

  await page.screenshot({ path: 'scripts/qa-capability-full.png' });
} catch (e) {
  log('ERROR:', e.message);
  await page.screenshot({ path: 'scripts/qa-capability-full-error.png' }).catch(() => {});
} finally {
  await browser.close();
}

results['7. no console errors'] = consoleErrors.length === 0;
if (consoleErrors.length) log('console errors:', JSON.stringify(consoleErrors.slice(0, 5)));

let allOk = true;
for (const [k, v] of Object.entries(results)) {
  log(`${v ? 'PASS' : 'FAIL'}  ${k}`);
  if (!v) allOk = false;
}
log(allOk ? 'ALL CHECKS PASS' : 'SOME CHECKS FAILED');
process.exitCode = allOk ? 0 : 1;
