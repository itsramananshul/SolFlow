// Live UI QA (Phase 3.1): the capability node runs end to end through the
// UI. Loads the "Capability Call" sample, switches to Local Controller
// (which has the demo provider registered), runs, and verifies the run
// returns 42 with extcall/extresult trace rows.
import { chromium } from 'playwright-core';

const URL = process.env.QA_URL ?? 'http://localhost:4173/';
const CHROME =
  process.env.CHROME ?? 'C:/Program Files/Google/Chrome/Application/chrome.exe';
const log = (...a) => console.log('[qa-cap-ui]', ...a);

const browser = await chromium.launch({ executablePath: CHROME, headless: true });
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
page.on('pageerror', (e) => log('PAGE ERROR:', e.message));

try {
  await page.goto(URL, { waitUntil: 'networkidle' });
  await page.waitForSelector('.run-btn', { timeout: 15000 });
  const skip = page.locator('.welcome-backdrop .skip-btn');
  if (await skip.count()) {
    await skip.first().click();
    await page.waitForSelector('.welcome-backdrop', { state: 'detached', timeout: 5000 }).catch(() => {});
  }

  // Load the Capability Call sample.
  await page.getByRole('button', { name: 'Samples' }).click();
  await page.waitForSelector('.dropdown-menu', { timeout: 5000 });
  const items = page.locator('.menu-item');
  const n = await items.count();
  let loaded = false;
  for (let i = 0; i < n; i++) {
    const t = (await items.nth(i).locator('.menu-title').innerText()).trim();
    if (/capability/i.test(t)) { await items.nth(i).click(); loaded = true; log('loaded:', t); break; }
  }
  if (!loaded) throw new Error('Capability Call sample not found in menu');
  await page.waitForTimeout(700);

  // The SOL preview should contain the emitted capability call.
  const sol = (await page.locator('body').innerText()).replace(/\s+/g, ' ');
  log('emits call("demo.add"):', /call\("demo\.add"/.test(sol));

  // Run on Local Controller.
  await page.locator('.run-btn').click();
  await page.waitForSelector('.target-toggle', { timeout: 10000 });
  await page.locator('.target-btn', { hasText: 'Local Controller' }).click();
  await page.waitForTimeout(1200);
  await page.getByRole('button', { name: /Re-run|Running/ }).click().catch(() => {});
  await page.waitForFunction(
    () => !document.body.innerText.includes('Running…'),
    { timeout: 20000 },
  ).catch(() => log('warn: still Running'));
  await page.waitForTimeout(800);

  // Trace tab.
  await page.locator('.tab', { hasText: 'Trace' }).click();
  await page.waitForTimeout(400);
  const kinds = await page.locator('.trace-kind').allInnerTexts().catch(() => []);
  const kindset = [...new Set(kinds.map((k) => k.trim().toLowerCase()))];
  log('trace kinds:', JSON.stringify(kindset));

  // Output tab for the return value / printed line.
  await page.locator('.tab', { hasText: 'Output' }).first().click().catch(() => {});
  await page.waitForTimeout(300);
  const bodyText = (await page.locator('body').innerText()).replace(/\s+/g, ' ');
  const has42 = /\b42\b/.test(bodyText);
  log('shows 42 (return / output):', has42);

  // Status bar should report no validation errors for the sample.
  const errBadge = await page.locator('.status-bar, [class*="status"]').allInnerTexts().catch(() => []);
  const noErrors = !/\b1 error\b|\b[2-9] errors\b/.test(errBadge.join(' '));
  log('no validation errors:', noErrors);

  await page.screenshot({ path: 'scripts/qa-capability.png' });

  const ok = kindset.includes('extcall') && kindset.includes('extresult') && has42 && noErrors;
  log(ok ? 'PASS: capability node ran end to end via the UI' : 'INCONCLUSIVE: see screenshot');
  process.exitCode = ok ? 0 : 1;
} catch (e) {
  log('ERROR:', e.message);
  await page.screenshot({ path: 'scripts/qa-capability-error.png' }).catch(() => {});
  process.exitCode = 1;
} finally {
  await browser.close();
}
