//! `SheetLaw::render`: a filled sheet set drawn as SVG pages with page and set identifiers.
//!
//! oracles: (1) the pages of four complete fills (`tests/data/sheet_fill_example.json` and the
//! three `sheet_fill_spread_*.json`, a small young figure, a stylised one with ears and a tail, a
//! tall adult) pinned byte for byte, with their page and set identifiers; (2) closed
//! forms for the drawn geometry: the wrist in a T pose at `shoulder_x + arm` eye heights from the
//! centre line, in an A pose turned down about the shoulder by the arm angle, and the feet lifted
//! by the footwear's sole; (3) every text on a page is a page name, a slot name, a filled value
//! with its unit, or a landmark name, as the sheet law's `text allow` lines say, computed here
//! from the law and the fill and not from the renderer; (4) no page is drawn from a fill with
//! errors (the two `sheet_fill_outside_answer_*.json`, written by an outside language model from
//! an earlier spec, lack what the law now asks for), while warnings are returned and drawn past;
//! (5) a page's identifier moves with that page's values and no other's.
//!
//! `ALICE_LOL_PRINT_RENDER=1 cargo test -p alice-lol --test sheet_render` rewrites the pinned pages.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::suboptimal_flops, // the closed forms are written unfused, as the renderer draws them
    clippy::many_single_char_names // plane geometry (x, y, e, w, s, c, …)
)]

use std::fmt::Write as _;

use std::path::{Path, PathBuf};

use alice_lol::proportion_law::ProportionLaw;
use alice_lol::sheet_law::{Page, SheetLaw};
use alice_lol::sheet_render::{hex, SheetRender, SCALE_BODY};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// The sheet law, resolved, and the proportion laws it uses by alias (a stylised law derives
/// from its reference law)
fn laws() -> (SheetLaw, Vec<(String, ProportionLaw)>) {
    let read = |p: &str| std::fs::read_to_string(root().join(p)).unwrap();
    let mut l = SheetLaw::from_file(&read("laws/avatar/sheet.law")).unwrap();
    let body = ProportionLaw::from_file(&read("laws/avatar/body.law")).unwrap();
    let face = ProportionLaw::from_file(&read("laws/avatar/face.law")).unwrap();
    let body_s = ProportionLaw::from_file_with(
        &read("laws/avatar/body_stylised.law"),
        &[("avatar_body", &body)],
    )
    .unwrap();
    let face_s = ProportionLaw::from_file_with(
        &read("laws/avatar/face_stylised.law"),
        &[("avatar_face", &face)],
    )
    .unwrap();
    let all = vec![
        ("avatar_body".to_string(), body),
        ("avatar_face".to_string(), face),
        ("avatar_body_stylised".to_string(), body_s),
        ("avatar_face_stylised".to_string(), face_s),
    ];
    let refs: Vec<(&str, &ProportionLaw)> = all.iter().map(|(a, l)| (a.as_str(), l)).collect();
    l.resolve(&refs).unwrap();
    (l, all)
}

fn fill(name: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/data")
            .join(name),
    )
    .unwrap()
}

fn render(fill: &str) -> Result<SheetRender, String> {
    let (l, all) = laws();
    let refs: Vec<(&str, &ProportionLaw)> = all.iter().map(|(a, l)| (a.as_str(), l)).collect();
    l.render(fill, &refs)
}

/// Replace one JSON value in a fill (a key's first occurrence; the fills use each key once per page)
fn with(fill: &str, key: &str, value: &str) -> String {
    let at = fill
        .find(&format!("\"{key}\":"))
        .unwrap_or_else(|| panic!("no key {key}"));
    let colon = at + key.len() + 3;
    let start = colon + fill[colon..].len() - fill[colon..].trim_start().len();
    let rest = &fill[start..];
    let end = rest.find([',', '}']).unwrap();
    format!("{}{}{}", &fill[..start], value, &rest[end..])
}

fn golden_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/sheet_render")
        .join(name)
}

fn pinned(name: &str, data: &str) {
    let r = render(&fill(data)).unwrap();
    let dir = golden_dir(name);
    let mut ids = String::new();
    for p in &r.pages {
        let _ = writeln!(ids, "{} {}", p.name, hex(&p.page_id));
    }
    let _ = writeln!(ids, "set {}", hex(&r.set_id));
    if std::env::var_os("ALICE_LOL_PRINT_RENDER").is_some() {
        std::fs::create_dir_all(&dir).unwrap();
        for p in &r.pages {
            std::fs::write(dir.join(format!("{}.svg", p.name)), &p.svg).unwrap();
        }
        std::fs::write(dir.join("ids.txt"), &ids).unwrap();
        return;
    }
    assert_eq!(r.pages.len(), 9, "one page per page of the law");
    for p in &r.pages {
        let want = std::fs::read_to_string(dir.join(format!("{}.svg", p.name))).unwrap();
        assert!(
            p.svg == want,
            "{name}/{}.svg differs from the pinned page",
            p.name
        );
    }
    assert_eq!(
        ids,
        std::fs::read_to_string(dir.join("ids.txt")).unwrap(),
        "{name}: identifiers"
    );
}

#[test]
fn the_hand_written_fill_draws_the_pinned_pages() {
    pinned("example", "sheet_fill_example.json");
}

#[test]
fn the_small_young_fill_draws_the_pinned_pages() {
    pinned("spread_small_young", "sheet_fill_spread_small_young.json");
}

#[test]
fn the_stylised_fill_draws_the_pinned_pages() {
    pinned("spread_stylised", "sheet_fill_spread_stylised.json");
}

#[test]
fn the_tall_adult_fill_draws_the_pinned_pages() {
    pinned("spread_tall_adult", "sheet_fill_spread_tall_adult.json");
}

#[test]
fn the_outside_fills_have_errors_and_draw_nothing() {
    for data in [
        "sheet_fill_outside_answer_1.json",
        "sheet_fill_outside_answer_2.json",
    ] {
        let e = render(&fill(data)).unwrap_err();
        assert!(e.contains("not complete"), "{data}: {e}");
        assert!(e.contains("base_body.skin_material Missing"), "{data}: {e}");
        assert!(!e.contains("<svg"), "{data}: no page in the refusal");
    }
}

/// A warning (an unused colour) is returned with the pages and does not stop the drawing
#[test]
fn a_warning_is_returned_and_the_pages_are_drawn() {
    let f = fill("sheet_fill_example.json");
    let at = f.find("\"colours\": [").unwrap() + 12;
    let spare = format!(
        "{}{{\"name\": \"spare\", \"srgb\": \"#123456\"}}, {}",
        &f[..at],
        &f[at..]
    );
    let r = render(&spare).unwrap();
    assert_eq!(r.pages.len(), 9);
    assert_eq!(r.warnings.len(), 1, "{:?}", r.warnings);
    assert!(
        r.warnings[0].starts_with("palette.colours"),
        "{:?}",
        r.warnings
    );
    assert!(render(&f).unwrap().warnings.is_empty());
    let palette = r.pages.iter().find(|p| p.name == "palette").unwrap();
    assert!(
        palette.svg.contains(">spare</text>"),
        "the spare colour is drawn"
    );
}

/// The refusal of a render with this sheet law (changed by `edit`) and these laws, from the example fill
fn refused(edit: impl FnOnce(&mut SheetLaw), give: &[&str]) -> String {
    let (mut l, all) = laws();
    edit(&mut l);
    let refs: Vec<(&str, &ProportionLaw)> = all
        .iter()
        .filter(|(a, _)| give.contains(&a.as_str()))
        .map(|(a, l)| (a.as_str(), l))
        .collect();
    l.render(&fill("sheet_fill_example.json"), &refs)
        .unwrap_err()
}

const ALL: [&str; 4] = [
    "avatar_body",
    "avatar_face",
    "avatar_body_stylised",
    "avatar_face_stylised",
];

/// A law the fill's family names but the caller did not give: nothing is drawn
#[test]
fn a_law_the_family_names_but_not_given_draws_nothing() {
    let e = refused(
        |_| {},
        &[
            "avatar_body",
            "avatar_body_stylised",
            "avatar_face_stylised",
        ],
    );
    assert_eq!(e, "the law avatar_face is not given");
    // the stylised laws are not needed for a reference fill: the two its family names are enough
    let (l, all) = laws();
    let refs: Vec<(&str, &ProportionLaw)> = all
        .iter()
        .filter(|(a, _)| a == "avatar_body" || a == "avatar_face")
        .map(|(a, l)| (a.as_str(), l))
        .collect();
    assert!(l.render(&fill("sheet_fill_example.json"), &refs).is_ok());
}

/// The fill's family value with no `family` line: nothing is drawn
#[test]
fn a_family_value_without_a_family_line_draws_nothing() {
    let e = refused(|l| l.families.retain(|f| f.value != "reference"), &ALL);
    assert_eq!(
        e,
        "base_body.family reference is not a family of the sheet law"
    );
}

/// A `family` line that does not name the face law: nothing is drawn
#[test]
fn a_family_line_without_the_face_alias_draws_nothing() {
    let e = refused(
        |l| {
            for f in l.families.iter_mut().filter(|f| f.value == "reference") {
                f.laws.retain(|(a, _)| a != "face");
            }
        },
        &ALL,
    );
    assert_eq!(e, "the family reference names no face law");
}

/// A page of the law the renderer has no drawing for: nothing is drawn
#[test]
fn a_page_without_a_drawing_draws_nothing() {
    let e = refused(
        |l| {
            l.pages.push(Page {
                name: "storyboard".to_string(),
                purpose: "a page this renderer does not draw".to_string(),
            });
        },
        &ALL,
    );
    assert_eq!(e, "no drawing for the page storyboard");
}

#[test]
fn rendering_twice_gives_the_same_bytes() {
    let f = fill("sheet_fill_spread_stylised.json");
    assert_eq!(render(&f).unwrap(), render(&f).unwrap());
}

#[test]
fn an_incomplete_fill_draws_nothing() {
    let f = fill("sheet_fill_example.json").replacen("\"stature\": 1.52,", "", 1);
    let e = render(&f).unwrap_err();
    assert!(e.contains("not complete"), "{e}");
    assert!(e.contains("turnaround.stature"), "{e}");
}

/// The page whose value changed gets a new identifier, every other page keeps its own
#[test]
fn a_page_identifier_moves_with_its_own_values_only() {
    let f = fill("sheet_fill_example.json");
    let a = render(&f).unwrap();
    let b = render(&with(&f, "stature", "1.6")).unwrap();
    for (x, y) in a.pages.iter().zip(&b.pages) {
        if x.name == "turnaround" {
            assert_ne!(x.page_id, y.page_id, "turnaround");
        } else {
            assert_eq!(x.page_id, y.page_id, "{}", x.name);
        }
    }
    assert_ne!(a.set_id, b.set_id);
}

fn texts(svg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(i) = rest.find("<text ") {
        let body = &rest[i..];
        let s = body.find('>').unwrap() + 1;
        let e = body.find("</text>").unwrap();
        out.push(
            body[s..e]
                .replace("&amp;", "&")
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&quot;", "\""),
        );
        rest = &body[e..];
    }
    out
}

/// What `text allow` permits, from the law and the fill: page names, slot names, each filled value
/// with its unit (alone, or after its slot name), and the landmark names
fn allowed(f: &str) -> Vec<String> {
    let (l, _) = laws();
    let mut out: Vec<String> = l.pages.iter().map(|p| p.name.clone()).collect();
    let doc: serde_like::V = serde_like::parse(f);
    for s in &l.slots {
        out.push(s.name.clone());
        let page = doc.get("pages").get(&s.page);
        let vals: Vec<serde_like::V> = s.group.as_ref().map_or_else(
            || vec![page.get(&s.name)],
            |g| {
                page.get(g)
                    .items()
                    .into_iter()
                    .map(|i| i.get(&s.name))
                    .collect()
            },
        );
        for v in vals {
            if let Some(t) = v.text() {
                let with_unit = match (&s.unit, v.is_num()) {
                    (Some(u), true) => format!("{t} {u}"),
                    _ => t,
                };
                out.push(format!("{} {with_unit}", s.name));
                out.push(with_unit);
            }
        }
    }
    for n in [
        "head_top",
        "eye_line",
        "chin",
        "shoulder",
        "hips",
        "hip_joint",
        "knee",
        "ankle",
        "sole",
        "front",
        "side",
        "back",
        "top",
    ] {
        out.push(n.to_string());
    }
    out
}

#[test]
fn every_text_on_a_page_is_one_the_sheet_law_allows() {
    for data in [
        "sheet_fill_example.json",
        "sheet_fill_spread_small_young.json",
        "sheet_fill_spread_stylised.json",
        "sheet_fill_spread_tall_adult.json",
    ] {
        let f = fill(data);
        let ok = allowed(&f);
        let r = render(&f).unwrap();
        let mut n = 0;
        for p in &r.pages {
            for t in texts(&p.svg) {
                assert!(
                    ok.contains(&t),
                    "{data} page {}: {t:?} is not a text the law allows",
                    p.name
                );
                n += 1;
            }
        }
        assert!(n > 100, "{data}: {n} texts read");
    }
}

/// The left arm's polyline (shoulder, elbow, wrist) of the front view, in page mm
fn left_arm(svg: &str) -> Vec<(f64, f64)> {
    let i = svg.find("<polyline class=\"bone\" points=\"").unwrap() + 31;
    let pts = &svg[i..i + svg[i..].find('"').unwrap()];
    pts.split(' ')
        .map(|p| {
            let (x, y) = p.split_once(',').unwrap();
            (x.parse().unwrap(), y.parse().unwrap())
        })
        .collect()
}

fn values(f: &str, keys: &[&str]) -> Vec<f64> {
    let doc = serde_like::parse(f);
    keys.iter()
        .map(|k| {
            let (page, name) = k.split_once('.').unwrap();
            doc.get("pages").get(page).get(name).num().unwrap()
        })
        .collect()
}

#[test]
fn the_wrist_is_at_shoulder_x_plus_arm_in_a_t_pose_and_turned_down_in_an_a_pose() {
    let base = fill("sheet_fill_example.json");
    let v = values(
        &base,
        &[
            "turnaround.stature",
            "base_body.head_top",
            "base_body.shoulder_x",
            "base_body.arm",
        ],
    );
    let unit = v[0] * 1000.0 / v[1] / SCALE_BODY;
    let t = with(&base, "pose", "\"T\"");
    let arm = left_arm(&render(&t).unwrap().pages[0].svg);
    let (sh, wr) = (arm[0], arm[2]);
    assert!(
        (wr.0 - sh.0 - v[3] * unit).abs() < 0.002,
        "T: wrist {wr:?} shoulder {sh:?}"
    );
    assert!((wr.1 - sh.1).abs() < 0.002, "T: the arm is level");
    assert!(
        (sh.0 - (110.0 + v[2] * unit)).abs() < 0.002,
        "the shoulder at shoulder_x from the centre line"
    );
    let a = with(&base, "arm_angle", "60");
    let arm = left_arm(&render(&a).unwrap().pages[0].svg);
    let (sh, wr) = (arm[0], arm[2]);
    let (c, s) = (0.5, 3.0_f64.sqrt() / 2.0);
    assert!(
        (wr.0 - sh.0 - v[3] * unit * c).abs() < 0.002,
        "A 60: wrist x {wr:?}"
    );
    assert!(
        (wr.1 - sh.1 - v[3] * unit * s).abs() < 0.002,
        "A 60: wrist y {wr:?} (down the page)"
    );
}

#[test]
fn the_footwear_sole_lifts_the_feet_above_the_ground_line() {
    let f = fill("sheet_fill_example.json");
    let svg = &render(&f).unwrap().pages[0].svg;
    // the second bone polyline is the left leg: hip joint, knee, ankle, sole of the foot
    let second = svg
        .match_indices("<polyline class=\"bone\" points=\"")
        .nth(1)
        .unwrap()
        .0
        + 31;
    let pts = &svg[second..second + svg[second..].find('"').unwrap()];
    let foot: f64 = pts
        .rsplit(' ')
        .next()
        .unwrap()
        .split_once(',')
        .unwrap()
        .1
        .parse()
        .unwrap();
    // the ground line is at 270 mm; the shoes' sole is 8 mm at 1:10
    assert!(
        (foot - (270.0 - 8.0 / SCALE_BODY)).abs() < 0.002,
        "foot at {foot}"
    );
    let bare = with(&f, "sole", "0");
    let svg = &render(&bare).unwrap().pages[0].svg;
    let second = svg
        .match_indices("<polyline class=\"bone\" points=\"")
        .nth(1)
        .unwrap()
        .0
        + 31;
    let pts = &svg[second..second + svg[second..].find('"').unwrap()];
    let foot: f64 = pts
        .rsplit(' ')
        .next()
        .unwrap()
        .split_once(',')
        .unwrap()
        .1
        .parse()
        .unwrap();
    assert!(
        (foot - 270.0).abs() < 0.002,
        "no sole: foot on the ground ({foot})"
    );
}

/// A stylised fill is drawn with the stylised body law: its shoulder is the filled `shoulder`
/// (0.87 eye heights), where the reference law fixes it at 0.9
#[test]
fn the_fill_family_picks_the_body_law_that_draws_it() {
    let f = fill("sheet_fill_spread_stylised.json");
    let v = values(
        &f,
        &[
            "turnaround.stature",
            "base_body.head_top",
            "base_body.shoulder",
        ],
    );
    let unit = v[0] * 1000.0 / v[1] / SCALE_BODY;
    let svg = &render(&f).unwrap().pages[0].svg;
    let sh = left_arm(svg)[0];
    // the ground line at 270 mm, the body lifted by the 6 mm sole at 1:10
    let want = 270.0 - 6.0 / SCALE_BODY - v[2] * unit;
    assert!(
        (sh.1 - want).abs() < 0.002,
        "shoulder at {} (want {want}, 0.87 eye heights)",
        sh.1
    );
    let reference = with(&f, "family", "\"reference\"");
    let e = render(&reference).unwrap_err();
    assert!(
        e.contains("not complete"),
        "a stylised knee is out of the reference range: {e}"
    );
}

/// The `(x1, y1, x2, y2)` of every `<line>` of a class
fn lines(svg: &str, class: &str) -> Vec<[f64; 4]> {
    let tag = format!("<line class=\"{class}\" ");
    svg.match_indices(&tag)
        .map(|(i, _)| {
            let body = &svg[i..i + svg[i..].find("/>").unwrap()];
            let at = |k: &str| -> f64 {
                let s = body.find(&format!(" {k}=\"")).unwrap() + k.len() + 3;
                body[s..s + body[s..].find('"').unwrap()].parse().unwrap()
            };
            [at("x1"), at("y1"), at("x2"), at("y2")]
        })
        .collect()
}

/// A sleeve worn apart from the elbow to the wrist covers the arm from the elbow to its hem
/// (`hem` 0.9 of the way to the wrist), along the arm bone of the same page
#[test]
fn a_sleeve_on_the_arm_chain_runs_along_the_arm_to_its_hem() {
    let r = render(&fill("sheet_fill_spread_stylised.json")).unwrap();
    let svg = &r.pages.iter().find(|p| p.name == "clothing").unwrap().svg;
    let arm = left_arm(svg);
    let (e, w) = (arm[1], arm[2]);
    let s = lines(svg, "sleeve");
    assert_eq!(s.len(), 1, "one layer on the arm chain");
    let want = (e.0 + (w.0 - e.0) * 0.9, e.1 + (w.1 - e.1) * 0.9);
    assert!(
        (s[0][0] - e.0).abs() < 0.002 && (s[0][1] - e.1).abs() < 0.002,
        "from the elbow {s:?} {e:?}"
    );
    assert!(
        (s[0][2] - want.0).abs() < 0.002 && (s[0][3] - want.1).abs() < 0.002,
        "to the hem {s:?} {want:?}"
    );
}

/// Ears come in a pair, mirrored about the centre line; a tail is one; each is drawn at its length
#[test]
fn extra_parts_are_drawn_at_their_length_one_or_a_pair() {
    let f = fill("sheet_fill_spread_stylised.json");
    let v = values(&f, &["turnaround.stature", "base_body.head_top"]);
    let unit = v[0] * 1000.0 / v[1] / SCALE_BODY;
    let r = render(&f).unwrap();
    let svg = &r.pages.iter().find(|p| p.name == "anatomy").unwrap().svg;
    let x = lines(svg, "extra");
    assert_eq!(x.len(), 3, "a pair of ears and one tail: {x:?}");
    // sqrt of the sum of squares: f64::hypot is not correctly rounded and is banned in this workspace
    #[allow(clippy::imprecise_flops)]
    let len = |l: &[f64; 4]| {
        let (dx, dy) = (l[2] - l[0], l[3] - l[1]);
        (dx * dx + dy * dy).sqrt()
    };
    for (l, want) in [(&x[0], 0.09), (&x[1], 0.09), (&x[2], 0.62)] {
        assert!((len(l) - want * unit).abs() < 0.004, "{l:?} length {want}");
    }
    assert!(
        (x[0][0] - 110.0 + (x[1][0] - 110.0)).abs() < 0.002,
        "the ears mirror about the centre line"
    );
    assert!(x[0][3] < x[0][1], "an ear points up the page");
    assert!(x[2][3] > x[2][1], "a tail hangs down the page");
}

/// A minimal JSON reader for the oracles, independent of the crate's own
mod serde_like {
    #[derive(Clone, Debug)]
    pub enum V {
        Null,
        Bool(bool),
        Num(f64),
        Str(String),
        Arr(Vec<Self>),
        Obj(Vec<(String, Self)>),
    }

    impl V {
        pub fn get(&self, k: &str) -> Self {
            match self {
                Self::Obj(o) => o
                    .iter()
                    .find(|(n, _)| n == k)
                    .map_or(Self::Null, |(_, v)| v.clone()),
                _ => Self::Null,
            }
        }
        pub fn items(&self) -> Vec<Self> {
            match self {
                Self::Arr(a) => a.clone(),
                _ => Vec::new(),
            }
        }
        pub const fn num(&self) -> Option<f64> {
            match self {
                Self::Num(x) => Some(*x),
                _ => None,
            }
        }
        pub const fn is_num(&self) -> bool {
            matches!(self, Self::Num(_))
        }
        /// the value as written on a page: a number in its shortest round-trip form
        pub fn text(&self) -> Option<String> {
            match self {
                Self::Bool(b) => Some(b.to_string()),
                Self::Num(x) => Some(format!("{x}")),
                Self::Str(s) => Some(s.clone()),
                _ => None,
            }
        }
    }

    pub fn parse(s: &str) -> V {
        let b = s.as_bytes();
        let mut i = 0;
        value(b, &mut i)
    }

    const fn ws(b: &[u8], i: &mut usize) {
        while *i < b.len() && b[*i].is_ascii_whitespace() {
            *i += 1;
        }
    }

    fn value(b: &[u8], i: &mut usize) -> V {
        ws(b, i);
        match b[*i] {
            b'{' => {
                *i += 1;
                let mut o = Vec::new();
                loop {
                    ws(b, i);
                    if b[*i] == b'}' {
                        *i += 1;
                        return V::Obj(o);
                    }
                    let V::Str(k) = value(b, i) else {
                        panic!("key")
                    };
                    ws(b, i);
                    *i += 1; // ':'
                    let v = value(b, i);
                    o.push((k, v));
                    ws(b, i);
                    if b[*i] == b',' {
                        *i += 1;
                    }
                }
            }
            b'[' => {
                *i += 1;
                let mut a = Vec::new();
                loop {
                    ws(b, i);
                    if b[*i] == b']' {
                        *i += 1;
                        return V::Arr(a);
                    }
                    a.push(value(b, i));
                    ws(b, i);
                    if b[*i] == b',' {
                        *i += 1;
                    }
                }
            }
            b'"' => {
                *i += 1;
                let mut s = String::new();
                while b[*i] != b'"' {
                    if b[*i] == b'\\' {
                        *i += 1;
                        s.push(match b[*i] {
                            b'n' => '\n',
                            b't' => '\t',
                            c => c as char,
                        });
                        *i += 1;
                        continue;
                    }
                    let start = *i;
                    while b[*i] != b'"' && b[*i] != b'\\' {
                        *i += 1;
                    }
                    s.push_str(std::str::from_utf8(&b[start..*i]).unwrap());
                }
                *i += 1;
                V::Str(s)
            }
            b't' => {
                *i += 4;
                V::Bool(true)
            }
            b'f' => {
                *i += 5;
                V::Bool(false)
            }
            b'n' => {
                *i += 4;
                V::Null
            }
            _ => {
                let start = *i;
                while *i < b.len()
                    && (b[*i] == b'-'
                        || b[*i] == b'.'
                        || b[*i] == b'e'
                        || b[*i] == b'E'
                        || b[*i] == b'+'
                        || b[*i].is_ascii_digit())
                {
                    *i += 1;
                }
                V::Num(std::str::from_utf8(&b[start..*i]).unwrap().parse().unwrap())
            }
        }
    }
}
