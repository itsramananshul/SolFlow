// Live QA for the test-payload UX (Part B).
import { chromium } from 'playwright-core';
const URL = process.env.QA_URL ?? 'http://localhost:4173/';
const CHROME = process.env.CHROME ?? 'C:/Program Files/Google/Chrome/Application/chrome.exe';
const log = (...a) => console.log('[qa-payload]', ...a);
const results = {};
const consoleErrors = [];

const browser = await chromium.launch({ executablePath: CHROME, headless: true });
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
page.on('pageerror', (e) => consoleErrors.push(`pageerror: ${e.message}`));
page.on('console', (m) => {
  if (m.type() === 'error') {
    const t = m.text();
    if (!/Failed to load resource|net::ERR|favicon|loopback|Private Network|CORS/i.test(t)) consoleErrors.push(t);
  }
});
const body = async () => (await page.locator('body').innerText()).replace(/\s+/g, ' ');

async function loadWebhookSample() {
  await page.getByRole('button', { name: 'Samples' }).click();
  await page.waitForSelector('.dropdown-menu', { timeout: 5000 });
  const items = page.locator('.menu-item');
  const n = await items.count();
  for (let i = 0; i < n; i++) {
    const t = (await items.nth(i).locator('.menu-title').innerText()).trim();
    if (/webhook order/i.test(t)) {
      results['badge "Needs test payload"'] = /needs test payload/i.test(
        (await items.nth(i).locator('.menu-runnable').innerText()).trim(),
      );
      await items.nth(i).click();
      return true;
    }
  }
  return false;
}
try {
  await page.goto(URL, { waitUntil: 'networkidle' });
  await page.waitForSelector('.run-btn', { timeout: 15000 });
  const skip = page.locator('.welcome-backdrop .skip-btn');
  if (await skip.count()) { await skip.first().click(); await page.waitForSelector('.welcome-backdrop', { state: 'detached', timeout: 5000 }).catch(() => {}); }

  results['sample loads'] = await loadWebhookSample();
  await page.waitForTimeout(600);

  // Open Run modal (browser-sim default). Payload section should be present + prefilled.
  await page.locator('.run-btn').click();
  await page.waitForSelector('.modal', { timeout: 10000 });
  await page.waitForFunction(() => !document.body.innerText.includes('Running…'), { timeout: 15000 }).catch(() => {});
  await page.waitForTimeout(500);

  const editor = page.locator('.payload-editor');
  results['payload section visible'] = (await editor.count()) > 0;
  const prefill = (await editor.inputValue().catch(() => '')) || '';
  results['payload prefilled with sample'] = /"total"\s*:\s*1200/.test(prefill);

  // It ran with the prefilled payload → return 42? no, total=1200.
  results['ran with payload (shows 1200)'] = /\b1200\b/.test(await body());

  // Clear the payload and re-run → friendly "Missing test payload".
  await editor.fill('');
  await page.getByRole('button', { name: /Re-run|Running/ }).click().catch(() => {});
  await page.waitForFunction(() => !document.body.innerText.includes('Running…'), { timeout: 15000 }).catch(() => {});
  await page.waitForTimeout(500);
  const errText = await body();
  results['friendly "Missing test payload"'] = /missing test payload/i.test(errText);
  results['has How to fix'] = /how to fix/i.test(errText);
  results['has Add test payload button'] = (await page.getByRole('button', { name: /add test payload/i }).count()) > 0;
  results['source line marker present'] = (await page.locator('.cm-error-line').count()) > 0
    || (await page.locator('.cm-error-gutter-dot').count()) > 0;

  // Invalid JSON blocks the run with a validation message.
  await editor.fill('{ not json');
  await page.getByRole('button', { name: /Re-run|Running/ }).click().catch(() => {});
  await page.waitForTimeout(400);
  results['invalid JSON blocks with message'] = /invalid json/i.test(await body());

  // Add test payload button restores a valid payload.
  await editor.fill('{ "total": 1200 }');
  await page.getByRole('button', { name: /Re-run|Running/ }).click().catch(() => {});
  await page.waitForFunction(() => !document.body.innerText.includes('Running…'), { timeout: 15000 }).catch(() => {});
  await page.waitForTimeout(500);
  results['re-run with valid payload shows 1200'] = /\b1200\b/.test(await body());

  await page.screenshot({ path: 'scripts/qa-payload.png' });
  results['no console errors'] = consoleErrors.length === 0;
  if (consoleErrors.length) log('console:', JSON.stringify(consoleErrors.slice(0, 4)));
} catch (e) {
  log('ERROR:', e.message);
  await page.screenshot({ path: 'scripts/qa-payload-error.png' }).catch(() => {});
} finally {
  await browser.close();
}

let ok = true;
for (const [k, v] of Object.entries(results)) { log(`${v ? 'PASS' : 'FAIL'}  ${k}`); if (!v) ok = false; }
log(ok ? 'ALL PAYLOAD CHECKS PASS' : 'SOME CHECKS FAILED');
process.exitCode = ok ? 0 : 1;
