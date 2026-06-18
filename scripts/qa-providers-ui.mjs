// Live UI QA: Controller Settings lists the real registered providers.
// Connects the editor to a running Local Controller (which has the demo
// provider registered) and asserts the provider row renders.
import { chromium } from 'playwright-core';

const URL = process.env.QA_URL ?? 'http://localhost:4173/';
const CHROME =
  process.env.CHROME ?? 'C:/Program Files/Google/Chrome/Application/chrome.exe';
const log = (...a) => console.log('[qa-prov]', ...a);

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

  // Open the Run modal and select Local Controller so the store connects
  // and fetches providers.
  await page.locator('.run-btn').click();
  await page.waitForSelector('.target-toggle', { timeout: 10000 });
  const localBtn = page.locator('.target-btn', { hasText: 'Local Controller' });
  await localBtn.click();
  await page.waitForTimeout(1500);
  // Close the run modal.
  await page.keyboard.press('Escape').catch(() => {});
  await page.waitForTimeout(300);

  // Open Controller Settings (gear / settings affordance).
  const settingsBtn = page
    .locator('button[title*="Controller" i], button[aria-label*="Controller" i], button[title*="settings" i]')
    .first();
  if (await settingsBtn.count()) {
    await settingsBtn.click();
  } else {
    // Fallback: some builds expose settings inside the run modal footer.
    log('warn: settings button not found by title; trying text');
    await page.getByText('Controller Settings', { exact: false }).first().click().catch(() => {});
  }
  await page.waitForTimeout(800);

  const bodyText = (await page.locator('body').innerText()).replace(/\s+/g, ' ');
  const hasSection = /Registered providers/i.test(bodyText);
  const hasDemo = /demo/.test(bodyText) && /127\.0\.0\.1:8099/.test(bodyText);
  log('has "Registered providers" section:', hasSection);
  log('shows demo provider + url:', hasDemo);

  await page.screenshot({ path: 'scripts/qa-providers.png' });
  const ok = hasSection && hasDemo;
  log(ok ? 'PASS: provider listing renders in the UI' : 'INCONCLUSIVE: see screenshot');
  process.exitCode = ok ? 0 : 1;
} catch (e) {
  log('ERROR:', e.message);
  await page.screenshot({ path: 'scripts/qa-providers-error.png' }).catch(() => {});
  process.exitCode = 1;
} finally {
  await browser.close();
}
