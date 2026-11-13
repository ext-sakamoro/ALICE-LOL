// The frame collector (../collect.mjs) on a real page in Chromium: each property the frame law
// reads, measured on fixtures/frame.html where its value is known. Run with `node --test` from a
// directory where `playwright` resolves (CI installs it; locally set PLAYWRIGHT_FROM to a
// directory with node_modules/playwright). The law itself is tested in Rust (src/frame.rs).
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { collectFrame } from '../collect.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const from = process.env.PLAYWRIGHT_FROM || process.cwd();
const { chromium } = createRequire(path.join(from, 'package.json'))('playwright');
const page_url = pathToFileURL(path.join(here, 'fixtures', 'frame.html')).href;

let browser;
before(async () => { browser = await chromium.launch(); });
after(async () => { await browser?.close(); });

async function report(viewport, touch) {
  const ctx = await browser.newContext({ viewport, hasTouch: touch, isMobile: touch });
  const page = await ctx.newPage();
  await page.goto(page_url);
  const r = await collectFrame(page, {});
  await ctx.close();
  const byId = new Map(r.items.map((i) => [i.id, i]));
  return { r, item: (id) => { assert.ok(byId.has(id), `item ${id} in the report`); return byId.get(id); } };
}

test('viewport: pointer and screen width', async () => {
  const phone = (await report({ width: 390, height: 844 }, true)).r.viewport;
  assert.equal(phone.coarse, true);
  assert.equal(phone.w, 390);
  assert.equal(phone.screen_w, 390);
  const desk = (await report({ width: 1280, height: 900 }, false)).r.viewport;
  assert.equal(desk.coarse, false);
});

test('a box clipped by an overflow-hidden ancestor is reported by its shown part', async () => {
  const { item } = await report({ width: 390, height: 844 }, true);
  const c = item('#clipped');
  assert.equal(c.rect[2], 20, 'the bar shows 20 of 60 px');
  assert.deepEqual(c.hit, c.rect);
});

test('a control half inside a scrolling panel is a target by its whole box', async () => {
  const { item } = await report({ width: 390, height: 844 }, true);
  const h = item('#half');
  assert.ok(h.rect[3] < 30, `shown strip ${h.rect[3]}`);
  assert.equal(h.hit[3], 48);
});

test('an element taking the pointer without a role is a control', async () => {
  const { item } = await report({ width: 390, height: 844 }, true);
  const h = item('#handle');
  assert.equal(h.role, 'control');
  assert.equal(h.interactive, true);
});

test('overlay: its own button is in_overlay, a knob over it is sampled on top', async () => {
  const { r, item } = await report({ width: 390, height: 844 }, true);
  assert.equal(item('#inside').in_overlay, true);
  assert.equal(item('#knob').in_overlay, false);
  const s = r.samples.find((x) => x.probe_for === '#knob');
  assert.ok(s, 'the knob over the sheet is sampled');
  assert.equal(s.inside, false);
  assert.ok(r.samples.some((x) => x.inside), 'the sheet itself is sampled');
});

test('text sizes: font size, transform scale, reported canvas, missing report, own minimum', async () => {
  const { item } = await report({ width: 390, height: 844 }, true);
  assert.equal(item('#body16').text_px, 16);
  assert.equal(item('#tiny6').text_px, 6);
  assert.equal(item('#scaled').text_px, 8);
  const d = item('#drawn');
  assert.equal(d.text_px, 9);
  assert.equal(d.text_lines, 2);
  assert.equal(d.text_expected, true);
  const s = item('#silent');
  assert.equal(s.text_px, null);
  assert.equal(s.text_expected, true);
  const c = item('#caption');
  assert.equal(c.text_min_px, 9);
  assert.equal(c.text_min_reason, 'legal caption');
});

test('a link inside a sentence is inline text', async () => {
  const { item } = await report({ width: 390, height: 844 }, true);
  assert.equal(item('#inlink').inline_text, true);
  assert.equal(item('#clipped').inline_text, false);
});
