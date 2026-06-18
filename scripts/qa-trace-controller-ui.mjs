// Live UI QA: Local Controller trace renders in the Run modal (Phase 2).
// Drives the built app in system Chrome against a RUNNING local controller
// (127.0.0.1:3939): loads a runnable sample, switches the run target to
// Local Controller, runs, opens the Trace tab, and verifies real rows.
import { chromium } from 'playwright-core';

const URL = process.env.QA_URL ?? 'http://localhost:4173/';
const CHROME =
  process.env.CHROME ?? 'C:/Program Files/Google/Chrome/Application/chrome.exe';
const log = (...a) => console.log('[qa-ctl-ui]', ...a);

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

  // Load the payment sample.
  await page.getByRole('button', { name: 'Samples' }).click();
  await page.waitForSelector('.dropdown-menu', { timeout: 5000 });
  const items = page.locator('.menu-item');
  const n = await items.count();
  for (let i = 0; i < n; i++) {
    const t = (await items.nth(i).locator('.menu-title').innerText()).trim();
    if (/payment/i.test(t)) { await items.nth(i).click(); log('loaded:', t); break; }
  }
  await page.waitForTimeout(600);

  // Open the Run modal.
  await page.locator('.run-btn').click();
  await page.waitForSelector('.target-toggle', { timeout: 10000 });

  // Switch run target to Local Controller.
  const localBtn = page.locator('.target-btn', { hasText: 'Local Controller' });
  await localBtn.waitFor({ timeout: 5000 });
  if (await localBtn.isDisabled()) throw new Error('Local Controller target is disabled (no URL configured)');
  await localBtn.click();
  log('selected Local Controller target');
  // Give the health check + run a moment.
  await page.waitForTimeout(1200);

  // Re-run in the new target.
  await page.getByRole('button', { name: /Re-run|Running/ }).click().catch(() => {});
  await page.waitForFunction(
    () => !document.body.innerText.includes('Running…'),
    { timeout: 20000 },
  ).catch(() => log('warn: still Running after timeout'));
  await page.waitForTimeout(800);

  // Open the Trace tab.
  await page.locator('.tab', { hasText: 'Trace' }).click();
  await page.waitForTimeout(400);

  const rowCount = await page.locator('.trace-row').count();
  const kinds = await page.locator('.trace-kind').allInnerTexts().catch(() => []);
  const kindset = [...new Set(kinds.map((k) => k.trim().toLowerCase()))];
  const header = await page.locator('.header-left .subtle').innerText().catch(() => '');
  log('run target header:', header.replace(/\s+/g, ' ').trim());
  log('controller trace rows:', rowCount);
  log('distinct kinds:', JSON.stringify(kindset));

  const firstRows = await page.locator('.trace-row').evaluateAll((els) =>
    els.slice(0, 6).map((el) => el.innerText.replace(/\s+/g, ' ').trim()),
  );
  firstRows.forEach((r) => log('  ', r));

  await page.screenshot({ path: 'scripts/qa-trace-controller.png' });

  const ok = rowCount > 0 && kindset.includes('call') && kindset.includes('return');
  log(ok ? 'PASS: Local Controller trace renders in the UI' : 'FAIL');
  process.exitCode = ok ? 0 : 1;
} catch (e) {
  log('ERROR:', e.message);
  await page.screenshot({ path: 'scripts/qa-trace-controller-error.png' }).catch(() => {});
  process.exitCode = 1;
} finally {
  await browser.close();
}
