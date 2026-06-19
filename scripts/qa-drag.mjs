// Floating-panel UX QA: the Run panel is draggable, clamped, non-clipping,
// and its controls + internal scroll keep working. Tested at several sizes.
import { chromium } from 'playwright-core';

const URL = process.env.QA_URL ?? 'http://localhost:4173/';
const CHROME = process.env.CHROME ?? 'C:/Program Files/Google/Chrome/Application/chrome.exe';
const log = (...a) => console.log('[qa-drag]', ...a);
const SIZES = [
  { w: 1366, h: 768 },
  { w: 1280, h: 720 },
  { w: 1440, h: 900 },
  { w: 1120, h: 720 }, // narrow-ish
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
      if (!/Failed to load resource|net::ERR|favicon|loopback|Private Network|CORS policy/i.test(t)) consoleErrors.push(t);
    }
  });
  const results = {};
  const rect = (sel) => page.locator(sel).first().evaluate((el) => {
    const r = el.getBoundingClientRect();
    return { x: r.x, y: r.y, w: r.width, h: r.height, right: r.right, bottom: r.bottom };
  });
  const fits = (r) => r.x >= -2 && r.y >= -2 && r.right <= w + 2 && r.bottom <= h + 2;

  try {
    await page.goto(URL, { waitUntil: 'networkidle' });
    await page.waitForSelector('.run-btn', { timeout: 20000 });
    const skip = page.locator('.welcome-backdrop .skip-btn');
    if (await skip.count()) { await skip.first().click(); await page.waitForSelector('.welcome-backdrop', { state: 'detached', timeout: 5000 }).catch(() => {}); }

    // Load a sample with a long trace, open Run modal.
    await page.getByRole('button', { name: 'Samples' }).click();
    await page.waitForSelector('.dropdown-menu', { timeout: 5000 });
    const items = page.locator('.menu-item');
    const n = await items.count();
    for (let i = 0; i < n; i++) { const t = (await items.nth(i).locator('.menu-title').innerText()).trim(); if (/payment/i.test(t)) { await items.nth(i).click(); break; } }
    await page.waitForTimeout(600);
    await page.locator('.run-btn').click();
    await page.waitForSelector('.modal', { timeout: 10000 });
    await page.waitForFunction(() => !document.body.innerText.includes('Running…'), { timeout: 15000 }).catch(() => {});
    await page.waitForTimeout(300);

    const center = (r) => Math.abs(r.x - (w - r.w) / 2) < 24;

    // 1. Not clipped at open.
    results['not clipped on open'] = fits(await rect('.modal'));

    // 2. Draggable: grab the header (empty title area) and move by a delta.
    const before = await rect('.modal');
    const handle = await rect('.modal .drag-handle');
    const startX = handle.x + 16, startY = handle.y + handle.h / 2;
    await page.mouse.move(startX, startY);
    await page.mouse.down();
    await page.mouse.move(startX + 120, startY + 90, { steps: 8 });
    await page.mouse.up();
    await page.waitForTimeout(150);
    const after = await rect('.modal');
    results['draggable (panel moved)'] = Math.abs(after.x - (before.x + 120)) < 14 && Math.abs(after.y - (before.y + 90)) < 14;

    // 3. Recenter button brings it back to center.
    await page.locator('.modal button[title="Recenter panel"]').click();
    await page.waitForTimeout(150);
    results['recenter button works'] = center(await rect('.modal'));

    // 4. Double-click the header (title visible at center) also recenters.
    const h1 = await rect('.modal .drag-handle');
    await page.mouse.move(h1.x + 16, h1.y + h1.h / 2);
    await page.mouse.down();
    await page.mouse.move(h1.x + 16 + 140, h1.y + h1.h / 2 + 60, { steps: 6 });
    await page.mouse.up();
    await page.waitForTimeout(120);
    const h2 = await rect('.modal .drag-handle');
    await page.mouse.dblclick(h2.x + 16, h2.y + h2.h / 2);
    await page.waitForTimeout(150);
    results['double-click recenters'] = center(await rect('.modal'));

    // 5. Clamp: drag far off-screen bottom-right, then top-left; the panel
    // must keep a usable slice on screen (never fully lost).
    const c0 = await rect('.modal');
    await page.mouse.move(c0.x + 16, c0.y + 12);
    await page.mouse.down();
    await page.mouse.move(w + 4000, h + 4000, { steps: 6 });
    await page.mouse.up();
    await page.waitForTimeout(120);
    const clampedBR = await rect('.modal');
    const visBR = clampedBR.x < w - 80 && clampedBR.y < h - 20 && clampedBR.right > 40 && clampedBR.bottom > 20;
    const c1 = await rect('.modal');
    await page.mouse.move(c1.x + 16, c1.y + 12);
    await page.mouse.down();
    await page.mouse.move(-4000, -4000, { steps: 6 });
    await page.mouse.up();
    await page.waitForTimeout(120);
    const clampedTL = await rect('.modal');
    const visTL = clampedTL.right > 80 && clampedTL.bottom > 20 && clampedTL.y >= -2;
    results['clamps inside viewport'] = visBR && visTL;

    // 6. Recover a stranded panel by closing + reopening (initPos recenters
    // a remembered-but-stranded position so the panel is never lost).
    await page.keyboard.press('Escape').catch(() => {});
    await page.waitForTimeout(200);
    await page.locator('.run-btn').click();
    await page.waitForSelector('.modal', { timeout: 10000 });
    await page.waitForTimeout(300);
    results['reopen recovers a lost panel'] = center(await rect('.modal')) && fits(await rect('.modal'));

    // 7. Controls still work: switch to the Trace tab.
    await page.locator('.modal .tab', { hasText: 'Trace' }).click();
    await page.waitForTimeout(200);
    results['tabs clickable after drag'] = (await page.locator('.trace-row').count()) > 0;

    // 8. Internal scroll still configured (overflow-y auto on the pane).
    results['trace pane scrollable'] = await page.locator('.modal section.pane').first().evaluate((el) => {
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
log(allOk ? 'ALL DRAG/RESPONSIVE CHECKS PASS' : 'SOME CHECKS FAILED');
process.exitCode = allOk ? 0 : 1;
