// Live QA for the real execution trace (Phase 2).
// Drives the built app in system Chrome: loads a runnable sample,
// runs it in Browser Simulation, opens the Trace tab, and verifies
// real trace rows render (step kind, function, source line).
import { chromium } from 'playwright-core';

const URL = process.env.QA_URL ?? 'http://localhost:4173/';
const CHROME =
  process.env.CHROME ?? 'C:/Program Files/Google/Chrome/Application/chrome.exe';

const log = (...a) => console.log('[qa-trace]', ...a);

const browser = await chromium.launch({ executablePath: CHROME, headless: true });
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
page.on('pageerror', (e) => log('PAGE ERROR:', e.message));

try {
  await page.goto(URL, { waitUntil: 'networkidle' });
  await page.waitForSelector('.run-btn', { timeout: 15000 });

  // Dismiss the welcome overlay if present (it intercepts clicks).
  const skip = page.locator('.welcome-backdrop .skip-btn');
  if (await skip.count()) {
    await skip.first().click();
    await page.waitForSelector('.welcome-backdrop', { state: 'detached', timeout: 5000 }).catch(() => {});
    log('dismissed welcome overlay');
  }

  // Open the Samples menu and load the payments sample (runnable, has a
  // helper + deploy path that prints).
  await page.getByRole('button', { name: 'Samples' }).click();
  await page.waitForSelector('.dropdown-menu', { timeout: 5000 });
  const items = page.locator('.menu-item');
  const count = await items.count();
  let loaded = null;
  for (let i = 0; i < count; i++) {
    const title = (await items.nth(i).locator('.menu-title').innerText()).trim();
    if (/payment/i.test(title)) { await items.nth(i).click(); loaded = title; break; }
  }
  if (!loaded) {
    // fall back to the first runnable sample
    await items.first().click();
    loaded = (await items.first().locator('.menu-title').innerText().catch(() => 'first')).trim();
  }
  log('loaded sample:', loaded);
  await page.waitForTimeout(800);

  // Run.
  await page.locator('.run-btn').click();
  await page.waitForSelector('.tabs', { timeout: 10000 });
  // Wait for the run to finish (Running… disappears).
  await page.waitForFunction(
    () => !document.body.innerText.includes('Running…'),
    { timeout: 15000 },
  ).catch(() => log('warn: still showing Running after timeout'));
  await page.waitForTimeout(500);

  // Open the Trace tab.
  const traceTab = page.locator('.tab', { hasText: 'Trace' });
  await traceTab.click();
  await page.waitForTimeout(300);

  const rowCount = await page.locator('.trace-row').count();
  log('trace rows rendered:', rowCount);

  const kinds = await page.locator('.trace-kind').allInnerTexts().catch(() => []);
  const kindset = [...new Set(kinds.map((k) => k.trim().toLowerCase()))];
  log('distinct step kinds:', JSON.stringify(kindset));

  const firstRows = await page.locator('.trace-row').evaluateAll((els) =>
    els.slice(0, 6).map((el) => el.innerText.replace(/\s+/g, ' ').trim()),
  );
  log('first rows:');
  firstRows.forEach((r) => log('  ', r));

  // Badge on the tab shows the count.
  const badge = await page.locator('.tab-badge').innerText().catch(() => '(none)');
  log('trace tab badge:', badge);

  await page.screenshot({ path: 'scripts/qa-trace.png', fullPage: false });
  log('screenshot: scripts/qa-trace.png');

  const ok = rowCount > 0;
  log(ok ? 'PASS: trace rows render in the browser UI' : 'FAIL: no trace rows');
  process.exitCode = ok ? 0 : 1;
} catch (e) {
  log('ERROR:', e.message);
  await page.screenshot({ path: 'scripts/qa-trace-error.png' }).catch(() => {});
  process.exitCode = 1;
} finally {
  await browser.close();
}
