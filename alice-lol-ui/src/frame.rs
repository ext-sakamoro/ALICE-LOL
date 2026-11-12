//! `FrameLaw`: checks over one rendered frame of a page or game (what is on screen now), as
//! opposed to [`UiLaw`](crate::UiLaw), which checks a primitive's declared size.
//!
//! The input is a [`FrameReport`] measured in a browser by `alice-lol-ui/js/collect.mjs` (DOM
//! elements, the items a canvas reports through its probe, and points sampled inside overlays).
//! Each check counts what it compared; a check that compared nothing fails
//! ([`FrameViolation::NothingCompared`]), so a report that lost its overlays, controls or probe
//! cannot pass by being empty. A project runs the checks it declares ([`FrameLaw::checks`]); a
//! page with no canvas leaves [`FrameCheck::ControlCover`] out instead of passing it on 0 items.
//!
//! Checks (roles are the `role` of [`FrameItem`]):
//! - [`FrameCheck::LayerOrder`]: every point sampled inside a visible overlay has the overlay (or
//!   one of its descendants) on top.
//! - [`FrameCheck::HitTarget`]: every interactive item's hit rect has a shorter side of at least
//!   [`FrameLaw::min_touch_target`]: on a phone the static law's touch target
//!   ([`UiLaw`](crate::UiLaw), 44 / 48 CSS px), with a mouse WCAG 2.2 SC 2.5.8's 24 CSS px; a
//!   link inside a sentence is exempt (the inline exception of SC 2.5.8).
//! - [`FrameCheck::ControlCover`]: a control or `panel` outside an overlay covers at most
//!   [`FrameLaw::max_control_cover`] of the area of any `subject` or `trajectory` item (a sheet
//!   is meant to cover the scene, so its buttons are not counted).
//! - [`FrameCheck::ControlOverlap`]: no two interactive hit rects intersect.
//! - [`FrameCheck::Text`]: no text item (`readout`, `hint`, `control` with text) overflows its box,
//!   and no two text items intersect unless one contains the other.
//! - [`FrameCheck::Offscreen`]: the page is not laid out wider than the screen (when the report
//!   knows the screen width), and every item other than `scene` fits the viewport across; down, a
//!   pinned item (fixed / sticky), or any item of a page that does not scroll.
//! - [`FrameCheck::ChromeShare`]: the union of `chrome` items covers at most
//!   [`FrameLaw::max_chrome_share`] of the viewport (separate maxima for phone and desktop).

#[cfg(feature = "serde")]
use serde::Deserialize;

/// A rectangle in CSS px: `[x, y, w, h]`, top-left origin
pub type Rect = [f32; 4];

/// The report format version this module reads
pub const REPORT_VERSION: u32 = 1;

/// Tolerance in CSS px for edges (sub-pixel layout rounding)
const EDGE_EPS: f32 = 0.5;

/// The viewport the frame was measured in
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Deserialize))]
pub struct Viewport {
    /// width in CSS px
    pub w: f32,
    /// height in CSS px
    pub h: f32,
    /// device pixel ratio
    pub dpr: f32,
    /// whether the primary pointer is coarse (a finger: CSS `(pointer: coarse)`); when absent the
    /// viewport's size decides phone or desktop
    #[cfg_attr(feature = "serde", serde(default))]
    pub coarse: Option<bool>,
    /// whether the page scrolls vertically (its content is taller than the viewport): what lies
    /// below the fold is reachable, so only pinned items have to fit vertically
    #[cfg_attr(feature = "serde", serde(default))]
    pub scrolls_y: bool,
    /// the width of the screen in CSS px (`screen.width`), when known: a page laid out wider than
    /// the screen is zoomed out to fit, so everything on it is drawn smaller than measured
    #[cfg_attr(feature = "serde", serde(default))]
    pub screen_w: Option<f32>,
}

/// The text box of a text-bearing DOM item (its scroll and client sizes)
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Deserialize))]
pub struct TextBox {
    /// scrollWidth
    pub scroll_w: f32,
    /// clientWidth
    pub client_w: f32,
    /// scrollHeight
    pub scroll_h: f32,
    /// clientHeight
    pub client_h: f32,
}

/// One item on screen: a DOM element or an item a canvas reported through its probe
// the flags are independent observations of one element measured by the collector (takes input,
// inside an overlay, pinned, inline in text), not states of a machine: a flat record matches the
// report's JSON
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Deserialize))]
pub struct FrameItem {
    /// a stable name (`#id`, a DOM path, or the probe's id)
    pub id: String,
    /// `dom` or `probe`
    pub source: String,
    /// `chrome`, `overlay`, `control`, `panel`, `readout`, `hint`, `scene`, `subject`,
    /// `trajectory`, `decor`
    pub role: String,
    /// the drawn rect
    pub rect: Rect,
    /// the rect that accepts input (interactive items)
    #[cfg_attr(feature = "serde", serde(default))]
    pub hit: Option<Rect>,
    /// whether the item takes input
    #[cfg_attr(feature = "serde", serde(default))]
    pub interactive: bool,
    /// the text box, for a DOM item holding text
    #[cfg_attr(feature = "serde", serde(default))]
    pub text: Option<TextBox>,
    /// whether the item is inside a visible overlay (a sheet or dialog meant to cover the scene)
    #[cfg_attr(feature = "serde", serde(default))]
    pub in_overlay: bool,
    /// whether the item (or an ancestor) is pinned to the viewport (CSS position fixed or sticky):
    /// it does not scroll with the page, so it has to fit the viewport
    #[cfg_attr(feature = "serde", serde(default))]
    pub pinned: bool,
    /// whether the item is a link inside a sentence (displayed inline, its parent holding text of
    /// its own): WCAG 2.2 SC 2.5.8 exempts such a target from the size minimum (its size is set
    /// by the line height of the text around it)
    #[cfg_attr(feature = "serde", serde(default))]
    pub inline_text: bool,
}

/// A point sampled inside an overlay: what was on top there
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Deserialize))]
pub struct LayerSample {
    /// the overlay sampled
    pub overlay: String,
    /// x in CSS px
    pub x: f32,
    /// y in CSS px
    pub y: f32,
    /// the item on top at the point (`None`: nothing)
    pub top: Option<String>,
    /// whether the item on top is the overlay or inside it
    pub inside: bool,
}

/// One frame: the viewport, the items and the overlay samples
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Deserialize))]
pub struct FrameReport {
    /// the report format version (see [`REPORT_VERSION`])
    pub version: u32,
    /// the viewport
    pub viewport: Viewport,
    /// the items on screen
    pub items: Vec<FrameItem>,
    /// points sampled inside overlays
    #[cfg_attr(feature = "serde", serde(default))]
    pub samples: Vec<LayerSample>,
}

/// The checks of [`FrameLaw`]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FrameCheck {
    /// an overlay is on top wherever it is drawn
    LayerOrder,
    /// interactive items are large enough to touch
    HitTarget,
    /// a control or panel does not cover much of what it controls
    ControlCover,
    /// interactive items do not overlap
    ControlOverlap,
    /// text fits its box and text items do not overlap
    Text,
    /// items lie inside the viewport
    Offscreen,
    /// the chrome takes a bounded share of the viewport
    ChromeShare,
}

impl FrameCheck {
    /// every check, in report order
    pub const ALL: [Self; 7] = [
        Self::LayerOrder,
        Self::HitTarget,
        Self::ControlCover,
        Self::ControlOverlap,
        Self::Text,
        Self::Offscreen,
        Self::ChromeShare,
    ];

    /// the check's name in reports and configs
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::LayerOrder => "layer-order",
            Self::HitTarget => "hit-target",
            Self::ControlCover => "control-cover",
            Self::ControlOverlap => "control-overlap",
            Self::Text => "text",
            Self::Offscreen => "offscreen",
            Self::ChromeShare => "chrome-share",
        }
    }

    /// the check named `name` (as [`FrameCheck::name`] writes it)
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.name() == name)
    }
}

/// A violation of [`FrameLaw`]
#[derive(Debug, Clone, PartialEq)]
pub enum FrameViolation {
    /// a check compared nothing
    NothingCompared(FrameCheck),
    /// the report is in a format version this module does not read
    UnknownVersion(u32),
    /// `covering` is on top of `overlay` at `points` sampled points (first at `at`)
    OverlayCovered {
        /// the overlay
        overlay: String,
        /// the item on top (`None`: nothing hit)
        covering: Option<String>,
        /// how many sampled points
        points: usize,
        /// the first such point
        at: [f32; 2],
    },
    /// an interactive item's hit rect is smaller than the minimum touch target
    HitTargetTooSmall {
        /// the item
        id: String,
        /// its hit rect's shorter side in CSS px
        side: f32,
        /// the minimum in CSS px
        min: f32,
    },
    /// a control or panel covers too much of a subject or trajectory
    ControlCovers {
        /// the control or panel
        control: String,
        /// the subject or trajectory
        subject: String,
        /// the share of the subject's area covered (0..=1)
        share: f32,
        /// the maximum share
        max: f32,
    },
    /// two interactive hit rects intersect
    ControlsOverlap {
        /// one control
        a: String,
        /// the other control
        b: String,
        /// the intersection's area in CSS px²
        area: f32,
    },
    /// a text item's content is larger than its box
    TextOverflows {
        /// the item
        id: String,
        /// content width × height
        content: [f32; 2],
        /// box width × height
        boxed: [f32; 2],
    },
    /// two text items intersect
    TextOverlaps {
        /// one item
        a: String,
        /// the other item
        b: String,
        /// the intersection's area in CSS px²
        area: f32,
    },
    /// the page is laid out wider than the screen (the browser zooms it out to fit)
    PageWiderThanScreen {
        /// the layout width in CSS px
        layout_w: f32,
        /// the screen width in CSS px
        screen_w: f32,
    },
    /// an item lies partly or wholly outside the viewport
    Offscreen {
        /// the item
        id: String,
        /// its rect
        rect: Rect,
    },
    /// the chrome takes more of the viewport than allowed
    ChromeTooLarge {
        /// the share of the viewport (0..=1)
        share: f32,
        /// the maximum share
        max: f32,
    },
}

impl FrameViolation {
    /// the check this violation belongs to (`None`: the report itself was refused)
    #[must_use]
    pub const fn check(&self) -> Option<FrameCheck> {
        match self {
            Self::NothingCompared(c) => Some(*c),
            Self::UnknownVersion(_) => None,
            Self::OverlayCovered { .. } => Some(FrameCheck::LayerOrder),
            Self::HitTargetTooSmall { .. } => Some(FrameCheck::HitTarget),
            Self::ControlCovers { .. } => Some(FrameCheck::ControlCover),
            Self::ControlsOverlap { .. } => Some(FrameCheck::ControlOverlap),
            Self::TextOverflows { .. } | Self::TextOverlaps { .. } => Some(FrameCheck::Text),
            Self::Offscreen { .. } | Self::PageWiderThanScreen { .. } => {
                Some(FrameCheck::Offscreen)
            }
            Self::ChromeTooLarge { .. } => Some(FrameCheck::ChromeShare),
        }
    }

    /// the items the violation names (for outlining them on a screenshot)
    #[must_use]
    pub fn ids(&self) -> Vec<&str> {
        match self {
            Self::NothingCompared(_)
            | Self::UnknownVersion(_)
            | Self::ChromeTooLarge { .. }
            | Self::PageWiderThanScreen { .. } => vec![],
            Self::OverlayCovered {
                overlay, covering, ..
            } => {
                let mut v = vec![overlay.as_str()];
                v.extend(covering.as_deref());
                v
            }
            Self::HitTargetTooSmall { id, .. }
            | Self::TextOverflows { id, .. }
            | Self::Offscreen { id, .. } => {
                vec![id.as_str()]
            }
            Self::ControlCovers {
                control, subject, ..
            } => vec![control.as_str(), subject.as_str()],
            Self::ControlsOverlap { a, b, .. } | Self::TextOverlaps { a, b, .. } => {
                vec![a.as_str(), b.as_str()]
            }
        }
    }
}

impl core::fmt::Display for FrameViolation {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NothingCompared(c) => write!(f, "{}: compared nothing", c.name()),
            Self::UnknownVersion(v) => write!(f, "report version {v} (this law reads {REPORT_VERSION})"),
            Self::OverlayCovered { overlay, covering, points, at } => write!(
                f,
                "layer-order: {} is on top of overlay {overlay} at {points} point(s), first at ({:.0}, {:.0})",
                covering.as_deref().unwrap_or("nothing"),
                at[0],
                at[1]
            ),
            Self::HitTargetTooSmall { id, side, min } => {
                write!(f, "hit-target: {id} shorter side {side:.1} px < {min} px")
            }
            Self::ControlCovers { control, subject, share, max } => write!(
                f,
                "control-cover: {control} covers {:.1} % of {subject} (max {:.1} %)",
                share * 100.0,
                max * 100.0
            ),
            Self::ControlsOverlap { a, b, area } => write!(f, "control-overlap: {a} and {b} overlap by {area:.0} px²"),
            Self::TextOverflows { id, content, boxed } => write!(
                f,
                "text: {id} content {:.0}x{:.0} overflows its box {:.0}x{:.0}",
                content[0], content[1], boxed[0], boxed[1]
            ),
            Self::TextOverlaps { a, b, area } => write!(f, "text: {a} and {b} overlap by {area:.0} px²"),
            Self::PageWiderThanScreen { layout_w, screen_w } => write!(
                f,
                "offscreen: the page is laid out {layout_w:.0} px wide on a {screen_w:.0} px screen (zoomed out to fit)"
            ),
            Self::Offscreen { id, rect } => write!(
                f,
                "offscreen: {id} at [{:.0}, {:.0}, {:.0}, {:.0}] is not inside the viewport",
                rect[0], rect[1], rect[2], rect[3]
            ),
            Self::ChromeTooLarge { share, max } => write!(
                f,
                "chrome-share: chrome covers {:.1} % of the viewport (max {:.1} %)",
                share * 100.0,
                max * 100.0
            ),
        }
    }
}

/// The outcome of [`FrameLaw::check`]: how much each check compared, and the violations
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FrameVerdict {
    /// per check run, the number of things compared
    pub compared: Vec<(FrameCheck, usize)>,
    /// the violations, in check order
    pub violations: Vec<FrameViolation>,
}

impl FrameVerdict {
    /// whether the frame passed every check
    #[must_use]
    pub const fn passed(&self) -> bool {
        self.violations.is_empty()
    }
}

/// The frame law: which checks run and their thresholds.
///
/// Built only through [`FrameLaw::default`], [`FrameLaw::from_ui_law`], [`FrameLaw::with_checks`] or (feature
/// `serde`) `FrameLaw::from_config`, which validate what they set
#[derive(Debug, Clone, PartialEq)]
pub struct FrameLaw {
    checks: Vec<FrameCheck>,
    min_touch_target_phone: f32,
    min_touch_target_desktop: f32,
    max_control_cover: f32,
    max_chrome_share_phone: f32,
    max_chrome_share_desktop: f32,
    phone_max_short_side: f32,
}

/// Default maximum share of a subject a control may cover.
///
/// Measured on Rewind Machine's tilt knob over its plank, 4.5 % on a 1280x720 desktop (accepted) and 16 % / 24 % on 844x390 / 390x844
/// phones (reported as covering the part); 0.10 lies between, about twice the accepted case
pub const DEFAULT_MAX_CONTROL_COVER: f32 = 0.10;
/// Default minimum target with a mouse, CSS px: WCAG 2.2 success criterion 2.5.8 (Target Size,
/// level AA); on a phone the static law's touch target applies (44, iOS HIG; 48 strict)
pub const DEFAULT_MIN_POINTER_TARGET: f32 = 24.0;
/// Default maximum share of a phone viewport the chrome may cover
pub const DEFAULT_MAX_CHROME_SHARE_PHONE: f32 = 0.12;
/// Default maximum share of a desktop viewport the chrome may cover
pub const DEFAULT_MAX_CHROME_SHARE_DESKTOP: f32 = 0.08;
/// Default: a viewport whose shorter side is below this many CSS px is a phone
pub const DEFAULT_PHONE_MAX_SHORT_SIDE: f32 = 600.0;

impl Default for FrameLaw {
    /// every check, the WCAG AA touch target (44 px) and the default shares (see the constants)
    fn default() -> Self {
        Self::from_ui_law(&crate::UiLaw::default_wcag_aa())
    }
}

impl FrameLaw {
    /// every check, the static law's touch target on phones, WCAG 2.5.8's 24 px with a mouse, and
    /// the default shares
    #[must_use]
    pub fn from_ui_law(law: &crate::UiLaw) -> Self {
        Self {
            checks: FrameCheck::ALL.to_vec(),
            min_touch_target_phone: law.min_touch_target,
            min_touch_target_desktop: DEFAULT_MIN_POINTER_TARGET,
            max_control_cover: DEFAULT_MAX_CONTROL_COVER,
            max_chrome_share_phone: DEFAULT_MAX_CHROME_SHARE_PHONE,
            max_chrome_share_desktop: DEFAULT_MAX_CHROME_SHARE_DESKTOP,
            phone_max_short_side: DEFAULT_PHONE_MAX_SHORT_SIDE,
        }
    }

    /// the same law running only `checks`
    ///
    /// # Errors
    /// an empty list (a law that checks nothing would pass every frame)
    pub fn with_checks(mut self, checks: &[FrameCheck]) -> Result<Self, String> {
        if checks.is_empty() {
            return Err("checks: the list is empty".into());
        }
        self.checks = checks.to_vec();
        Ok(self)
    }

    /// the checks this law runs
    #[must_use]
    pub fn checks(&self) -> &[FrameCheck] {
        &self.checks
    }

    /// minimum shorter side of an interactive item's hit rect, CSS px, on a phone or a desktop
    #[must_use]
    pub const fn min_touch_target(&self, phone: bool) -> f32 {
        if phone {
            self.min_touch_target_phone
        } else {
            self.min_touch_target_desktop
        }
    }

    /// maximum share of a subject's / trajectory's area a control or panel may cover
    #[must_use]
    pub const fn max_control_cover(&self) -> f32 {
        self.max_control_cover
    }

    /// maximum share of the viewport the chrome may cover, for a phone or a desktop viewport
    #[must_use]
    pub const fn max_chrome_share(&self, phone: bool) -> f32 {
        if phone {
            self.max_chrome_share_phone
        } else {
            self.max_chrome_share_desktop
        }
    }

    /// whether `viewport` counts as a phone: its pointer is coarse when the report says, otherwise
    /// its shorter side is below the phone limit
    #[must_use]
    pub fn is_phone(&self, viewport: Viewport) -> bool {
        viewport.coarse.unwrap_or_else(|| {
            crate::det_math::min(viewport.w, viewport.h) < self.phone_max_short_side
        })
    }

    /// checks one frame with the declared checks, in [`FrameCheck::ALL`] order
    #[must_use]
    pub fn check(&self, report: &FrameReport) -> FrameVerdict {
        let mut v = FrameVerdict::default();
        if report.version != REPORT_VERSION {
            v.violations
                .push(FrameViolation::UnknownVersion(report.version));
            return v;
        }
        for c in FrameCheck::ALL
            .into_iter()
            .filter(|c| self.checks.contains(c))
        {
            let n = match c {
                FrameCheck::LayerOrder => layer_order(report, &mut v.violations),
                FrameCheck::HitTarget => self.hit_target(report, &mut v.violations),
                FrameCheck::ControlCover => self.control_cover(report, &mut v.violations),
                FrameCheck::ControlOverlap => control_overlap(report, &mut v.violations),
                FrameCheck::Text => text(report, &mut v.violations),
                FrameCheck::Offscreen => offscreen(report, &mut v.violations),
                FrameCheck::ChromeShare => self.chrome_share(report, &mut v.violations),
            };
            v.compared.push((c, n));
            if n == 0 {
                v.violations.push(FrameViolation::NothingCompared(c));
            }
        }
        v
    }

    fn hit_target(&self, report: &FrameReport, out: &mut Vec<FrameViolation>) -> usize {
        let min = self.min_touch_target(self.is_phone(report.viewport));
        let mut n = 0;
        for item in report
            .items
            .iter()
            .filter(|i| i.interactive && !i.inline_text)
        {
            let Some(hit) = item.hit else { continue };
            n += 1;
            let side = crate::det_math::min(hit[2], hit[3]);
            if side < min {
                out.push(FrameViolation::HitTargetTooSmall {
                    id: item.id.clone(),
                    side,
                    min,
                });
            }
        }
        n
    }

    fn control_cover(&self, report: &FrameReport, out: &mut Vec<FrameViolation>) -> usize {
        // a control inside an overlay is part of a sheet meant to cover the scene
        let controls: Vec<&FrameItem> = report
            .items
            .iter()
            .filter(|i| {
                (i.interactive || i.role == "control" || i.role == "panel") && !i.in_overlay
            })
            .collect();
        let subjects: Vec<&FrameItem> = report
            .items
            .iter()
            .filter(|i| i.role == "subject" || i.role == "trajectory")
            .collect();
        let mut n = 0;
        for s in &subjects {
            let area = s.rect[2] * s.rect[3];
            if area <= 0.0 {
                continue;
            }
            for c in &controls {
                n += 1;
                let share = intersection_area(c.rect, s.rect) / area;
                if share > self.max_control_cover {
                    out.push(FrameViolation::ControlCovers {
                        control: c.id.clone(),
                        subject: s.id.clone(),
                        share,
                        max: self.max_control_cover,
                    });
                }
            }
        }
        n
    }

    fn chrome_share(&self, report: &FrameReport, out: &mut Vec<FrameViolation>) -> usize {
        let vp = report.viewport;
        let screen = [0.0, 0.0, vp.w, vp.h];
        let chrome: Vec<Rect> = report
            .items
            .iter()
            .filter(|i| i.role == "chrome")
            .filter_map(|i| clip(i.rect, screen))
            .collect();
        if chrome.is_empty() || vp.w <= 0.0 || vp.h <= 0.0 {
            return 0;
        }
        let share = union_area(&chrome) / (vp.w * vp.h);
        let max = self.max_chrome_share(self.is_phone(vp));
        if share > max {
            out.push(FrameViolation::ChromeTooLarge { share, max });
        }
        chrome.len()
    }
}

fn layer_order(report: &FrameReport, out: &mut Vec<FrameViolation>) -> usize {
    // one violation per (overlay, covering item), in the order first seen
    let mut groups: Vec<(String, Option<String>, usize, [f32; 2])> = Vec::new();
    for s in report.samples.iter().filter(|s| !s.inside) {
        if let Some(g) = groups.iter_mut().find(|g| g.0 == s.overlay && g.1 == s.top) {
            g.2 += 1;
        } else {
            groups.push((s.overlay.clone(), s.top.clone(), 1, [s.x, s.y]));
        }
    }
    for (overlay, covering, points, at) in groups {
        out.push(FrameViolation::OverlayCovered {
            overlay,
            covering,
            points,
            at,
        });
    }
    report.samples.len()
}

fn control_overlap(report: &FrameReport, out: &mut Vec<FrameViolation>) -> usize {
    let hits: Vec<(&str, Rect)> = report
        .items
        .iter()
        .filter(|i| i.interactive)
        .filter_map(|i| i.hit.map(|h| (i.id.as_str(), h)))
        .collect();
    for (k, (a, ra)) in hits.iter().enumerate() {
        for (b, rb) in &hits[k + 1..] {
            let area = intersection_area(*ra, *rb);
            if area > EDGE_EPS * EDGE_EPS && !contains(*ra, *rb) && !contains(*rb, *ra) {
                out.push(FrameViolation::ControlsOverlap {
                    a: (*a).into(),
                    b: (*b).into(),
                    area,
                });
            }
        }
    }
    hits.len()
}

fn text(report: &FrameReport, out: &mut Vec<FrameViolation>) -> usize {
    let texts: Vec<&FrameItem> = report
        .items
        .iter()
        .filter(|i| i.text.is_some() && matches!(i.role.as_str(), "readout" | "hint" | "control"))
        .collect();
    for i in &texts {
        let Some(t) = i.text else { continue };
        if t.scroll_w > t.client_w + 1.0 || t.scroll_h > t.client_h + 1.0 {
            out.push(FrameViolation::TextOverflows {
                id: i.id.clone(),
                content: [t.scroll_w, t.scroll_h],
                boxed: [t.client_w, t.client_h],
            });
        }
    }
    for (k, a) in texts.iter().enumerate() {
        for b in &texts[k + 1..] {
            let area = intersection_area(a.rect, b.rect);
            if area > EDGE_EPS * EDGE_EPS && !contains(a.rect, b.rect) && !contains(b.rect, a.rect)
            {
                out.push(FrameViolation::TextOverlaps {
                    a: a.id.clone(),
                    b: b.id.clone(),
                    area,
                });
            }
        }
    }
    texts.len()
}

fn offscreen(report: &FrameReport, out: &mut Vec<FrameViolation>) -> usize {
    let vp = report.viewport;
    let mut n = 0;
    if let Some(screen_w) = vp.screen_w {
        n += 1;
        if vp.w > screen_w + EDGE_EPS {
            out.push(FrameViolation::PageWiderThanScreen {
                layout_w: vp.w,
                screen_w,
            });
        }
    }
    for i in report.items.iter().filter(|i| i.role != "scene") {
        n += 1;
        // across, every item has to fit (a page does not scroll sideways by design); down, only
        // a pinned item, or any item of a page that does not scroll
        let down = !vp.scrolls_y || i.pinned;
        let fits_across = i.rect[0] >= -EDGE_EPS && i.rect[0] + i.rect[2] <= vp.w + EDGE_EPS;
        let fits_down =
            !down || (i.rect[1] >= -EDGE_EPS && i.rect[1] + i.rect[3] <= vp.h + EDGE_EPS);
        if !(fits_across && fits_down) {
            out.push(FrameViolation::Offscreen {
                id: i.id.clone(),
                rect: i.rect,
            });
        }
    }
    n
}

/// the area of `a ∩ b`
fn intersection_area(a: Rect, b: Rect) -> f32 {
    use crate::det_math::{max, min};
    let w = min(a[0] + a[2], b[0] + b[2]) - max(a[0], b[0]);
    let h = min(a[1] + a[3], b[1] + b[3]) - max(a[1], b[1]);
    if w > 0.0 && h > 0.0 {
        w * h
    } else {
        0.0
    }
}

/// whether `outer` contains `inner` (within the edge tolerance)
fn contains(outer: Rect, inner: Rect) -> bool {
    inner[0] >= outer[0] - EDGE_EPS
        && inner[1] >= outer[1] - EDGE_EPS
        && inner[0] + inner[2] <= outer[0] + outer[2] + EDGE_EPS
        && inner[1] + inner[3] <= outer[1] + outer[3] + EDGE_EPS
}

/// `r` clipped to `bounds` (`None` if nothing of it is inside)
fn clip(r: Rect, bounds: Rect) -> Option<Rect> {
    use crate::det_math::{max, min};
    let x0 = max(r[0], bounds[0]);
    let y0 = max(r[1], bounds[1]);
    let x1 = min(r[0] + r[2], bounds[0] + bounds[2]);
    let y1 = min(r[1] + r[3], bounds[1] + bounds[3]);
    (x1 > x0 && y1 > y0).then_some([x0, y0, x1 - x0, y1 - y0])
}

/// the area of the union of `rects` (coordinate compression; exact for axis-aligned rects)
fn union_area(rects: &[Rect]) -> f32 {
    let mut xs: Vec<f32> = rects.iter().flat_map(|r| [r[0], r[0] + r[2]]).collect();
    let mut ys: Vec<f32> = rects.iter().flat_map(|r| [r[1], r[1] + r[3]]).collect();
    xs.sort_by(f32::total_cmp);
    xs.dedup();
    ys.sort_by(f32::total_cmp);
    ys.dedup();
    let mut area = 0.0;
    for wx in xs.windows(2) {
        for wy in ys.windows(2) {
            let (cx, cy) = (f32::midpoint(wx[0], wx[1]), f32::midpoint(wy[0], wy[1]));
            if rects
                .iter()
                .any(|r| cx > r[0] && cx < r[0] + r[2] && cy > r[1] && cy < r[1] + r[3])
            {
                // a fused multiply-add would round once instead of twice and change the bits
                // between targets; the law gives the same answer everywhere
                #[allow(clippy::suboptimal_flops)]
                {
                    area += (wx[1] - wx[0]) * (wy[1] - wy[0]);
                }
            }
        }
    }
    area
}

#[cfg(feature = "serde")]
mod config {
    use super::{FrameCheck, FrameLaw};
    use serde::Deserialize;

    /// The minimum target, CSS px, per kind of viewport
    #[derive(Debug, Clone, Copy, Deserialize)]
    pub struct TouchTarget {
        /// phone viewports (a finger)
        pub phone: f32,
        /// desktop viewports (a mouse)
        pub desktop: f32,
    }

    /// The shares of the viewport the chrome may take, per kind of viewport
    #[derive(Debug, Clone, Copy, Deserialize)]
    pub struct ChromeShare {
        /// phone viewports
        pub phone: f32,
        /// desktop viewports
        pub desktop: f32,
    }

    /// A project's frame check config (`ui-frame.json`) as read, before validation.
    ///
    /// The law reads these fields; the runner reads the others (`viewports`, `roles`, ...), so unknown fields
    /// are allowed. [`FrameLaw::from_config`] validates every value it sets
    #[derive(Debug, Clone, Default, Deserialize)]
    pub struct FrameConfig {
        /// the checks to run (default: all)
        #[serde(default)]
        pub checks: Option<Vec<String>>,
        /// minimum target in CSS px per kind of viewport (default phone 44, desktop 24)
        #[serde(default)]
        pub touch_target: Option<TouchTarget>,
        /// maximum share of a subject a control may cover (default 0.10)
        #[serde(default)]
        pub max_control_cover: Option<f32>,
        /// maximum chrome share (default phone 0.12, desktop 0.08)
        #[serde(default)]
        pub chrome_share: Option<ChromeShare>,
        /// shorter side below which a viewport is a phone (default 600)
        #[serde(default)]
        pub phone_max_short_side: Option<f32>,
    }

    impl FrameLaw {
        /// the law a project's config declares, from the defaults
        ///
        /// # Errors
        /// a check name that is not a [`FrameCheck`], an empty check list, or a threshold that is
        /// not finite and positive (a share above 1)
        pub fn from_config(cfg: &FrameConfig) -> Result<Self, String> {
            let mut law = Self::default();
            if let Some(names) = &cfg.checks {
                let checks: Vec<FrameCheck> = names
                    .iter()
                    .map(|n| {
                        FrameCheck::from_name(n)
                            .ok_or_else(|| format!("checks: unknown check {n:?}"))
                    })
                    .collect::<Result<_, _>>()?;
                law = law.with_checks(&checks)?;
            }
            let positive = |name: &str, v: f32, max: f32| {
                if v.is_finite() && v > 0.0 && v <= max {
                    Ok(v)
                } else {
                    Err(format!("{name}: {v} is not in (0, {max}]"))
                }
            };
            if let Some(t) = cfg.touch_target {
                law.min_touch_target_phone = positive("touch_target.phone", t.phone, f32::MAX)?;
                law.min_touch_target_desktop =
                    positive("touch_target.desktop", t.desktop, f32::MAX)?;
            }
            if let Some(v) = cfg.max_control_cover {
                law.max_control_cover = positive("max_control_cover", v, 1.0)?;
            }
            if let Some(c) = cfg.chrome_share {
                law.max_chrome_share_phone = positive("chrome_share.phone", c.phone, 1.0)?;
                law.max_chrome_share_desktop = positive("chrome_share.desktop", c.desktop, 1.0)?;
            }
            if let Some(v) = cfg.phone_max_short_side {
                law.phone_max_short_side = positive("phone_max_short_side", v, f32::MAX)?;
            }
            Ok(law)
        }
    }
}

#[cfg(feature = "serde")]
pub use config::{ChromeShare, FrameConfig, TouchTarget};

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, role: &str, rect: Rect, interactive: bool) -> FrameItem {
        FrameItem {
            id: id.into(),
            source: "dom".into(),
            role: role.into(),
            rect,
            hit: interactive.then_some(rect),
            interactive,
            text: None,
            in_overlay: false,
            pinned: false,
            inline_text: false,
        }
    }

    fn texted(id: &str, role: &str, rect: Rect, scroll: [f32; 2]) -> FrameItem {
        FrameItem {
            text: Some(TextBox {
                scroll_w: scroll[0],
                client_w: rect[2],
                scroll_h: scroll[1],
                client_h: rect[3],
            }),
            ..item(id, role, rect, false)
        }
    }

    fn sample(overlay: &str, top: &str, inside: bool) -> LayerSample {
        LayerSample {
            overlay: overlay.into(),
            x: 10.0,
            y: 20.0,
            top: Some(top.into()),
            inside,
        }
    }

    fn report(items: Vec<FrameItem>, samples: Vec<LayerSample>) -> FrameReport {
        FrameReport {
            version: REPORT_VERSION,
            viewport: Viewport {
                w: 390.0,
                h: 844.0,
                dpr: 3.0,
                coarse: None,
                scrolls_y: false,
                screen_w: None,
            },
            items,
            samples,
        }
    }

    fn only(c: FrameCheck) -> FrameLaw {
        FrameLaw::default().with_checks(&[c]).unwrap()
    }

    #[test]
    fn a_clean_frame_passes_every_check_and_counts_what_it_compared() {
        let r = report(
            vec![
                item("header", "chrome", [0.0, 0.0, 390.0, 80.0], false),
                item("#sheet", "overlay", [20.0, 200.0, 300.0, 200.0], false),
                item("#ok", "control", [20.0, 700.0, 60.0, 48.0], true),
                item("#plank", "subject", [100.0, 500.0, 120.0, 10.0], false),
                texted("#hint", "hint", [20.0, 100.0, 200.0, 20.0], [200.0, 20.0]),
                item("canvas", "scene", [0.0, 0.0, 390.0, 900.0], false),
            ],
            vec![
                sample("#sheet", "#sheet>p", true),
                sample("#sheet", "#sheet", true),
            ],
        );
        let v = FrameLaw::default().check(&r);
        assert!(v.passed(), "{:?}", v.violations);
        assert_eq!(
            v.compared,
            vec![
                (FrameCheck::LayerOrder, 2),
                (FrameCheck::HitTarget, 1),
                (FrameCheck::ControlCover, 1),
                (FrameCheck::ControlOverlap, 1),
                (FrameCheck::Text, 1),
                (FrameCheck::Offscreen, 5),
                (FrameCheck::ChromeShare, 1),
            ]
        );
    }

    #[test]
    fn an_item_on_top_of_an_overlay_is_reported_once_with_its_point_count() {
        let r = report(
            vec![],
            vec![
                sample("#sheet", "#knob", false),
                sample("#sheet", "#knob", false),
                sample("#sheet", "#sheet", true),
            ],
        );
        let v = only(FrameCheck::LayerOrder).check(&r);
        assert_eq!(
            v.violations,
            vec![FrameViolation::OverlayCovered {
                overlay: "#sheet".into(),
                covering: Some("#knob".into()),
                points: 2,
                at: [10.0, 20.0]
            }]
        );
    }

    #[test]
    fn a_hit_rect_below_the_static_laws_minimum_fails_and_the_strict_law_raises_it() {
        let r = report(
            vec![item("#b", "control", [0.0, 0.0, 120.0, 46.0], true)],
            vec![],
        );
        assert!(only(FrameCheck::HitTarget).check(&r).passed());
        let strict = FrameLaw::from_ui_law(&crate::UiLaw::strict_wcag_aaa())
            .with_checks(&[FrameCheck::HitTarget])
            .unwrap();
        assert_eq!(
            strict.check(&r).violations,
            vec![FrameViolation::HitTargetTooSmall {
                id: "#b".into(),
                side: 46.0,
                min: 48.0
            }]
        );
        // the shorter side is what is compared
        let r = report(
            vec![item("#tall", "control", [0.0, 0.0, 22.0, 200.0], true)],
            vec![],
        );
        assert!(
            matches!(only(FrameCheck::HitTarget).check(&r).violations[..], [FrameViolation::HitTargetTooSmall { side, .. }] if side.to_bits() == 22.0f32.to_bits())
        );
    }

    #[test]
    fn a_link_inside_a_sentence_is_exempt_from_the_target_size() {
        let in_text = FrameItem {
            inline_text: true,
            ..item("p>a", "control", [40.0, 300.0, 60.0, 18.0], true)
        };
        let alone = item("li>a", "control", [40.0, 400.0, 60.0, 18.0], true);
        let v = only(FrameCheck::HitTarget).check(&report(vec![in_text, alone], vec![]));
        assert_eq!(
            v.violations,
            vec![FrameViolation::HitTargetTooSmall {
                id: "li>a".into(),
                side: 18.0,
                min: 44.0
            }]
        );
        // the exempt link is not compared
        assert_eq!(v.compared, vec![(FrameCheck::HitTarget, 1)]);
    }

    #[test]
    fn the_target_minimum_is_44_on_a_phone_and_24_with_a_mouse() {
        // a 33.6 px desktop button: under the touch target, over WCAG 2.5.8's 24 px
        let button = item("#b", "control", [0.0, 0.0, 120.0, 33.6], true);
        let mut r = report(vec![button], vec![]);
        let phone = only(FrameCheck::HitTarget).check(&r);
        assert!(
            matches!(&phone.violations[..], [FrameViolation::HitTargetTooSmall { min, .. }] if min.to_bits() == 44.0f32.to_bits())
        );
        r.viewport = Viewport {
            w: 1280.0,
            h: 720.0,
            dpr: 1.0,
            coarse: None,
            scrolls_y: false,
            screen_w: None,
        };
        assert!(only(FrameCheck::HitTarget).check(&r).passed());
        // the pointer decides over the size: a large touch screen is held to the touch target
        r.viewport.coarse = Some(true);
        assert!(!only(FrameCheck::HitTarget).check(&r).passed());
        // and a 22 px control fails with a mouse too
        r.viewport.coarse = Some(false);
        r.items[0].rect[3] = 22.0;
        r.items[0].hit = Some(r.items[0].rect);
        assert!(!only(FrameCheck::HitTarget).check(&r).passed());
    }

    #[test]
    fn a_knob_covering_a_quarter_of_its_plank_fails_and_a_twentieth_passes() {
        // the measured cases: a 36 px knob centred on the end of a 75 px plank (phone) covers 18 / 75
        // of its length; a 22 px knob on a 246 px plank (desktop) covers 11 / 246
        let plank = item("plank", "subject", [100.0, 300.0, 75.0, 6.0], false);
        let knob = item("#rotHandle", "control", [157.0, 285.0, 36.0, 36.0], true);
        let v = only(FrameCheck::ControlCover).check(&report(vec![plank, knob], vec![]));
        assert!(
            matches!(&v.violations[..], [FrameViolation::ControlCovers { share, .. }] if (*share - 18.0 / 75.0).abs() < 1e-6),
            "{:?}",
            v.violations
        );
        let plank = item("plank", "subject", [100.0, 300.0, 246.0, 6.0], false);
        let knob = item("#rotHandle", "control", [335.0, 292.0, 22.0, 22.0], true);
        assert!(only(FrameCheck::ControlCover)
            .check(&report(vec![plank, knob], vec![]))
            .passed());
    }

    #[test]
    fn a_panel_over_a_trajectory_counts_as_covering_it() {
        let path = item("trail", "trajectory", [0.0, 600.0, 390.0, 100.0], false);
        let pad = item("#pad", "panel", [60.0, 560.0, 310.0, 160.0], false);
        let v = only(FrameCheck::ControlCover).check(&report(vec![path, pad], vec![]));
        assert!(
            matches!(&v.violations[..], [FrameViolation::ControlCovers { control, .. }] if control == "#pad")
        );
    }

    #[test]
    fn a_button_on_a_sheet_over_the_trajectory_is_not_a_control_covering_it() {
        let path = item("trail", "trajectory", [0.0, 300.0, 390.0, 60.0], false);
        let on_sheet = FrameItem {
            in_overlay: true,
            ..item("#next", "control", [20.0, 290.0, 200.0, 48.0], true)
        };
        let v = only(FrameCheck::ControlCover)
            .check(&report(vec![path.clone(), on_sheet.clone()], vec![]));
        assert_eq!(
            v.violations,
            vec![FrameViolation::NothingCompared(FrameCheck::ControlCover)]
        );
        let loose = FrameItem {
            in_overlay: false,
            ..on_sheet
        };
        let v = only(FrameCheck::ControlCover).check(&report(vec![path, loose], vec![]));
        assert!(
            matches!(&v.violations[..], [FrameViolation::ControlCovers { control, .. }] if control == "#next")
        );
    }

    #[test]
    fn overlapping_controls_fail_but_a_control_inside_another_does_not() {
        let a = item("#a", "control", [0.0, 0.0, 50.0, 50.0], true);
        let b = item("#b", "control", [40.0, 0.0, 50.0, 50.0], true);
        let inner = item("#a>span", "control", [5.0, 5.0, 10.0, 10.0], true);
        let v = only(FrameCheck::ControlOverlap).check(&report(vec![a, b, inner], vec![]));
        assert_eq!(
            v.violations,
            vec![FrameViolation::ControlsOverlap {
                a: "#a".into(),
                b: "#b".into(),
                area: 500.0
            }]
        );
    }

    #[test]
    fn overflowing_and_overlapping_text_fail() {
        let hint = texted("#hint", "hint", [10.0, 200.0, 360.0, 18.0], [360.0, 36.0]);
        let readout = texted(
            "#selInfo",
            "readout",
            [10.0, 210.0, 300.0, 18.0],
            [300.0, 18.0],
        );
        let v = only(FrameCheck::Text).check(&report(vec![hint, readout], vec![]));
        assert_eq!(
            v.violations,
            vec![
                FrameViolation::TextOverflows {
                    id: "#hint".into(),
                    content: [360.0, 36.0],
                    boxed: [360.0, 18.0]
                },
                FrameViolation::TextOverlaps {
                    a: "#hint".into(),
                    b: "#selInfo".into(),
                    area: 2400.0
                },
            ]
        );
    }

    #[test]
    fn an_item_past_the_viewport_edge_is_offscreen_and_the_scene_is_exempt() {
        let menu = item("#menu", "control", [350.0, 10.0, 60.0, 44.0], true);
        let canvas = item("canvas", "scene", [-10.0, -10.0, 500.0, 1000.0], false);
        let v = only(FrameCheck::Offscreen).check(&report(vec![menu, canvas], vec![]));
        assert_eq!(
            v.violations,
            vec![FrameViolation::Offscreen {
                id: "#menu".into(),
                rect: [350.0, 10.0, 60.0, 44.0]
            }]
        );
        assert_eq!(v.compared, vec![(FrameCheck::Offscreen, 1)]);
    }

    #[test]
    fn below_the_fold_of_a_scrolling_page_is_reachable_but_a_pinned_item_must_fit() {
        let section = item("#section", "readout", [20.0, 1500.0, 300.0, 40.0], false);
        let banner = FrameItem {
            pinned: true,
            ..item("#consent", "control", [37.0, 700.0, 300.0, 191.0], true)
        };
        let wide = item("#wide", "readout", [0.0, 1200.0, 480.0, 40.0], false);
        let mut r = report(vec![section, banner, wide], vec![]);
        // a page that does not scroll: everything has to fit
        let ids = |v: &FrameVerdict| -> Vec<String> {
            v.violations
                .iter()
                .filter_map(|x| match x {
                    FrameViolation::Offscreen { id, .. } => Some(id.clone()),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(
            ids(&only(FrameCheck::Offscreen).check(&r)),
            ["#section", "#consent", "#wide"]
        );
        // a scrolling page: the section below the fold is fine, the pinned banner past the bottom
        // is not, and the over-wide item fails across
        r.viewport.scrolls_y = true;
        assert_eq!(
            ids(&only(FrameCheck::Offscreen).check(&r)),
            ["#consent", "#wide"]
        );
    }

    #[test]
    fn a_page_laid_out_wider_than_the_screen_fails() {
        let button = item("#b", "control", [10.0, 10.0, 60.0, 48.0], true);
        let mut r = report(vec![button], vec![]);
        r.viewport.screen_w = Some(390.0);
        // laid out at the screen width: fits
        let v = only(FrameCheck::Offscreen).check(&r);
        assert!(v.passed());
        assert_eq!(v.compared, vec![(FrameCheck::Offscreen, 2)]);
        // laid out 521 px wide on a 390 px screen: the browser zooms the page out
        r.viewport.w = 521.0;
        assert_eq!(
            only(FrameCheck::Offscreen).check(&r).violations,
            vec![FrameViolation::PageWiderThanScreen {
                layout_w: 521.0,
                screen_w: 390.0
            }]
        );
    }

    #[test]
    fn chrome_share_uses_the_union_and_the_phone_or_desktop_maximum() {
        // two overlapping header rows: union 390 x 200 of 390 x 844 = 23.7 % on a phone
        let rows = vec![
            item("header", "chrome", [0.0, 0.0, 390.0, 150.0], false),
            item("nav", "chrome", [0.0, 100.0, 390.0, 100.0], false),
        ];
        let v = only(FrameCheck::ChromeShare).check(&report(rows, vec![]));
        assert!(
            matches!(&v.violations[..], [FrameViolation::ChromeTooLarge { share, max }] if (*share - 200.0 / 844.0).abs() < 1e-6 && max.to_bits() == 0.12f32.to_bits()),
            "{:?}",
            v.violations
        );
        // a 1280 x 720 desktop: 50 / 720 = 6.9 % passes the 8 % maximum, 60 / 720 = 8.3 % fails
        let mut desk = report(
            vec![item("header", "chrome", [0.0, 0.0, 1280.0, 50.0], false)],
            vec![],
        );
        desk.viewport = Viewport {
            w: 1280.0,
            h: 720.0,
            dpr: 1.0,
            coarse: None,
            scrolls_y: false,
            screen_w: None,
        };
        assert!(only(FrameCheck::ChromeShare).check(&desk).passed());
        desk.items[0].rect[3] = 60.0;
        assert!(!only(FrameCheck::ChromeShare).check(&desk).passed());
    }

    #[test]
    fn nothing_to_compare_fails_each_declared_check() {
        let v = FrameLaw::default().check(&report(vec![], vec![]));
        let nothing: Vec<FrameViolation> = FrameCheck::ALL
            .into_iter()
            .map(FrameViolation::NothingCompared)
            .collect();
        assert_eq!(v.violations, nothing);
        // a page with no canvas leaves control-cover out instead of failing it on 0 subjects
        let law = only(FrameCheck::HitTarget);
        assert!(law
            .check(&report(
                vec![item("#b", "control", [0.0, 0.0, 44.0, 44.0], true)],
                vec![]
            ))
            .passed());
        // and a law with no checks cannot be built
        assert!(FrameLaw::default().with_checks(&[]).is_err());
    }

    #[test]
    fn an_unknown_report_version_is_refused_before_any_check() {
        let mut r = report(vec![], vec![]);
        r.version = 2;
        let v = FrameLaw::default().check(&r);
        assert_eq!(v.violations, vec![FrameViolation::UnknownVersion(2)]);
        assert!(v.compared.is_empty());
    }

    #[test]
    fn union_area_matches_inclusion_exclusion() {
        let a = [0.0, 0.0, 10.0, 10.0];
        let b = [5.0, 5.0, 10.0, 10.0];
        let c = [20.0, 0.0, 1.0, 1.0];
        assert_eq!(union_area(&[a, b, c]).to_bits(), 176.0f32.to_bits()); // 100 + 100 - 25 + 1
        assert_eq!(union_area(&[a, a]).to_bits(), 100.0f32.to_bits());
    }

    #[test]
    fn check_names_round_trip() {
        for c in FrameCheck::ALL {
            assert_eq!(FrameCheck::from_name(c.name()), Some(c));
        }
        assert_eq!(FrameCheck::from_name("layer_order"), None);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn a_config_sets_checks_and_thresholds_and_refuses_bad_values() {
        let cfg: FrameConfig = serde_json::from_str(
            r#"{"checks":["layer-order","chrome-share"],"chrome_share":{"phone":0.2,"desktop":0.1},"viewports":[[390,844]]}"#,
        )
        .unwrap();
        let law = FrameLaw::from_config(&cfg).unwrap();
        assert_eq!(
            law.checks(),
            &[FrameCheck::LayerOrder, FrameCheck::ChromeShare]
        );
        assert_eq!(
            (law.max_chrome_share(true), law.max_chrome_share(false)),
            (0.2, 0.1)
        );
        for bad in [
            r#"{"checks":["nope"]}"#,
            r#"{"checks":[]}"#,
            r#"{"max_control_cover":1.5}"#,
            r#"{"touch_target":{"phone":0,"desktop":24}}"#,
        ] {
            let cfg: FrameConfig = serde_json::from_str(bad).unwrap();
            assert!(FrameLaw::from_config(&cfg).is_err(), "{bad}");
        }
    }
}
