//! Parser (= LOL DSL → `Expr` AST)

use crate::ast::*;
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    Ident, Result, Token,
};

pub struct LolInput {
    pub body: Expr,
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

/// 6 floats (no child)
#[allow(clippy::many_single_char_names)]
fn parse_6f(input: ParseStream) -> Result<(V, V, V, V, V, V)> {
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
    check_empty(input)?;
    Ok((a, b, c, d, e, f))
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

/// 5 floats
#[allow(clippy::many_single_char_names)]
fn parse_5f(input: ParseStream) -> Result<(V, V, V, V, V)> {
    let a = parse_val(input)?;
    eat_comma(input)?;
    let b = parse_val(input)?;
    eat_comma(input)?;
    let c = parse_val(input)?;
    eat_comma(input)?;
    let d = parse_val(input)?;
    eat_comma(input)?;
    let e = parse_val(input)?;
    check_empty(input)?;
    Ok((a, b, c, d, e))
}

/// 9 floats
#[allow(clippy::many_single_char_names)]
fn parse_9f(input: ParseStream) -> Result<(V, V, V, V, V, V, V, V, V)> {
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
    let g = parse_val(input)?;
    eat_comma(input)?;
    let h = parse_val(input)?;
    eat_comma(input)?;
    let i = parse_val(input)?;
    check_empty(input)?;
    Ok((a, b, c, d, e, f, g, h, i))
}

/// 10 floats
#[allow(clippy::many_single_char_names)]
fn parse_10f(input: ParseStream) -> Result<(V, V, V, V, V, V, V, V, V, V)> {
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
    let g = parse_val(input)?;
    eat_comma(input)?;
    let h = parse_val(input)?;
    eat_comma(input)?;
    let i = parse_val(input)?;
    eat_comma(input)?;
    let j = parse_val(input)?;
    check_empty(input)?;
    Ok((a, b, c, d, e, f, g, h, i, j))
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

        // ── v1.0 プリミティブ ──
        "triangle" => {
            let (ax, ay, az, bx, by, bz, cx, cy, cz) = parse_9f(&content)?;
            Ok(Expr::Triangle {
                ax,
                ay,
                az,
                bx,
                by,
                bz,
                cx,
                cy,
                cz,
            })
        }
        "bezier" => {
            let (ax, ay, az, bx, by, bz, cx, cy, cz, r) = parse_10f(&content)?;
            Ok(Expr::BezierPrim {
                ax,
                ay,
                az,
                bx,
                by,
                bz,
                cx,
                cy,
                cz,
                radius: r,
            })
        }
        "triangular_prism" => {
            let (w, d) = parse_2f(&content)?;
            Ok(Expr::TriangularPrism {
                width: w,
                half_depth: d,
            })
        }
        "cut_sphere" => {
            let (r, h) = parse_2f(&content)?;
            Ok(Expr::CutSphere {
                radius: r,
                cut_height: h,
            })
        }
        "cut_hollow_sphere" => {
            let (r, h, t) = parse_3f(&content)?;
            Ok(Expr::CutHollowSphere {
                radius: r,
                cut_height: h,
                thickness: t,
            })
        }
        "death_star" => {
            let (ra, rb, d) = parse_3f(&content)?;
            Ok(Expr::DeathStar { ra, rb, d })
        }
        "solid_angle" => {
            let (a, r) = parse_2f(&content)?;
            Ok(Expr::SolidAngle {
                angle: a,
                radius: r,
            })
        }
        "rhombus" => {
            let (la, lb, h, r) = parse_4f(&content)?;
            Ok(Expr::Rhombus {
                la,
                lb,
                half_height: h,
                round_radius: r,
            })
        }
        "horseshoe" => {
            let (a, r, l, w, t) = parse_5f(&content)?;
            Ok(Expr::Horseshoe {
                angle: a,
                radius: r,
                half_length: l,
                width: w,
                thickness: t,
            })
        }
        "vesica" => {
            let (r, d) = parse_2f(&content)?;
            Ok(Expr::Vesica {
                radius: r,
                half_dist: d,
            })
        }
        "infinite_cylinder" => {
            let r = parse_1f(&content)?;
            Ok(Expr::InfiniteCylinder { radius: r })
        }
        "infinite_cone" => {
            let a = parse_1f(&content)?;
            Ok(Expr::InfiniteCone { angle: a })
        }
        "gyroid" => {
            let (s, t) = parse_2f(&content)?;
            Ok(Expr::GyroidPrim {
                scale: s,
                thickness: t,
            })
        }
        "chamfered_cube" => {
            let (hx, hy, hz, c) = parse_4f(&content)?;
            Ok(Expr::ChamferedCube {
                hx,
                hy,
                hz,
                chamfer: c,
            })
        }
        "schwarz_p" => {
            let (s, t) = parse_2f(&content)?;
            Ok(Expr::SchwarzPPrim {
                scale: s,
                thickness: t,
            })
        }
        "superellipsoid" => {
            let (hx, hy, hz, e1, e2) = parse_5f(&content)?;
            Ok(Expr::SuperellipsoidPrim { hx, hy, hz, e1, e2 })
        }
        "rounded_x" => {
            let (w, r, h) = parse_3f(&content)?;
            Ok(Expr::RoundedXPrim {
                width: w,
                round_radius: r,
                half_height: h,
            })
        }
        "pie" => {
            let (a, r, h) = parse_3f(&content)?;
            Ok(Expr::PiePrim {
                angle: a,
                radius: r,
                half_height: h,
            })
        }
        "trapezoid" => {
            let (r1, r2, th, d) = parse_4f(&content)?;
            Ok(Expr::TrapezoidPrim {
                r1,
                r2,
                trap_height: th,
                half_depth: d,
            })
        }
        "parallelogram" => {
            let (w, h, s, d) = parse_4f(&content)?;
            Ok(Expr::ParallelogramPrim {
                width: w,
                para_height: h,
                skew: s,
                half_depth: d,
            })
        }
        "tunnel" => {
            let (w, h, d) = parse_3f(&content)?;
            Ok(Expr::TunnelPrim {
                width: w,
                height_2d: h,
                half_depth: d,
            })
        }
        "uneven_capsule" => {
            let (r1, r2, h, d) = parse_4f(&content)?;
            Ok(Expr::UnevenCapsulePrim {
                r1,
                r2,
                cap_height: h,
                half_depth: d,
            })
        }
        "arc_shape" => {
            let (a, r, t, h) = parse_4f(&content)?;
            Ok(Expr::ArcShapePrim {
                aperture: a,
                radius: r,
                thickness: t,
                half_height: h,
            })
        }
        "moon" => {
            let (d, ra, rb, h) = parse_4f(&content)?;
            Ok(Expr::MoonPrim {
                d,
                ra,
                rb,
                half_height: h,
            })
        }
        "blobby_cross" => {
            let (s, h) = parse_2f(&content)?;
            Ok(Expr::BlobbyCrossPrim {
                size: s,
                half_height: h,
            })
        }
        "parabola_segment" => {
            let (w, h, d) = parse_3f(&content)?;
            Ok(Expr::ParabolaSegmentPrim {
                width: w,
                para_height: h,
                half_depth: d,
            })
        }
        "regular_polygon" => {
            let (r, n, h) = parse_3f(&content)?;
            Ok(Expr::RegularPolygonPrim {
                radius: r,
                n_sides: n,
                half_height: h,
            })
        }
        "stairs_prim" => {
            let (sw, sh, n, d) = parse_4f(&content)?;
            Ok(Expr::StairsPrim {
                step_width: sw,
                step_height: sh,
                n_steps: n,
                half_depth: d,
            })
        }
        "dodecahedron" => {
            let r = parse_1f(&content)?;
            Ok(Expr::DodecahedronPrim { radius: r })
        }
        "icosahedron" => {
            let r = parse_1f(&content)?;
            Ok(Expr::IcosahedronPrim { radius: r })
        }
        "truncated_octahedron" => {
            let r = parse_1f(&content)?;
            Ok(Expr::TruncatedOctahedronPrim { radius: r })
        }
        "truncated_icosahedron" => {
            let r = parse_1f(&content)?;
            Ok(Expr::TruncatedIcosahedronPrim { radius: r })
        }
        "diamond_surface" => {
            let (s, t) = parse_2f(&content)?;
            Ok(Expr::DiamondSurfacePrim {
                scale: s,
                thickness: t,
            })
        }
        "neovius" => {
            let (s, t) = parse_2f(&content)?;
            Ok(Expr::NeoviusPrim {
                scale: s,
                thickness: t,
            })
        }
        "lidinoid" => {
            let (s, t) = parse_2f(&content)?;
            Ok(Expr::LidinoidPrim {
                scale: s,
                thickness: t,
            })
        }
        "iwp" => {
            let (s, t) = parse_2f(&content)?;
            Ok(Expr::IWPPrim {
                scale: s,
                thickness: t,
            })
        }
        "frd" => {
            let (s, t) = parse_2f(&content)?;
            Ok(Expr::FRDPrim {
                scale: s,
                thickness: t,
            })
        }
        "fischer_koch_s" => {
            let (s, t) = parse_2f(&content)?;
            Ok(Expr::FischerKochSPrim {
                scale: s,
                thickness: t,
            })
        }
        "pmy" => {
            let (s, t) = parse_2f(&content)?;
            Ok(Expr::PMYPrim {
                scale: s,
                thickness: t,
            })
        }
        "circle_2d" => {
            let (r, h) = parse_2f(&content)?;
            Ok(Expr::Circle2DPrim {
                radius: r,
                half_height: h,
            })
        }
        "rect_2d" => {
            let (hx, hy, h) = parse_3f(&content)?;
            Ok(Expr::Rect2DPrim {
                hx,
                hy,
                half_height: h,
            })
        }
        "segment_2d" => {
            let (ax, ay, bx, by, t, h) = parse_6f(&content)?;
            Ok(Expr::Segment2DPrim {
                ax,
                ay,
                bx,
                by,
                thickness: t,
                half_height: h,
            })
        }
        "rounded_rect_2d" => {
            let (hx, hy, r, h) = parse_4f(&content)?;
            Ok(Expr::RoundedRect2DPrim {
                hx,
                hy,
                round_radius: r,
                half_height: h,
            })
        }
        "annular_2d" => {
            let (r, t, h) = parse_3f(&content)?;
            Ok(Expr::Annular2DPrim {
                outer_radius: r,
                thickness: t,
                half_height: h,
            })
        }
        "terrain" => {
            let (s, a) = parse_2f(&content)?;
            Ok(Expr::TerrainPrim {
                scale: s,
                amplitude: a,
            })
        }
        // ── v1.0 モディファイア ──
        "sweep_bezier" => {
            let (p0x, p0y, p1x, p1y, p2x, p2y, child) = parse_6f_child(&content)?;
            Ok(Expr::SweepBezierMod {
                p0x,
                p0y,
                p1x,
                p1y,
                p2x,
                p2y,
                child: Box::new(child),
            })
        }

        // ── 3D Print Structural Intent ──
        "lattice_infill" => {
            let (st, ls, lt, child) = parse_3f_child(&content)?;
            Ok(Expr::LatticeInfill {
                shell_thickness: st,
                lattice_scale: ls,
                lattice_thickness: lt,
                child: Box::new(child),
            })
        }
        "diamond_infill" => {
            let (st, ls, lt, child) = parse_3f_child(&content)?;
            Ok(Expr::DiamondInfill {
                shell_thickness: st,
                lattice_scale: ls,
                lattice_thickness: lt,
                child: Box::new(child),
            })
        }
        "schwarz_infill" => {
            let (st, ls, lt, child) = parse_3f_child(&content)?;
            Ok(Expr::SchwarzInfill {
                shell_thickness: st,
                lattice_scale: ls,
                lattice_thickness: lt,
                child: Box::new(child),
            })
        }

        other => Err(syn::Error::new(
            name.span(),
            format!("unknown LOL expression: `{other}`"),
        )),
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
