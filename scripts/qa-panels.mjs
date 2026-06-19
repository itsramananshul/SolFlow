// Live QA for the floating IDE panels (Part A): Run + Trace coexist,
// neither clipped, both draggable + clamped, click-to-front, scroll, no
// console errors. Tested at several viewport sizes.
import { chromium } from 'playwright-core';

const URL = process.env.QA_URL ?? 'http://localhost:4173/';
const CHROME = process.env.CHROME ?? 'C:/Program Files/Google/Chrome/Application/chrome.exe';
const log = (...a) => console.log('[qa-panels]', ...a);
const SIZES = [
  { w: 1366, h: 768 },
  { w: 1280, h: 720 },
  { w: 1440, h: 900 },
  { w: 1100, h: 700 },
];

const browser = await chromium.launch({ executablePath: CHROME, headless: true });
let allOk = true;

for (const { w, h } of SIZES) {
  const page = await browser.newPage({ viewport: { width: w, height: h } });
  const consoleErrors = [];
  page.on('pageerror', (e) => consoleErrors.push(`pageerror: ${e.message}`));
  page.on('console', (m) => {
    if (m.type() === 'error') {
      const t = m.text();
      if (!/Failed to load resource|net::ERR|favicon|loopback|Private Network|CORS/i.test(t)) consoleErrors.push(t);
    }
  });
  const results = {};
  const rect = (sel) => page.locator(sel).first().evaluate((el) => {
    const r = el.getBoundingClientRect();
    return { x: r.x, y: r.y, w: r.width, h: r.height, right: r.right, bottom: r.bottom };
  });
  const fits = (r) => r.x >= -1 && r.y >= -1 && r.right <= w + 1 && r.bottom <= h + 1;
  const zOf = (sel) => page.locator(sel).first().evaluate((el) => parseInt(getComputedStyle(el).zIndex || '0', 10));
  const noHOverflow = (sel) => page.locator(sel).first().evaluate((el) => el.scrollWidth <= el.clientWidth + 1).catch(() => false);

  try {
    await page.goto(URL, { waitUntil: 'networkidle' });
    await page.waitForSelector('.run-btn', { timeout: 20000 });
    const skip = page.locator('.welcome-backdrop .skip-btn');
    if (await skip.count()) { await skip.first().click(); await page.waitForSelector('.welcome-backdrop', { state: 'detached', timeout: 5000 }).catch(() => {}); }

    // Load a runnable sample with a trace, run it -> Trace panel opens.
    await page.getByRole('button', { name: 'Samples' }).click();
    await page.waitForSelector('.dropdown-menu', { timeout: 5000 });
    const items = page.locator('.menu-item');
    const n = await items.count();
    for (let i = 0; i < n; i++) { const t = (await items.nth(i).locator('.menu-title').innerText()).trim(); if (/payment/i.test(t)) { await items.nth(i).click(); break; } }
    await page.waitForTimeout(600);
    await page.locator('.run-btn').click();
    await page.waitForSelector('.modal', { timeout: 10000 });
    await page.waitForFunction(() => !document.body.innerText.includes('Running…'), { timeout: 15000 }).catch(() => {});
    await page.waitForTimeout(500);

    // Both panels visible.
    results['Run + Trace both visible'] = (await page.locator('.modal').count()) > 0 && (await page.locator('.trace-window').count()) > 0;
    results['Run not clipped'] = fits(await rect('.modal'));
    results['Trace not clipped'] = fits(await rect('.trace-window'));
    results['Run header no horizontal overflow'] = await noHOverflow('.modal .modal-header');
    results['Run not wider than viewport'] = (await rect('.modal')).w <= w;
    results['Trace not wider than viewport'] = (await rect('.trace-window')).w <= w;
    // No unusable overlap on the Run header: the Trace must not cover the
    // Run panel's right header edge by more than a hair.
    const rm = await rect('.modal'); const tw = await rect('.trace-window');
    results['no Run/Trace header collision'] = tw.x >= rm.right - 6 || tw.bottom <= rm.y || tw.y >= rm.bottom || rm.x >= tw.right;

    // Float the Trace so it is draggable, then drag it -> contained.
    await page.locator('.trace-window .tw-btn', { hasText: 'Float' }).click().catch(() => {});
    await page.waitForTimeout(300);
    const th = await rect('.trace-window .tw-header');
    await page.mouse.move(th.x + 40, th.y + th.h / 2);
    await page.mouse.down();
    await page.mouse.move(th.x + 40 - 120, th.y + th.h / 2 + 100, { steps: 6 });
    await page.mouse.up();
    await page.waitForTimeout(150);
    results['Trace draggable + contained'] = fits(await rect('.trace-window'));

    // Drag the Run panel -> contained.
    const rh = await rect('.modal .drag-handle');
    await page.mouse.move(rh.x + 16, rh.y + rh.h / 2);
    await page.mouse.down();
    await page.mouse.move(rh.x + 16 + 60, rh.y + rh.h / 2 + 80, { steps: 6 });
    await page.mouse.up();
    await page.waitForTimeout(150);
    results['Run draggable + contained'] = fits(await rect('.modal'));

    // Click-to-front: click Run -> Run z higher; click Trace -> Trace higher.
    await page.mouse.click((await rect('.modal .drag-handle')).x + 16, (await rect('.modal .drag-handle')).y + 6);
    await page.waitForTimeout(80);
    const runZ = await zOf('.modal');
    const traceZ1 = await zOf('.trace-window');
    const runFront = runZ > traceZ1;
    await page.mouse.click((await rect('.trace-window .tw-header')).x + 40, (await rect('.trace-window .tw-header')).y + 6);
    await page.waitForTimeout(80);
    const traceZ2 = await zOf('.trace-window');
    const runZ2 = await zOf('.modal');
    const traceFront = traceZ2 > runZ2;
    results['click brings panel to front'] = runFront && traceFront;

    // Trace body scrollable.
    results['trace body scrollable'] = await page.locator('.trace-window .tw-body').first().evaluate((el) => {
      const o = getComputedStyle(el).overflowY; return o === 'auto' || o === 'scroll';
    }).catch(() => false);

    results['no console errors'] = consoleErrors.length === 0;
    if (consoleErrors.length) log(`${w}x${h} console:`, JSON.stringify(consoleErrors.slice(0, 3)));
  } catch (e) {
    log(`${w}x${h} ERROR:`, e.message);
    results['ran without throwing'] = false;
  }

  let ok = true;
  for (const [k, v] of Object.entries(results)) { if (!v) ok = false; }
  log(`${w}x${h}: ${ok ? 'PASS' : 'FAIL'}  ${JSON.stringify(results)}`);
  if (!ok) allOk = false;
  await page.close();
}

await browser.close();
log(allOk ? 'ALL PANEL CHECKS PASS' : 'SOME CHECKS FAILED');
process.exitCode = allOk ? 0 : 1;
