// Reproduce the reported right-pane bug: selecting a node + clicking "Edit"
// in the SOL panel makes the inspector / source editor overflow off-screen
// with no scroll. Measures the right-pane sub-panels and screenshots.
import { chromium } from 'playwright-core';
import fs from 'node:fs';

const URL = process.env.QA_URL ?? 'http://localhost:5173/';
const CHROME = process.env.CHROME ?? 'C:/Program Files/Google/Chrome/Application/chrome.exe';
const OUT = process.env.OUT ?? 'C:/Users/akans/AppData/Local/Temp/qa-edit';
const log = (...a) => console.log('[qa-edit]', ...a);
fs.mkdirSync(OUT, { recursive: true });

// Sizes that mimic the embedded iframe inside the platform (short heights).
const SIZES = [
  { w: 1280, h: 720 },
  { w: 1280, h: 600 },
  { w: 1120, h: 560 },
];

const browser = await chromium.launch({ executablePath: CHROME, headless: true });

const rect = (page, sel) => page.locator(sel).first().evaluate((el) => {
  const r = el.getBoundingClientRect();
  const cs = getComputedStyle(el);
  return { x: r.x, y: r.y, w: Math.round(r.width), h: Math.round(r.height), right: Math.round(r.right), bottom: Math.round(r.bottom),
    overflowY: cs.overflowY, scrollH: el.scrollHeight, clientH: el.clientHeight };
}).catch(() => null);

for (const { w, h } of SIZES) {
  const page = await browser.newPage({ viewport: { width: w, height: h } });
  const errs = [];
  page.on('pageerror', (e) => errs.push(e.message));
  try {
    await page.goto(URL, { waitUntil: 'networkidle' });
    await page.waitForSelector('.run-btn', { timeout: 20000 });
    const skip = page.locator('.welcome-backdrop .skip-btn');
    if (await skip.count()) { await skip.first().click(); await page.waitForTimeout(300); }

    // Load a sample so there are nodes + source.
    await page.getByRole('button', { name: 'Samples' }).click();
    await page.waitForSelector('.dropdown-menu', { timeout: 5000 });
    const items = page.locator('.menu-item');
    const n = await items.count();
    let picked = '';
    for (let i = 0; i < n; i++) {
      const t = (await items.nth(i).locator('.menu-title').innerText()).trim();
      if (/payment|order|notify|warehouse/i.test(t)) { picked = t; await items.nth(i).click(); break; }
    }
    if (!picked && n) { picked = (await items.nth(0).locator('.menu-title').innerText()).trim(); await items.nth(0).click(); }
    await page.waitForTimeout(700);

    // Select a node so the Inspector fills with fields.
    const node = page.locator('.vue-flow__node').first();
    if (await node.count()) { await node.click({ position: { x: 30, y: 16 } }); await page.waitForTimeout(300); }

    // Click "Edit" in the SOL panel.
    const editBtn = page.locator('.source-preview .copy-btn', { hasText: /^Edit$/ });
    const hadEdit = await editBtn.count();
    if (hadEdit) { await editBtn.first().click(); await page.waitForTimeout(700); }

    const vp = { w, h };
    const measures = {
      sample: picked,
      rightPane: await rect(page, '.right-pane'),
      inspectorSlot: await rect(page, '.inspector-slot'),
      inspectorBody: await rect(page, '.inspector .body'),
      sourceSlot: await rect(page, '.source-slot'),
      sourcePreview: await rect(page, '.source-preview'),
      diagPanel: await rect(page, '.source-preview .panel'),
      editor: await rect(page, '.source-preview .editor'),
    };

    // Verdicts
    const v = {};
    const rp = measures.rightPane, ss = measures.sourceSlot, ed = measures.editor, ib = measures.inspectorBody, dp = measures.diagPanel;
    if (rp) v['right-pane within viewport'] = rp.bottom <= h + 1 && rp.right <= w + 1;
    if (ed) v['editor has usable height (>=80px)'] = ed.h >= 80;
    if (ed) v['editor bottom within viewport'] = ed.bottom <= h + 1;
    if (ss && ed) v['editor bottom within source-slot'] = ed.bottom <= ss.bottom + 1;
    if (ib) v['inspector body scrolls (not clipped overflow)'] = ib.overflowY === 'auto' || ib.overflowY === 'scroll';
    if (ib) v['inspector body within viewport'] = ib.bottom <= h + 1;
    if (dp) v['diag panel bounded'] = dp.h <= 181;
    v['no page errors'] = errs.length === 0;

    await page.screenshot({ path: `${OUT}/edit-${w}x${h}.png` });
    const ok = Object.values(v).every(Boolean);
    log(`${w}x${h} editBtn=${hadEdit?'yes':'NO'} sample="${picked}" -> ${ok ? 'PASS' : 'FAIL'}`);
    log('  verdicts', JSON.stringify(v));
    log('  measures', JSON.stringify(measures));
  } catch (e) {
    log(`${w}x${h} ERROR: ${e.message}`);
  }
  await page.close();
}
await browser.close();
log('done; screenshots in ' + OUT);
