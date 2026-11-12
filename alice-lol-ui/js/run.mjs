#!/usr/bin/env node
// Runs the alice-lol-ui frame check on a web page or game at each of a project's viewports.
//
//   node run.mjs --config ui-frame.json --scenario scenario.mjs --url http://127.0.0.1:4173 \
//                --out <dir> --checker <path to alice-ui-frame-check> [--shots <dir>] [--label <name>]
//
// - config (`ui-frame.json`, per project): `viewports` [{ name, w, h, touch? }], `roles`
//   { selector: role }, `probe` (default `__uiProbe`), the law's `checks` / thresholds (read by
//   the checker; see `alice_lol_ui::frame::FrameConfig`), and `scenarios` { name: { checks } }
//   for a scenario that runs other checks than the project's.
// - scenario: a module exporting `scenarios` { name: async (page, viewport, base) => {} }, each
//   bringing the page to one state to check (a selected part, a cleared stage, ...), or a single
//   `prepare(page, viewport, base)` (scenario `default`).
// - per scenario and viewport: `<out>/<scenario>-<viewport>.json` (the frame report),
//   `<out>/<scenario>-<viewport>.png` (with the items each violation names outlined), and a copy in
//   `--shots` named `<YYYYMMDD-HHMM>_<label>-<scenario>-<viewport>_<sha8>.png` (sha8 of the
//   current directory's git HEAD).
// - exit status: the checker's (0 pass, 1 a violation, 2 unreadable); 3 when a viewport could not
//   be prepared; 2 when the config lists no viewport.
// Playwright is resolved from the current directory (the consuming project's dependency).

import { createRequire } from 'node:module';
import { execFileSync, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { collectFrame } from './collect.mjs';

const args = Object.fromEntries(
  process.argv.slice(2).reduce((acc, a, i, all) => (a.startsWith('--') ? [...acc, [a.slice(2), all[i + 1]]] : acc), []),
);
for (const k of ['config', 'scenario', 'url', 'out', 'checker']) {
  if (!args[k]) {
    console.error(`run.mjs: --${k} is required`);
    process.exit(2);
  }
}
const config = JSON.parse(fs.readFileSync(args.config, 'utf8'));
const viewports = config.viewports || [];
if (viewports.length === 0) {
  console.error('run.mjs: the config lists no viewport');
  process.exit(2);
}
const mod = await import(pathToFileURL(path.resolve(args.scenario)).href);
const scenarios = Object.entries(mod.scenarios || (mod.prepare ? { default: mod.prepare } : {}));
if (scenarios.length === 0) {
  console.error('run.mjs: the scenario module exports no `scenarios` and no `prepare`');
  process.exit(2);
}
const require = createRequire(path.join(process.cwd(), 'package.json'));
const { chromium } = require('playwright');
fs.mkdirSync(args.out, { recursive: true });

let sha8 = 'nogit';
try {
  sha8 = execFileSync('git', ['rev-parse', '--short=8', 'HEAD'], { encoding: 'utf8' }).trim();
} catch { /* not a repository */ }
const now = new Date();
const stamp = `${now.getFullYear()}${String(now.getMonth() + 1).padStart(2, '0')}${String(now.getDate()).padStart(2, '0')}-${String(now.getHours()).padStart(2, '0')}${String(now.getMinutes()).padStart(2, '0')}`;
const label = args.label || path.basename(process.cwd());

const browser = await chromium.launch();
let status = 0;
// one viewport at a time: prepare, collect, check, outline, screenshot, close (a page left open
// keeps animating and slows the next viewport's scenario)
for (const [sname, prepare] of scenarios) for (const v of viewports) {
  const name = `${sname}-${v.name}`;
  const ctx = await browser.newContext({ viewport: { width: v.w, height: v.h }, hasTouch: !!v.touch, isMobile: !!v.touch });
  const page = await ctx.newPage();
  try {
    await prepare(page, v, args.url);
  } catch (e) {
    console.error(`[frame-check] ${name}: the scenario did not prepare the page: ${e.message}`);
    await browser.close();
    process.exit(3);
  }
  const report = await collectFrame(page, { roles: config.roles || {}, probe: config.probe });
  const file = path.join(args.out, `${name}.json`);
  fs.writeFileSync(file, JSON.stringify(report, null, 1));
  console.log(`[frame-check] ${name}: ${report.items.length} items, ${report.samples.length} overlay samples, probe ${report.probe}`);

  // a scenario may declare its own checks (a state with no overlay leaves layer-order out)
  let cfgPath = args.config;
  const own = config.scenarios?.[sname]?.checks;
  if (own) {
    cfgPath = path.join(args.out, `${sname}.config.json`);
    fs.writeFileSync(cfgPath, JSON.stringify({ ...config, checks: own }));
  }
  const checked = spawnSync(args.checker, ['--config', cfgPath, '--json', file], { encoding: 'utf8' });
  if (checked.status === 2 || checked.status === null) {
    console.error(checked.stderr);
    await browser.close();
    process.exit(2);
  }
  status = Math.max(status, checked.status);
  const verdict = JSON.parse(checked.stdout.trim());

  const rects = new Map(report.items.map((i) => [i.id, i.rect]));
  const marks = [];
  for (const viol of verdict.violations) {
    for (const id of viol.ids) if (rects.has(id)) marks.push({ rect: rects.get(id), label: `${viol.check}: ${id}` });
  }
  await page.evaluate((marks) => {
    for (const m of marks) {
      const d = document.createElement('div');
      d.setAttribute('data-frame-check-mark', '');
      Object.assign(d.style, {
        position: 'fixed', left: m.rect[0] + 'px', top: m.rect[1] + 'px', width: m.rect[2] + 'px', height: m.rect[3] + 'px',
        outline: '2px solid #ff2d55', pointerEvents: 'none', zIndex: '2147483647', font: '10px sans-serif', color: '#ff2d55',
      });
      d.textContent = m.label;
      document.body.appendChild(d);
    }
  }, marks);
  const shot = path.join(args.out, `${name}.png`);
  await page.screenshot({ path: shot });
  if (args.shots) {
    fs.mkdirSync(args.shots, { recursive: true });
    fs.copyFileSync(shot, path.join(args.shots, `${stamp}_${label}-${name}_${sha8}.png`));
  }
  const counts = Object.entries(verdict.compared).map(([k, n]) => `${k} ${n}`).join(', ');
  console.log(`[frame-check] ${name}: ${verdict.passed ? 'PASS' : verdict.violations.length + ' violation(s)'} (compared ${counts})`);
  for (const viol of verdict.violations) console.log(`  ${viol.text}`);
  await ctx.close();
}
await browser.close();
process.exit(status);
