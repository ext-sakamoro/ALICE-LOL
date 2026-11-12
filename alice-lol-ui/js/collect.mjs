// Frame report collector for the alice-lol-ui frame law (`alice_lol_ui::frame`).
//
// Runs in a Playwright page and returns one frame report (JSON) for the current viewport:
// - DOM items: elements tagged `data-ui="<role>"`, elements matched by the project config's
//   `roles` (selector -> role), and visible controls (button, a[href], input, select, textarea,
//   [role=button], [tabindex]) as `control`.
// - probe items: what the page's canvas drew this frame, from `window.<probe>()` (default
//   `__uiProbe`), each `{ id, role, rect: [x, y, w, h], hit?: [x, y, w, h], interactive?, layer?,
//   text_px?, text_min_px?, text_min_reason? }`
//   in CSS px. A page without a probe contributes no probe items (the report says so).
// - text sizes: every visible element holding text of its own carries `text_px` (font size x
//   transform scale, or the page's own `data-ui-text-px` for canvas / SDF text); elements with no
//   role become `text` items, read only by the text-size check.
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
      // With `scrolled`, an ancestor that scrolls (overflow auto / scroll) does not clip: a control
      // half inside a scrolling panel is reached by scrolling, so its target is its whole box
      const isScroller = (s) => /^(auto|scroll)$/.test(s.overflowX) || /^(auto|scroll)$/.test(s.overflowY);
      const shown = (el, scrolled = false) => {
        const b = el.getBoundingClientRect();
        let x0 = b.left, y0 = b.top, x1 = b.right, y1 = b.bottom;
        const fixed = getComputedStyle(el).position === 'fixed';
        for (let a = el.parentElement; a && a !== document.documentElement; a = a.parentElement) {
          const s = getComputedStyle(a);
          if (fixed && s.transform === 'none' && s.position !== 'fixed') continue;
          if (scrolled && isScroller(s)) continue;
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

      // the rendered text size of an element holding text of its own (a text node child): its
      // font size in CSS px times the scale its transforms apply; a canvas / SDF text element
      // reports its drawn size itself (`data-ui-text-px`). Its own minimum, for an inline caption,
      // is `data-ui-text-min` with the reason in `data-ui-text-reason`
      const ownText = (el) => Array.from(el.childNodes).some((n) => n.nodeType === 3 && n.textContent.trim().length > 0);
      const num = (v) => (v === null || v === '' || !Number.isFinite(Number(v)) ? null : Number(v));
      const textSize = (el) => {
        const reported = num(el.getAttribute('data-ui-text-px'));
        let px = reported;
        if (px === null && ownText(el)) {
          px = parseFloat(getComputedStyle(el).fontSize);
          // the scale of the transforms on the way up: the box drawn on screen over its layout size
          const w = el.offsetWidth;
          const b = el.getBoundingClientRect();
          if (w > 0 && b.width > 0) px *= b.width / w;
        }
        if (px === null || !Number.isFinite(px)) return null;
        return {
          text_px: px,
          text_min_px: num(el.getAttribute('data-ui-text-min')),
          text_min_reason: el.getAttribute('data-ui-text-reason'),
        };
      };

      const items = [];
      const overlays = [];
      for (const [el, role] of roleOf) {
        const vis = visible(el);
        if (!vis) continue;
        const r = shown(el);
        const textual = el.childNodes.length > 0 && (el.textContent || '').trim().length > 0;
        const interactive = role === 'control' || el.matches(controlSelector) || takesPointer(el);
        // pinned to the viewport (fixed / sticky, itself or an ancestor): it does not scroll away
        let pinned = false;
        for (let a = el; a && a !== document.documentElement; a = a.parentElement) {
          const pos = getComputedStyle(a).position;
          if (pos === 'fixed' || pos === 'sticky') { pinned = true; break; }
        }
        // inside a visible overlay (a sheet or dialog meant to cover the scene)
        const inOverlay = Array.from(roleOf).some(([o, r]) => r === 'overlay' && o !== el && o.contains(el) && visible(o));
        items.push({
          id: idOf(el),
          source: 'dom',
          role,
          rect: rectOf(r),
          hit: interactive ? rectOf(shown(el, true)) : null,
          interactive,
          text: textual
            ? { scroll_w: el.scrollWidth, client_w: el.clientWidth, scroll_h: el.scrollHeight, client_h: el.clientHeight }
            : null,
          layer: null,
          in_overlay: inOverlay,
          pinned,
          // a link inside a sentence: displayed inline, its parent holding text of its own
          inline_text: el.tagName === 'A' && getComputedStyle(el).display === 'inline' && !!el.parentElement
            && ownText(el.parentElement),
          ...(textSize(el) || {}),
        });
        if (role === 'overlay') overlays.push(el);
      }
      // every other visible element holding text of its own, as a `text` item (only the text-size
      // check reads it)
      for (const el of document.body.querySelectorAll('*')) {
        if (roleOf.has(el) || !visible(el)) continue;
        const size = textSize(el);
        if (!size || !(ownText(el) || el.hasAttribute('data-ui-text-px'))) continue;
        items.push({
          id: idOf(el), source: 'dom', role: 'text', rect: rectOf(shown(el)), hit: null,
          interactive: false, text: null, layer: null, ...size,
        });
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
            text_px: Number.isFinite(it.text_px) ? it.text_px : null,
            text_min_px: Number.isFinite(it.text_min_px) ? it.text_min_px : null,
            text_min_reason: typeof it.text_min_reason === 'string' ? it.text_min_reason : null,
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

      return { version, viewport: {
          w: vw,
          h: vh,
          dpr: window.devicePixelRatio,
          coarse: matchMedia('(pointer: coarse)').matches,
          // the page scrolls down: what lies below the fold is reachable
          scrolls_y: (document.scrollingElement || document.documentElement).scrollHeight > vh + 1,
          // the screen width: a page laid out wider than this is zoomed out to fit
          screen_w: window.screen.width,
        }, probe: probeState, items, samples };
    },
    { roles: cfg.roles || {}, probe: cfg.probe || '__uiProbe', grid: cfg.grid || 7, controlSelector: CONTROL_SELECTOR, version: REPORT_VERSION },
  );
}
