//! ランタイム LOL パーサー
//!
//! LLM が生成した LOL テキストを `&str` → [`SdfNode`] に変換する。
//! `proc_macro` 版と同等の 76 構文をサポートするが、値は `f32` リテラルのみ
//! （Rust 式キャプチャは不要 — LLM 出力は数値定数のみ）。
//!
//! ```
//! use alice_lol::runtime_parser::parse_lol;
//!
//! let node = parse_lol("smooth_union(0.3, sphere(1.0), box3d(0.5, 0.5, 0.5))").unwrap();
//! let dist = alice_lol::eval(&node, glam::Vec3::ZERO);
//! ```

use crate::SdfNode;
use glam::{EulerRot, Quat, Vec3};
use std::sync::Arc;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// エラー型
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// LOL パースエラー
#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub position: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "LOL parse error at pos {}: {}",
            self.position, self.message
        )
    }
}

/// `parse_6f_child` の戻り値型エイリアス
type SixFloatsChild = (f32, f32, f32, f32, f32, f32, SdfNode);

impl std::error::Error for ParseError {}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// トークナイザー
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Ident(String),
    Number(f32),
    LParen,
    RParen,
    Comma,
}

struct Lexer<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    const fn new(input: &'a str) -> Self {
        Self {
            input: input.as_bytes(),
            pos: 0,
        }
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() && self.input[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn next_token(&mut self) -> Result<Option<Token>, ParseError> {
        self.skip_whitespace();
        if self.pos >= self.input.len() {
            return Ok(None);
        }
        let ch = self.input[self.pos];
        match ch {
            b'(' => {
                self.pos += 1;
                Ok(Some(Token::LParen))
            }
            b')' => {
                self.pos += 1;
                Ok(Some(Token::RParen))
            }
            b',' => {
                self.pos += 1;
                Ok(Some(Token::Comma))
            }
            b'-' | b'0'..=b'9' => self.read_number(),
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.read_ident(),
            _ => Err(ParseError {
                message: format!("unexpected character: '{}'", ch as char),
                position: self.pos,
            }),
        }
    }

    fn read_number(&mut self) -> Result<Option<Token>, ParseError> {
        let start = self.pos;
        if self.pos < self.input.len() && self.input[self.pos] == b'-' {
            self.pos += 1;
        }
        while self.pos < self.input.len() && self.input[self.pos].is_ascii_digit() {
            self.pos += 1;
        }
        if self.pos < self.input.len() && self.input[self.pos] == b'.' {
            self.pos += 1;
            while self.pos < self.input.len() && self.input[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
        }
        // 科学的記数法 (e.g. 1e-3)
        if self.pos < self.input.len()
            && (self.input[self.pos] == b'e' || self.input[self.pos] == b'E')
        {
            self.pos += 1;
            if self.pos < self.input.len()
                && (self.input[self.pos] == b'+' || self.input[self.pos] == b'-')
            {
                self.pos += 1;
            }
            while self.pos < self.input.len() && self.input[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
        }
        let s = std::str::from_utf8(&self.input[start..self.pos]).map_err(|_| ParseError {
            message: "invalid UTF-8 in number".into(),
            position: start,
        })?;
        let v: f32 = s.parse().map_err(|_| ParseError {
            message: format!("invalid number: '{s}'"),
            position: start,
        })?;
        Ok(Some(Token::Number(v)))
    }

    fn read_ident(&mut self) -> Result<Option<Token>, ParseError> {
        let start = self.pos;
        while self.pos < self.input.len()
            && (self.input[self.pos].is_ascii_alphanumeric() || self.input[self.pos] == b'_')
        {
            self.pos += 1;
        }
        let s = std::str::from_utf8(&self.input[start..self.pos]).map_err(|_| ParseError {
            message: "invalid UTF-8 in identifier".into(),
            position: start,
        })?;
        Ok(Some(Token::Ident(s.to_owned())))
    }

    const fn position(&self) -> usize {
        self.pos
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// パーサー
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

struct Parser<'a> {
    lexer: Lexer<'a>,
    peeked: Option<Token>,
}

impl<'a> Parser<'a> {
    const fn new(input: &'a str) -> Self {
        Self {
            lexer: Lexer::new(input),
            peeked: None,
        }
    }

    fn peek(&mut self) -> Result<Option<&Token>, ParseError> {
        if self.peeked.is_none() {
            self.peeked = self.lexer.next_token()?;
        }
        Ok(self.peeked.as_ref())
    }

    fn next(&mut self) -> Result<Option<Token>, ParseError> {
        if let Some(t) = self.peeked.take() {
            Ok(Some(t))
        } else {
            self.lexer.next_token()
        }
    }

    fn expect_number(&mut self) -> Result<f32, ParseError> {
        match self.next()? {
            Some(Token::Number(v)) => Ok(v),
            other => Err(ParseError {
                message: format!("expected number, got {other:?}"),
                position: self.lexer.position(),
            }),
        }
    }

    fn expect_comma(&mut self) -> Result<(), ParseError> {
        match self.next()? {
            Some(Token::Comma) => Ok(()),
            other => Err(ParseError {
                message: format!("expected comma, got {other:?}"),
                position: self.lexer.position(),
            }),
        }
    }

    fn expect_rparen(&mut self) -> Result<(), ParseError> {
        match self.next()? {
            Some(Token::RParen) => Ok(()),
            other => Err(ParseError {
                message: format!("expected ')', got {other:?}"),
                position: self.lexer.position(),
            }),
        }
    }

    fn at_rparen(&mut self) -> Result<bool, ParseError> {
        Ok(matches!(self.peek()?, Some(Token::RParen)))
    }

    /// f32 値 1 個
    fn parse_1f(&mut self) -> Result<f32, ParseError> {
        let v = self.expect_number()?;
        self.expect_rparen()?;
        Ok(v)
    }

    /// f32 値 2 個
    fn parse_2f(&mut self) -> Result<(f32, f32), ParseError> {
        let a = self.expect_number()?;
        self.expect_comma()?;
        let b = self.expect_number()?;
        self.expect_rparen()?;
        Ok((a, b))
    }

    /// f32 値 3 個
    fn parse_3f(&mut self) -> Result<(f32, f32, f32), ParseError> {
        let a = self.expect_number()?;
        self.expect_comma()?;
        let b = self.expect_number()?;
        self.expect_comma()?;
        let c = self.expect_number()?;
        self.expect_rparen()?;
        Ok((a, b, c))
    }

    /// f32 値 4 個
    fn parse_4f(&mut self) -> Result<(f32, f32, f32, f32), ParseError> {
        let a = self.expect_number()?;
        self.expect_comma()?;
        let b = self.expect_number()?;
        self.expect_comma()?;
        let c = self.expect_number()?;
        self.expect_comma()?;
        let d = self.expect_number()?;
        self.expect_rparen()?;
        Ok((a, b, c, d))
    }

    /// f32 + 子ノード 1 個
    fn parse_1f_child(&mut self) -> Result<(f32, SdfNode), ParseError> {
        let v = self.expect_number()?;
        self.expect_comma()?;
        let child = self.parse_expr()?;
        self.expect_rparen()?;
        Ok((v, child))
    }

    /// f32 3 個 + 子ノード 1 個
    fn parse_3f_child(&mut self) -> Result<(f32, f32, f32, SdfNode), ParseError> {
        let a = self.expect_number()?;
        self.expect_comma()?;
        let b = self.expect_number()?;
        self.expect_comma()?;
        let c = self.expect_number()?;
        self.expect_comma()?;
        let child = self.parse_expr()?;
        self.expect_rparen()?;
        Ok((a, b, c, child))
    }

    /// 子ノードのみ
    fn parse_child_only(&mut self) -> Result<SdfNode, ParseError> {
        let child = self.parse_expr()?;
        self.expect_rparen()?;
        Ok(child)
    }

    /// k + 2個以上の子ノード
    fn parse_k_children(&mut self) -> Result<(f32, Vec<SdfNode>), ParseError> {
        let k = self.expect_number()?;
        self.expect_comma()?;
        let children = self.parse_children()?;
        Ok((k, children))
    }

    /// f32 2 個 + 2 個以上の子ノード
    fn parse_2f_children(&mut self) -> Result<(f32, f32, Vec<SdfNode>), ParseError> {
        let a = self.expect_number()?;
        self.expect_comma()?;
        let b = self.expect_number()?;
        self.expect_comma()?;
        let children = self.parse_children()?;
        Ok((a, b, children))
    }

    /// k + 2 子ノード（subtract 系）
    fn parse_1f_ab(&mut self) -> Result<(f32, SdfNode, SdfNode), ParseError> {
        let k = self.expect_number()?;
        self.expect_comma()?;
        let a = self.parse_expr()?;
        self.expect_comma()?;
        let b = self.parse_expr()?;
        self.expect_rparen()?;
        Ok((k, a, b))
    }

    /// f32 2 個 + 2 子ノード
    fn parse_2f_ab(&mut self) -> Result<(f32, f32, SdfNode, SdfNode), ParseError> {
        let v1 = self.expect_number()?;
        self.expect_comma()?;
        let v2 = self.expect_number()?;
        self.expect_comma()?;
        let a = self.parse_expr()?;
        self.expect_comma()?;
        let b = self.parse_expr()?;
        self.expect_rparen()?;
        Ok((v1, v2, a, b))
    }

    /// `f32` 6 個 + 子ノード 1 個 (`repeat_finite`)
    #[allow(clippy::many_single_char_names)]
    fn parse_6f_child(&mut self) -> Result<SixFloatsChild, ParseError> {
        let a = self.expect_number()?;
        self.expect_comma()?;
        let b = self.expect_number()?;
        self.expect_comma()?;
        let c = self.expect_number()?;
        self.expect_comma()?;
        let d = self.expect_number()?;
        self.expect_comma()?;
        let e = self.expect_number()?;
        self.expect_comma()?;
        let f = self.expect_number()?;
        self.expect_comma()?;
        let child = self.parse_expr()?;
        self.expect_rparen()?;
        Ok((a, b, c, d, e, f, child))
    }

    /// 2 個以上の子ノード（カンマ区切り、`)` まで）
    fn parse_children(&mut self) -> Result<Vec<SdfNode>, ParseError> {
        let mut children = vec![self.parse_expr()?];
        while !self.at_rparen()? {
            self.expect_comma()?;
            if self.at_rparen()? {
                break;
            }
            children.push(self.parse_expr()?);
        }
        self.expect_rparen()?;
        if children.len() < 2 {
            return Err(ParseError {
                message: "operations require at least 2 children".into(),
                position: self.lexer.position(),
            });
        }
        Ok(children)
    }

    /// N-ary 子ノードを左畳み込みで binary [`SdfNode`] に変換
    fn fold_left<F>(children: Vec<SdfNode>, f: F) -> SdfNode
    where
        F: Fn(SdfNode, SdfNode) -> SdfNode,
    {
        let mut iter = children.into_iter();
        let first = iter.next().expect("at least 2 children");
        iter.fold(first, &f)
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // メイン式パーサー
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    #[allow(clippy::too_many_lines)]
    fn parse_expr(&mut self) -> Result<SdfNode, ParseError> {
        // `field Name { ... }` ラッパーをスキップ
        let name = match self.next()? {
            Some(Token::Ident(s)) => s,
            other => {
                return Err(ParseError {
                    message: format!("expected function name, got {other:?}"),
                    position: self.lexer.position(),
                })
            }
        };

        if name == "field" {
            // field Name { body } — 名前を読み飛ばして中身をパース
            // ランタイムでは field ラッパーは不要だが互換性のため受け付ける
            let _field_name = match self.next()? {
                Some(Token::Ident(s)) => s,
                other => {
                    return Err(ParseError {
                        message: format!("expected field name, got {other:?}"),
                        position: self.lexer.position(),
                    })
                }
            };
            // '{' は Ident/Number 扱いされないので特殊処理不要
            // field は proc_macro 専用。ランタイムではスキップして中身を直接パース
            return self.parse_expr();
        }

        // '(' を期待
        match self.next()? {
            Some(Token::LParen) => {}
            other => {
                return Err(ParseError {
                    message: format!("expected '(' after '{name}', got {other:?}"),
                    position: self.lexer.position(),
                })
            }
        }

        match name.as_str() {
            // ── プリミティブ (27) ──
            "sphere" => {
                let r = self.parse_1f()?;
                Ok(SdfNode::Sphere { radius: r })
            }
            "box3d" => {
                let (hx, hy, hz) = self.parse_3f()?;
                Ok(SdfNode::Box3d {
                    half_extents: Vec3::new(hx, hy, hz),
                })
            }
            "rounded_box" => {
                let (hx, hy, hz, r) = self.parse_4f()?;
                Ok(SdfNode::RoundedBox {
                    half_extents: Vec3::new(hx, hy, hz),
                    round_radius: r,
                })
            }
            "cylinder" => {
                let (r, h) = self.parse_2f()?;
                Ok(SdfNode::Cylinder {
                    radius: r,
                    half_height: h,
                })
            }
            "torus" => {
                let (major, minor) = self.parse_2f()?;
                Ok(SdfNode::Torus {
                    major_radius: major,
                    minor_radius: minor,
                })
            }
            "cone" => {
                let (r, h) = self.parse_2f()?;
                Ok(SdfNode::Cone {
                    radius: r,
                    half_height: h,
                })
            }
            "capsule" => {
                let (r, h) = self.parse_2f()?;
                Ok(SdfNode::Capsule {
                    point_a: Vec3::new(0.0, -h, 0.0),
                    point_b: Vec3::new(0.0, h, 0.0),
                    radius: r,
                })
            }
            "ellipsoid" => {
                let (rx, ry, rz) = self.parse_3f()?;
                Ok(SdfNode::Ellipsoid {
                    radii: Vec3::new(rx, ry, rz),
                })
            }
            "plane" => {
                let (nx, ny, nz, d) = self.parse_4f()?;
                Ok(SdfNode::Plane {
                    normal: Vec3::new(nx, ny, nz),
                    distance: d,
                })
            }
            "octahedron" => {
                let s = self.parse_1f()?;
                Ok(SdfNode::Octahedron { size: s })
            }
            "rounded_cone" => {
                let (r1, r2, h) = self.parse_3f()?;
                Ok(SdfNode::RoundedCone {
                    r1,
                    r2,
                    half_height: h,
                })
            }
            "pyramid" => {
                let h = self.parse_1f()?;
                Ok(SdfNode::Pyramid { half_height: h })
            }
            "hex_prism" => {
                let (r, h) = self.parse_2f()?;
                Ok(SdfNode::HexPrism {
                    hex_radius: r,
                    half_height: h,
                })
            }
            "link" => {
                let (l, r1, r2) = self.parse_3f()?;
                Ok(SdfNode::Link {
                    half_length: l,
                    r1,
                    r2,
                })
            }
            "capped_cone" => {
                let (h, r1, r2) = self.parse_3f()?;
                Ok(SdfNode::CappedCone {
                    half_height: h,
                    r1,
                    r2,
                })
            }
            "capped_torus" => {
                let (maj, min, ang) = self.parse_3f()?;
                Ok(SdfNode::CappedTorus {
                    major_radius: maj,
                    minor_radius: min,
                    cap_angle: ang,
                })
            }
            "rounded_cylinder" => {
                let (r, rr, h) = self.parse_3f()?;
                Ok(SdfNode::RoundedCylinder {
                    radius: r,
                    round_radius: rr,
                    half_height: h,
                })
            }
            "tube" => {
                let (or, t, h) = self.parse_3f()?;
                Ok(SdfNode::Tube {
                    outer_radius: or,
                    thickness: t,
                    half_height: h,
                })
            }
            "barrel" => {
                let (r, h, b) = self.parse_3f()?;
                Ok(SdfNode::Barrel {
                    radius: r,
                    half_height: h,
                    bulge: b,
                })
            }
            "heart" => {
                let s = self.parse_1f()?;
                Ok(SdfNode::Heart { size: s })
            }
            "egg" => {
                let (ra, rb) = self.parse_2f()?;
                Ok(SdfNode::Egg { ra, rb })
            }
            "helix" => {
                let (mr, mi, p, h) = self.parse_4f()?;
                Ok(SdfNode::Helix {
                    major_r: mr,
                    minor_r: mi,
                    pitch: p,
                    half_height: h,
                })
            }
            "tetrahedron" => {
                let s = self.parse_1f()?;
                Ok(SdfNode::Tetrahedron { size: s })
            }
            "box_frame" => {
                let (hx, hy, hz, e) = self.parse_4f()?;
                Ok(SdfNode::BoxFrame {
                    half_extents: Vec3::new(hx, hy, hz),
                    edge: e,
                })
            }
            "diamond" => {
                let (r, h) = self.parse_2f()?;
                Ok(SdfNode::Diamond {
                    radius: r,
                    half_height: h,
                })
            }
            "star_polygon" => {
                let (r, n, m, h) = self.parse_4f()?;
                Ok(SdfNode::StarPolygon {
                    radius: r,
                    n_points: n,
                    m,
                    half_height: h,
                })
            }
            "cross_shape" => {
                let (l, t, r, h) = self.parse_4f()?;
                Ok(SdfNode::CrossShape {
                    length: l,
                    thickness: t,
                    round_radius: r,
                    half_height: h,
                })
            }

            // ── オペレーション (23) ──
            "union" => {
                let children = self.parse_children()?;
                Ok(Self::fold_left(children, |a, b| SdfNode::Union {
                    a: Arc::new(a),
                    b: Arc::new(b),
                }))
            }
            "smooth_union" => {
                let (k, children) = self.parse_k_children()?;
                Ok(Self::fold_left(children, |a, b| SdfNode::SmoothUnion {
                    a: Arc::new(a),
                    b: Arc::new(b),
                    k,
                }))
            }
            "intersection" => {
                let children = self.parse_children()?;
                Ok(Self::fold_left(children, |a, b| SdfNode::Intersection {
                    a: Arc::new(a),
                    b: Arc::new(b),
                }))
            }
            "smooth_intersection" => {
                let (k, children) = self.parse_k_children()?;
                Ok(Self::fold_left(children, |a, b| {
                    SdfNode::SmoothIntersection {
                        a: Arc::new(a),
                        b: Arc::new(b),
                        k,
                    }
                }))
            }
            "subtract" => {
                let a = self.parse_expr()?;
                self.expect_comma()?;
                let b = self.parse_expr()?;
                self.expect_rparen()?;
                Ok(SdfNode::Subtraction {
                    a: Arc::new(a),
                    b: Arc::new(b),
                })
            }
            "smooth_subtract" => {
                let (k, a, b) = self.parse_1f_ab()?;
                Ok(SdfNode::SmoothSubtraction {
                    a: Arc::new(a),
                    b: Arc::new(b),
                    k,
                })
            }
            "chamfer_union" => {
                let (r, children) = self.parse_k_children()?;
                Ok(Self::fold_left(children, |a, b| SdfNode::ChamferUnion {
                    a: Arc::new(a),
                    b: Arc::new(b),
                    r,
                }))
            }
            "chamfer_intersection" => {
                let (r, children) = self.parse_k_children()?;
                Ok(Self::fold_left(children, |a, b| {
                    SdfNode::ChamferIntersection {
                        a: Arc::new(a),
                        b: Arc::new(b),
                        r,
                    }
                }))
            }
            "chamfer_subtraction" => {
                let (r, a, b) = self.parse_1f_ab()?;
                Ok(SdfNode::ChamferSubtraction {
                    a: Arc::new(a),
                    b: Arc::new(b),
                    r,
                })
            }
            "stairs_union" => {
                let (r, n, children) = self.parse_2f_children()?;
                Ok(Self::fold_left(children, |a, b| SdfNode::StairsUnion {
                    a: Arc::new(a),
                    b: Arc::new(b),
                    r,
                    n,
                }))
            }
            "stairs_intersection" => {
                let (r, n, children) = self.parse_2f_children()?;
                Ok(Self::fold_left(children, |a, b| {
                    SdfNode::StairsIntersection {
                        a: Arc::new(a),
                        b: Arc::new(b),
                        r,
                        n,
                    }
                }))
            }
            "stairs_subtraction" => {
                let (r, n, a, b) = self.parse_2f_ab()?;
                Ok(SdfNode::StairsSubtraction {
                    a: Arc::new(a),
                    b: Arc::new(b),
                    r,
                    n,
                })
            }
            "xor" => {
                let a = self.parse_expr()?;
                self.expect_comma()?;
                let b = self.parse_expr()?;
                self.expect_rparen()?;
                Ok(SdfNode::XOR {
                    a: Arc::new(a),
                    b: Arc::new(b),
                })
            }
            "pipe" => {
                let (r, a, b) = self.parse_1f_ab()?;
                Ok(SdfNode::Pipe {
                    a: Arc::new(a),
                    b: Arc::new(b),
                    r,
                })
            }
            "engrave" => {
                let (r, a, b) = self.parse_1f_ab()?;
                Ok(SdfNode::Engrave {
                    a: Arc::new(a),
                    b: Arc::new(b),
                    r,
                })
            }
            "groove" => {
                let (ra, rb, a, b) = self.parse_2f_ab()?;
                Ok(SdfNode::Groove {
                    a: Arc::new(a),
                    b: Arc::new(b),
                    ra,
                    rb,
                })
            }
            "tongue" => {
                let (ra, rb, a, b) = self.parse_2f_ab()?;
                Ok(SdfNode::Tongue {
                    a: Arc::new(a),
                    b: Arc::new(b),
                    ra,
                    rb,
                })
            }
            "columns_union" => {
                let (r, n, children) = self.parse_2f_children()?;
                Ok(Self::fold_left(children, |a, b| SdfNode::ColumnsUnion {
                    a: Arc::new(a),
                    b: Arc::new(b),
                    r,
                    n,
                }))
            }
            "columns_intersection" => {
                let (r, n, children) = self.parse_2f_children()?;
                Ok(Self::fold_left(children, |a, b| {
                    SdfNode::ColumnsIntersection {
                        a: Arc::new(a),
                        b: Arc::new(b),
                        r,
                        n,
                    }
                }))
            }
            "columns_subtraction" => {
                let (r, n, a, b) = self.parse_2f_ab()?;
                Ok(SdfNode::ColumnsSubtraction {
                    a: Arc::new(a),
                    b: Arc::new(b),
                    r,
                    n,
                })
            }
            "exp_smooth_union" => {
                let (k, children) = self.parse_k_children()?;
                Ok(Self::fold_left(children, |a, b| SdfNode::ExpSmoothUnion {
                    a: Arc::new(a),
                    b: Arc::new(b),
                    k,
                }))
            }
            "exp_smooth_intersection" => {
                let (k, children) = self.parse_k_children()?;
                Ok(Self::fold_left(children, |a, b| {
                    SdfNode::ExpSmoothIntersection {
                        a: Arc::new(a),
                        b: Arc::new(b),
                        k,
                    }
                }))
            }
            "exp_smooth_subtraction" => {
                let (k, a, b) = self.parse_1f_ab()?;
                Ok(SdfNode::ExpSmoothSubtraction {
                    a: Arc::new(a),
                    b: Arc::new(b),
                    k,
                })
            }

            // ── トランスフォーム (4) ──
            "translate" => {
                let (x, y, z, child) = self.parse_3f_child()?;
                Ok(SdfNode::Translate {
                    child: Arc::new(child),
                    offset: Vec3::new(x, y, z),
                })
            }
            "rotate" => {
                let (rx, ry, rz, child) = self.parse_3f_child()?;
                Ok(SdfNode::Rotate {
                    child: Arc::new(child),
                    rotation: Quat::from_euler(
                        EulerRot::XYZ,
                        rx.to_radians(),
                        ry.to_radians(),
                        rz.to_radians(),
                    ),
                })
            }
            "scale" => {
                let (f, child) = self.parse_1f_child()?;
                Ok(SdfNode::Scale {
                    child: Arc::new(child),
                    factor: f,
                })
            }
            "scale_non_uniform" => {
                let (sx, sy, sz, child) = self.parse_3f_child()?;
                Ok(SdfNode::ScaleNonUniform {
                    child: Arc::new(child),
                    factors: Vec3::new(sx, sy, sz),
                })
            }

            // ── モディファイア (19) ──
            "round" => {
                let (r, child) = self.parse_1f_child()?;
                Ok(SdfNode::Round {
                    child: Arc::new(child),
                    radius: r,
                })
            }
            "onion" => {
                let (t, child) = self.parse_1f_child()?;
                Ok(SdfNode::Onion {
                    child: Arc::new(child),
                    thickness: t,
                })
            }
            "twist" => {
                let (s, child) = self.parse_1f_child()?;
                Ok(SdfNode::Twist {
                    child: Arc::new(child),
                    strength: s,
                })
            }
            "bend" => {
                let (c, child) = self.parse_1f_child()?;
                Ok(SdfNode::Bend {
                    child: Arc::new(child),
                    curvature: c,
                })
            }
            "mirror" => {
                let (ax, ay, az, child) = self.parse_3f_child()?;
                Ok(SdfNode::Mirror {
                    child: Arc::new(child),
                    axes: Vec3::new(ax, ay, az),
                })
            }
            "repeat" => {
                let (sx, sy, sz, child) = self.parse_3f_child()?;
                Ok(SdfNode::RepeatInfinite {
                    child: Arc::new(child),
                    spacing: Vec3::new(sx, sy, sz),
                })
            }
            "elongate" => {
                let (ax, ay, az, child) = self.parse_3f_child()?;
                Ok(SdfNode::Elongate {
                    child: Arc::new(child),
                    amount: Vec3::new(ax, ay, az),
                })
            }
            "revolution" => {
                let (off, child) = self.parse_1f_child()?;
                Ok(SdfNode::Revolution {
                    child: Arc::new(child),
                    offset: off,
                })
            }
            "extrude" => {
                let (h, child) = self.parse_1f_child()?;
                Ok(SdfNode::Extrude {
                    child: Arc::new(child),
                    half_height: h,
                })
            }
            "taper" => {
                let (f, child) = self.parse_1f_child()?;
                Ok(SdfNode::Taper {
                    child: Arc::new(child),
                    factor: f,
                })
            }
            "displacement" => {
                let (s, child) = self.parse_1f_child()?;
                Ok(SdfNode::Displacement {
                    child: Arc::new(child),
                    strength: s,
                })
            }
            "polar_repeat" => {
                let (c, child) = self.parse_1f_child()?;
                #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                Ok(SdfNode::PolarRepeat {
                    child: Arc::new(child),
                    count: c as u32,
                })
            }
            "shear" => {
                let (xy, xz, yz, child) = self.parse_3f_child()?;
                Ok(SdfNode::Shear {
                    child: Arc::new(child),
                    shear: Vec3::new(xy, xz, yz),
                })
            }
            "noise" => {
                let (amp, freq, seed, child) = self.parse_3f_child()?;
                #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                Ok(SdfNode::Noise {
                    child: Arc::new(child),
                    amplitude: amp,
                    frequency: freq,
                    seed: seed as u32,
                })
            }
            "repeat_finite" => {
                let (cx, cy, cz, sx, sy, sz, child) = self.parse_6f_child()?;
                #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                Ok(SdfNode::RepeatFinite {
                    child: Arc::new(child),
                    count: [cx as u32, cy as u32, cz as u32],
                    spacing: Vec3::new(sx, sy, sz),
                })
            }
            "octant_mirror" => {
                let child = self.parse_child_only()?;
                Ok(SdfNode::OctantMirror {
                    child: Arc::new(child),
                })
            }
            "icosahedral_symmetry" => {
                let child = self.parse_child_only()?;
                Ok(SdfNode::IcosahedralSymmetry {
                    child: Arc::new(child),
                })
            }
            "with_material" => {
                let (id, child) = self.parse_1f_child()?;
                #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                Ok(SdfNode::WithMaterial {
                    child: Arc::new(child),
                    material_id: id as u32,
                })
            }
            "surface_roughness" => {
                let (freq, amp, oct, child) = self.parse_3f_child()?;
                #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                Ok(SdfNode::SurfaceRoughness {
                    child: Arc::new(child),
                    frequency: freq,
                    amplitude: amp,
                    octaves: oct as u32,
                })
            }

            // ── 時間制御 (2) ──
            "animate" => {
                let speed = self.expect_number()?;
                self.expect_comma()?;
                let amplitude = self.expect_number()?;
                self.expect_comma()?;
                let child = self.parse_expr()?;
                self.expect_rparen()?;
                Ok(SdfNode::Animated {
                    child: Arc::new(child),
                    speed,
                    amplitude,
                })
            }
            "morph" => {
                let t = self.expect_number()?;
                self.expect_comma()?;
                let a = self.parse_expr()?;
                self.expect_comma()?;
                let b = self.parse_expr()?;
                self.expect_rparen()?;
                Ok(SdfNode::Morph {
                    a: Arc::new(a),
                    b: Arc::new(b),
                    t,
                })
            }

            other => Err(ParseError {
                message: format!("unknown LOL expression: '{other}'"),
                position: self.lexer.position(),
            }),
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 公開 API
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// LOL テキストを [`SdfNode`] にパースする。
///
/// LLM が生成した LOL 構文テキストを受け取り、ALICE-SDF の [`SdfNode`] ツリーに変換。
/// `proc_macro` 版と同等の 76 構文をサポート（値は `f32` リテラルのみ）。
///
/// # Errors
///
/// 構文エラー、未知の関数名、引数の不足・過剰の場合に [`ParseError`] を返す。
///
/// # Examples
///
/// ```
/// use alice_lol::runtime_parser::parse_lol;
///
/// let node = parse_lol("sphere(1.0)").unwrap();
/// let dist = alice_lol::eval(&node, glam::Vec3::ZERO);
/// assert!((dist - (-1.0)).abs() < 1e-6);
/// ```
///
/// ```
/// use alice_lol::runtime_parser::parse_lol;
///
/// let node = parse_lol("smooth_union(0.3, sphere(1.0), box3d(0.5, 0.5, 0.5))").unwrap();
/// ```
pub fn parse_lol(input: &str) -> Result<SdfNode, ParseError> {
    let mut parser = Parser::new(input);
    let node = parser.parse_expr()?;
    // 末尾にゴミがないか確認
    parser.lexer.skip_whitespace();
    if parser.lexer.position() < parser.lexer.input.len() {
        return Err(ParseError {
            message: "unexpected trailing content".into(),
            position: parser.lexer.position(),
        });
    }
    Ok(node)
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// テスト
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sphere() {
        let node = parse_lol("sphere(1.0)").unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!((d - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn test_box3d() {
        let node = parse_lol("box3d(1.0, 1.0, 1.0)").unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!(d < 0.0); // 内部
    }

    #[test]
    fn test_union() {
        let node = parse_lol("union(sphere(1.0), box3d(0.5, 0.5, 0.5))").unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!(d < 0.0);
    }

    #[test]
    fn test_smooth_union() {
        let node = parse_lol("smooth_union(0.3, sphere(1.0), box3d(0.5, 0.5, 0.5))").unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!(d < 0.0);
    }

    #[test]
    fn test_translate() {
        let node = parse_lol("translate(2.0, 0.0, 0.0, sphere(0.5))").unwrap();
        let d_origin = crate::eval(&node, Vec3::ZERO);
        let d_offset = crate::eval(&node, Vec3::new(2.0, 0.0, 0.0));
        assert!(d_origin > 0.0); // 原点は外
        assert!(d_offset < 0.0); // 移動先は内
    }

    #[test]
    fn test_rotate() {
        let node = parse_lol("rotate(0.0, 90.0, 0.0, box3d(2.0, 0.5, 0.5))").unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!(d < 0.0);
    }

    #[test]
    fn test_scale() {
        let node = parse_lol("scale(2.0, sphere(1.0))").unwrap();
        let d = crate::eval(&node, Vec3::new(1.5, 0.0, 0.0));
        assert!(d < 0.0); // r=2 に拡大
    }

    #[test]
    fn test_subtract() {
        let node = parse_lol("subtract(sphere(1.0), sphere(0.5))").unwrap();
        let d_origin = crate::eval(&node, Vec3::ZERO);
        assert!(d_origin > 0.0); // 内部がくり抜かれている
    }

    #[test]
    fn test_intersection() {
        let node = parse_lol("intersection(sphere(1.0), box3d(0.5, 0.5, 0.5))").unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!(d < 0.0);
    }

    #[test]
    fn test_round() {
        let node = parse_lol("round(0.1, box3d(1.0, 1.0, 1.0))").unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!(d < 0.0);
    }

    #[test]
    fn test_onion() {
        let node = parse_lol("onion(0.1, sphere(1.0))").unwrap();
        let d_origin = crate::eval(&node, Vec3::ZERO);
        assert!(d_origin > 0.0); // 中空
    }

    #[test]
    fn test_twist() {
        let node = parse_lol("twist(0.5, box3d(1.0, 2.0, 1.0))").unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!(d < 0.0);
    }

    #[test]
    fn test_mirror() {
        let node =
            parse_lol("mirror(1.0, 0.0, 0.0, translate(1.0, 0.0, 0.0, sphere(0.3)))").unwrap();
        let d_pos = crate::eval(&node, Vec3::new(1.0, 0.0, 0.0));
        let d_neg = crate::eval(&node, Vec3::new(-1.0, 0.0, 0.0));
        assert!(d_pos < 0.0);
        assert!(d_neg < 0.0); // ミラーで反対側にもある
    }

    #[test]
    fn test_polar_repeat() {
        let node = parse_lol("polar_repeat(6, translate(2.0, 0.0, 0.0, sphere(0.3)))").unwrap();
        let d = crate::eval(&node, Vec3::new(2.0, 0.0, 0.0));
        assert!(d < 0.0);
    }

    #[test]
    fn test_torus() {
        let node = parse_lol("torus(1.0, 0.3)").unwrap();
        let d = crate::eval(&node, Vec3::new(1.0, 0.0, 0.0));
        assert!(d < 0.0);
    }

    #[test]
    fn test_cylinder() {
        let node = parse_lol("cylinder(0.5, 1.0)").unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!(d < 0.0);
    }

    #[test]
    fn test_capsule() {
        let node = parse_lol("capsule(0.3, 1.0)").unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!(d < 0.0);
    }

    #[test]
    fn test_cone() {
        let node = parse_lol("cone(1.0, 1.0)").unwrap();
        let d = crate::eval(&node, Vec3::new(0.0, 0.5, 0.0));
        assert!(d < 0.0);
    }

    #[test]
    fn test_ellipsoid() {
        let node = parse_lol("ellipsoid(1.0, 0.5, 0.7)").unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!(d < 0.0);
    }

    #[test]
    fn test_octahedron() {
        let node = parse_lol("octahedron(1.0)").unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!(d < 0.0);
    }

    #[test]
    fn test_heart() {
        let node = parse_lol("heart(1.0)").unwrap();
        assert!(parse_lol("heart(1.0)").is_ok());
        let _ = crate::eval(&node, Vec3::ZERO);
    }

    #[test]
    fn test_egg() {
        let node = parse_lol("egg(1.0, 0.3)").unwrap();
        let _ = crate::eval(&node, Vec3::ZERO);
    }

    #[test]
    fn test_tetrahedron() {
        let node = parse_lol("tetrahedron(1.0)").unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!(d < 0.0);
    }

    #[test]
    fn test_diamond() {
        let node = parse_lol("diamond(0.8, 1.0)").unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!(d < 0.0);
    }

    #[test]
    fn test_box_frame() {
        let node = parse_lol("box_frame(1.0, 1.0, 1.0, 0.1)").unwrap();
        let _ = crate::eval(&node, Vec3::ZERO);
    }

    #[test]
    fn test_helix() {
        let node = parse_lol("helix(1.0, 0.1, 1.0, 2.0)").unwrap();
        let _ = crate::eval(&node, Vec3::ZERO);
    }

    #[test]
    fn test_nary_union() {
        let node = parse_lol("union(sphere(1.0), sphere(0.5), sphere(0.3))").unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!(d < 0.0);
    }

    #[test]
    fn test_smooth_subtract() {
        let node = parse_lol("smooth_subtract(0.1, sphere(1.0), sphere(0.5))").unwrap();
        let _ = crate::eval(&node, Vec3::ZERO);
    }

    #[test]
    fn test_xor() {
        let node = parse_lol("xor(sphere(1.0), sphere(0.8))").unwrap();
        let _ = crate::eval(&node, Vec3::ZERO);
    }

    #[test]
    fn test_morph() {
        let node = parse_lol("morph(0.5, sphere(1.0), box3d(1.0, 1.0, 1.0))").unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!(d < 0.0);
    }

    #[test]
    fn test_scale_non_uniform() {
        let node = parse_lol("scale_non_uniform(2.0, 1.0, 1.0, sphere(1.0))").unwrap();
        let d = crate::eval(&node, Vec3::new(1.5, 0.0, 0.0));
        assert!(d < 0.0);
    }

    #[test]
    fn test_negative_number() {
        let node = parse_lol("translate(-1.0, -2.0, 0.0, sphere(0.5))").unwrap();
        let d = crate::eval(&node, Vec3::new(-1.0, -2.0, 0.0));
        assert!(d < 0.0);
    }

    #[test]
    fn test_nested_complex() {
        let input = "smooth_union(0.2, \
            translate(0.0, 1.0, 0.0, scale_non_uniform(1.5, 0.4, 1.5, sphere(1.0))), \
            cylinder(0.3, 0.8))";
        let node = parse_lol(input).unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!(d < 0.0);
    }

    #[test]
    fn test_whitespace_tolerance() {
        let input = "  smooth_union( 0.3 , sphere( 1.0 ) , box3d( 0.5 , 0.5 , 0.5 ) )  ";
        let node = parse_lol(input).unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!(d < 0.0);
    }

    #[test]
    fn test_multiline() {
        let input = "union(\n  sphere(1.0),\n  translate(0.0, 2.0, 0.0, sphere(0.5))\n)";
        let node = parse_lol(input).unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!(d < 0.0);
    }

    #[test]
    fn test_error_unknown_function() {
        let result = parse_lol("foobar(1.0)");
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("unknown"));
    }

    #[test]
    fn test_error_missing_rparen() {
        let result = parse_lol("sphere(1.0");
        assert!(result.is_err());
    }

    #[test]
    fn test_error_trailing_content() {
        let result = parse_lol("sphere(1.0) extra");
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("trailing"));
    }

    #[test]
    fn test_chamfer_union() {
        let node = parse_lol("chamfer_union(0.1, sphere(1.0), box3d(0.5, 0.5, 0.5))").unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!(d < 0.0);
    }

    #[test]
    fn test_elongate() {
        let node = parse_lol("elongate(0.0, 1.0, 0.0, sphere(0.5))").unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!(d < 0.0);
    }

    #[test]
    fn test_extrude() {
        let node = parse_lol("extrude(0.5, sphere(1.0))").unwrap();
        let _ = crate::eval(&node, Vec3::ZERO);
    }

    #[test]
    fn test_taper() {
        let node = parse_lol("taper(0.3, cylinder(1.0, 2.0))").unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!(d < 0.0);
    }

    #[test]
    fn test_displacement() {
        let node = parse_lol("displacement(0.1, sphere(1.0))").unwrap();
        let _ = crate::eval(&node, Vec3::ZERO);
    }

    #[test]
    fn test_noise() {
        let node = parse_lol("noise(0.1, 2.0, 42, sphere(1.0))").unwrap();
        let _ = crate::eval(&node, Vec3::ZERO);
    }

    #[test]
    fn test_bend() {
        let node = parse_lol("bend(0.3, box3d(0.5, 2.0, 0.5))").unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!(d < 0.0);
    }

    #[test]
    fn test_shear() {
        let node = parse_lol("shear(0.5, 0.0, 0.0, box3d(1.0, 1.0, 1.0))").unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!(d < 0.0);
    }

    #[test]
    fn test_revolution() {
        let node = parse_lol("revolution(1.0, sphere(0.3))").unwrap();
        let _ = crate::eval(&node, Vec3::ZERO);
    }

    #[test]
    fn test_pipe() {
        let node = parse_lol("pipe(0.1, sphere(1.0), box3d(1.0, 1.0, 1.0))").unwrap();
        let _ = crate::eval(&node, Vec3::ZERO);
    }

    #[test]
    fn test_engrave() {
        let node = parse_lol("engrave(0.05, sphere(1.0), box3d(0.5, 0.5, 0.5))").unwrap();
        let _ = crate::eval(&node, Vec3::ZERO);
    }

    #[test]
    fn test_animate() {
        let node = parse_lol("animate(1.0, 0.5, sphere(1.0))").unwrap();
        let _ = crate::eval(&node, Vec3::ZERO);
    }

    #[test]
    fn test_with_material() {
        let node = parse_lol("with_material(1, sphere(1.0))").unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!((d - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn test_snowman_example() {
        let input = "union(\
            sphere(1.0),\
            translate(0.0, 1.3, 0.0, sphere(0.7)),\
            translate(0.0, 2.2, 0.0, sphere(0.5))\
        )";
        let node = parse_lol(input).unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!(d < 0.0);
    }

    #[test]
    fn test_gear_example() {
        let input = "subtract(\
            polar_repeat(12, translate(1.5, 0.0, 0.0, cylinder(0.15, 0.2))),\
            subtract(cylinder(1.8, 0.2), cylinder(0.5, 0.3))\
        )";
        let node = parse_lol(input).unwrap();
        let _ = crate::eval(&node, Vec3::ZERO);
    }

    #[test]
    fn test_plane() {
        let node = parse_lol("plane(0.0, 1.0, 0.0, 0.0)").unwrap();
        let d_below = crate::eval(&node, Vec3::new(0.0, -1.0, 0.0));
        let d_above = crate::eval(&node, Vec3::new(0.0, 1.0, 0.0));
        assert!(d_below < 0.0);
        assert!(d_above > 0.0);
    }

    #[test]
    fn test_pyramid() {
        let node = parse_lol("pyramid(1.0)").unwrap();
        let _ = crate::eval(&node, Vec3::ZERO);
    }

    #[test]
    fn test_hex_prism() {
        let node = parse_lol("hex_prism(0.5, 1.0)").unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!(d < 0.0);
    }

    #[test]
    fn test_link() {
        let node = parse_lol("link(0.5, 0.5, 0.1)").unwrap();
        let _ = crate::eval(&node, Vec3::ZERO);
    }

    #[test]
    fn test_capped_cone() {
        let node = parse_lol("capped_cone(1.0, 0.8, 0.3)").unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!(d < 0.0);
    }

    #[test]
    fn test_rounded_cone() {
        let node = parse_lol("rounded_cone(0.5, 0.2, 1.0)").unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!(d < 0.0);
    }

    #[test]
    fn test_rounded_cylinder() {
        let node = parse_lol("rounded_cylinder(0.5, 0.05, 1.0)").unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!(d < 0.0);
    }

    #[test]
    fn test_tube() {
        let node = parse_lol("tube(1.0, 0.1, 1.0)").unwrap();
        let _ = crate::eval(&node, Vec3::ZERO);
    }

    #[test]
    fn test_barrel() {
        let node = parse_lol("barrel(0.8, 1.0, 0.2)").unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!(d < 0.0);
    }

    #[test]
    fn test_star_polygon() {
        let node = parse_lol("star_polygon(1.0, 5.0, 0.4, 0.3)").unwrap();
        let _ = crate::eval(&node, Vec3::ZERO);
    }

    #[test]
    fn test_cross_shape() {
        let node = parse_lol("cross_shape(1.0, 0.3, 0.05, 0.3)").unwrap();
        let _ = crate::eval(&node, Vec3::ZERO);
    }

    #[test]
    fn test_rounded_box() {
        let node = parse_lol("rounded_box(1.0, 1.0, 1.0, 0.1)").unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!(d < 0.0);
    }

    #[test]
    fn test_capped_torus() {
        let node = parse_lol("capped_torus(1.0, 0.3, 1.57)").unwrap();
        let _ = crate::eval(&node, Vec3::ZERO);
    }

    #[test]
    fn test_repeat_finite() {
        let node = parse_lol("repeat_finite(3.0, 1.0, 3.0, 2.0, 0.0, 2.0, sphere(0.3))").unwrap();
        let _ = crate::eval(&node, Vec3::ZERO);
    }

    #[test]
    fn test_octant_mirror() {
        let node = parse_lol("octant_mirror(translate(1.0, 1.0, 1.0, sphere(0.2)))").unwrap();
        let _ = crate::eval(&node, Vec3::ZERO);
    }

    #[test]
    fn test_icosahedral_symmetry() {
        let node =
            parse_lol("icosahedral_symmetry(translate(1.0, 0.0, 0.0, sphere(0.2)))").unwrap();
        let _ = crate::eval(&node, Vec3::ZERO);
    }

    #[test]
    fn test_surface_roughness() {
        let node = parse_lol("surface_roughness(5.0, 0.1, 3, sphere(1.0))").unwrap();
        let _ = crate::eval(&node, Vec3::ZERO);
    }

    #[test]
    fn test_groove() {
        let node = parse_lol("groove(0.1, 0.05, sphere(1.0), box3d(0.8, 0.8, 0.8))").unwrap();
        let _ = crate::eval(&node, Vec3::ZERO);
    }

    #[test]
    fn test_tongue() {
        let node = parse_lol("tongue(0.1, 0.05, sphere(1.0), box3d(0.8, 0.8, 0.8))").unwrap();
        let _ = crate::eval(&node, Vec3::ZERO);
    }

    #[test]
    fn test_columns_union() {
        let node = parse_lol("columns_union(0.1, 4.0, sphere(1.0), box3d(0.5, 0.5, 0.5))").unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!(d < 0.0);
    }

    #[test]
    fn test_exp_smooth_union() {
        let node = parse_lol("exp_smooth_union(0.3, sphere(1.0), box3d(0.5, 0.5, 0.5))").unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!(d < 0.0);
    }

    #[test]
    fn test_stairs_union() {
        let node = parse_lol("stairs_union(0.2, 4.0, sphere(1.0), box3d(0.5, 0.5, 0.5))").unwrap();
        let d = crate::eval(&node, Vec3::ZERO);
        assert!(d < 0.0);
    }
}
