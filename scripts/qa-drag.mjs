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
  { w: 1100, h: 700 }, // narrow-ish
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
  // Strict viewport containment: left>=0, top>=0, right<=innerWidth,
  // bottom<=innerHeight (1px tolerance for sub-pixel rounding).
  const fits = (r) => r.x >= -1 && r.y >= -1 && r.right <= w + 1 && r.bottom <= h + 1;
  // No hidden horizontal overflow in the panel chrome (header/tabs): the
  // element must not scroll wider than its client box.
  const noHOverflow = (sel) =>
    page.locator(sel).first().evaluate((el) => el.scrollWidth <= el.clientWidth + 1).catch(() => false);

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

    // 1. Strict viewport containment on open (left>=0, top>=0,
    // right<=innerWidth, bottom<=innerHeight) and no header overflow.
    const onOpen = await rect('.modal');
    results['contained on open (all 4 edges)'] = fits(onOpen);
    results['never wider than viewport'] = onOpen.w <= w;
    results['header no horizontal overflow'] = await noHOverflow('.modal .modal-header');
    results['tabs no horizontal overflow'] = await noHOverflow('.modal .tabs');

    // 2. Draggable: grab the header (empty title area) and move by a delta;
    // the panel moves AND stays fully contained.
    const before = await rect('.modal');
    const handle = await rect('.modal .drag-handle');
    const startX = handle.x + 16, startY = handle.y + handle.h / 2;
    await page.mouse.move(startX, startY);
    await page.mouse.down();
    await page.mouse.move(startX + 120, startY + 90, { steps: 8 });
    await page.mouse.up();
    await page.waitForTimeout(150);
    const after = await rect('.modal');
    // The panel moved meaningfully (exact delta may be clamped when the
    // wide panel nearly fills a narrow viewport) AND stays fully contained.
    results['draggable (panel moved)'] = (Math.abs(after.x - before.x) > 20 || Math.abs(after.y - before.y) > 20);
    results['contained after drag (all 4 edges)'] = fits(after);
    results['header no overflow after drag'] = await noHOverflow('.modal .modal-header');

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

    // 6. Reopen restores a fully-contained position (a remembered position
    // is always re-clamped inside the viewport, so the panel is never lost).
    await page.keyboard.press('Escape').catch(() => {});
    await page.waitForTimeout(200);
    await page.locator('.run-btn').click();
    await page.waitForSelector('.modal', { timeout: 10000 });
    await page.waitForTimeout(400);
    results['reopen restores a contained panel'] = fits(await rect('.modal'));

    // 7. Controls still work: switch to the Trace tab.
    await page.locator('.modal .tab', { hasText: 'Trace' }).click();
    await page.waitForTimeout(200);
    results['tabs clickable after drag'] = (await page.locator('.trace-row').count()) > 0;

    // 8. Trace view (a tab in this panel): dragging it keeps the panel
    // contained inside the viewport with no header overflow.
    const th = await rect('.modal .drag-handle');
    await page.mouse.move(th.x + 16, th.y + th.h / 2);
    await page.mouse.down();
    await page.mouse.move(th.x + 16 - 200, th.y + th.h / 2 + 120, { steps: 6 });
    await page.mouse.up();
    await page.waitForTimeout(120);
    const traceDragged = await rect('.modal');
    results['trace draggable + contained'] =
      fits(traceDragged) && (await noHOverflow('.modal .modal-header'));

    // 9. Internal scroll still configured (overflow-y auto on the pane).
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
