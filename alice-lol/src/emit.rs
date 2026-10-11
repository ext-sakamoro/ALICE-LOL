//! `SdfNode` → LOL text (Track C0、`runtime_parser::parse_lol` の逆変換)
//!
//! [`to_lol`] は tree を LOL DSL text に書き戻す 出力は `lol.gbnf` (LLM 経路
//! grammar: comment なし、token 間 whitespace 1 文字) と `parse_lol` の両方が
//! 受理する正規形で、`parse_lol(to_lol(n))` は `n` と **eval parity** を持つ
//! (`Rotate` の Quat ↔ Euler 往復等で bit 一致はしないため、round-trip の判定は
//! 数点 eval の一致で行う `tests/emit_roundtrip.rs`)
//!
//! # 用途
//!
//! - 合成 data generator (C1〜): random tree / product shortcut 展開形を LOL
//!   text 化して (caption, LOL) pair の教師 data に
//! - `llm_bench` 等の出力表示、JSONL への保存
//! - `Program::to_lol` (`program(<sdf>, entities(...), <intent>)`)
//!
//! # 書けないもの
//!
//! LOL text 構文を持たない variant は [`EmitError::Unsupported`]:
//! `IFS` / `SdfSkinning` / `LatticeDeform` / `ProjectiveTransform` /
//! `HeightmapDisplacement` / `SineDisplacement` / `Polygon2D` `Capsule` は
//! Y 軸対称なら `capsule(r, h)`、それ以外は `capsule_ab(ax, ay, az, bx, by, bz, r)`
//!
//! # 正規形
//!
//! - 数値: 整数値は `1.0` (`fmt_f32`)、それ以外は shortest repr
//! - 可変長 op (`union` / `smooth_union` 等) は parser の `fold_left` が作る
//!   左結合 2 分木を平坦化して `union(a, b, c)` に戻す (同 op・同数値引数の
//!   左 spine だけ) stdlib product の穴格子は nested だと深さ 2,400 になる
//! - 角度は度 (parser 入力と同じ)、`Rotate` は `EulerRot::XYZ`

use std::fmt::Write as _;

use crate::intent::Program;
use crate::SdfNode;
use glam::{EulerRot, Vec2, Vec3};

/// [`to_lol`] のエラー
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EmitError {
    /// LOL text 構文を持たない variant
    Unsupported {
        /// variant 名 (`"IFS"` 等)
        variant: &'static str,
        /// 理由
        reason: &'static str,
    },
    /// 数値に NaN か ±∞ がある (LOL の数は有限の literal だけなので、書いても読み戻せない)
    NonFinite,
}

impl std::fmt::Display for EmitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported { variant, reason } => {
                write!(f, "SdfNode::{variant} has no LOL text form: {reason}")
            }
            Self::NonFinite => {
                write!(
                    f,
                    "the tree holds a number that is NaN or infinite (LOL numbers are finite)"
                )
            }
        }
    }
}

impl std::error::Error for EmitError {}

/// `f32` を LOL 数値リテラルに (`1` → `1.0`、それ以外は shortest round-trip repr)
///
/// `{v:?}` (`Debug`) は Rust の f32 の shortest round-trip 実装そのもの
/// (`parse::<f32>(fmt_f32(v)) == v` が全有限値で bit 一致、`-0.0` も符号を保つ)
/// 旧実装は `|v| < 1e-6` (`-0.0` 含む) を符号ごと `0.0` に丸めていたが、これは
/// `parse(emit(n)) == n` という faithful round-trip の不変条件そのものを破る
/// (`ellipsoid(5e-7,...)` の eval が丸めの前後で 1.0 → 1.73 に変わる実測、
/// `lidinoid(-0.0,1e8)` の符号消失による `eval` 不一致も同じ原因)
/// 丸めが要らなくなった意味の無い `-0.0` ノイズ (stdlib product 展開の
/// cos/sin から出る、[`canon_deg`] 自身の丸めが生む残差も含む) は、emit の
/// 手前でなく**その値を計算している側**で `+ 0.0` して消す (構築時の正規化、
/// 構築箇所は `alice-lol/src/stdlib/hardsurface/mount.rs` の `extrusion_profile`
/// と `canon_deg` 自身)
/// `NaN` / `inf` はそのまま出す (parser は読まない) [`to_lol`] は書く前に [`EmitError::NonFinite`] で止める
#[must_use]
pub fn fmt_f32(v: f32) -> String {
    format!("{v:?}")
}

/// Euler 角 (度) の正規形: 1e-3 度に丸め、`(-180, 180]` に wrap、`±180` は `+180`
///
/// 度 → rad → `Quat` → rad → 度 の往復は 1 ulp ずつ流れる (`1.0 → 1.0000001 →
/// 1.0000002`) ので、丸めないと `emit ∘ parse` が固定点にならない 1e-3 度
/// (1.7e-5 rad) は round-trip の eval tolerance (1e-4) より小さい
/// `(-180, ε, -180)` と `(180, ε, 180)` は同じ回転で `Quat::to_euler` は
/// どちらも返しうるため片側に寄せる
fn canon_deg(a: f32) -> f32 {
    let mut a = (a * 1000.0).round() / 1000.0;
    a = a.rem_euclid(360.0);
    if a > 180.0 {
        a -= 360.0;
    }
    if (a.abs() - 180.0).abs() < 1e-3 {
        180.0
    } else {
        a + 0.0 // -0.0 -> 0.0: a residual this small has no rotational meaning
    }
}

fn u(v: u32) -> String {
    format!("{v}.0")
}

/// `SdfNode` tree を LOL text に変換する
///
/// # Errors
///
/// tree 内に LOL 構文のない variant があると [`EmitError::Unsupported`]、
/// NaN か ±∞ の数があると [`EmitError::NonFinite`] (parser は有限の数しか読まないので、
/// 書いた text は必ず読み戻せる)
pub fn to_lol(node: &SdfNode) -> Result<String, EmitError> {
    let mut out = String::new();
    write_node(node, &mut out)?;
    Ok(out)
}

/// 深い tree でも stack を使い切らないよう、再帰ごとに必要なら stack を伸ばす
/// (`runtime_parser::parse_expr` と対、stdlib product の subtract 2,400 連鎖対応)
fn write_node(node: &SdfNode, out: &mut String) -> Result<(), EmitError> {
    stacker::maybe_grow(1024 * 1024, 16 * 1024 * 1024, || {
        write_node_inner(node, out)
    })
}

#[allow(clippy::too_many_lines)] // 121 variant の逆写像 table、分割すると parser との対応が追いにくい
fn write_node_inner(node: &SdfNode, out: &mut String) -> Result<(), EmitError> {
    // 引数だけの primitive
    macro_rules! prim {
        ($name:expr, $($arg:expr),+) => {{
            let args: Vec<String> = vec![$($arg),+];
            let _ = write!(out, "{}({})", $name, args.join(", "));
            Ok(())
        }};
    }
    // 数値 N 個 + child 1 個
    macro_rules! modif {
        ($name:expr, $child:expr $(, $arg:expr)*) => {{
            let _ = write!(out, "{}(", $name);
            $( let _ = write!(out, "{}, ", $arg); )*
            write_node($child, out)?;
            out.push(')');
            Ok(())
        }};
    }
    // 数値 N 個 + a, b
    macro_rules! binop {
        ($name:expr, $a:expr, $b:expr $(, $arg:expr)*) => {{
            let _ = write!(out, "{}(", $name);
            $( let _ = write!(out, "{}, ", $arg); )*
            write_node($a, out)?;
            out.push_str(", ");
            write_node($b, out)?;
            out.push(')');
            Ok(())
        }};
    }
    // every number goes through these: a NaN or an infinity is recorded and the node is
    // refused below (fmt_f32 would write `inf` / `NaN`, which the parser does not read)
    let non_finite = std::cell::Cell::new(false);
    let f = |v: f32| {
        if !v.is_finite() {
            non_finite.set(true);
        }
        fmt_f32(v)
    };
    let v3 = |v: Vec3| format!("{}, {}, {}", f(v.x), f(v.y), f(v.z));
    let v2 = |v: Vec2| format!("{}, {}", f(v.x), f(v.y));

    let written = match node {
        // ── Primitives ──
        SdfNode::Sphere { radius } => prim!("sphere", f(*radius)),
        SdfNode::Box3d { half_extents } => prim!("box3d", v3(*half_extents)),
        SdfNode::RoundedBox {
            half_extents,
            round_radius,
        } => prim!("rounded_box", v3(*half_extents), f(*round_radius)),
        SdfNode::Cylinder {
            radius,
            half_height,
        } => prim!("cylinder", f(*radius), f(*half_height)),
        SdfNode::Torus {
            major_radius,
            minor_radius,
        } => prim!("torus", f(*major_radius), f(*minor_radius)),
        SdfNode::Cone {
            radius,
            half_height,
        } => prim!("cone", f(*radius), f(*half_height)),
        SdfNode::Capsule {
            point_a,
            point_b,
            radius,
        } => {
            // capsule(r, h) は Y 軸対称 (0,-h,0)-(0,h,0) の短縮形、それ以外は capsule_ab
            // short form は X/Z の座標を書かない (読み戻すと常に 0.0) ので、判定は
            // 書く値そのもので行う exact 0.0 (`== 0.0` は `-0.0` も含めて真になる、
            // IEEE754 の符号無し比較) fmt_f32 が faithful になった今、ここで近傍 0 を
            // 0 扱いにすると、1e-30 のような非 0 座標の short form が round-trip で
            // その座標を失う (`capsule_ab` のまま書くべき形を short form に丸めてしまう)
            let h = point_b.y;
            let symmetric = point_a.x == 0.0
                && point_a.z == 0.0
                && point_b.x == 0.0
                && point_b.z == 0.0
                && (point_a.y + h).abs() <= f32::EPSILON * h.abs().max(1.0);
            if symmetric {
                prim!("capsule", f(*radius), f(h))
            } else {
                prim!("capsule_ab", v3(*point_a), v3(*point_b), f(*radius))
            }
        }
        SdfNode::Ellipsoid { radii } => prim!("ellipsoid", v3(*radii)),
        SdfNode::Plane { normal, distance } => prim!("plane", v3(*normal), f(*distance)),
        SdfNode::Octahedron { size } => prim!("octahedron", f(*size)),
        SdfNode::RoundedCone {
            r1,
            r2,
            half_height,
        } => prim!("rounded_cone", f(*r1), f(*r2), f(*half_height)),
        SdfNode::Pyramid { half_height } => prim!("pyramid", f(*half_height)),
        SdfNode::HexPrism {
            hex_radius,
            half_height,
        } => prim!("hex_prism", f(*hex_radius), f(*half_height)),
        SdfNode::Link {
            half_length,
            r1,
            r2,
        } => prim!("link", f(*half_length), f(*r1), f(*r2)),
        SdfNode::CappedCone {
            half_height,
            r1,
            r2,
        } => prim!("capped_cone", f(*half_height), f(*r1), f(*r2)),
        SdfNode::CappedTorus {
            major_radius,
            minor_radius,
            cap_angle,
        } => prim!(
            "capped_torus",
            f(*major_radius),
            f(*minor_radius),
            f(*cap_angle)
        ),
        SdfNode::RoundedCylinder {
            radius,
            round_radius,
            half_height,
        } => prim!(
            "rounded_cylinder",
            f(*radius),
            f(*round_radius),
            f(*half_height)
        ),
        SdfNode::Tube {
            outer_radius,
            thickness,
            half_height,
        } => prim!("tube", f(*outer_radius), f(*thickness), f(*half_height)),
        SdfNode::Barrel {
            radius,
            half_height,
            bulge,
        } => prim!("barrel", f(*radius), f(*half_height), f(*bulge)),
        SdfNode::Heart { size } => prim!("heart", f(*size)),
        SdfNode::Egg { ra, rb } => prim!("egg", f(*ra), f(*rb)),
        SdfNode::Helix {
            major_r,
            minor_r,
            pitch,
            half_height,
        } => prim!(
            "helix",
            f(*major_r),
            f(*minor_r),
            f(*pitch),
            f(*half_height)
        ),
        SdfNode::Tetrahedron { size } => prim!("tetrahedron", f(*size)),
        SdfNode::BoxFrame { half_extents, edge } => {
            prim!("box_frame", v3(*half_extents), f(*edge))
        }
        SdfNode::Diamond {
            radius,
            half_height,
        } => prim!("diamond", f(*radius), f(*half_height)),
        SdfNode::StarPolygon {
            radius,
            n_points,
            m,
            half_height,
        } => prim!(
            "star_polygon",
            f(*radius),
            f(*n_points),
            f(*m),
            f(*half_height)
        ),
        SdfNode::CrossShape {
            length,
            thickness,
            round_radius,
            half_height,
        } => prim!(
            "cross_shape",
            f(*length),
            f(*thickness),
            f(*round_radius),
            f(*half_height)
        ),
        SdfNode::Triangle {
            point_a,
            point_b,
            point_c,
        } => prim!("triangle", v3(*point_a), v3(*point_b), v3(*point_c)),
        SdfNode::Bezier {
            point_a,
            point_b,
            point_c,
            radius,
        } => prim!(
            "bezier",
            v3(*point_a),
            v3(*point_b),
            v3(*point_c),
            f(*radius)
        ),
        SdfNode::TriangularPrism { width, half_depth } => {
            prim!("triangular_prism", f(*width), f(*half_depth))
        }
        SdfNode::CutSphere { radius, cut_height } => {
            prim!("cut_sphere", f(*radius), f(*cut_height))
        }
        SdfNode::CutHollowSphere {
            radius,
            cut_height,
            thickness,
        } => prim!(
            "cut_hollow_sphere",
            f(*radius),
            f(*cut_height),
            f(*thickness)
        ),
        SdfNode::DeathStar { ra, rb, d } => prim!("death_star", f(*ra), f(*rb), f(*d)),
        SdfNode::SolidAngle { angle, radius } => prim!("solid_angle", f(*angle), f(*radius)),
        SdfNode::Rhombus {
            la,
            lb,
            half_height,
            round_radius,
        } => prim!("rhombus", f(*la), f(*lb), f(*half_height), f(*round_radius)),
        SdfNode::Horseshoe {
            angle,
            radius,
            half_length,
            width,
            thickness,
        } => prim!(
            "horseshoe",
            f(*angle),
            f(*radius),
            f(*half_length),
            f(*width),
            f(*thickness)
        ),
        SdfNode::Vesica { radius, half_dist } => prim!("vesica", f(*radius), f(*half_dist)),
        SdfNode::InfiniteCylinder { radius } => prim!("infinite_cylinder", f(*radius)),
        SdfNode::InfiniteCone { angle } => prim!("infinite_cone", f(*angle)),
        SdfNode::Gyroid { scale, thickness } => prim!("gyroid", f(*scale), f(*thickness)),
        SdfNode::ChamferedCube {
            half_extents,
            chamfer,
        } => prim!("chamfered_cube", v3(*half_extents), f(*chamfer)),
        SdfNode::SchwarzP { scale, thickness } => prim!("schwarz_p", f(*scale), f(*thickness)),
        SdfNode::Superellipsoid {
            half_extents,
            e1,
            e2,
        } => prim!("superellipsoid", v3(*half_extents), f(*e1), f(*e2)),
        SdfNode::RoundedX {
            width,
            round_radius,
            half_height,
        } => prim!("rounded_x", f(*width), f(*round_radius), f(*half_height)),
        SdfNode::Pie {
            angle,
            radius,
            half_height,
        } => prim!("pie", f(*angle), f(*radius), f(*half_height)),
        SdfNode::Trapezoid {
            r1,
            r2,
            trap_height,
            half_depth,
        } => prim!("trapezoid", f(*r1), f(*r2), f(*trap_height), f(*half_depth)),
        SdfNode::Parallelogram {
            width,
            para_height,
            skew,
            half_depth,
        } => prim!(
            "parallelogram",
            f(*width),
            f(*para_height),
            f(*skew),
            f(*half_depth)
        ),
        SdfNode::Tunnel {
            width,
            height_2d,
            half_depth,
        } => prim!("tunnel", f(*width), f(*height_2d), f(*half_depth)),
        SdfNode::UnevenCapsule {
            r1,
            r2,
            cap_height,
            half_depth,
        } => prim!(
            "uneven_capsule",
            f(*r1),
            f(*r2),
            f(*cap_height),
            f(*half_depth)
        ),
        SdfNode::ArcShape {
            aperture,
            radius,
            thickness,
            half_height,
        } => prim!(
            "arc_shape",
            f(*aperture),
            f(*radius),
            f(*thickness),
            f(*half_height)
        ),
        SdfNode::Moon {
            d,
            ra,
            rb,
            half_height,
        } => prim!("moon", f(*d), f(*ra), f(*rb), f(*half_height)),
        SdfNode::BlobbyCross { size, half_height } => {
            prim!("blobby_cross", f(*size), f(*half_height))
        }
        SdfNode::ParabolaSegment {
            width,
            para_height,
            half_depth,
        } => prim!(
            "parabola_segment",
            f(*width),
            f(*para_height),
            f(*half_depth)
        ),
        SdfNode::RegularPolygon {
            radius,
            n_sides,
            half_height,
        } => prim!("regular_polygon", f(*radius), f(*n_sides), f(*half_height)),
        SdfNode::Stairs {
            step_width,
            step_height,
            n_steps,
            half_depth,
        } => prim!(
            "stairs_prim",
            f(*step_width),
            f(*step_height),
            f(*n_steps),
            f(*half_depth)
        ),
        SdfNode::Dodecahedron { radius } => prim!("dodecahedron", f(*radius)),
        SdfNode::Icosahedron { radius } => prim!("icosahedron", f(*radius)),
        SdfNode::TruncatedOctahedron { radius } => prim!("truncated_octahedron", f(*radius)),
        SdfNode::TruncatedIcosahedron { radius } => prim!("truncated_icosahedron", f(*radius)),
        SdfNode::DiamondSurface { scale, thickness } => {
            prim!("diamond_surface", f(*scale), f(*thickness))
        }
        SdfNode::Neovius { scale, thickness } => prim!("neovius", f(*scale), f(*thickness)),
        SdfNode::Lidinoid { scale, thickness } => prim!("lidinoid", f(*scale), f(*thickness)),
        SdfNode::IWP { scale, thickness } => prim!("iwp", f(*scale), f(*thickness)),
        SdfNode::FRD { scale, thickness } => prim!("frd", f(*scale), f(*thickness)),
        SdfNode::FischerKochS { scale, thickness } => {
            prim!("fischer_koch_s", f(*scale), f(*thickness))
        }
        SdfNode::PMY { scale, thickness } => prim!("pmy", f(*scale), f(*thickness)),
        SdfNode::Circle2D {
            radius,
            half_height,
        } => prim!("circle_2d", f(*radius), f(*half_height)),
        SdfNode::Rect2D {
            half_extents,
            half_height,
        } => prim!("rect_2d", v2(*half_extents), f(*half_height)),
        SdfNode::Segment2D {
            a,
            b,
            thickness,
            half_height,
        } => prim!("segment_2d", v2(*a), v2(*b), f(*thickness), f(*half_height)),
        SdfNode::RoundedRect2D {
            half_extents,
            round_radius,
            half_height,
        } => prim!(
            "rounded_rect_2d",
            v2(*half_extents),
            f(*round_radius),
            f(*half_height)
        ),
        SdfNode::Annular2D {
            outer_radius,
            thickness,
            half_height,
        } => prim!(
            "annular_2d",
            f(*outer_radius),
            f(*thickness),
            f(*half_height)
        ),
        SdfNode::Terrain { scale, amplitude } => prim!("terrain", f(*scale), f(*amplitude)),

        // ── CSG ──
        // 可変長 op (parser が fold_left で左結合 2 分木にするもの) は左 spine を
        // 平坦化して variadic で書く = fold_left の正確な逆写像 nested のままだと
        // stdlib product の穴格子 union が深さ 2,400 の tree になり parser の再帰
        // (parse_expr の大 frame) で stack が尽きる
        SdfNode::Union { .. }
        | SdfNode::Intersection { .. }
        | SdfNode::SmoothUnion { .. }
        | SdfNode::SmoothIntersection { .. }
        | SdfNode::ChamferUnion { .. }
        | SdfNode::ChamferIntersection { .. }
        | SdfNode::StairsUnion { .. }
        | SdfNode::StairsIntersection { .. }
        | SdfNode::ColumnsUnion { .. }
        | SdfNode::ColumnsIntersection { .. }
        | SdfNode::ExpSmoothUnion { .. }
        | SdfNode::ExpSmoothIntersection { .. } => write_variadic(node, out),
        SdfNode::Subtraction { a, b } => binop!("subtract", a, b),
        SdfNode::XOR { a, b } => binop!("xor", a, b),
        SdfNode::SmoothSubtraction { a, b, k } => binop!("smooth_subtract", a, b, f(*k)),
        SdfNode::ChamferSubtraction { a, b, r } => binop!("chamfer_subtraction", a, b, f(*r)),
        SdfNode::StairsSubtraction { a, b, r, n } => {
            binop!("stairs_subtraction", a, b, f(*r), f(*n))
        }
        SdfNode::Pipe { a, b, r } => binop!("pipe", a, b, f(*r)),
        SdfNode::Engrave { a, b, r } => binop!("engrave", a, b, f(*r)),
        SdfNode::Groove { a, b, ra, rb } => binop!("groove", a, b, f(*ra), f(*rb)),
        SdfNode::Tongue { a, b, ra, rb } => binop!("tongue", a, b, f(*ra), f(*rb)),
        SdfNode::ColumnsSubtraction { a, b, r, n } => {
            binop!("columns_subtraction", a, b, f(*r), f(*n))
        }
        SdfNode::ExpSmoothSubtraction { a, b, k } => {
            binop!("exp_smooth_subtraction", a, b, f(*k))
        }
        SdfNode::Morph { a, b, t } => binop!("morph", a, b, f(*t)),

        // ── Transforms ──
        SdfNode::Translate { child, offset } => modif!("translate", child, v3(*offset)),
        SdfNode::Rotate { child, rotation } => {
            let (rx, ry, rz) = rotation.to_euler(EulerRot::XYZ);
            modif!(
                "rotate",
                child,
                f(canon_deg(rx.to_degrees())),
                f(canon_deg(ry.to_degrees())),
                f(canon_deg(rz.to_degrees()))
            )
        }
        SdfNode::Scale { child, factor } => modif!("scale", child, f(*factor)),
        SdfNode::ScaleNonUniform { child, factors } => {
            modif!("scale_non_uniform", child, v3(*factors))
        }

        // ── Modifiers ──
        SdfNode::Round { child, radius } => modif!("round", child, f(*radius)),
        SdfNode::Onion { child, thickness } => modif!("onion", child, f(*thickness)),
        SdfNode::Twist { child, strength } => modif!("twist", child, f(*strength)),
        SdfNode::Bend { child, curvature } => modif!("bend", child, f(*curvature)),
        SdfNode::Mirror { child, axes } => modif!("mirror", child, v3(*axes)),
        SdfNode::RepeatInfinite { child, spacing } => modif!("repeat", child, v3(*spacing)),
        SdfNode::Elongate { child, amount } => modif!("elongate", child, v3(*amount)),
        SdfNode::Revolution { child, offset } => modif!("revolution", child, f(*offset)),
        SdfNode::Extrude { child, half_height } => modif!("extrude", child, f(*half_height)),
        SdfNode::Taper { child, factor, .. } => modif!("taper", child, f(*factor)),
        SdfNode::Displacement { child, strength } => modif!("displacement", child, f(*strength)),
        SdfNode::PolarRepeat { child, count } => modif!("polar_repeat", child, u(*count)),
        SdfNode::Shear { child, shear } => modif!("shear", child, v3(*shear)),
        SdfNode::Noise {
            child,
            amplitude,
            frequency,
            seed,
        } => modif!("noise", child, f(*amplitude), f(*frequency), u(*seed)),
        SdfNode::RepeatFinite {
            child,
            count,
            spacing,
        } => modif!(
            "repeat_finite",
            child,
            u(count[0]),
            u(count[1]),
            u(count[2]),
            v3(*spacing)
        ),
        SdfNode::OctantMirror { child } => modif!("octant_mirror", child),
        SdfNode::IcosahedralSymmetry { child } => modif!("icosahedral_symmetry", child),
        SdfNode::WithMaterial { child, material_id } => {
            modif!("with_material", child, u(*material_id))
        }
        SdfNode::SurfaceRoughness {
            child,
            frequency,
            amplitude,
            octaves,
        } => modif!(
            "surface_roughness",
            child,
            f(*frequency),
            f(*amplitude),
            u(*octaves)
        ),
        SdfNode::SweepBezier { child, p0, p1, p2 } => {
            modif!("sweep_bezier", child, v2(*p0), v2(*p1), v2(*p2))
        }
        SdfNode::Animated {
            child,
            speed,
            amplitude,
        } => modif!("animate", child, f(*speed), f(*amplitude)),

        // ── LOL 構文なし ──
        SdfNode::IFS { .. } => Err(EmitError::Unsupported {
            variant: "IFS",
            reason: "iterated function systems have no LOL construct",
        }),
        SdfNode::SdfSkinning { .. } => Err(EmitError::Unsupported {
            variant: "SdfSkinning",
            reason: "bone skinning has no LOL construct",
        }),
        SdfNode::LatticeDeform { .. } => Err(EmitError::Unsupported {
            variant: "LatticeDeform",
            reason: "lattice deformation has no LOL construct",
        }),
        SdfNode::ProjectiveTransform { .. } => Err(EmitError::Unsupported {
            variant: "ProjectiveTransform",
            reason: "4x4 projective transform has no LOL construct",
        }),
        SdfNode::HeightmapDisplacement { .. } => Err(EmitError::Unsupported {
            variant: "HeightmapDisplacement",
            reason: "heightmap data (Phase 1) has no LOL construct",
        }),
        SdfNode::SineDisplacement { .. } => Err(EmitError::Unsupported {
            variant: "SineDisplacement",
            reason: "sine displacement has no LOL construct",
        }),
        SdfNode::Polygon2D { .. } => Err(EmitError::Unsupported {
            variant: "Polygon2D",
            reason: "arbitrary vertex lists have no LOL construct",
        }),
        SdfNode::MetricBall { .. } => Err(EmitError::Unsupported {
            variant: "MetricBall",
            reason: "weighted-norm balls have no LOL construct (alice-sdf 4.0)",
        }),
        SdfNode::MetricBlend { .. } => Err(EmitError::Unsupported {
            variant: "MetricBlend",
            reason: "metric blends have no LOL construct (alice-sdf 4.0)",
        }),

        // `SdfNode` は alice-sdf 4.0 で `#[non_exhaustive]` になったので、
        // この arm が無いと下流はコンパイルできない。代わりに「新しい variant
        // を足したら emit が落ちる」という以前の性質は失われるので、**未知の
        // variant は黙って捨てず Unsupported として報告する**。LOL 構文を
        // 持つべき variant が本当に抜けていないかは、grammar corpus の
        // round-trip test (`tests/emit_roundtrip.rs`) が別途見ている。
        _ => Err(EmitError::Unsupported {
            variant: "<unknown>",
            reason: "variant added in a newer alice-sdf than this emitter knows",
        }),
    };
    written?;
    if non_finite.get() {
        return Err(EmitError::NonFinite);
    }
    Ok(())
}

/// 可変長 op の識別: (name, 数値引数, a, b) 同 name + 同引数の左 spine を平坦化する
fn variadic_parts(node: &SdfNode) -> Option<(&'static str, Vec<f32>, &SdfNode, &SdfNode)> {
    Some(match node {
        SdfNode::Union { a, b } => ("union", vec![], a, b),
        SdfNode::Intersection { a, b } => ("intersection", vec![], a, b),
        SdfNode::SmoothUnion { a, b, k } => ("smooth_union", vec![*k], a, b),
        SdfNode::SmoothIntersection { a, b, k } => ("smooth_intersection", vec![*k], a, b),
        SdfNode::ChamferUnion { a, b, r } => ("chamfer_union", vec![*r], a, b),
        SdfNode::ChamferIntersection { a, b, r } => ("chamfer_intersection", vec![*r], a, b),
        SdfNode::StairsUnion { a, b, r, n } => ("stairs_union", vec![*r, *n], a, b),
        SdfNode::StairsIntersection { a, b, r, n } => ("stairs_intersection", vec![*r, *n], a, b),
        SdfNode::ColumnsUnion { a, b, r, n } => ("columns_union", vec![*r, *n], a, b),
        SdfNode::ColumnsIntersection { a, b, r, n } => ("columns_intersection", vec![*r, *n], a, b),
        SdfNode::ExpSmoothUnion { a, b, k } => ("exp_smooth_union", vec![*k], a, b),
        SdfNode::ExpSmoothIntersection { a, b, k } => ("exp_smooth_intersection", vec![*k], a, b),
        _ => return None,
    })
}

/// 左 spine を平坦化して `name(args…, c1, c2, …)` を書く (`fold_left` の逆)
fn write_variadic(node: &SdfNode, out: &mut String) -> Result<(), EmitError> {
    let (name, args, mut a, b) =
        variadic_parts(node).expect("write_variadic called on a non-variadic node");
    // 左 spine: 同 name・同数値引数の間だけ潰す (k が違う smooth_union は別 op)
    let mut rev_children: Vec<&SdfNode> = vec![b];
    while let Some((n2, args2, a2, b2)) = variadic_parts(a) {
        // bit-exact: fmt_f32 is now a faithful (lossless) round-trip, so two values
        // that print the same text are, by construction, bit-identical -- comparing
        // the raw bits already matches "the same as written"
        let same = args2.len() == args.len()
            && args2
                .iter()
                .zip(&args)
                .all(|(x, y)| x.to_bits() == y.to_bits());
        if n2 != name || !same {
            break;
        }
        rev_children.push(b2);
        a = a2;
    }
    rev_children.push(a);
    if args.iter().any(|v| !v.is_finite()) {
        return Err(EmitError::NonFinite);
    }
    let _ = write!(out, "{name}(");
    for v in &args {
        let _ = write!(out, "{}, ", fmt_f32(*v));
    }
    for (i, c) in rev_children.iter().rev().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        write_node(c, out)?;
    }
    out.push(')');
    Ok(())
}

impl Program {
    /// `program(<sdf>, entities(...), <intent>)` 形式の LOL text
    ///
    /// registry も intent も空なら裸の `<sdf>` を返す (`parse_program` は
    /// どちらも受理し、後者は `Program::sdf_only` に戻る)
    ///
    /// # Errors
    ///
    /// `sdf` / `sdf_registry` に LOL 構文のない variant があると [`EmitError::Unsupported`]
    pub fn to_lol(&self) -> Result<String, EmitError> {
        if self.sdf_registry.is_empty() && self.intent.is_none() {
            return to_lol(&self.sdf);
        }
        let mut out = String::from("program(");
        write_node(&self.sdf, &mut out)?;
        out.push_str(", entities(");
        for (i, e) in self.sdf_registry.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            write_node(e, &mut out)?;
        }
        out.push(')');
        if let Some(intent) = &self.intent {
            out.push_str(", ");
            out.push_str(&intent.to_lol());
        }
        out.push(')');
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_parser::{parse_lol, parse_program};

    #[test]
    fn fmt_f32_forms() {
        assert_eq!(fmt_f32(1.0), "1.0");
        assert_eq!(fmt_f32(-2.0), "-2.0");
        assert_eq!(fmt_f32(0.5), "0.5");
        assert_eq!(fmt_f32(1.25), "1.25");
        assert_eq!(fmt_f32(0.1), "0.1");
        assert_eq!(fmt_f32(-0.0), "-0.0");
        assert_eq!(fmt_f32(4e-7), "4e-7");
    }

    #[test]
    fn primitives_and_csg_round_trip_textually() {
        for src in [
            "sphere(1.0)",
            "box3d(1.0, 2.0, 3.0)",
            "union(sphere(1.0), box3d(0.5, 0.5, 0.5), cylinder(0.3, 2.0))",
            "smooth_union(0.3, sphere(1.0), box3d(0.5, 0.5, 0.5))",
            "subtract(cylinder(2.5, 0.125), polar_repeat(12.0, translate(1.8, 0.0, 0.0, cylinder(0.3, 0.2))))",
            "repeat_finite(3.0, 2.0, 1.0, 10.0, 10.0, 0.0, sphere(1.0))",
            "translate(0.0, 1.5, 0.0, scale(2.0, sphere(0.5)))",
        ] {
            let node = parse_lol(src).unwrap();
            assert_eq!(to_lol(&node).unwrap(), src);
        }
    }

    #[test]
    fn variadic_ops_are_flattened_back() {
        // parser: union(a, b, c) → Union{Union{a,b},c} (fold_left) → emit は平坦化して戻す
        let node = parse_lol("union(sphere(1.0), sphere(2.0), sphere(3.0))").unwrap();
        assert_eq!(
            to_lol(&node).unwrap(),
            "union(sphere(1.0), sphere(2.0), sphere(3.0))"
        );
        // 明示的に nested で書いた union も同じ正規形に落ちる (同じ tree なので)
        let nested = parse_lol("union(union(sphere(1.0), sphere(2.0)), sphere(3.0))").unwrap();
        assert_eq!(
            to_lol(&nested).unwrap(),
            "union(sphere(1.0), sphere(2.0), sphere(3.0))"
        );
        // 右側の nested は別 tree なので潰さない
        let right = parse_lol("union(sphere(1.0), union(sphere(2.0), sphere(3.0)))").unwrap();
        assert_eq!(
            to_lol(&right).unwrap(),
            "union(sphere(1.0), union(sphere(2.0), sphere(3.0)))"
        );
        // k が違う smooth_union は別 op として残る
        let mixed = parse_lol(
            "smooth_union(0.5, smooth_union(0.2, sphere(1.0), sphere(2.0)), sphere(3.0))",
        )
        .unwrap();
        assert_eq!(
            to_lol(&mixed).unwrap(),
            "smooth_union(0.5, smooth_union(0.2, sphere(1.0), sphere(2.0)), sphere(3.0))"
        );
    }

    /// The merge decision is exact bit equality, not a `< 1e-6` tolerance: two
    /// `smooth_union` `k` values that differ by less than 1e-6 but are not
    /// bit-identical are a genuinely different operation each (`fmt_f32` now
    /// writes each faithfully, so they print different text too) -- merging
    /// them into one chain would silently drop one of the two k values.
    #[test]
    fn variadic_merge_does_not_fold_values_closer_than_the_old_tolerance() {
        let text =
            "smooth_union(0.5, smooth_union(0.5000001, sphere(1.0), sphere(2.0)), sphere(3.0))";
        let node = parse_lol(text).unwrap();
        assert_eq!(
            to_lol(&node).unwrap(),
            text,
            "two different (if close) k values must not merge into one smooth_union call"
        );
    }

    /// Bit equality, not plain `==`: `0.0 == -0.0` is true under IEEE754 even
    /// though the two have different bit patterns (and `fmt_f32` -- faithful
    /// now -- writes them as different text), so a merge check weakened to
    /// `x == y` would still fold these two apart k values into one call.
    #[test]
    fn variadic_merge_does_not_fold_negative_and_positive_zero() {
        let text = "smooth_union(0.0, smooth_union(-0.0, sphere(1.0), sphere(2.0)), sphere(3.0))";
        let node = parse_lol(text).unwrap();
        assert_eq!(
            to_lol(&node).unwrap(),
            text,
            "0.0 and -0.0 k values must not merge into one smooth_union call"
        );
    }

    #[test]
    fn rotate_round_trips_through_euler_degrees() {
        let node = parse_lol("rotate(90.0, 0.0, 0.0, cylinder(1.0, 2.0))").unwrap();
        let text = to_lol(&node).unwrap();
        assert!(text.starts_with("rotate("), "{text}");
        let back = parse_lol(&text).unwrap();
        for p in [
            Vec3::new(0.0, 0.0, 1.5),
            Vec3::new(0.0, 1.5, 0.0),
            Vec3::ZERO,
        ] {
            assert!((crate::eval(&node, p) - crate::eval(&back, p)).abs() < 1e-4);
        }
    }

    #[test]
    fn program_round_trip() {
        let src = "program(sphere(1.0), entities(box3d(0.1, 0.1, 0.1)), seq(grasp(0, right, 5.0), rest(500)))";
        let p = parse_program(src).unwrap();
        assert_eq!(p.to_lol().unwrap(), src);
        let bare = parse_program("sphere(2.0)").unwrap();
        assert_eq!(bare.to_lol().unwrap(), "sphere(2.0)");
    }

    #[test]
    fn capsule_uses_short_form_only_when_y_symmetric() {
        let node = SdfNode::Capsule {
            point_a: Vec3::new(1.0, 0.0, 0.0),
            point_b: Vec3::new(0.0, 1.0, 0.0),
            radius: 0.5,
        };
        assert_eq!(
            to_lol(&node).unwrap(),
            "capsule_ab(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.5)"
        );
        let sym = parse_lol("capsule(0.5, 2.0)").unwrap();
        assert_eq!(to_lol(&sym).unwrap(), "capsule(0.5, 2.0)");
        let back = parse_lol(&to_lol(&node).unwrap()).unwrap();
        assert!(matches!(back, SdfNode::Capsule { radius, .. } if (radius - 0.5).abs() < 1e-6));
    }

    /// The symmetry check is exact equality (`== 0.0`), not a `< 1e-6`
    /// tolerance, and it is independently checked on all four coordinates
    /// (`point_a.x`, `point_a.z`, `point_b.x`, `point_b.z`): a tiny but
    /// nonzero value on any one of them (`fmt_f32` is faithful now, so this is
    /// no longer "noise" that a tolerance check could reasonably discard) must
    /// stay in the long `capsule_ab` form, or that coordinate is silently lost
    /// on the first round trip through the short form. Pinning only one of
    /// the four (as an earlier version of this test did) left the other three
    /// weakenable to `|v| < 1e-6` without any test going red.
    #[test]
    fn capsule_tiny_nonzero_on_any_symmetry_coordinate_is_not_short_formed() {
        let tiny = 1e-7_f32;
        // Y is made exactly symmetric in every case (point_a.y == -point_b.y)
        // so each case isolates its own X/Z check alone -- a Y mismatch would
        // already force the long form on its own, masking a mutant that only
        // weakens one X/Z check.
        let cases: [(&str, Vec3, Vec3); 4] = [
            (
                "point_a.x",
                Vec3::new(tiny, -1.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
            ),
            (
                "point_a.z",
                Vec3::new(0.0, -1.0, tiny),
                Vec3::new(0.0, 1.0, 0.0),
            ),
            (
                "point_b.x",
                Vec3::new(0.0, -1.0, 0.0),
                Vec3::new(tiny, 1.0, 0.0),
            ),
            (
                "point_b.z",
                Vec3::new(0.0, -1.0, 0.0),
                Vec3::new(0.0, 1.0, tiny),
            ),
        ];
        for (name, point_a, point_b) in cases {
            let node = SdfNode::Capsule {
                point_a,
                point_b,
                radius: 0.5,
            };
            let text = to_lol(&node).unwrap();
            assert!(
                text.starts_with("capsule_ab("),
                "{name}: a 1e-7 coordinate must not be treated as symmetric: {text:?}"
            );
            let back = parse_lol(&text).unwrap();
            let SdfNode::Capsule {
                point_a: pa,
                point_b: pb,
                ..
            } = back
            else {
                panic!("expected Capsule, got {back:?}");
            };
            let got = match name {
                "point_a.x" => pa.x,
                "point_a.z" => pa.z,
                "point_b.x" => pb.x,
                "point_b.z" => pb.z,
                _ => unreachable!(),
            };
            assert_eq!(got.to_bits(), tiny.to_bits(), "{name}: {text:?}");
        }
    }

    #[test]
    fn ifs_is_unsupported() {
        let node = SdfNode::IFS {
            child: std::sync::Arc::new(SdfNode::Sphere { radius: 1.0 }),
            transforms: Vec::new(),
            iterations: 1,
        };
        assert!(matches!(
            to_lol(&node),
            Err(EmitError::Unsupported { variant: "IFS", .. })
        ));
    }
}
