//! ALICE-LOL `proc_macro`: LOL DSL → `SdfNode` construction code
//!
//! # Syntax (v0.5)
//!
//! ```ignore
//! lol! {
//!     field SceneName {
//!         smooth_union(0.2,
//!             sphere(1.0),
//!             translate(2.0, 0.0, 0.0,
//!                 box3d(0.5, 0.5, 0.5)
//!             )
//!         )
//!     }
//! }
//! ```
//!
//! Also supports bare expressions without the `field` wrapper:
//! ```ignore
//! lol! { sphere(1.0) }
//! ```
//!
//! Runtime variable capture with `{expr}` in numeric positions:
//! ```ignore
//! let r = 1.5_f32;
//! lol! { sphere({r}) }
//! lol! { translate({x}, {y}, 0.0, sphere({r * 2.0})) }
//! ```

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    Ident, Result, Token,
};

/// Value token: either a literal float or a runtime expression.
type V = TokenStream2;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// LOL Internal AST
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[allow(clippy::enum_variant_names)]
enum Expr {
    // ── Primitives (27) ──
    Sphere {
        radius: V,
    },
    Box3d {
        hx: V,
        hy: V,
        hz: V,
    },
    RoundedBox {
        hx: V,
        hy: V,
        hz: V,
        round: V,
    },
    Cylinder {
        radius: V,
        half_height: V,
    },
    Torus {
        major: V,
        minor: V,
    },
    Cone {
        radius: V,
        half_height: V,
    },
    Capsule {
        radius: V,
        half_height: V,
    },
    Ellipsoid {
        rx: V,
        ry: V,
        rz: V,
    },
    Plane {
        nx: V,
        ny: V,
        nz: V,
        d: V,
    },
    Octahedron {
        size: V,
    },
    // v0.4 追加
    RoundedCone {
        r1: V,
        r2: V,
        half_height: V,
    },
    Pyramid {
        half_height: V,
    },
    HexPrism {
        hex_radius: V,
        half_height: V,
    },
    Link {
        half_length: V,
        r1: V,
        r2: V,
    },
    CappedCone {
        half_height: V,
        r1: V,
        r2: V,
    },
    CappedTorus {
        major_radius: V,
        minor_radius: V,
        cap_angle: V,
    },
    RoundedCylinder {
        radius: V,
        round_radius: V,
        half_height: V,
    },
    Tube {
        outer_radius: V,
        thickness: V,
        half_height: V,
    },
    Barrel {
        radius: V,
        half_height: V,
        bulge: V,
    },
    Heart {
        size: V,
    },
    Egg {
        ra: V,
        rb: V,
    },
    Helix {
        major_r: V,
        minor_r: V,
        pitch: V,
        half_height: V,
    },
    Tetrahedron {
        size: V,
    },
    BoxFrame {
        hx: V,
        hy: V,
        hz: V,
        edge: V,
    },
    DiamondPrim {
        radius: V,
        half_height: V,
    },
    StarPolygon {
        radius: V,
        n_points: V,
        m: V,
        half_height: V,
    },
    CrossShape {
        length: V,
        thickness: V,
        round_radius: V,
        half_height: V,
    },

    // ── Operations (23) ──
    Union {
        children: Vec<Self>,
    },
    SmoothUnion {
        k: V,
        children: Vec<Self>,
    },
    Intersection {
        children: Vec<Self>,
    },
    SmoothIntersection {
        k: V,
        children: Vec<Self>,
    },
    Subtract {
        a: Box<Self>,
        b: Box<Self>,
    },
    SmoothSubtract {
        k: V,
        a: Box<Self>,
        b: Box<Self>,
    },
    // v0.4 追加
    ChamferUnion {
        r: V,
        children: Vec<Self>,
    },
    ChamferIntersection {
        r: V,
        children: Vec<Self>,
    },
    ChamferSubtraction {
        r: V,
        a: Box<Self>,
        b: Box<Self>,
    },
    StairsUnion {
        r: V,
        n: V,
        children: Vec<Self>,
    },
    StairsIntersection {
        r: V,
        n: V,
        children: Vec<Self>,
    },
    StairsSubtraction {
        r: V,
        n: V,
        a: Box<Self>,
        b: Box<Self>,
    },
    Xor {
        a: Box<Self>,
        b: Box<Self>,
    },
    PipeOp {
        r: V,
        a: Box<Self>,
        b: Box<Self>,
    },
    Engrave {
        r: V,
        a: Box<Self>,
        b: Box<Self>,
    },
    Groove {
        ra: V,
        rb: V,
        a: Box<Self>,
        b: Box<Self>,
    },
    Tongue {
        ra: V,
        rb: V,
        a: Box<Self>,
        b: Box<Self>,
    },
    ColumnsUnion {
        r: V,
        n: V,
        children: Vec<Self>,
    },
    ColumnsIntersection {
        r: V,
        n: V,
        children: Vec<Self>,
    },
    ColumnsSubtraction {
        r: V,
        n: V,
        a: Box<Self>,
        b: Box<Self>,
    },
    ExpSmoothUnion {
        k: V,
        children: Vec<Self>,
    },
    ExpSmoothIntersection {
        k: V,
        children: Vec<Self>,
    },
    ExpSmoothSubtraction {
        k: V,
        a: Box<Self>,
        b: Box<Self>,
    },

    // ── Transforms (4) ──
    Translate {
        x: V,
        y: V,
        z: V,
        child: Box<Self>,
    },
    Rotate {
        rx: V,
        ry: V,
        rz: V,
        child: Box<Self>,
    },
    Scale {
        factor: V,
        child: Box<Self>,
    },
    // v0.4 追加
    ScaleNonUniform {
        sx: V,
        sy: V,
        sz: V,
        child: Box<Self>,
    },

    // ── Time (2) ──
    Animate {
        speed: V,
        amplitude: V,
        child: Box<Self>,
    },
    Morph {
        t: V,
        a: Box<Self>,
        b: Box<Self>,
    },

    // ── Modifiers (19) ──
    Round {
        radius: V,
        child: Box<Self>,
    },
    Onion {
        thickness: V,
        child: Box<Self>,
    },
    Twist {
        strength: V,
        child: Box<Self>,
    },
    Bend {
        curvature: V,
        child: Box<Self>,
    },
    Mirror {
        ax: V,
        ay: V,
        az: V,
        child: Box<Self>,
    },
    Repeat {
        sx: V,
        sy: V,
        sz: V,
        child: Box<Self>,
    },
    // v0.4 追加
    Elongate {
        ax: V,
        ay: V,
        az: V,
        child: Box<Self>,
    },
    Revolution {
        offset: V,
        child: Box<Self>,
    },
    Extrude {
        half_height: V,
        child: Box<Self>,
    },
    Taper {
        factor: V,
        child: Box<Self>,
    },
    Displacement {
        strength: V,
        child: Box<Self>,
    },
    PolarRepeat {
        count: V,
        child: Box<Self>,
    },
    ShearMod {
        xy: V,
        xz: V,
        yz: V,
        child: Box<Self>,
    },
    NoiseMod {
        amplitude: V,
        frequency: V,
        seed: V,
        child: Box<Self>,
    },
    RepeatFinite {
        cx: V,
        cy: V,
        cz: V,
        sx: V,
        sy: V,
        sz: V,
        child: Box<Self>,
    },
    OctantMirror {
        child: Box<Self>,
    },
    IcosahedralSymmetry {
        child: Box<Self>,
    },
    WithMaterial {
        material_id: V,
        child: Box<Self>,
    },
    SurfaceRoughness {
        frequency: V,
        amplitude: V,
        octaves: V,
        child: Box<Self>,
    },
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Parser
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

struct LolInput {
    body: Expr,
}

impl Parse for LolInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let fork = input.fork();
        if let Ok(kw) = fork.parse::<Ident>() {
            if kw == "field" {
                input.parse::<Ident>()?;
                let _name: Ident = input.parse()?;
                let content;
                syn::braced!(content in input);
                let body = parse_expr(&content)?;
                check_empty(&content)?;
                return Ok(Self { body });
            }
        }
        let body = parse_expr(input)?;
        Ok(Self { body })
    }
}

/// リテラル数値、`{式}` (ランタイム式)、裸の変数名を受け付ける
fn parse_val(input: ParseStream) -> Result<V> {
    // {expr} — 任意のRust式
    if input.peek(syn::token::Brace) {
        let content;
        syn::braced!(content in input);
        let expr: syn::Expr = content.parse()?;
        return Ok(quote!( (#expr) as f32 ));
    }
    // 数値リテラル（負号付き含む）
    let neg = if input.peek(Token![-]) {
        input.parse::<Token![-]>()?;
        true
    } else {
        false
    };
    if input.peek(syn::LitFloat) {
        let v: f32 = input.parse::<syn::LitFloat>()?.base10_parse()?;
        let v = if neg { -v } else { v };
        return Ok(quote!( #v ));
    }
    if input.peek(syn::LitInt) {
        #[allow(clippy::cast_precision_loss)]
        let v = input.parse::<syn::LitInt>()?.base10_parse::<i64>()? as f32;
        let v = if neg { -v } else { v };
        return Ok(quote!( #v ));
    }
    // 裸の変数名（DSLキーワードでなくても OK — 数値位置なので衝突しない）
    if !neg && input.peek(Ident) {
        let id: Ident = input.parse()?;
        return Ok(quote!( #id ));
    }
    Err(input.error("expected number, {{expr}}, or variable name"))
}

fn eat_comma(input: ParseStream) -> Result<()> {
    input.parse::<Token![,]>().map(|_| ())
}

fn check_empty(input: ParseStream) -> Result<()> {
    if input.is_empty() {
        Ok(())
    } else {
        Err(input.error("unexpected extra arguments"))
    }
}

/// Parse comma-separated child expressions (at least 2).
fn parse_children(input: ParseStream) -> Result<Vec<Expr>> {
    let mut children = vec![parse_expr(input)?];
    while !input.is_empty() && input.peek(Token![,]) {
        eat_comma(input)?;
        if input.is_empty() {
            break;
        }
        children.push(parse_expr(input)?);
    }
    if children.len() < 2 {
        return Err(input.error("operations require at least 2 children"));
    }
    Ok(children)
}

fn parse_1f(input: ParseStream) -> Result<V> {
    let v = parse_val(input)?;
    check_empty(input)?;
    Ok(v)
}

fn parse_2f(input: ParseStream) -> Result<(V, V)> {
    let a = parse_val(input)?;
    eat_comma(input)?;
    let b = parse_val(input)?;
    check_empty(input)?;
    Ok((a, b))
}

fn parse_3f(input: ParseStream) -> Result<(V, V, V)> {
    let a = parse_val(input)?;
    eat_comma(input)?;
    let b = parse_val(input)?;
    eat_comma(input)?;
    let c = parse_val(input)?;
    check_empty(input)?;
    Ok((a, b, c))
}

fn parse_4f(input: ParseStream) -> Result<(V, V, V, V)> {
    let a = parse_val(input)?;
    eat_comma(input)?;
    let b = parse_val(input)?;
    eat_comma(input)?;
    let c = parse_val(input)?;
    eat_comma(input)?;
    let d = parse_val(input)?;
    check_empty(input)?;
    Ok((a, b, c, d))
}

fn parse_k_children(input: ParseStream) -> Result<(V, Vec<Expr>)> {
    let k = parse_val(input)?;
    eat_comma(input)?;
    let children = parse_children(input)?;
    check_empty(input)?;
    Ok((k, children))
}

fn parse_2f_children(input: ParseStream) -> Result<(V, V, Vec<Expr>)> {
    let a = parse_val(input)?;
    eat_comma(input)?;
    let b = parse_val(input)?;
    eat_comma(input)?;
    let children = parse_children(input)?;
    check_empty(input)?;
    Ok((a, b, children))
}

fn parse_1f_child(input: ParseStream) -> Result<(V, Expr)> {
    let v = parse_val(input)?;
    eat_comma(input)?;
    let child = parse_expr(input)?;
    check_empty(input)?;
    Ok((v, child))
}

fn parse_3f_child(input: ParseStream) -> Result<(V, V, V, Expr)> {
    let a = parse_val(input)?;
    eat_comma(input)?;
    let b = parse_val(input)?;
    eat_comma(input)?;
    let c = parse_val(input)?;
    eat_comma(input)?;
    let child = parse_expr(input)?;
    check_empty(input)?;
    Ok((a, b, c, child))
}

fn parse_child_only(input: ParseStream) -> Result<Expr> {
    let child = parse_expr(input)?;
    check_empty(input)?;
    Ok(child)
}

/// 1 float + 2 binary children (like `smooth_subtract`)
fn parse_1f_ab(input: ParseStream) -> Result<(V, Expr, Expr)> {
    let k = parse_val(input)?;
    eat_comma(input)?;
    let a = parse_expr(input)?;
    eat_comma(input)?;
    let b = parse_expr(input)?;
    check_empty(input)?;
    Ok((k, a, b))
}

/// 2 floats + 2 binary children
fn parse_2f_ab(input: ParseStream) -> Result<(V, V, Expr, Expr)> {
    let v1 = parse_val(input)?;
    eat_comma(input)?;
    let v2 = parse_val(input)?;
    eat_comma(input)?;
    let a = parse_expr(input)?;
    eat_comma(input)?;
    let b = parse_expr(input)?;
    check_empty(input)?;
    Ok((v1, v2, a, b))
}

/// 6 floats + 1 child (for `repeat_finite`)
#[allow(clippy::many_single_char_names)]
fn parse_6f_child(input: ParseStream) -> Result<(V, V, V, V, V, V, Expr)> {
    let a = parse_val(input)?;
    eat_comma(input)?;
    let b = parse_val(input)?;
    eat_comma(input)?;
    let c = parse_val(input)?;
    eat_comma(input)?;
    let d = parse_val(input)?;
    eat_comma(input)?;
    let e = parse_val(input)?;
    eat_comma(input)?;
    let f = parse_val(input)?;
    eat_comma(input)?;
    let child = parse_expr(input)?;
    check_empty(input)?;
    Ok((a, b, c, d, e, f, child))
}

#[allow(clippy::too_many_lines)]
fn parse_expr(input: ParseStream) -> Result<Expr> {
    let name: Ident = input.parse()?;
    let content;
    syn::parenthesized!(content in input);

    match name.to_string().as_str() {
        // ── Primitives ──
        "sphere" => {
            let r = parse_1f(&content)?;
            Ok(Expr::Sphere { radius: r })
        }
        "box3d" => {
            let (hx, hy, hz) = parse_3f(&content)?;
            Ok(Expr::Box3d { hx, hy, hz })
        }
        "rounded_box" => {
            let (hx, hy, hz, r) = parse_4f(&content)?;
            Ok(Expr::RoundedBox {
                hx,
                hy,
                hz,
                round: r,
            })
        }
        "cylinder" => {
            let (r, h) = parse_2f(&content)?;
            Ok(Expr::Cylinder {
                radius: r,
                half_height: h,
            })
        }
        "torus" => {
            let (major, minor) = parse_2f(&content)?;
            Ok(Expr::Torus { major, minor })
        }
        "cone" => {
            let (r, h) = parse_2f(&content)?;
            Ok(Expr::Cone {
                radius: r,
                half_height: h,
            })
        }
        "capsule" => {
            let (r, h) = parse_2f(&content)?;
            Ok(Expr::Capsule {
                radius: r,
                half_height: h,
            })
        }
        "ellipsoid" => {
            let (rx, ry, rz) = parse_3f(&content)?;
            Ok(Expr::Ellipsoid { rx, ry, rz })
        }
        "plane" => {
            let (nx, ny, nz, d) = parse_4f(&content)?;
            Ok(Expr::Plane { nx, ny, nz, d })
        }
        "octahedron" => {
            let s = parse_1f(&content)?;
            Ok(Expr::Octahedron { size: s })
        }
        // v0.4 プリミティブ
        "rounded_cone" => {
            let (r1, r2, h) = parse_3f(&content)?;
            Ok(Expr::RoundedCone {
                r1,
                r2,
                half_height: h,
            })
        }
        "pyramid" => {
            let h = parse_1f(&content)?;
            Ok(Expr::Pyramid { half_height: h })
        }
        "hex_prism" => {
            let (r, h) = parse_2f(&content)?;
            Ok(Expr::HexPrism {
                hex_radius: r,
                half_height: h,
            })
        }
        "link" => {
            let (l, r1, r2) = parse_3f(&content)?;
            Ok(Expr::Link {
                half_length: l,
                r1,
                r2,
            })
        }
        "capped_cone" => {
            let (h, r1, r2) = parse_3f(&content)?;
            Ok(Expr::CappedCone {
                half_height: h,
                r1,
                r2,
            })
        }
        "capped_torus" => {
            let (maj, min, ang) = parse_3f(&content)?;
            Ok(Expr::CappedTorus {
                major_radius: maj,
                minor_radius: min,
                cap_angle: ang,
            })
        }
        "rounded_cylinder" => {
            let (r, rr, h) = parse_3f(&content)?;
            Ok(Expr::RoundedCylinder {
                radius: r,
                round_radius: rr,
                half_height: h,
            })
        }
        "tube" => {
            let (or, t, h) = parse_3f(&content)?;
            Ok(Expr::Tube {
                outer_radius: or,
                thickness: t,
                half_height: h,
            })
        }
        "barrel" => {
            let (r, h, b) = parse_3f(&content)?;
            Ok(Expr::Barrel {
                radius: r,
                half_height: h,
                bulge: b,
            })
        }
        "heart" => {
            let s = parse_1f(&content)?;
            Ok(Expr::Heart { size: s })
        }
        "egg" => {
            let (ra, rb) = parse_2f(&content)?;
            Ok(Expr::Egg { ra, rb })
        }
        "helix" => {
            let (mr, mi, p, h) = parse_4f(&content)?;
            Ok(Expr::Helix {
                major_r: mr,
                minor_r: mi,
                pitch: p,
                half_height: h,
            })
        }
        "tetrahedron" => {
            let s = parse_1f(&content)?;
            Ok(Expr::Tetrahedron { size: s })
        }
        "box_frame" => {
            let (hx, hy, hz, e) = parse_4f(&content)?;
            Ok(Expr::BoxFrame {
                hx,
                hy,
                hz,
                edge: e,
            })
        }
        "diamond" => {
            let (r, h) = parse_2f(&content)?;
            Ok(Expr::DiamondPrim {
                radius: r,
                half_height: h,
            })
        }
        "star_polygon" => {
            let (r, n, m, h) = parse_4f(&content)?;
            Ok(Expr::StarPolygon {
                radius: r,
                n_points: n,
                m,
                half_height: h,
            })
        }
        "cross_shape" => {
            let (l, t, r, h) = parse_4f(&content)?;
            Ok(Expr::CrossShape {
                length: l,
                thickness: t,
                round_radius: r,
                half_height: h,
            })
        }

        // ── Operations ──
        "union" => {
            let children = parse_children(&content)?;
            check_empty(&content)?;
            Ok(Expr::Union { children })
        }
        "smooth_union" => {
            let (k, children) = parse_k_children(&content)?;
            Ok(Expr::SmoothUnion { k, children })
        }
        "intersection" => {
            let children = parse_children(&content)?;
            check_empty(&content)?;
            Ok(Expr::Intersection { children })
        }
        "smooth_intersection" => {
            let (k, children) = parse_k_children(&content)?;
            Ok(Expr::SmoothIntersection { k, children })
        }
        "subtract" => {
            let a = parse_expr(&content)?;
            eat_comma(&content)?;
            let b = parse_expr(&content)?;
            check_empty(&content)?;
            Ok(Expr::Subtract {
                a: Box::new(a),
                b: Box::new(b),
            })
        }
        "smooth_subtract" => {
            let (k, a, b) = parse_1f_ab(&content)?;
            Ok(Expr::SmoothSubtract {
                k,
                a: Box::new(a),
                b: Box::new(b),
            })
        }
        // v0.4 オペレーション
        "chamfer_union" => {
            let (r, children) = parse_k_children(&content)?;
            Ok(Expr::ChamferUnion { r, children })
        }
        "chamfer_intersection" => {
            let (r, children) = parse_k_children(&content)?;
            Ok(Expr::ChamferIntersection { r, children })
        }
        "chamfer_subtraction" => {
            let (r, a, b) = parse_1f_ab(&content)?;
            Ok(Expr::ChamferSubtraction {
                r,
                a: Box::new(a),
                b: Box::new(b),
            })
        }
        "stairs_union" => {
            let (r, n, children) = parse_2f_children(&content)?;
            Ok(Expr::StairsUnion { r, n, children })
        }
        "stairs_intersection" => {
            let (r, n, children) = parse_2f_children(&content)?;
            Ok(Expr::StairsIntersection { r, n, children })
        }
        "stairs_subtraction" => {
            let (r, n, a, b) = parse_2f_ab(&content)?;
            Ok(Expr::StairsSubtraction {
                r,
                n,
                a: Box::new(a),
                b: Box::new(b),
            })
        }
        "xor" => {
            let a = parse_expr(&content)?;
            eat_comma(&content)?;
            let b = parse_expr(&content)?;
            check_empty(&content)?;
            Ok(Expr::Xor {
                a: Box::new(a),
                b: Box::new(b),
            })
        }
        "pipe" => {
            let (r, a, b) = parse_1f_ab(&content)?;
            Ok(Expr::PipeOp {
                r,
                a: Box::new(a),
                b: Box::new(b),
            })
        }
        "engrave" => {
            let (r, a, b) = parse_1f_ab(&content)?;
            Ok(Expr::Engrave {
                r,
                a: Box::new(a),
                b: Box::new(b),
            })
        }
        "groove" => {
            let (ra, rb, a, b) = parse_2f_ab(&content)?;
            Ok(Expr::Groove {
                ra,
                rb,
                a: Box::new(a),
                b: Box::new(b),
            })
        }
        "tongue" => {
            let (ra, rb, a, b) = parse_2f_ab(&content)?;
            Ok(Expr::Tongue {
                ra,
                rb,
                a: Box::new(a),
                b: Box::new(b),
            })
        }
        "columns_union" => {
            let (r, n, children) = parse_2f_children(&content)?;
            Ok(Expr::ColumnsUnion { r, n, children })
        }
        "columns_intersection" => {
            let (r, n, children) = parse_2f_children(&content)?;
            Ok(Expr::ColumnsIntersection { r, n, children })
        }
        "columns_subtraction" => {
            let (r, n, a, b) = parse_2f_ab(&content)?;
            Ok(Expr::ColumnsSubtraction {
                r,
                n,
                a: Box::new(a),
                b: Box::new(b),
            })
        }
        "exp_smooth_union" => {
            let (k, children) = parse_k_children(&content)?;
            Ok(Expr::ExpSmoothUnion { k, children })
        }
        "exp_smooth_intersection" => {
            let (k, children) = parse_k_children(&content)?;
            Ok(Expr::ExpSmoothIntersection { k, children })
        }
        "exp_smooth_subtraction" => {
            let (k, a, b) = parse_1f_ab(&content)?;
            Ok(Expr::ExpSmoothSubtraction {
                k,
                a: Box::new(a),
                b: Box::new(b),
            })
        }

        // ── Transforms ──
        "translate" => {
            let (x, y, z, child) = parse_3f_child(&content)?;
            Ok(Expr::Translate {
                x,
                y,
                z,
                child: Box::new(child),
            })
        }
        "rotate" => {
            let (rx, ry, rz, child) = parse_3f_child(&content)?;
            Ok(Expr::Rotate {
                rx,
                ry,
                rz,
                child: Box::new(child),
            })
        }
        "scale" => {
            let (f, child) = parse_1f_child(&content)?;
            Ok(Expr::Scale {
                factor: f,
                child: Box::new(child),
            })
        }
        "scale_non_uniform" => {
            let (sx, sy, sz, child) = parse_3f_child(&content)?;
            Ok(Expr::ScaleNonUniform {
                sx,
                sy,
                sz,
                child: Box::new(child),
            })
        }

        // ── Modifiers ──
        "round" => {
            let (r, child) = parse_1f_child(&content)?;
            Ok(Expr::Round {
                radius: r,
                child: Box::new(child),
            })
        }
        "onion" => {
            let (t, child) = parse_1f_child(&content)?;
            Ok(Expr::Onion {
                thickness: t,
                child: Box::new(child),
            })
        }
        "twist" => {
            let (s, child) = parse_1f_child(&content)?;
            Ok(Expr::Twist {
                strength: s,
                child: Box::new(child),
            })
        }
        "bend" => {
            let (c, child) = parse_1f_child(&content)?;
            Ok(Expr::Bend {
                curvature: c,
                child: Box::new(child),
            })
        }
        "mirror" => {
            let (ax, ay, az, child) = parse_3f_child(&content)?;
            Ok(Expr::Mirror {
                ax,
                ay,
                az,
                child: Box::new(child),
            })
        }
        "repeat" => {
            let (sx, sy, sz, child) = parse_3f_child(&content)?;
            Ok(Expr::Repeat {
                sx,
                sy,
                sz,
                child: Box::new(child),
            })
        }
        // v0.4 モディファイア
        "elongate" => {
            let (ax, ay, az, child) = parse_3f_child(&content)?;
            Ok(Expr::Elongate {
                ax,
                ay,
                az,
                child: Box::new(child),
            })
        }
        "revolution" => {
            let (off, child) = parse_1f_child(&content)?;
            Ok(Expr::Revolution {
                offset: off,
                child: Box::new(child),
            })
        }
        "extrude" => {
            let (h, child) = parse_1f_child(&content)?;
            Ok(Expr::Extrude {
                half_height: h,
                child: Box::new(child),
            })
        }
        "taper" => {
            let (f, child) = parse_1f_child(&content)?;
            Ok(Expr::Taper {
                factor: f,
                child: Box::new(child),
            })
        }
        "displacement" => {
            let (s, child) = parse_1f_child(&content)?;
            Ok(Expr::Displacement {
                strength: s,
                child: Box::new(child),
            })
        }
        "polar_repeat" => {
            let (c, child) = parse_1f_child(&content)?;
            Ok(Expr::PolarRepeat {
                count: c,
                child: Box::new(child),
            })
        }
        "shear" => {
            let (xy, xz, yz, child) = parse_3f_child(&content)?;
            Ok(Expr::ShearMod {
                xy,
                xz,
                yz,
                child: Box::new(child),
            })
        }
        "noise" => {
            let (amp, freq, seed, child) = parse_3f_child(&content)?;
            Ok(Expr::NoiseMod {
                amplitude: amp,
                frequency: freq,
                seed,
                child: Box::new(child),
            })
        }
        "repeat_finite" => {
            let (cx, cy, cz, sx, sy, sz, child) = parse_6f_child(&content)?;
            Ok(Expr::RepeatFinite {
                cx,
                cy,
                cz,
                sx,
                sy,
                sz,
                child: Box::new(child),
            })
        }
        "octant_mirror" => {
            let child = parse_child_only(&content)?;
            Ok(Expr::OctantMirror {
                child: Box::new(child),
            })
        }
        "icosahedral_symmetry" => {
            let child = parse_child_only(&content)?;
            Ok(Expr::IcosahedralSymmetry {
                child: Box::new(child),
            })
        }
        "with_material" => {
            let (id, child) = parse_1f_child(&content)?;
            Ok(Expr::WithMaterial {
                material_id: id,
                child: Box::new(child),
            })
        }
        "surface_roughness" => {
            let (freq, amp, oct, child) = parse_3f_child(&content)?;
            Ok(Expr::SurfaceRoughness {
                frequency: freq,
                amplitude: amp,
                octaves: oct,
                child: Box::new(child),
            })
        }

        // ── Time ──
        "animate" => {
            let speed = parse_val(&content)?;
            eat_comma(&content)?;
            let amplitude = parse_val(&content)?;
            eat_comma(&content)?;
            let child = parse_expr(&content)?;
            check_empty(&content)?;
            Ok(Expr::Animate {
                speed,
                amplitude,
                child: Box::new(child),
            })
        }
        "morph" => {
            let t = parse_val(&content)?;
            eat_comma(&content)?;
            let a = parse_expr(&content)?;
            eat_comma(&content)?;
            let b = parse_expr(&content)?;
            check_empty(&content)?;
            Ok(Expr::Morph {
                t,
                a: Box::new(a),
                b: Box::new(b),
            })
        }

        other => Err(syn::Error::new(
            name.span(),
            format!("unknown LOL expression: `{other}`"),
        )),
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Codegen: Expr → Rust `TokenStream` (`SdfNode` construction)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[allow(clippy::too_many_lines)]
fn codegen(expr: &Expr) -> TokenStream2 {
    match expr {
        // ── Primitives ──
        Expr::Sphere { radius } => {
            quote! { ::alice_lol::SdfNode::Sphere { radius: #radius } }
        }
        Expr::Box3d { hx, hy, hz } => {
            quote! { ::alice_lol::SdfNode::Box3d { half_extents: ::alice_lol::Vec3::new(#hx, #hy, #hz) } }
        }
        Expr::RoundedBox { hx, hy, hz, round } => {
            quote! { ::alice_lol::SdfNode::RoundedBox { half_extents: ::alice_lol::Vec3::new(#hx, #hy, #hz), round_radius: #round } }
        }
        Expr::Cylinder {
            radius,
            half_height,
        } => {
            quote! { ::alice_lol::SdfNode::Cylinder { radius: #radius, half_height: #half_height } }
        }
        Expr::Torus { major, minor } => {
            quote! { ::alice_lol::SdfNode::Torus { major_radius: #major, minor_radius: #minor } }
        }
        Expr::Cone {
            radius,
            half_height,
        } => {
            quote! { ::alice_lol::SdfNode::Cone { radius: #radius, half_height: #half_height } }
        }
        Expr::Capsule {
            radius,
            half_height,
        } => {
            quote! { ::alice_lol::SdfNode::Capsule { point_a: ::alice_lol::Vec3::new(0.0, -(#half_height), 0.0), point_b: ::alice_lol::Vec3::new(0.0, #half_height, 0.0), radius: #radius } }
        }
        Expr::Ellipsoid { rx, ry, rz } => {
            quote! { ::alice_lol::SdfNode::Ellipsoid { radii: ::alice_lol::Vec3::new(#rx, #ry, #rz) } }
        }
        Expr::Plane { nx, ny, nz, d } => {
            quote! { ::alice_lol::SdfNode::Plane { normal: ::alice_lol::Vec3::new(#nx, #ny, #nz), distance: #d } }
        }
        Expr::Octahedron { size } => {
            quote! { ::alice_lol::SdfNode::Octahedron { size: #size } }
        }
        // v0.4 プリミティブ
        Expr::RoundedCone {
            r1,
            r2,
            half_height,
        } => {
            quote! { ::alice_lol::SdfNode::RoundedCone { r1: #r1, r2: #r2, half_height: #half_height } }
        }
        Expr::Pyramid { half_height } => {
            quote! { ::alice_lol::SdfNode::Pyramid { half_height: #half_height } }
        }
        Expr::HexPrism {
            hex_radius,
            half_height,
        } => {
            quote! { ::alice_lol::SdfNode::HexPrism { hex_radius: #hex_radius, half_height: #half_height } }
        }
        Expr::Link {
            half_length,
            r1,
            r2,
        } => {
            quote! { ::alice_lol::SdfNode::Link { half_length: #half_length, r1: #r1, r2: #r2 } }
        }
        Expr::CappedCone {
            half_height,
            r1,
            r2,
        } => {
            quote! { ::alice_lol::SdfNode::CappedCone { half_height: #half_height, r1: #r1, r2: #r2 } }
        }
        Expr::CappedTorus {
            major_radius,
            minor_radius,
            cap_angle,
        } => {
            quote! { ::alice_lol::SdfNode::CappedTorus { major_radius: #major_radius, minor_radius: #minor_radius, cap_angle: #cap_angle } }
        }
        Expr::RoundedCylinder {
            radius,
            round_radius,
            half_height,
        } => {
            quote! { ::alice_lol::SdfNode::RoundedCylinder { radius: #radius, round_radius: #round_radius, half_height: #half_height } }
        }
        Expr::Tube {
            outer_radius,
            thickness,
            half_height,
        } => {
            quote! { ::alice_lol::SdfNode::Tube { outer_radius: #outer_radius, thickness: #thickness, half_height: #half_height } }
        }
        Expr::Barrel {
            radius,
            half_height,
            bulge,
        } => {
            quote! { ::alice_lol::SdfNode::Barrel { radius: #radius, half_height: #half_height, bulge: #bulge } }
        }
        Expr::Heart { size } => {
            quote! { ::alice_lol::SdfNode::Heart { size: #size } }
        }
        Expr::Egg { ra, rb } => {
            quote! { ::alice_lol::SdfNode::Egg { ra: #ra, rb: #rb } }
        }
        Expr::Helix {
            major_r,
            minor_r,
            pitch,
            half_height,
        } => {
            quote! { ::alice_lol::SdfNode::Helix { major_r: #major_r, minor_r: #minor_r, pitch: #pitch, half_height: #half_height } }
        }
        Expr::Tetrahedron { size } => {
            quote! { ::alice_lol::SdfNode::Tetrahedron { size: #size } }
        }
        Expr::BoxFrame { hx, hy, hz, edge } => {
            quote! { ::alice_lol::SdfNode::BoxFrame { half_extents: ::alice_lol::Vec3::new(#hx, #hy, #hz), edge: #edge } }
        }
        Expr::DiamondPrim {
            radius,
            half_height,
        } => {
            quote! { ::alice_lol::SdfNode::Diamond { radius: #radius, half_height: #half_height } }
        }
        Expr::StarPolygon {
            radius,
            n_points,
            m,
            half_height,
        } => {
            quote! { ::alice_lol::SdfNode::StarPolygon { radius: #radius, n_points: #n_points, m: #m, half_height: #half_height } }
        }
        Expr::CrossShape {
            length,
            thickness,
            round_radius,
            half_height,
        } => {
            quote! { ::alice_lol::SdfNode::CrossShape { length: #length, thickness: #thickness, round_radius: #round_radius, half_height: #half_height } }
        }

        // ── Operations (left-fold for N-ary → binary) ──
        Expr::Union { children } => fold_left(children, |a, b| {
            quote! { ::alice_lol::SdfNode::Union { a: ::std::sync::Arc::new(#a), b: ::std::sync::Arc::new(#b) } }
        }),
        Expr::SmoothUnion { k, children } => fold_left(children, |a, b| {
            quote! { ::alice_lol::SdfNode::SmoothUnion { a: ::std::sync::Arc::new(#a), b: ::std::sync::Arc::new(#b), k: #k } }
        }),
        Expr::Intersection { children } => fold_left(children, |a, b| {
            quote! { ::alice_lol::SdfNode::Intersection { a: ::std::sync::Arc::new(#a), b: ::std::sync::Arc::new(#b) } }
        }),
        Expr::SmoothIntersection { k, children } => fold_left(children, |a, b| {
            quote! { ::alice_lol::SdfNode::SmoothIntersection { a: ::std::sync::Arc::new(#a), b: ::std::sync::Arc::new(#b), k: #k } }
        }),
        Expr::Subtract { a, b } => {
            let (ac, bc) = (codegen(a), codegen(b));
            quote! { ::alice_lol::SdfNode::Subtraction { a: ::std::sync::Arc::new(#ac), b: ::std::sync::Arc::new(#bc) } }
        }
        Expr::SmoothSubtract { k, a, b } => {
            let (ac, bc) = (codegen(a), codegen(b));
            quote! { ::alice_lol::SdfNode::SmoothSubtraction { a: ::std::sync::Arc::new(#ac), b: ::std::sync::Arc::new(#bc), k: #k } }
        }
        // v0.4 オペレーション
        Expr::ChamferUnion { r, children } => fold_left(children, |a, b| {
            quote! { ::alice_lol::SdfNode::ChamferUnion { a: ::std::sync::Arc::new(#a), b: ::std::sync::Arc::new(#b), r: #r } }
        }),
        Expr::ChamferIntersection { r, children } => fold_left(children, |a, b| {
            quote! { ::alice_lol::SdfNode::ChamferIntersection { a: ::std::sync::Arc::new(#a), b: ::std::sync::Arc::new(#b), r: #r } }
        }),
        Expr::ChamferSubtraction { r, a, b } => {
            let (ac, bc) = (codegen(a), codegen(b));
            quote! { ::alice_lol::SdfNode::ChamferSubtraction { a: ::std::sync::Arc::new(#ac), b: ::std::sync::Arc::new(#bc), r: #r } }
        }
        Expr::StairsUnion { r, n, children } => fold_left(children, |a, b| {
            quote! { ::alice_lol::SdfNode::StairsUnion { a: ::std::sync::Arc::new(#a), b: ::std::sync::Arc::new(#b), r: #r, n: #n } }
        }),
        Expr::StairsIntersection { r, n, children } => fold_left(children, |a, b| {
            quote! { ::alice_lol::SdfNode::StairsIntersection { a: ::std::sync::Arc::new(#a), b: ::std::sync::Arc::new(#b), r: #r, n: #n } }
        }),
        Expr::StairsSubtraction { r, n, a, b } => {
            let (ac, bc) = (codegen(a), codegen(b));
            quote! { ::alice_lol::SdfNode::StairsSubtraction { a: ::std::sync::Arc::new(#ac), b: ::std::sync::Arc::new(#bc), r: #r, n: #n } }
        }
        Expr::Xor { a, b } => {
            let (ac, bc) = (codegen(a), codegen(b));
            quote! { ::alice_lol::SdfNode::XOR { a: ::std::sync::Arc::new(#ac), b: ::std::sync::Arc::new(#bc) } }
        }
        Expr::PipeOp { r, a, b } => {
            let (ac, bc) = (codegen(a), codegen(b));
            quote! { ::alice_lol::SdfNode::Pipe { a: ::std::sync::Arc::new(#ac), b: ::std::sync::Arc::new(#bc), r: #r } }
        }
        Expr::Engrave { r, a, b } => {
            let (ac, bc) = (codegen(a), codegen(b));
            quote! { ::alice_lol::SdfNode::Engrave { a: ::std::sync::Arc::new(#ac), b: ::std::sync::Arc::new(#bc), r: #r } }
        }
        Expr::Groove { ra, rb, a, b } => {
            let (ac, bc) = (codegen(a), codegen(b));
            quote! { ::alice_lol::SdfNode::Groove { a: ::std::sync::Arc::new(#ac), b: ::std::sync::Arc::new(#bc), ra: #ra, rb: #rb } }
        }
        Expr::Tongue { ra, rb, a, b } => {
            let (ac, bc) = (codegen(a), codegen(b));
            quote! { ::alice_lol::SdfNode::Tongue { a: ::std::sync::Arc::new(#ac), b: ::std::sync::Arc::new(#bc), ra: #ra, rb: #rb } }
        }
        Expr::ColumnsUnion { r, n, children } => fold_left(children, |a, b| {
            quote! { ::alice_lol::SdfNode::ColumnsUnion { a: ::std::sync::Arc::new(#a), b: ::std::sync::Arc::new(#b), r: #r, n: #n } }
        }),
        Expr::ColumnsIntersection { r, n, children } => fold_left(children, |a, b| {
            quote! { ::alice_lol::SdfNode::ColumnsIntersection { a: ::std::sync::Arc::new(#a), b: ::std::sync::Arc::new(#b), r: #r, n: #n } }
        }),
        Expr::ColumnsSubtraction { r, n, a, b } => {
            let (ac, bc) = (codegen(a), codegen(b));
            quote! { ::alice_lol::SdfNode::ColumnsSubtraction { a: ::std::sync::Arc::new(#ac), b: ::std::sync::Arc::new(#bc), r: #r, n: #n } }
        }
        Expr::ExpSmoothUnion { k, children } => fold_left(children, |a, b| {
            quote! { ::alice_lol::SdfNode::ExpSmoothUnion { a: ::std::sync::Arc::new(#a), b: ::std::sync::Arc::new(#b), k: #k } }
        }),
        Expr::ExpSmoothIntersection { k, children } => fold_left(children, |a, b| {
            quote! { ::alice_lol::SdfNode::ExpSmoothIntersection { a: ::std::sync::Arc::new(#a), b: ::std::sync::Arc::new(#b), k: #k } }
        }),
        Expr::ExpSmoothSubtraction { k, a, b } => {
            let (ac, bc) = (codegen(a), codegen(b));
            quote! { ::alice_lol::SdfNode::ExpSmoothSubtraction { a: ::std::sync::Arc::new(#ac), b: ::std::sync::Arc::new(#bc), k: #k } }
        }

        // ── Transforms ──
        Expr::Translate { x, y, z, child } => {
            let c = codegen(child);
            quote! { ::alice_lol::SdfNode::Translate { child: ::std::sync::Arc::new(#c), offset: ::alice_lol::Vec3::new(#x, #y, #z) } }
        }
        Expr::Rotate { rx, ry, rz, child } => {
            let c = codegen(child);
            quote! {
                ::alice_lol::SdfNode::Rotate {
                    child: ::std::sync::Arc::new(#c),
                    rotation: ::alice_lol::Quat::from_euler(
                        ::alice_lol::EulerRot::XYZ,
                        (#rx as f32).to_radians(),
                        (#ry as f32).to_radians(),
                        (#rz as f32).to_radians(),
                    ),
                }
            }
        }
        Expr::Scale { factor, child } => {
            let c = codegen(child);
            quote! { ::alice_lol::SdfNode::Scale { child: ::std::sync::Arc::new(#c), factor: #factor } }
        }
        Expr::ScaleNonUniform { sx, sy, sz, child } => {
            let c = codegen(child);
            quote! { ::alice_lol::SdfNode::ScaleNonUniform { child: ::std::sync::Arc::new(#c), factors: ::alice_lol::Vec3::new(#sx, #sy, #sz) } }
        }

        // ── Modifiers ──
        Expr::Round { radius, child } => {
            let c = codegen(child);
            quote! { ::alice_lol::SdfNode::Round { child: ::std::sync::Arc::new(#c), radius: #radius } }
        }
        Expr::Onion { thickness, child } => {
            let c = codegen(child);
            quote! { ::alice_lol::SdfNode::Onion { child: ::std::sync::Arc::new(#c), thickness: #thickness } }
        }
        Expr::Twist { strength, child } => {
            let c = codegen(child);
            quote! { ::alice_lol::SdfNode::Twist { child: ::std::sync::Arc::new(#c), strength: #strength } }
        }
        Expr::Bend { curvature, child } => {
            let c = codegen(child);
            quote! { ::alice_lol::SdfNode::Bend { child: ::std::sync::Arc::new(#c), curvature: #curvature } }
        }
        Expr::Mirror { ax, ay, az, child } => {
            let c = codegen(child);
            quote! { ::alice_lol::SdfNode::Mirror { child: ::std::sync::Arc::new(#c), axes: ::alice_lol::Vec3::new(#ax, #ay, #az) } }
        }
        Expr::Repeat { sx, sy, sz, child } => {
            let c = codegen(child);
            quote! { ::alice_lol::SdfNode::RepeatInfinite { child: ::std::sync::Arc::new(#c), spacing: ::alice_lol::Vec3::new(#sx, #sy, #sz) } }
        }
        // v0.4 モディファイア
        Expr::Elongate { ax, ay, az, child } => {
            let c = codegen(child);
            quote! { ::alice_lol::SdfNode::Elongate { child: ::std::sync::Arc::new(#c), amount: ::alice_lol::Vec3::new(#ax, #ay, #az) } }
        }
        Expr::Revolution { offset, child } => {
            let c = codegen(child);
            quote! { ::alice_lol::SdfNode::Revolution { child: ::std::sync::Arc::new(#c), offset: #offset } }
        }
        Expr::Extrude { half_height, child } => {
            let c = codegen(child);
            quote! { ::alice_lol::SdfNode::Extrude { child: ::std::sync::Arc::new(#c), half_height: #half_height } }
        }
        Expr::Taper { factor, child } => {
            let c = codegen(child);
            quote! { ::alice_lol::SdfNode::Taper { child: ::std::sync::Arc::new(#c), factor: #factor } }
        }
        Expr::Displacement { strength, child } => {
            let c = codegen(child);
            quote! { ::alice_lol::SdfNode::Displacement { child: ::std::sync::Arc::new(#c), strength: #strength } }
        }
        Expr::PolarRepeat { count, child } => {
            let c = codegen(child);
            quote! { ::alice_lol::SdfNode::PolarRepeat { child: ::std::sync::Arc::new(#c), count: #count as u32 } }
        }
        Expr::ShearMod { xy, xz, yz, child } => {
            let c = codegen(child);
            quote! { ::alice_lol::SdfNode::Shear { child: ::std::sync::Arc::new(#c), shear: ::alice_lol::Vec3::new(#xy, #xz, #yz) } }
        }
        Expr::NoiseMod {
            amplitude,
            frequency,
            seed,
            child,
        } => {
            let c = codegen(child);
            quote! { ::alice_lol::SdfNode::Noise { child: ::std::sync::Arc::new(#c), amplitude: #amplitude, frequency: #frequency, seed: #seed as u32 } }
        }
        Expr::RepeatFinite {
            cx,
            cy,
            cz,
            sx,
            sy,
            sz,
            child,
        } => {
            let c = codegen(child);
            quote! { ::alice_lol::SdfNode::RepeatFinite { child: ::std::sync::Arc::new(#c), count: [#cx as u32, #cy as u32, #cz as u32], spacing: ::alice_lol::Vec3::new(#sx, #sy, #sz) } }
        }
        Expr::OctantMirror { child } => {
            let c = codegen(child);
            quote! { ::alice_lol::SdfNode::OctantMirror { child: ::std::sync::Arc::new(#c) } }
        }
        Expr::IcosahedralSymmetry { child } => {
            let c = codegen(child);
            quote! { ::alice_lol::SdfNode::IcosahedralSymmetry { child: ::std::sync::Arc::new(#c) } }
        }
        Expr::WithMaterial { material_id, child } => {
            let c = codegen(child);
            quote! { ::alice_lol::SdfNode::WithMaterial { child: ::std::sync::Arc::new(#c), material_id: #material_id as u32 } }
        }
        Expr::SurfaceRoughness {
            frequency,
            amplitude,
            octaves,
            child,
        } => {
            let c = codegen(child);
            quote! { ::alice_lol::SdfNode::SurfaceRoughness { child: ::std::sync::Arc::new(#c), frequency: #frequency, amplitude: #amplitude, octaves: #octaves as u32 } }
        }

        // ── Time ──
        Expr::Animate {
            speed,
            amplitude,
            child,
        } => {
            let c = codegen(child);
            quote! { ::alice_lol::SdfNode::Animated { child: ::std::sync::Arc::new(#c), speed: #speed, amplitude: #amplitude } }
        }
        Expr::Morph { t, a, b } => {
            let ac = codegen(a);
            let bc = codegen(b);
            quote! { ::alice_lol::SdfNode::Morph { a: ::std::sync::Arc::new(#ac), b: ::std::sync::Arc::new(#bc), t: #t } }
        }
    }
}

/// Left-fold N children into a binary tree: `fold(A, B, C)` → `Op(Op(A, B), C)`
fn fold_left<F>(children: &[Expr], make: F) -> TokenStream2
where
    F: Fn(TokenStream2, TokenStream2) -> TokenStream2,
{
    let mut iter = children.iter();
    let first = codegen(
        iter.next()
            .expect("at least 2 children guaranteed by parser"),
    );
    iter.fold(first, |acc, child| {
        let c = codegen(child);
        make(acc, c)
    })
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Entry Point
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// LOL (Law-Oriented Language) `proc_macro`.
///
/// Parses LOL DSL and generates Rust code that constructs an `SdfNode` tree.
///
/// # Usage
///
/// ```ignore
/// use alice_lol::lol;
///
/// // With field wrapper
/// let node = lol! {
///     field MyScene {
///         smooth_union(0.2,
///             sphere(1.0),
///             translate(2.0, 0.0, 0.0, box3d(0.5, 0.5, 0.5))
///         )
///     }
/// };
///
/// // Bare expression
/// let node = lol! { sphere(1.0) };
///
/// // Variable capture with {expr}
/// let r = 1.5_f32;
/// let node = lol! { sphere({r}) };
/// let node = lol! { translate({x}, {y}, 0.0, sphere({r * 2.0})) };
/// ```
#[proc_macro]
pub fn lol(input: TokenStream) -> TokenStream {
    let scene = syn::parse_macro_input!(input as LolInput);
    let node_code = codegen(&scene.body);
    node_code.into()
}
