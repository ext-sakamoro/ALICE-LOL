// Frame report collector for the alice-lol-ui frame law (`alice_lol_ui::frame`).
//
// Runs in a Playwright page and returns one frame report (JSON) for the current viewport:
// - DOM items: elements tagged `data-ui="<role>"`, elements matched by the project config's
//   `roles` (selector -> role), and visible controls (button, a[href], input, select, textarea,
//   [role=button], [tabindex]) as `control`.
// - probe items: what the page's canvas drew this frame, from `window.<probe>()` (default
//   `__uiProbe`), each `{ id, role, rect: [x, y, w, h], hit?: [x, y, w, h], interactive?, layer? }`
//   in CSS px. A page without a probe contributes no probe items (the report says so).
// - layer samples: a grid of points inside every visible overlay; for each, the element on top
//   (document.elementFromPoint) and whether it belongs to the overlay.
//
// The checks themselves live in Rust (`alice_lol_ui::frame::FrameLaw`); this file only measures.
// Usage:
//   import { collectFrame } from 'alice-lol-ui/js/collect.mjs';
//   const report = await collectFrame(page, { roles: { '#clear .sheet': 'overlay' } });

export const REPORT_VERSION = 1;

const CONTROL_SELECTOR = 'button, a[href], input, select, textarea, [role="button"], [tabindex]';

/**
 * @param {import('playwright').Page} page
 * @param {{ roles?: Record<string, string>, probe?: string, grid?: number }} [cfg]
 */
export async function collectFrame(page, cfg = {}) {
  return page.evaluate(
    ({ roles, probe, grid, controlSelector, version }) => {
      const vw = window.innerWidth;
      const vh = window.innerHeight;
      const rectOf = (r) => [r.left, r.top, r.width, r.height];
      // the part of an element its ancestors let show: its box clipped by every ancestor whose
      // overflow is not visible (a header with overflow: hidden hides the buttons past its edge).
      // A fixed element escapes the clip of non-fixed ancestors, as the browser draws it
      const shown = (el) => {
        const b = el.getBoundingClientRect();
        let x0 = b.left, y0 = b.top, x1 = b.right, y1 = b.bottom;
        const fixed = getComputedStyle(el).position === 'fixed';
        for (let a = el.parentElement; a && a !== document.documentElement; a = a.parentElement) {
          const s = getComputedStyle(a);
          if (fixed && s.transform === 'none' && s.position !== 'fixed') continue;
          if (s.overflowX !== 'visible' || s.overflowY !== 'visible') {
            const r = a.getBoundingClientRect();
            if (s.overflowX !== 'visible') { x0 = Math.max(x0, r.left); x1 = Math.min(x1, r.right); }
            if (s.overflowY !== 'visible') { y0 = Math.max(y0, r.top); y1 = Math.min(y1, r.bottom); }
          }
        }
        return { left: x0, top: y0, width: Math.max(0, x1 - x0), height: Math.max(0, y1 - y0), right: x1, bottom: y1 };
      };
      const visible = (el) => {
        const s = getComputedStyle(el);
        if (s.display === 'none' || s.visibility === 'hidden' || Number(s.opacity) === 0) return false;
        if (el.closest('[hidden]')) return false;
        const r = shown(el);
        return r.width > 0 && r.height > 0;
      };
      const idOf = (el) => {
        if (el.id) return '#' + el.id;
        const path = [];
        for (let e = el; e && e !== document.body; e = e.parentElement) {
          if (e.id) { path.unshift('#' + e.id); break; }
          const sib = e.parentElement ? Array.from(e.parentElement.children).indexOf(e) : 0;
          path.unshift(`${e.tagName.toLowerCase()}:${sib}`);
        }
        return path.join('>');
      };

      /** @type {Map<Element, string>} */
      const roleOf = new Map();
      for (const el of document.querySelectorAll('[data-ui]')) roleOf.set(el, el.getAttribute('data-ui'));
      for (const [sel, role] of Object.entries(roles || {})) {
        for (const el of document.querySelectorAll(sel)) if (!roleOf.has(el)) roleOf.set(el, role);
      }
      for (const el of document.querySelectorAll(controlSelector)) if (!roleOf.has(el)) roleOf.set(el, 'control');
      // a drag handle often has no role or tabindex: an element whose own cursor says it takes the
      // pointer (pointer / grab / move) is a control too
      const takesPointer = (el) => {
        const c = getComputedStyle(el).cursor;
        if (!/^(pointer|grab|grabbing|move)$/.test(c)) return false;
        return !el.parentElement || getComputedStyle(el.parentElement).cursor !== c;
      };
      for (const el of document.body.querySelectorAll('*')) if (!roleOf.has(el) && visible(el) && takesPointer(el)) roleOf.set(el, 'control');

      const items = [];
      const overlays = [];
      for (const [el, role] of roleOf) {
        const vis = visible(el);
        if (!vis) continue;
        const r = shown(el);
        const textual = el.childNodes.length > 0 && (el.textContent || '').trim().length > 0;
        const interactive = role === 'control' || el.matches(controlSelector) || takesPointer(el);
        items.push({
          id: idOf(el),
          source: 'dom',
          role,
          rect: rectOf(r),
          hit: interactive ? rectOf(r) : null,
          interactive,
          text: textual
            ? { scroll_w: el.scrollWidth, client_w: el.clientWidth, scroll_h: el.scrollHeight, client_h: el.clientHeight }
            : null,
          layer: null,
        });
        if (role === 'overlay') overlays.push(el);
      }

      let probeState = 'absent';
      const fn = window[probe];
      if (typeof fn === 'function') {
        probeState = 'present';
        for (const it of fn() || []) {
          items.push({
            id: String(it.id),
            source: 'probe',
            role: it.role,
            rect: it.rect,
            hit: it.hit || (it.interactive ? it.rect : null),
            interactive: !!it.interactive,
            text: null,
            layer: Number.isFinite(it.layer) ? it.layer : null,
          });
        }
      }

      const samples = [];
      // what can cover an overlay: every item, plus any positioned element (a floating knob, a toast)
      const positioned = Array.from(document.body.querySelectorAll('*')).filter((el) => {
        const p = getComputedStyle(el).position;
        return (p === 'fixed' || p === 'absolute' || p === 'sticky') && visible(el);
      });
      const domItems = Array.from(new Set([...Array.from(roleOf.keys()).filter(visible), ...positioned]));
      for (const ov of overlays) {
        const r = shown(ov);
        // the centre of every other item's overlap with the overlay: a grid alone can miss a
        // small item (a 22 px knob) on a large overlay
        for (const el of domItems) {
          if (el === ov || ov.contains(el) || el.contains(ov)) continue;
          const e = shown(el);
          const x0 = Math.max(r.left, e.left), x1 = Math.min(r.right, e.right);
          const y0 = Math.max(r.top, e.top), y1 = Math.min(r.bottom, e.bottom);
          if (x1 - x0 <= 0 || y1 - y0 <= 0) continue;
          const x = (x0 + x1) / 2, y = (y0 + y1) / 2;
          if (x < 0 || y < 0 || x >= vw || y >= vh) continue;
          const top = document.elementFromPoint(x, y);
          samples.push({ overlay: idOf(ov), x, y, top: top ? idOf(top) : null, inside: !!top && ov.contains(top), probe_for: idOf(el) });
        }
        for (let i = 0; i < grid; i++) {
          for (let j = 0; j < grid; j++) {
            const x = r.left + ((i + 0.5) / grid) * r.width;
            const y = r.top + ((j + 0.5) / grid) * r.height;
            if (x < 0 || y < 0 || x >= vw || y >= vh) continue;
            const top = document.elementFromPoint(x, y);
            samples.push({ overlay: idOf(ov), x, y, top: top ? idOf(top) : null, inside: !!top && ov.contains(top) });
          }
        }
      }

      return { version, viewport: { w: vw, h: vh, dpr: window.devicePixelRatio }, probe: probeState, items, samples };
    },
    { roles: cfg.roles || {}, probe: cfg.probe || '__uiProbe', grid: cfg.grid || 7, controlSelector: CONTROL_SELECTOR, version: REPORT_VERSION },
  );
}
