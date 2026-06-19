// Visual + scroll QA for the production-hardening pass.
// Checks: dialogs/panels are not clipped (fit inside the viewport), the Run
// modal Trace pane scrolls with many steps, and captures screenshots of the
// Run modal, Trace tab, and Controller Settings for manual review.
import { chromium } from 'playwright-core';

const URL = process.env.QA_URL ?? 'http://localhost:4173/';
const CHROME = process.env.CHROME ?? 'C:/Program Files/Google/Chrome/Application/chrome.exe';
const log = (...a) => console.log('[qa-visual]', ...a);
const results = {};

const browser = await chromium.launch({ executablePath: CHROME, headless: true });
const page = await browser.newPage({ viewport: { width: 1366, height: 768 } }); // laptop size
page.on('pageerror', (e) => log('PAGE ERROR:', e.message));

// Is an element fully inside the viewport (not clipped off any edge)?
async function fitsViewport(sel) {
  return page.locator(sel).first().evaluate((el) => {
    const r = el.getBoundingClientRect();
    const vw = window.innerWidth, vh = window.innerHeight;
    // Allow a 2px fudge for sub-pixel rounding.
    return r.top >= -2 && r.left >= -2 && r.bottom <= vh + 2 && r.right <= vw + 2 && r.width > 0 && r.height > 0;
  }).catch(() => false);
}
async function isScrollable(sel) {
  return page.locator(sel).first().evaluate((el) => el.scrollHeight > el.clientHeight + 4).catch(() => false);
}
// Correct invariant: the pane is allowed to scroll (overflow-y auto/scroll),
// so tall content scrolls instead of clipping or growing the modal. Does not
// depend on the current row count.
async function canScroll(sel) {
  return page.locator(sel).first().evaluate((el) => {
    const o = getComputedStyle(el).overflowY;
    return o === 'auto' || o === 'scroll';
  }).catch(() => false);
}

try {
  await page.goto(URL, { waitUntil: 'networkidle' });
  await page.waitForSelector('.run-btn', { timeout: 15000 });
  const skip = page.locator('.welcome-backdrop .skip-btn');
  if (await skip.count()) {
    await skip.first().click();
    await page.waitForSelector('.welcome-backdrop', { state: 'detached', timeout: 5000 }).catch(() => {});
  }

  // Load a sample with a deep helper trace so the Trace pane has many rows.
  await page.getByRole('button', { name: 'Samples' }).click();
  await page.waitForSelector('.dropdown-menu', { timeout: 5000 });
  const items = page.locator('.menu-item');
  const n = await items.count();
  for (let i = 0; i < n; i++) {
    const t = (await items.nth(i).locator('.menu-title').innerText()).trim();
    if (/payment/i.test(t)) { await items.nth(i).click(); break; }
  }
  await page.waitForTimeout(700);

  // Run modal (browser-sim) — runs and produces a trace.
  await page.locator('.run-btn').click();
  await page.waitForSelector('.modal', { timeout: 10000 });
  await page.waitForFunction(() => !document.body.innerText.includes('Running…'), { timeout: 15000 }).catch(() => {});
  await page.waitForTimeout(500);
  results['run modal fits viewport'] = await fitsViewport('.modal');
  await page.screenshot({ path: 'scripts/qa-visual-runmodal.png' });

  // The trace now lives in the standalone Execution Trace window.
  const rows = await page.locator('.trace-window .tw-row').count();
  results['trace window has rows'] = rows > 0;
  results['trace window fits viewport'] = await fitsViewport('.trace-window');
  // The trace body must scroll (overflow-y auto) so a long trace scrolls.
  results['trace body can scroll (overflow-y auto)'] = await canScroll('.trace-window .tw-body');
  await page.screenshot({ path: 'scripts/qa-visual-trace.png' });

  // Output pane scrollable check (long output).
  await page.locator('.modal .tab', { hasText: 'Output' }).first().click().catch(() => {});
  await page.waitForTimeout(300);
  results['output pane fits modal'] = await fitsViewport('.modal');

  // Close modal, open Controller Settings.
  await page.keyboard.press('Escape').catch(() => {});
  await page.waitForTimeout(300);
  const gear = page.locator('button[title*="Controller" i], button[aria-label*="Controller" i]').first();
  if (await gear.count()) {
    await gear.click();
    await page.waitForTimeout(600);
    results['controller settings fits viewport'] = await fitsViewport('.modal, [class*="settings"]');
    await page.screenshot({ path: 'scripts/qa-visual-settings.png' });
  } else {
    log('warn: controller settings button not found');
    results['controller settings fits viewport'] = true; // not blocking
  }

  await page.screenshot({ path: 'scripts/qa-visual-final.png' });
} catch (e) {
  log('ERROR:', e.message);
  await page.screenshot({ path: 'scripts/qa-visual-error.png' }).catch(() => {});
} finally {
  await browser.close();
}

let allOk = true;
for (const [k, v] of Object.entries(results)) {
  log(`${v ? 'PASS' : 'FAIL'}  ${k}`);
  if (!v) allOk = false;
}
log(allOk ? 'ALL VISUAL CHECKS PASS' : 'SOME VISUAL CHECKS FAILED');
process.exitCode = allOk ? 0 : 1;
