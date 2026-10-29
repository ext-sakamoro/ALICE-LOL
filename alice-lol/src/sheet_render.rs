//! Drawing a filled character sheet (`sheet_fill` JSON checked against a sheet law) as one SVG per
//! page, each with an identifier, and an identifier for the set.
//!
//! Every shape comes from the laws the sheet uses (the body and face laws of the fill's family,
//! `avatar_body` / `avatar_face` or their stylised derivations, bound with the filled values) and from the slots' values; nothing is drawn freehand. The pages are pure functions
//! of the law, the fill and [`SHEET_RENDER_VERSION`]: coordinates are written with a fixed number of
//! decimals from integer arithmetic, rotations use `alice_det_math`, there is no timestamp, and the
//! only font is the generic `sans-serif`.
//!
//! Text on a page is limited to what the sheet law's `text allow` lines permit: page names, slot and
//! field names, the filled values with their unit, and the names of the laws' points. It is built
//! through [`Label`], whose constructors take only those, so no other string can reach a page.
//!
//! A page's identifier is SHA-256 of the sheet law's identifier, the canonical text of the values
//! bound on that page, and [`SHEET_RENDER_VERSION`]; the set's identifier is SHA-256 of the page
//! identifiers in the law's page order.

#![allow(
    clippy::suboptimal_flops,
    reason = "kept unfused to match the closed forms in the tests; fusing changes only the last ulp, which 3-decimal output hides except at a rounding boundary (not hit by any pinned golden)"
)]
#![allow(
    clippy::many_single_char_names,
    reason = "plane geometry: x, y, r, a, b, c, s are the coordinates, radius, points and cosine/sine"
)]

use core::fmt::Write as _;

use alice_det_math::{cos64, round64, sin64};
use sha2::{Digest, Sha256};

use crate::json_lite::{self, J};
use crate::proportion_law::ProportionLaw;
use crate::sheet_law::{SheetLaw, Slot, SlotType};

/// Renderer version: part of every page identifier (a change to the drawing is a new version)
pub const SHEET_RENDER_VERSION: &[u8] = b"lol.sheet_render/v1";

/// The drawing scale of the body pages, as `1 : SCALE_BODY` (mm on the page per mm of the figure)
pub const SCALE_BODY: f64 = 10.0;
/// The drawing scale of the hair and accessory pages
pub const SCALE_DETAIL: f64 = 5.0;
/// Page width and height in mm (A3, landscape)
pub const PAGE_MM: (f64, f64) = (420.0, 297.0);

/// One rendered page
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedPage {
    pub name: String,
    pub svg: String,
    pub page_id: [u8; 32],
}

/// A rendered sheet set
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetRender {
    pub law: String,
    pub pages: Vec<RenderedPage>,
    pub set_id: [u8; 32],
    /// The check's warnings (`rule warn …`): they do not stop the drawing, the caller shows them
    pub warnings: Vec<String>,
}

/// Text that may be drawn on a page (see the module doc); built only by the constructors below
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label(String);

impl Label {
    fn title(page: &str) -> Self {
        Self(page.to_string())
    }
    /// `name value unit` of a filled slot
    fn value(slot: &Slot, v: &J) -> Self {
        Self(format!("{} {}", slot.name, value_text(slot, v)))
    }
    /// a filled value alone, with its unit
    fn bare(slot: &Slot, v: &J) -> Self {
        Self(value_text(slot, v))
    }
    /// the name of a point or landmark of a law the sheet uses
    fn law_point(name: &str) -> Self {
        Self(name.to_string())
    }
    #[must_use]
    pub fn text(&self) -> &str {
        &self.0
    }
}

/// A filled value as text, with the slot's unit
fn value_text(slot: &Slot, v: &J) -> String {
    let s = canonical(v);
    match (&slot.ty, &slot.unit) {
        (SlotType::Number | SlotType::Int, Some(u)) => format!("{s} {u}"),
        _ => s,
    }
}

/// The canonical text of a value: numbers in Rust's shortest round-trip form (the same on every
/// target), strings as they are, booleans as `true` / `false`
fn canonical(v: &J) -> String {
    match v {
        J::Null => "null".to_string(),
        J::Bool(b) => b.to_string(),
        J::Num(x) => format!("{x}"),
        J::Str(s) => s.clone(),
        J::Arr(_) | J::Obj(_) => String::new(),
    }
}

/// Append formatted text to a page or a canonical form (writing to a `String` cannot fail)
fn push(out: &mut String, args: core::fmt::Arguments) {
    let _ = out.write_fmt(args);
}

/// An item's place in its group as a coordinate step (a group holds at most 24 items)
#[allow(
    clippy::cast_precision_loss,
    reason = "an index below 24 is exact in f64"
)]
const fn small(i: usize) -> f64 {
    i as f64
}

/// A coordinate in mm with 3 decimals, from integer arithmetic (no float formatting)
#[allow(
    clippy::cast_possible_truncation,
    reason = "page coordinates are a few hundred mm, far inside i64"
)]
fn fx(v: f64) -> String {
    let m = round64(v * 1000.0) as i64;
    let sign = if m < 0 { "-" } else { "" };
    let a = m.unsigned_abs();
    format!("{sign}{}.{:03}", a / 1000, a % 1000)
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// An SVG page under construction (mm units)
struct Svg {
    body: String,
    texts: Vec<String>,
}

impl Svg {
    const fn new() -> Self {
        Self {
            body: String::new(),
            texts: Vec::new(),
        }
    }
    fn line(&mut self, a: (f64, f64), b: (f64, f64), class: &str) {
        push(
            &mut self.body,
            format_args!(
                "<line class=\"{class}\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"/>\n",
                fx(a.0),
                fx(a.1),
                fx(b.0),
                fx(b.1)
            ),
        );
    }
    fn circle(&mut self, c: (f64, f64), r: f64, class: &str) {
        push(
            &mut self.body,
            format_args!(
                "<circle class=\"{class}\" cx=\"{}\" cy=\"{}\" r=\"{}\"/>\n",
                fx(c.0),
                fx(c.1),
                fx(r)
            ),
        );
    }
    fn disc(&mut self, c: (f64, f64), r: f64, fill: &str) {
        push(
            &mut self.body,
            format_args!(
                "<circle class=\"swatch\" cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\"/>\n",
                fx(c.0),
                fx(c.1),
                fx(r),
                xml(fill)
            ),
        );
    }
    fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, class: &str, fill: Option<&str>) {
        let f = fill
            .map(|c| format!(" fill=\"{}\"", xml(c)))
            .unwrap_or_default();
        push(
            &mut self.body,
            format_args!(
                "<rect class=\"{class}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{f}/>\n",
                fx(x),
                fx(y),
                fx(w),
                fx(h)
            ),
        );
    }
    fn polyline(&mut self, pts: &[(f64, f64)], class: &str) {
        let p: Vec<String> = pts
            .iter()
            .map(|(x, y)| format!("{},{}", fx(*x), fx(*y)))
            .collect();
        push(
            &mut self.body,
            format_args!("<polyline class=\"{class}\" points=\"{}\"/>\n", p.join(" ")),
        );
    }
    fn text(&mut self, at: (f64, f64), size: f64, label: &Label) {
        self.texts.push(label.0.clone());
        push(
            &mut self.body,
            format_args!(
                "<text x=\"{}\" y=\"{}\" font-size=\"{}\">{}</text>\n",
                fx(at.0),
                fx(at.1),
                fx(size),
                xml(&label.0)
            ),
        );
    }
    fn finish(self, page: &str, scale: Option<f64>) -> (String, Vec<String>) {
        let sc = scale
            .map(|s| format!(" data-scale=\"1:{}\"", fx(s)))
            .unwrap_or_default();
        let svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}mm\" height=\"{h}mm\" viewBox=\"0 0 {w} {h}\" \
             data-page=\"{p}\"{sc}>\n<style>line,polyline{{stroke:#222;stroke-width:0.35;fill:none}}\
             .guide{{stroke:#999;stroke-width:0.2;stroke-dasharray:2 1.5}}.bone{{stroke:#222;stroke-width:0.6}}\
             .hem{{stroke:#b04;stroke-width:0.5}}.sleeve{{stroke:#b04;stroke-width:1.6;stroke-opacity:0.5}}.extra{{stroke:#063;stroke-width:0.8}}.joint{{fill:#fff;stroke:#222;stroke-width:0.35}}\
             .box{{fill:none;stroke:#444;stroke-width:0.3}}.swatch{{stroke:#222;stroke-width:0.3}}\
             .weave{{stroke-width:0.4}}.mark{{fill:#222}}text{{font-family:sans-serif;fill:#222}}</style>\n{b}</svg>\n",
            w = fx(PAGE_MM.0),
            h = fx(PAGE_MM.1),
            p = xml(page),
            b = self.body
        );
        (svg, self.texts)
    }
}

/// The values filled on one page, by slot path, and its groups' items
struct PageFill<'a> {
    law: &'a SheetLaw,
    page: &'a str,
    obj: &'a [(String, J)],
}

impl<'a> PageFill<'a> {
    fn slot(&self, name: &str) -> Option<&'a Slot> {
        self.law
            .slots
            .iter()
            .find(|s| s.page == self.page && s.group.is_none() && s.name == name)
    }
    fn field(&self, group: &str, name: &str) -> Option<&'a Slot> {
        self.law
            .slots
            .iter()
            .find(|s| s.page == self.page && s.group.as_deref() == Some(group) && s.name == name)
    }
    fn get(&self, name: &str) -> Option<&'a J> {
        json_lite::get(self.obj, name).filter(|v| !matches!(v, J::Null))
    }
    fn num(&self, name: &str) -> Option<f64> {
        match self.get(name) {
            Some(J::Num(x)) => Some(*x),
            _ => None,
        }
    }
    fn items(&self, group: &str) -> Vec<&'a [(String, J)]> {
        match self.get(group) {
            Some(J::Arr(a)) => a
                .iter()
                .filter_map(|i| {
                    if let J::Obj(o) = i {
                        Some(o.as_slice())
                    } else {
                        None
                    }
                })
                .collect(),
            _ => Vec::new(),
        }
    }
    /// The page's values as canonical lines `path=value`, in the law's slot order (groups item by item)
    fn canonical(&self) -> String {
        let mut out = String::new();
        for s in self
            .law
            .slots
            .iter()
            .filter(|s| s.page == self.page && s.group.is_none())
        {
            if let Some(v) = self.get(&s.name) {
                push(&mut out, format_args!("{}={}\n", s.path(), canonical(v)));
            }
        }
        for g in self.law.groups.iter().filter(|g| g.page == self.page) {
            for (i, item) in self.items(&g.name).iter().enumerate() {
                for f in self
                    .law
                    .slots
                    .iter()
                    .filter(|s| s.page == self.page && s.group.as_deref() == Some(&g.name))
                {
                    if let Some(v) = json_lite::get(item, &f.name).filter(|v| !matches!(v, J::Null))
                    {
                        push(
                            &mut out,
                            format_args!(
                                "{}.{}[{i}].{}={}\n",
                                self.page,
                                g.name,
                                f.name,
                                canonical(v)
                            ),
                        );
                    }
                }
            }
        }
        out
    }
}

fn item_num(item: &[(String, J)], k: &str) -> Option<f64> {
    match json_lite::get(item, k) {
        Some(J::Num(x)) => Some(*x),
        _ => None,
    }
}

fn item_str<'a>(item: &'a [(String, J)], k: &str) -> Option<&'a str> {
    match json_lite::get(item, k) {
        Some(J::Str(s)) => Some(s.as_str()),
        _ => None,
    }
}

/// The body's landmarks in eye heights: (name, x of the left side, y)
struct Body {
    pts: Vec<(&'static str, (f64, f64))>,
}

impl Body {
    fn at(&self, n: &str) -> (f64, f64) {
        self.pts
            .iter()
            .find(|(k, _)| *k == n)
            .map_or((0.0, 0.0), |(_, p)| *p)
    }
    /// A landmark of the arm chain on the figure's left arm (the shoulder, the elbow, the wrist)
    fn arm_at(&self, lm: &str) -> Option<(f64, f64)> {
        match lm {
            "shoulder" => Some(self.at("shoulder_l")),
            "elbow" => Some(self.at("elbow_l")),
            "wrist" => Some(self.at("wrist_l")),
            _ => None,
        }
    }
    /// The height of a landmark of the clothing / accessory vocabulary
    fn landmark_y(&self, lm: &str) -> f64 {
        match lm {
            "head_top" => self.at("top").1,
            "chin" => self.at("chin").1,
            "shoulder" => self.at("shoulder_l").1,
            "elbow" => self.at("elbow_l").1,
            "wrist" => self.at("wrist_l").1,
            "hips" => self.at("hips").1,
            "hip_joint" => self.at("hip_l").1,
            "knee" => self.at("knee_l").1,
            "ankle" => self.at("ankle_l").1,
            _ => 0.0,
        }
    }
}

const BODY_POINTS: [&str; 10] = [
    "hips",
    "hip_l",
    "knee_l",
    "ankle_l",
    "shoulder_l",
    "elbow_l",
    "wrist_l",
    "chin",
    "eye_line",
    "top",
];

/// Bind `avatar_body` with the base body page and lower the arms by the turnaround's pose
fn body(law: &ProportionLaw, base: &PageFill, pose_a_deg: Option<f64>) -> Result<Body, String> {
    let names: Vec<String> = law.params().iter().map(|p| p.name.clone()).collect();
    let mut values = Vec::new();
    for n in &names {
        let v = base
            .num(n)
            .ok_or_else(|| format!("base_body.{n} is not filled"))?;
        values.push((n.as_str(), v));
    }
    let b = law
        .bind(&values)
        .map_err(|e| format!("avatar_body: {e:?}"))?;
    let mut pts = Vec::new();
    for n in BODY_POINTS {
        let p = b
            .point(n)
            .ok_or_else(|| format!("avatar_body has no point {n}"))?;
        pts.push((n, (p[0], p[1])));
    }
    if let Some(deg) = pose_a_deg {
        // the law stands in a T; an A pose turns the arm down about the shoulder by the angle
        let a = deg.to_radians();
        let (c, s) = (cos64(a), sin64(a));
        let sh = pts
            .iter()
            .find(|(k, _)| *k == "shoulder_l")
            .map(|(_, p)| *p)
            .unwrap_or_default();
        for (k, p) in &mut pts {
            if *k == "elbow_l" || *k == "wrist_l" {
                let dx = p.0 - sh.0;
                *p = (sh.0 + dx * c, sh.1 - dx * s);
            }
        }
    }
    Ok(Body { pts })
}

/// Where the figure stands: page coordinates of a body point (eye heights) in one view
struct View {
    cx: f64,
    ground: f64,
    mm_per_unit: f64,
    mirror: bool,
}

impl View {
    fn p(&self, (x, y): (f64, f64)) -> (f64, f64) {
        let x = if self.mirror { -x } else { x };
        (
            self.cx + x * self.mm_per_unit,
            self.ground - y * self.mm_per_unit,
        )
    }
}

fn draw_figure(svg: &mut Svg, b: &Body, v: &View, side: bool, joints: bool) {
    let flat = |p: (f64, f64)| if side { (0.0, p.1) } else { p };
    for sgn in [1.0, -1.0] {
        let m = |n: &str| {
            let p = flat(b.at(n));
            (p.0 * sgn, p.1)
        };
        let shoulder = m("shoulder_l");
        svg.polyline(
            &[v.p(shoulder), v.p(m("elbow_l")), v.p(m("wrist_l"))],
            "bone",
        );
        svg.polyline(
            &[
                v.p(m("hip_l")),
                v.p(m("knee_l")),
                v.p(m("ankle_l")),
                v.p((m("ankle_l").0, 0.0)),
            ],
            "bone",
        );
        if joints {
            for n in ["elbow_l", "wrist_l", "hip_l", "knee_l", "ankle_l"] {
                svg.circle(v.p(m(n)), 1.2, "joint");
            }
        }
    }
    let hips = b.at("hips");
    let sh_y = b.at("shoulder_l").1;
    // the head: from the chin to the top of the skull, two chin-to-eye distances wide (avatar_face)
    let half = 1.0 - b.at("chin").1;
    let (tl, br) = (v.p((-half, b.at("top").1)), v.p((half, b.at("chin").1)));
    svg.rect(
        v.cx - half * v.mm_per_unit,
        tl.1,
        (br.0 - tl.0).abs(),
        br.1 - tl.1,
        "box",
        None,
    );
    svg.line(
        v.p(flat((-b.at("hip_l").0, b.at("hip_l").1))),
        v.p(flat(b.at("hip_l"))),
        "bone",
    );
    svg.polyline(
        &[
            v.p(hips),
            v.p((0.0, sh_y)),
            v.p(b.at("chin")),
            v.p(b.at("top")),
        ],
        "bone",
    );
    svg.line(
        v.p(flat((-b.at("shoulder_l").0, sh_y))),
        v.p(flat(b.at("shoulder_l"))),
        "bone",
    );
}

/// The landmark guide lines, labelled with the landmark names
const GUIDES: [(&str, &str); 9] = [
    ("top", "head_top"),
    ("eye_line", "eye_line"),
    ("chin", "chin"),
    ("shoulder_l", "shoulder"),
    ("hips", "hips"),
    ("hip_l", "hip_joint"),
    ("knee_l", "knee"),
    ("ankle_l", "ankle"),
    ("sole", "sole"),
];

fn draw_guides(svg: &mut Svg, b: &Body, v: &View, x0: f64, x1: f64) {
    for (pt, name) in GUIDES {
        let y = if pt == "sole" { 0.0 } else { b.at(pt).1 };
        let (_, py) = v.p((0.0, y));
        svg.line((x0, py), (x1, py), "guide");
        svg.text((x1 + 1.0, py + 1.0), 2.5, &Label::law_point(name));
    }
}

fn title(svg: &mut Svg, page: &str, f: &PageFill) {
    svg.text((12.0, 14.0), 6.0, &Label::title(page));
    let mut y = 22.0;
    for s in f
        .law
        .slots
        .iter()
        .filter(|s| s.page == page && s.group.is_none())
    {
        if let Some(v) = f.get(&s.name) {
            if !matches!(v, J::Obj(_) | J::Arr(_)) {
                svg.text((12.0, y), 3.0, &Label::value(s, v));
                y += 4.2;
            }
        }
    }
}

/// The lift of the ball and of the heel of the foot, in mm, from the footwear items
fn footwear(acc: &PageFill) -> (f64, f64) {
    let mut ball: f64 = 0.0;
    let mut heel: f64 = 0.0;
    for it in acc.items("items") {
        if item_str(it, "kind") == Some("footwear") {
            let s = item_num(it, "sole").unwrap_or(0.0);
            let h = item_num(it, "heel").unwrap_or(0.0);
            if s > ball {
                ball = s;
            }
            if s + h > heel {
                heel = s + h;
            }
        }
    }
    (ball, heel)
}

/// The colour a palette name stands for (`#rrggbb`), if it is in the palette
fn colour<'a>(palette: &'a PageFill, name: &str) -> Option<&'a str> {
    palette
        .items("colours")
        .into_iter()
        .find(|c| item_str(c, "name") == Some(name))
        .and_then(|c| item_str(c, "srgb"))
}

struct Ctx<'a> {
    law: &'a SheetLaw,
    fills: Vec<PageFill<'a>>,
    body_law: &'a ProportionLaw,
    face_law: &'a ProportionLaw,
}

impl<'a> Ctx<'a> {
    fn page(&self, name: &str) -> &PageFill<'a> {
        self.fills
            .iter()
            .find(|f| f.page == name)
            .expect("every page of the law is read")
    }
    /// The eye height in mm (the stature is the top of the skull above the sole)
    fn eye_mm(&self) -> Result<f64, String> {
        let st = self
            .page("turnaround")
            .num("stature")
            .ok_or("turnaround.stature is not filled")?;
        let top = self
            .page("base_body")
            .num("head_top")
            .ok_or("base_body.head_top is not filled")?;
        Ok(st * 1000.0 / top)
    }
    fn pose(&self) -> Option<f64> {
        let t = self.page("turnaround");
        match t.get("pose") {
            Some(J::Str(p)) if p == "A" => t.num("arm_angle"),
            _ => None,
        }
    }
    fn body(&self) -> Result<Body, String> {
        body(self.body_law, self.page("base_body"), self.pose())
    }
}

fn page_turnaround(c: &Ctx) -> Result<Svg, String> {
    let f = c.page("turnaround");
    let mut svg = Svg::new();
    title(&mut svg, "turnaround", f);
    let b = c.body()?;
    let unit = c.eye_mm()? / SCALE_BODY;
    let (ball, heel) = footwear(c.page("accessories"));
    let ground = 270.0;
    // the footwear lifts the body: the sole of the foot stands `ball` mm above the ground line
    let lifted = ground - ball / SCALE_BODY;
    let joints = matches!(f.get("joint_marks"), Some(J::Bool(true)));
    for (i, (cx, side, mirror, name)) in [
        (110.0, false, false, "front"),
        (210.0, true, false, "side"),
        (310.0, false, true, "back"),
    ]
    .into_iter()
    .enumerate()
    {
        let v = View {
            cx,
            ground: lifted,
            mm_per_unit: unit,
            mirror,
        };
        if i == 0 {
            draw_guides(&mut svg, &b, &v, 60.0, 360.0);
        }
        draw_figure(&mut svg, &b, &v, side, joints);
        if heel > 0.0 {
            // the heel block under each foot, sole + heel high
            for sgn in [1.0, -1.0] {
                let ax = if side { 0.0 } else { b.at("ankle_l").0 * sgn };
                let (px, _) = v.p((ax, 0.0));
                svg.rect(
                    px - 1.0,
                    ground - heel / SCALE_BODY,
                    2.0,
                    heel / SCALE_BODY,
                    "box",
                    None,
                );
            }
        }
        svg.text((cx - 6.0, ground + 12.0), 3.0, &Label::law_point(name));
    }
    svg.line((60.0, ground), (360.0, ground), "bone");
    Ok(svg)
}

/// A proportion box: the slot it shows, from one point to another (eye heights)
type Dim = (&'static str, (f64, f64), (f64, f64));

fn page_base_body(c: &Ctx) -> Result<Svg, String> {
    let f = c.page("base_body");
    let mut svg = Svg::new();
    title(&mut svg, "base_body", f);
    let b = body(c.body_law, f, None)?;
    let unit = c.eye_mm()? / SCALE_BODY;
    let v = View {
        cx: 230.0,
        ground: 270.0,
        mm_per_unit: unit,
        mirror: false,
    };
    draw_guides(&mut svg, &b, &v, 150.0, 330.0);
    draw_figure(&mut svg, &b, &v, false, true);
    // proportion boxes: each height from the sole, each width from the centre line
    let dims: [Dim; 6] = [
        ("hips_y", (0.0, 0.0), (0.0, b.at("hips").1)),
        ("knee", (0.0, 0.0), (0.0, b.at("knee_l").1)),
        ("head_top", (0.0, 0.0), (0.0, b.at("top").1)),
        ("face_length", (0.0, b.at("chin").1), (0.0, 1.0)),
        ("hip_x", (0.0, b.at("hip_l").1), b.at("hip_l")),
        (
            "shoulder_x",
            (0.0, b.at("shoulder_l").1),
            b.at("shoulder_l"),
        ),
    ];
    let mut off = 0.0;
    for (name, a, z) in dims {
        let s = f
            .slot(name)
            .ok_or_else(|| format!("base_body.{name} is not a slot"))?;
        let val = f
            .get(name)
            .ok_or_else(|| format!("base_body.{name} is not filled"))?;
        let (pa, pz) = (v.p(a), v.p(z));
        if (a.0 - z.0).abs() < f64::EPSILON {
            let x = 140.0 - off;
            svg.rect(x, pz.1, 3.0, pa.1 - pz.1, "box", None);
            svg.text((x - 2.0, pz.1 - 1.5), 2.5, &Label::value(s, val));
            off += 14.0;
        } else {
            svg.rect(pa.0, pa.1 - 1.5, pz.0 - pa.0, 3.0, "box", None);
            svg.text((pa.0, pa.1 + 6.0), 2.5, &Label::value(s, val));
        }
    }
    Ok(svg)
}

fn page_face(c: &Ctx) -> Result<Svg, String> {
    let f = c.page("face");
    let mut svg = Svg::new();
    title(&mut svg, "face", f);
    let mut values = Vec::new();
    for p in c.face_law.params() {
        let v = f
            .num(&p.name)
            .ok_or_else(|| format!("face.{} is not filled", p.name))?;
        values.push((p.name.as_str(), v));
    }
    let b = c
        .face_law
        .bind(&values)
        .map_err(|e| format!("avatar_face: {e:?}"))?;
    let pt = |n: &str| {
        b.point(n)
            .ok_or_else(|| format!("avatar_face has no point {n}"))
    };
    let half = b
        .scalar("head_half_width")
        .ok_or("avatar_face has no head_half_width")?;
    let mouth_half = b
        .scalar("mouth_half_width")
        .ok_or("avatar_face has no mouth_half_width")?;
    let depth = b
        .scalar("head_depth")
        .ok_or("avatar_face has no head_depth")?;
    let (eye, size, bi, bo, mouth) = (
        pt("eye_l")?,
        pt("eye_size")?,
        pt("brow_inner_l")?,
        pt("brow_outer_l")?,
        pt("mouth")?,
    );
    // front view: one chin-to-eye distance is 40 mm on the page, the chin at the bottom
    let u = 40.0;
    let (cx, chin_y) = (150.0, 220.0);
    let p = |x: f64, y: f64| (cx + x * u, chin_y - y * u);
    let brows_hidden =
        matches!(c.page("hair").get("fringe"), Some(J::Str(s)) if s == "eye_covering");
    svg.line(p(-half, 0.0), p(-half, bi[1] + 0.3), "guide");
    svg.line(p(half, 0.0), p(half, bi[1] + 0.3), "guide");
    svg.line(p(-half, 1.0), p(half, 1.0), "guide");
    svg.text(
        (cx + half * u + 2.0, chin_y - u + 1.0),
        2.5,
        &Label::law_point("eye_line"),
    );
    svg.circle(p(0.0, 0.0), 1.0, "mark");
    svg.text((cx + 2.0, chin_y + 4.0), 2.5, &Label::law_point("chin"));
    let iris = f.get("eye_colour").and_then(|v| {
        if let J::Str(s) = v {
            colour(c.page("palette"), s)
        } else {
            None
        }
    });
    for sgn in [1.0, -1.0] {
        let (x, y) = p(eye[0] * sgn - size[0] / 2.0, eye[1] + size[1] / 2.0);
        svg.rect(x, y, size[0] * u, size[1] * u, "box", None);
        if let Some(col) = iris {
            svg.disc(p(eye[0] * sgn, eye[1]), size[1] * u / 2.5, col);
        }
        let class = if brows_hidden { "guide" } else { "bone" };
        svg.line(p(bi[0] * sgn, bi[1]), p(bo[0] * sgn, bo[1]), class);
    }
    svg.line(p(-mouth_half, mouth[1]), p(mouth_half, mouth[1]), "bone");
    // side view: the front of the face at z = 0, the back of the head `depth` behind it
    let sx = 300.0;
    let q = |z: f64, y: f64| (sx + z * u, chin_y - y * u);
    svg.line(q(0.0, 0.0), q(0.0, bi[1] + 0.3), "bone");
    svg.line(q(-depth, 0.0), q(-depth, bi[1] + 0.3), "guide");
    svg.line(q(-depth, 1.0), q(0.3, 1.0), "guide");
    svg.circle(q(eye[2], 1.0), 1.0, "mark");
    svg.line(q(-0.3, bi[1]), q(0.0, bi[1]), "bone");
    svg.line(q(-0.2, mouth[1]), q(0.0, mouth[1]), "bone");
    let mut y = 250.0;
    for item in f.items("expressions") {
        let mut x = 12.0;
        for field in ["name", "note"] {
            if let (Some(s), Some(v)) = (f.field("expressions", field), json_lite::get(item, field))
            {
                if !matches!(v, J::Null) {
                    svg.text((x, y), 2.5, &Label::bare(s, v));
                    x += 30.0;
                }
            }
        }
        y += 4.0;
    }
    Ok(svg)
}

fn page_hair(c: &Ctx) -> Result<Svg, String> {
    let f = c.page("hair");
    let mut svg = Svg::new();
    title(&mut svg, "hair", f);
    let eye = c.eye_mm()?;
    let face_len = c
        .page("base_body")
        .num("face_length")
        .ok_or("base_body.face_length is not filled")?;
    // the head's half width is one chin-to-eye distance (avatar_face), i.e. face_length eye heights
    let r = face_len * eye / SCALE_DETAIL;
    let (tx, ty) = (130.0, 120.0);
    svg.circle((tx, ty), r, "box");
    svg.text((tx - r, ty - r - 3.0), 2.5, &Label::law_point("top"));
    let parting = f.num("parting").unwrap_or(0.0).to_radians();
    // top view: the face towards the bottom of the page; the parting from the front of the head
    svg.line(
        (tx, ty),
        (tx + r * sin64(parting), ty + r * cos64(parting)),
        "hem",
    );
    let (sx, sy) = (300.0, 130.0);
    svg.circle((sx, sy), r, "box");
    let tie_field = |n: &str| f.field("ties", n);
    for (i, t) in f.items("ties").iter().enumerate() {
        let az = item_num(t, "azimuth").unwrap_or(0.0).to_radians();
        let h = item_num(t, "height").unwrap_or(0.0);
        let len = item_num(t, "length").unwrap_or(0.0) * eye / SCALE_DETAIL;
        // top view: azimuth 0 is the back (towards the top of the page), positive to the figure's left
        let tp = (tx + r * sin64(az), ty - r * cos64(az));
        svg.circle(tp, 1.2, "joint");
        // side view: from the nape (height 0, the back of the head low) to the crown (height 1, the top)
        let a = h * core::f64::consts::PI / 2.0;
        let sp = (sx + r * cos64(a), sy + r - r * sin64(a) * 2.0);
        svg.circle(sp, 1.2, "joint");
        svg.line(sp, (sp.0, sp.1 + len), "bone");
        let y = 200.0 + small(i) * 4.0;
        let mut x = 12.0;
        for n in ["height", "azimuth", "length", "accessory"] {
            if let (Some(s), Some(v)) = (tie_field(n), json_lite::get(t, n)) {
                svg.text((x, y), 2.5, &Label::value(s, v));
                x += 42.0;
            }
        }
    }
    // one strand of the longest length, waved by the curl (amplitude and pitch in mm)
    let len = f.num("length").unwrap_or(0.0) * eye / SCALE_DETAIL;
    let amp = f.num("curl_amplitude").unwrap_or(0.0) / SCALE_DETAIL;
    let pitch = f.num("curl_pitch").unwrap_or(1.0) / SCALE_DETAIL;
    let n = 64;
    let pts: Vec<(f64, f64)> = (0..=n)
        .map(|k| {
            let t = f64::from(k) / f64::from(n) * len;
            (
                220.0 + amp * sin64(t / pitch * 2.0 * core::f64::consts::PI),
                60.0 + t,
            )
        })
        .collect();
    svg.polyline(&pts, "bone");
    if let Some(col) = f.get("colour").and_then(|v| {
        if let J::Str(s) = v {
            colour(c.page("palette"), s)
        } else {
            None
        }
    }) {
        svg.rect(12.0, 170.0, 20.0, 12.0, "swatch", Some(col));
    }
    Ok(svg)
}

fn page_clothing(c: &Ctx) -> Result<Svg, String> {
    let f = c.page("clothing");
    let mut svg = Svg::new();
    title(&mut svg, "clothing", f);
    let b = c.body()?;
    let unit = c.eye_mm()? / SCALE_BODY;
    let v = View {
        cx: 120.0,
        ground: 270.0,
        mm_per_unit: unit,
        mirror: false,
    };
    draw_guides(&mut svg, &b, &v, 60.0, 180.0);
    draw_figure(&mut svg, &b, &v, false, false);
    let mut layers = f.items("layers");
    #[allow(
        clippy::cast_possible_truncation,
        reason = "a layer's order is an integer from 1 to 8"
    )]
    layers.sort_by_key(|l| item_num(l, "order").map_or(i64::MAX, |o| round64(o) as i64));
    // the labels of one layer follow those of the one before (a layer has up to 11 lines)
    let mut ty = 40.0;
    for (i, l) in layers.iter().enumerate() {
        let from = b.landmark_y(item_str(l, "from").unwrap_or("sole"));
        let to = b.landmark_y(item_str(l, "to").unwrap_or("sole"));
        let hem = item_num(l, "hem").unwrap_or(1.0);
        let hem_y = from + (to - from) * hem;
        // the layer stack: one column per layer from the skin outwards, its span and its hem
        let x = 200.0 + small(i) * 9.0;
        let (_, y0) = v.p((0.0, from));
        let (_, y1) = v.p((0.0, hem_y));
        svg.rect(x, y0, 6.0, y1 - y0, "box", None);
        svg.line((x - 1.0, y1), (x + 7.0, y1), "hem");
        let (lf, lt) = (
            item_str(l, "from").unwrap_or("sole"),
            item_str(l, "to").unwrap_or("sole"),
        );
        if let (Some(a), Some(z)) = (b.arm_at(lf), b.arm_at(lt)) {
            // a layer on the arm chain covers the arm from `from` to its hem, along the bone
            let h = (a.0 + (z.0 - a.0) * hem, a.1 + (z.1 - a.1) * hem);
            svg.line(v.p(a), v.p(h), "sleeve");
            svg.circle(v.p(h), 0.8, "mark");
        } else {
            let (fl, fr) = (v.p((-0.12, hem_y)), v.p((0.12, hem_y)));
            svg.line(fl, fr, "hem");
        }
        for n in [
            "name", "order", "class", "part_of", "sleeves", "from", "to", "hem", "ease", "closure",
            "material",
        ] {
            if let (Some(s), Some(val)) = (f.field("layers", n), json_lite::get(l, n)) {
                if n == "name" {
                    svg.text((300.0, ty), 3.0, &Label::bare(s, val));
                } else {
                    svg.text((300.0, ty), 2.2, &Label::value(s, val));
                }
                ty += 2.6;
            }
        }
        ty += 1.6;
    }
    Ok(svg)
}

fn page_accessories(c: &Ctx) -> Result<Svg, String> {
    let f = c.page("accessories");
    let mut svg = Svg::new();
    title(&mut svg, "accessories", f);
    let b = c.body()?;
    let unit = c.eye_mm()? / SCALE_BODY;
    let v = View {
        cx: 100.0,
        ground: 270.0,
        mm_per_unit: unit,
        mirror: false,
    };
    draw_figure(&mut svg, &b, &v, false, false);
    for (i, it) in f.items("items").iter().enumerate() {
        let attach = item_str(it, "attach").unwrap_or("sole");
        let y = if attach == "hair_tie" {
            b.at("top").1
        } else {
            b.landmark_y(attach)
        };
        svg.circle(v.p((0.0, y)), 1.0, "mark");
        let size = item_num(it, "size").unwrap_or(0.0) / SCALE_DETAIL;
        let row = 40.0 + small(i) * 22.0;
        svg.rect(180.0, row, size, 3.0, "box", None);
        let mut ty = row - 2.0;
        for n in [
            "name", "kind", "attach", "on_layer", "size", "sole", "heel", "material",
        ] {
            if let (Some(s), Some(val)) = (f.field("items", n), json_lite::get(it, n)) {
                if !matches!(val, J::Null) {
                    svg.text((320.0, ty), 2.2, &Label::value(s, val));
                    ty += 2.6;
                }
            }
        }
        if item_str(it, "kind") == Some("footwear") {
            // the sole under the ball of the foot and the heel lifting the back of the foot, at 1:2.5,
            // standing on one ground line in the lower part of the page
            let sole = item_num(it, "sole").unwrap_or(0.0) / SCALE_DETAIL * 2.0;
            let heel = item_num(it, "heel").unwrap_or(0.0) / SCALE_DETAIL * 2.0;
            let ground = 285.0;
            svg.line((180.0, ground), (240.0, ground), "bone");
            svg.rect(185.0, ground - sole, 30.0, sole, "box", None);
            svg.rect(215.0, ground - sole - heel, 10.0, sole + heel, "box", None);
            if let (Some(sf), Some(hf)) = (f.field("items", "sole"), f.field("items", "heel")) {
                if let (Some(sv), Some(hv)) =
                    (json_lite::get(it, "sole"), json_lite::get(it, "heel"))
                {
                    svg.text((245.0, ground - 6.0), 2.5, &Label::value(sf, sv));
                    svg.text((245.0, ground - 2.0), 2.5, &Label::value(hf, hv));
                }
            }
        }
    }
    Ok(svg)
}

/// Root and direction (per unit of length, figure's left) of an extra part on the front view;
/// the directions are fixed 3-4-5 triangles so no angle is computed
fn extra_root(b: &Body, part: &str, attach: &str) -> ((f64, f64), (f64, f64)) {
    let half = 1.0 - b.at("chin").1;
    let root = match attach {
        "head_top" => (half * 0.6, b.at("top").1),
        "shoulder" => b.at("shoulder_l"),
        _ => (0.0, b.at("hips").1),
    };
    let dir = match part {
        "ear" | "horn" => (0.6, 0.8),
        "wing" => (0.8, 0.6),
        _ => (0.6, -0.8),
    };
    (root, dir)
}

fn page_anatomy(c: &Ctx) -> Result<Svg, String> {
    let f = c.page("anatomy");
    let mut svg = Svg::new();
    title(&mut svg, "anatomy", f);
    let b = c.body()?;
    let unit = c.eye_mm()? / SCALE_BODY;
    let v = View {
        cx: 110.0,
        ground: 270.0,
        mm_per_unit: unit,
        mirror: false,
    };
    draw_figure(&mut svg, &b, &v, false, false);
    for (i, it) in f.items("extras").iter().enumerate() {
        let part = item_str(it, "part").unwrap_or("ear");
        let (root, dir) = extra_root(&b, part, item_str(it, "attach").unwrap_or("hips"));
        let len = item_num(it, "length").unwrap_or(0.0);
        let sides: &[f64] = if matches!(json_lite::get(it, "pair"), Some(J::Bool(true))) {
            &[1.0, -1.0]
        } else {
            &[1.0]
        };
        for &m in sides {
            let r = (root.0 * m, root.1);
            let tip = (r.0 + dir.0 * len * m, r.1 + dir.1 * len);
            svg.line(v.p(r), v.p(tip), "extra");
            svg.circle(v.p(tip), 0.8, "mark");
        }
        let mut ty = 40.0 + small(i) * 22.0;
        for n in ["part", "attach", "pair", "length", "material"] {
            if let (Some(s), Some(val)) = (f.field("extras", n), json_lite::get(it, n)) {
                svg.text((240.0, ty), 2.4, &Label::value(s, val));
                ty += 2.8;
            }
        }
    }
    Ok(svg)
}

#[allow(
    clippy::unnecessary_wraps,
    reason = "every page is drawn by the same signature"
)]
fn page_materials(c: &Ctx) -> Result<Svg, String> {
    let f = c.page("materials");
    let mut svg = Svg::new();
    title(&mut svg, "materials", f);
    let pal = c.page("palette");
    for (i, m) in f.items("items").iter().enumerate() {
        let (col, row) = (i % 4, i / 4);
        let x = 20.0 + small(col) * 100.0;
        let y = 40.0 + small(row) * 60.0;
        let base = item_str(m, "colour")
            .and_then(|n| colour(pal, n))
            .unwrap_or("#ffffff");
        match item_str(m, "transmission").unwrap_or("opaque") {
            "open_weave" => {
                // threads with gaps: the share of open area is `openness`
                let open = item_num(m, "openness").unwrap_or(0.5);
                let thread = item_str(m, "thread_colour")
                    .and_then(|n| colour(pal, n))
                    .unwrap_or(base);
                let gap = 1.0 + open * 3.0;
                let mut k = 0;
                loop {
                    let t = f64::from(k) * gap;
                    if t > 30.0 {
                        break;
                    }
                    k += 1;
                    push(&mut svg.body, format_args!(
                        "<line class=\"weave\" stroke=\"{}\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"/>\n",
                        xml(thread),
                        fx(x + t),
                        fx(y),
                        fx(x + t),
                        fx(y + 20.0)
                    ));
                }
                svg.rect(x, y, 30.0, 20.0, "box", None);
            }
            "diffuse" => {
                // light through, scattered: the colour over a bar pattern it blurs
                svg.rect(x, y, 30.0, 20.0, "swatch", Some(base));
                for k in 0..3 {
                    svg.rect(
                        x + 4.0 + f64::from(k) * 9.0,
                        y + 4.0,
                        4.0,
                        12.0,
                        "box",
                        None,
                    );
                }
            }
            "clear" => {
                let tint = item_str(m, "tint")
                    .and_then(|n| colour(pal, n))
                    .unwrap_or("#ffffff");
                svg.rect(x, y, 30.0, 20.0, "swatch", Some(tint));
                svg.line((x, y + 20.0), (x + 30.0, y), "guide");
            }
            _ => svg.rect(x, y, 30.0, 20.0, "swatch", Some(base)),
        }
        let mut ty = y + 25.0;
        for n in [
            "name",
            "class",
            "colour",
            "roughness",
            "metallic",
            "elasticity",
            "shade_shift",
            "shade_softness",
            "outline",
            "transmission",
            "openness",
            "transmittance",
            "scattering",
            "ior",
            "pattern",
            "used_by",
        ] {
            if let (Some(s), Some(val)) = (f.field("items", n), json_lite::get(m, n)) {
                if !matches!(val, J::Null) {
                    svg.text((x, ty), 2.0, &Label::value(s, val));
                    ty += 2.3;
                }
            }
        }
    }
    Ok(svg)
}

#[allow(
    clippy::unnecessary_wraps,
    reason = "every page is drawn by the same signature"
)]
fn page_palette(c: &Ctx) -> Result<Svg, String> {
    let f = c.page("palette");
    let mut svg = Svg::new();
    title(&mut svg, "palette", f);
    for (i, col) in f.items("colours").iter().enumerate() {
        let x = 20.0 + small(i % 6) * 65.0;
        let y = 40.0 + small(i / 6) * 45.0;
        svg.rect(x, y, 40.0, 25.0, "swatch", item_str(col, "srgb"));
        let mut ty = y + 30.0;
        for n in ["name", "srgb"] {
            if let (Some(s), Some(v)) = (f.field("colours", n), json_lite::get(col, n)) {
                svg.text((x, ty), 2.5, &Label::bare(s, v));
                ty += 3.0;
            }
        }
    }
    Ok(svg)
}

/// Every text a page of this law and fill may carry: page names, slot and field names, each filled
/// value with its unit (alone and after its name), and the laws' point and landmark names
fn allowed_texts(c: &Ctx) -> Vec<String> {
    let mut out: Vec<String> = c.law.pages.iter().map(|p| p.name.clone()).collect();
    for s in &c.law.slots {
        out.push(s.name.clone());
        let f = c.page(&s.page);
        let mut vals: Vec<&J> = Vec::new();
        if let Some(g) = &s.group {
            for it in f.items(g) {
                if let Some(v) = json_lite::get(it, &s.name) {
                    vals.push(v);
                }
            }
        } else if let Some(v) = f.get(&s.name) {
            vals.push(v);
        }
        for v in vals {
            out.push(value_text(s, v));
            out.push(format!("{} {}", s.name, value_text(s, v)));
        }
    }
    for (_, name) in GUIDES {
        out.push(name.to_string());
    }
    for n in ["front", "side", "back", "top", "chin", "eye_line"] {
        out.push(n.to_string());
    }
    out
}

impl SheetLaw {
    /// Draw every page of a filled sheet set (see the module doc). `laws` are the proportion laws
    /// the sheet uses, by alias (`avatar_body`, `avatar_face`).
    ///
    /// # Errors
    ///
    /// The check's findings when the fill is not complete (no page is drawn from an incomplete
    /// fill), a fill that is not JSON, a law the fill's family names that is not given, a family
    /// value or alias the sheet law's `family` lines do not name, or a page this renderer has no
    /// drawing for.
    pub fn render(
        &self,
        fill: &str,
        laws: &[(&str, &ProportionLaw)],
    ) -> Result<SheetRender, String> {
        let check = self.check(fill)?;
        if !check.findings.is_empty() || check.read == 0 {
            let list: Vec<String> = check
                .findings
                .iter()
                .map(|f| format!("{} {:?}", f.path, f.problem))
                .collect();
            return Err(format!(
                "the fill is not complete ({} finding(s)): no page is drawn\n{}",
                check.findings.len(),
                list.join("\n")
            ));
        }
        let root = json_lite::parse(fill)?;
        let J::Obj(root) = root else {
            return Err("the fill is not a JSON object".into());
        };
        let Some(J::Obj(pages)) = json_lite::get(&root, "pages") else {
            return Err("the fill has no pages object".into());
        };
        let law_of = |alias: &str| {
            laws.iter()
                .find(|(n, _)| *n == alias)
                .map(|(_, l)| *l)
                .ok_or_else(|| format!("the law {alias} is not given"))
        };
        let ctx = Ctx {
            law: self,
            fills: self
                .pages
                .iter()
                .map(|p| PageFill {
                    law: self,
                    page: p.name.as_str(),
                    obj: match json_lite::get(pages, &p.name) {
                        Some(J::Obj(o)) => o.as_slice(),
                        _ => &[],
                    },
                })
                .collect(),
            body_law: law_of(&self.family_law(pages, "body", "avatar_body")?)?,
            face_law: law_of(&self.family_law(pages, "face", "avatar_face")?)?,
        };
        let allowed = allowed_texts(&ctx);
        let law_id = self.law_id();
        let mut out = Vec::new();
        let mut set = Sha256::new();
        set.update(SHEET_RENDER_VERSION);
        set.update([0]);
        for p in &self.pages {
            let (svg, scale) = match p.name.as_str() {
                "turnaround" => (page_turnaround(&ctx)?, Some(SCALE_BODY)),
                "base_body" => (page_base_body(&ctx)?, Some(SCALE_BODY)),
                "face" => (page_face(&ctx)?, None),
                "hair" => (page_hair(&ctx)?, Some(SCALE_DETAIL)),
                "clothing" => (page_clothing(&ctx)?, Some(SCALE_BODY)),
                "accessories" => (page_accessories(&ctx)?, Some(SCALE_BODY)),
                "anatomy" => (page_anatomy(&ctx)?, Some(SCALE_BODY)),
                "materials" => (page_materials(&ctx)?, None),
                "palette" => (page_palette(&ctx)?, None),
                other => return Err(format!("no drawing for the page {other}")),
            };
            let (svg, texts) = svg.finish(&p.name, scale);
            // every text is a Label, built only from this law's page and slot names, the fill's values
            // with their unit and the landmark names, so none can fall outside `text allow`; checked
            // here in debug builds, and the tests compare each page's texts with the set they compute
            debug_assert!(
                texts.iter().all(|t| allowed.contains(t)),
                "page {} drew a text the sheet law does not allow",
                p.name
            );
            let mut h = Sha256::new();
            h.update(law_id);
            h.update([0]);
            h.update(ctx.page(&p.name).canonical().as_bytes());
            h.update([0]);
            h.update(SHEET_RENDER_VERSION);
            let page_id: [u8; 32] = h.finalize().into();
            set.update(page_id);
            out.push(RenderedPage {
                name: p.name.clone(),
                svg,
                page_id,
            });
        }
        Ok(SheetRender {
            law: self.name.clone(),
            pages: out,
            set_id: set.finalize().into(),
            warnings: check
                .warnings
                .iter()
                .map(|w| format!("{} {:?}", w.path, w.problem))
                .collect(),
        })
    }
}

impl SheetLaw {
    /// The used law an alias (`body`, `face`) names for this fill's family (`family` lines), or
    /// `default` when the sheet law has no family lines
    fn family_law(
        &self,
        pages: &[(String, J)],
        alias: &str,
        default: &str,
    ) -> Result<String, String> {
        let Some(first) = self.families.first() else {
            return Ok(default.to_string());
        };
        let (page, slot) = first
            .slot
            .split_once('.')
            .unwrap_or(("", first.slot.as_str()));
        let value = match json_lite::get(pages, page).and_then(|p| match p {
            J::Obj(o) => json_lite::get(o, slot),
            _ => None,
        }) {
            Some(J::Str(v)) => v.as_str(),
            _ => return Err(format!("{} is not filled", first.slot)),
        };
        let family = self
            .families
            .iter()
            .find(|f| f.value == value)
            .ok_or_else(|| format!("{} {value} is not a family of the sheet law", first.slot))?;
        family
            .laws
            .iter()
            .find(|(a, _)| a == alias)
            .map(|(_, l)| l.clone())
            .ok_or_else(|| format!("the family {value} names no {alias} law"))
    }
}

/// Lower-case hex of an identifier
#[must_use]
pub fn hex(id: &[u8; 32]) -> String {
    id.iter().fold(String::with_capacity(64), |mut s, b| {
        use core::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
        s
    })
}
