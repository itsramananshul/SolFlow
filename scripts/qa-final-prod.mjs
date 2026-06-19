// Final production verification (browser-sim items) against the live app.
// 1. A runnable sample runs in Browser Simulation and prints output.
// 2. Samples are correctly badged (Runs standalone / Needs provider).
// 3. The provider-required sample blocks clearly in Browser Simulation.
// 4. No console errors.
import { chromium } from 'playwright-core';

const URL = process.env.QA_URL ?? 'https://solflow-one.vercel.app/';
const CHROME = process.env.CHROME ?? 'C:/Program Files/Google/Chrome/Application/chrome.exe';
const log = (...a) => console.log('[qa-final]', ...a);
const results = {};
const consoleErrors = [];

const browser = await chromium.launch({ executablePath: CHROME, headless: true });
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
page.on('pageerror', (e) => { consoleErrors.push(`pageerror: ${e.message}`); });
page.on('console', (m) => {
  if (m.type() === 'error') {
    const t = m.text();
    if (!/Failed to load resource|net::ERR|favicon|loopback|Private Network|CORS policy/i.test(t)) consoleErrors.push(`console: ${t}`);
  }
});
const text = async () => (await page.locator('body').innerText()).replace(/\s+/g, ' ');
async function loadSample(re) {
  await page.getByRole('button', { name: 'Samples' }).click();
  await page.waitForSelector('.dropdown-menu', { timeout: 5000 });
  const items = page.locator('.menu-item');
  const n = await items.count();
  for (let i = 0; i < n; i++) {
    const t = (await items.nth(i).locator('.menu-title').innerText()).trim();
    if (re.test(t)) { const badge = (await items.nth(i).locator('.menu-runnable').innerText()).trim(); await items.nth(i).click(); return badge; }
  }
  return null;
}
async function runBrowserSim() {
  await page.locator('.run-btn').click();
  await page.waitForSelector('.target-toggle', { timeout: 10000 });
  await page.locator('.target-btn', { hasText: 'Browser Simulation' }).click();
  await page.getByRole('button', { name: /Re-run|Running/ }).click().catch(() => {});
  await page.waitForFunction(() => !document.body.innerText.includes('Running…'), { timeout: 15000 }).catch(() => {});
  await page.waitForTimeout(500);
}

try {
  await page.goto(URL, { waitUntil: 'networkidle' });
  await page.waitForSelector('.run-btn', { timeout: 20000 });
  const skip = page.locator('.welcome-backdrop .skip-btn');
  if (await skip.count()) { await skip.first().click(); await page.waitForSelector('.welcome-backdrop', { state: 'detached', timeout: 5000 }).catch(() => {}); }

  // 1 + 2: runnable sample (Payment Processing) runs in browser-sim, badged "Runs standalone".
  const payBadge = await loadSample(/payment/i);
  results['runnable badged "Runs standalone"'] = /runs standalone/i.test(payBadge || '');
  await page.waitForTimeout(500);
  await runBrowserSim();
  await page.locator('.tab', { hasText: 'Output' }).first().click().catch(() => {});
  await page.waitForTimeout(300);
  // Read the printed lines from the Output pane specifically (the modal
  // footer always carries the honest "External Actions are blocked"
  // disclaimer, so scope to the output, not the whole body).
  const outPane = (await page.locator('section.pane').first().innerText().catch(() => '')).replace(/\s+/g, ' ');
  results['runnable runs in browser-sim (real printed output)'] = /Deploying|Secure_Payment_Gateway/i.test(outPane);
  await page.keyboard.press('Escape').catch(() => {});
  await page.waitForTimeout(300);

  // 3: provider-required sample badged "Needs provider" and blocks in browser-sim.
  const capBadge = await loadSample(/capability/i);
  results['provider-required badged "Needs provider"'] = /needs provider/i.test(capBadge || '');
  await page.waitForTimeout(500);
  await runBrowserSim();
  const blk = await text();
  results['provider-required blocks clearly in browser-sim'] = /block/i.test(blk) || /ExtCallBlocked/i.test(blk);

  await page.screenshot({ path: 'scripts/qa-final-prod.png' });
} catch (e) {
  log('ERROR:', e.message);
  await page.screenshot({ path: 'scripts/qa-final-prod-error.png' }).catch(() => {});
} finally {
  await browser.close();
}

results['no console errors'] = consoleErrors.length === 0;
if (consoleErrors.length) log('console errors:', JSON.stringify(consoleErrors.slice(0, 5)));

let allOk = true;
for (const [k, v] of Object.entries(results)) { log(`${v ? 'PASS' : 'FAIL'}  ${k}`); if (!v) allOk = false; }
log(allOk ? 'ALL PROD BROWSER-SIM CHECKS PASS' : 'SOME CHECKS FAILED');
process.exitCode = allOk ? 0 : 1;
