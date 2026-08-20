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
use glam::{EulerRot, Quat, Vec2, Vec3};
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
        while self.pos < self.input.len() {
            if self.input[self.pos].is_ascii_whitespace() {
                self.pos += 1;
            } else if self.pos + 1 < self.input.len()
                && self.input[self.pos] == b'/'
                && self.input[self.pos + 1] == b'/'
            {
                // 行コメント: 行末までスキップ
                while self.pos < self.input.len() && self.input[self.pos] != b'\n' {
                    self.pos += 1;
                }
            } else {
                break;
            }
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

    /// f32 値 7 個 (Phase B.1.b `gridfinity_bin_ex` 用)
    fn parse_7f(&mut self) -> Result<(f32, f32, f32, f32, f32, f32, f32), ParseError> {
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
        let g = self.expect_number()?;
        self.expect_rparen()?;
        Ok((a, b, c, d, e, f, g))
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

    /// 5 floats
    #[allow(clippy::many_single_char_names)]
    fn parse_5f(&mut self) -> Result<(f32, f32, f32, f32, f32), ParseError> {
        let a = self.expect_number()?;
        self.expect_comma()?;
        let b = self.expect_number()?;
        self.expect_comma()?;
        let c = self.expect_number()?;
        self.expect_comma()?;
        let d = self.expect_number()?;
        self.expect_comma()?;
        let e = self.expect_number()?;
        self.expect_rparen()?;
        Ok((a, b, c, d, e))
    }

    /// 6 floats (no child)
    #[allow(clippy::many_single_char_names)]
    fn parse_6f(&mut self) -> Result<(f32, f32, f32, f32, f32, f32), ParseError> {
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
        self.expect_rparen()?;
        Ok((a, b, c, d, e, f))
    }

    /// 9 floats
    #[allow(clippy::many_single_char_names)]
    fn parse_9f(&mut self) -> Result<(f32, f32, f32, f32, f32, f32, f32, f32, f32), ParseError> {
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
        let g = self.expect_number()?;
        self.expect_comma()?;
        let h = self.expect_number()?;
        self.expect_comma()?;
        let i = self.expect_number()?;
        self.expect_rparen()?;
        Ok((a, b, c, d, e, f, g, h, i))
    }

    /// 10 floats
    #[allow(clippy::many_single_char_names)]
    fn parse_10f(
        &mut self,
    ) -> Result<(f32, f32, f32, f32, f32, f32, f32, f32, f32, f32), ParseError> {
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
        let g = self.expect_number()?;
        self.expect_comma()?;
        let h = self.expect_number()?;
        self.expect_comma()?;
        let i = self.expect_number()?;
        self.expect_comma()?;
        let j = self.expect_number()?;
        self.expect_rparen()?;
        Ok((a, b, c, d, e, f, g, h, i, j))
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

            // ── 3D Print Structural Intent (3) ──
            "lattice_infill" => {
                let (shell_t, scale, lattice_t, child) = self.parse_3f_child()?;
                Ok(SdfNode::Union {
                    a: Arc::new(SdfNode::Onion {
                        child: Arc::new(child.clone()),
                        thickness: shell_t,
                    }),
                    b: Arc::new(SdfNode::Intersection {
                        a: Arc::new(child),
                        b: Arc::new(SdfNode::Gyroid {
                            scale,
                            thickness: lattice_t,
                        }),
                    }),
                })
            }
            "diamond_infill" => {
                let (shell_t, scale, lattice_t, child) = self.parse_3f_child()?;
                Ok(SdfNode::Union {
                    a: Arc::new(SdfNode::Onion {
                        child: Arc::new(child.clone()),
                        thickness: shell_t,
                    }),
                    b: Arc::new(SdfNode::Intersection {
                        a: Arc::new(child),
                        b: Arc::new(SdfNode::DiamondSurface {
                            scale,
                            thickness: lattice_t,
                        }),
                    }),
                })
            }
            "schwarz_infill" => {
                let (shell_t, scale, lattice_t, child) = self.parse_3f_child()?;
                Ok(SdfNode::Union {
                    a: Arc::new(SdfNode::Onion {
                        child: Arc::new(child.clone()),
                        thickness: shell_t,
                    }),
                    b: Arc::new(SdfNode::Intersection {
                        a: Arc::new(child),
                        b: Arc::new(SdfNode::SchwarzP {
                            scale,
                            thickness: lattice_t,
                        }),
                    }),
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

            // ── v1.0 プリミティブ (44) ──
            "triangle" => {
                let (ax, ay, az, bx, by, bz, cx, cy, cz) = self.parse_9f()?;
                Ok(SdfNode::Triangle {
                    point_a: Vec3::new(ax, ay, az),
                    point_b: Vec3::new(bx, by, bz),
                    point_c: Vec3::new(cx, cy, cz),
                })
            }
            "bezier" => {
                let (ax, ay, az, bx, by, bz, cx, cy, cz, r) = self.parse_10f()?;
                Ok(SdfNode::Bezier {
                    point_a: Vec3::new(ax, ay, az),
                    point_b: Vec3::new(bx, by, bz),
                    point_c: Vec3::new(cx, cy, cz),
                    radius: r,
                })
            }
            "triangular_prism" => {
                let (w, d) = self.parse_2f()?;
                Ok(SdfNode::TriangularPrism {
                    width: w,
                    half_depth: d,
                })
            }
            "cut_sphere" => {
                let (r, h) = self.parse_2f()?;
                Ok(SdfNode::CutSphere {
                    radius: r,
                    cut_height: h,
                })
            }
            "cut_hollow_sphere" => {
                let (r, h, t) = self.parse_3f()?;
                Ok(SdfNode::CutHollowSphere {
                    radius: r,
                    cut_height: h,
                    thickness: t,
                })
            }
            "death_star" => {
                let (ra, rb, d) = self.parse_3f()?;
                Ok(SdfNode::DeathStar { ra, rb, d })
            }
            "solid_angle" => {
                let (a, r) = self.parse_2f()?;
                Ok(SdfNode::SolidAngle {
                    angle: a,
                    radius: r,
                })
            }
            "rhombus" => {
                let (la, lb, h, r) = self.parse_4f()?;
                Ok(SdfNode::Rhombus {
                    la,
                    lb,
                    half_height: h,
                    round_radius: r,
                })
            }
            "horseshoe" => {
                let (a, r, l, w, t) = self.parse_5f()?;
                Ok(SdfNode::Horseshoe {
                    angle: a,
                    radius: r,
                    half_length: l,
                    width: w,
                    thickness: t,
                })
            }
            "vesica" => {
                let (r, d) = self.parse_2f()?;
                Ok(SdfNode::Vesica {
                    radius: r,
                    half_dist: d,
                })
            }
            "infinite_cylinder" => {
                let r = self.parse_1f()?;
                Ok(SdfNode::InfiniteCylinder { radius: r })
            }
            "infinite_cone" => {
                let a = self.parse_1f()?;
                Ok(SdfNode::InfiniteCone { angle: a })
            }
            "gyroid" => {
                let (s, t) = self.parse_2f()?;
                Ok(SdfNode::Gyroid {
                    scale: s,
                    thickness: t,
                })
            }
            "chamfered_cube" => {
                let (hx, hy, hz, c) = self.parse_4f()?;
                Ok(SdfNode::ChamferedCube {
                    half_extents: Vec3::new(hx, hy, hz),
                    chamfer: c,
                })
            }
            "schwarz_p" => {
                let (s, t) = self.parse_2f()?;
                Ok(SdfNode::SchwarzP {
                    scale: s,
                    thickness: t,
                })
            }
            "superellipsoid" => {
                let (hx, hy, hz, e1, e2) = self.parse_5f()?;
                Ok(SdfNode::Superellipsoid {
                    half_extents: Vec3::new(hx, hy, hz),
                    e1,
                    e2,
                })
            }
            "rounded_x" => {
                let (w, r, h) = self.parse_3f()?;
                Ok(SdfNode::RoundedX {
                    width: w,
                    round_radius: r,
                    half_height: h,
                })
            }
            "pie" => {
                let (a, r, h) = self.parse_3f()?;
                Ok(SdfNode::Pie {
                    angle: a,
                    radius: r,
                    half_height: h,
                })
            }
            "trapezoid" => {
                let (r1, r2, th, d) = self.parse_4f()?;
                Ok(SdfNode::Trapezoid {
                    r1,
                    r2,
                    trap_height: th,
                    half_depth: d,
                })
            }
            "parallelogram" => {
                let (w, h, s, d) = self.parse_4f()?;
                Ok(SdfNode::Parallelogram {
                    width: w,
                    para_height: h,
                    skew: s,
                    half_depth: d,
                })
            }
            "tunnel" => {
                let (w, h, d) = self.parse_3f()?;
                Ok(SdfNode::Tunnel {
                    width: w,
                    height_2d: h,
                    half_depth: d,
                })
            }
            "uneven_capsule" => {
                let (r1, r2, h, d) = self.parse_4f()?;
                Ok(SdfNode::UnevenCapsule {
                    r1,
                    r2,
                    cap_height: h,
                    half_depth: d,
                })
            }
            "arc_shape" => {
                let (a, r, t, h) = self.parse_4f()?;
                Ok(SdfNode::ArcShape {
                    aperture: a,
                    radius: r,
                    thickness: t,
                    half_height: h,
                })
            }
            "moon" => {
                let (d, ra, rb, h) = self.parse_4f()?;
                Ok(SdfNode::Moon {
                    d,
                    ra,
                    rb,
                    half_height: h,
                })
            }
            "blobby_cross" => {
                let (s, h) = self.parse_2f()?;
                Ok(SdfNode::BlobbyCross {
                    size: s,
                    half_height: h,
                })
            }
            "parabola_segment" => {
                let (w, h, d) = self.parse_3f()?;
                Ok(SdfNode::ParabolaSegment {
                    width: w,
                    para_height: h,
                    half_depth: d,
                })
            }
            "regular_polygon" => {
                let (r, n, h) = self.parse_3f()?;
                Ok(SdfNode::RegularPolygon {
                    radius: r,
                    n_sides: n,
                    half_height: h,
                })
            }
            "stairs_prim" => {
                let (sw, sh, n, d) = self.parse_4f()?;
                Ok(SdfNode::Stairs {
                    step_width: sw,
                    step_height: sh,
                    n_steps: n,
                    half_depth: d,
                })
            }
            "dodecahedron" => {
                let r = self.parse_1f()?;
                Ok(SdfNode::Dodecahedron { radius: r })
            }
            "icosahedron" => {
                let r = self.parse_1f()?;
                Ok(SdfNode::Icosahedron { radius: r })
            }
            "truncated_octahedron" => {
                let r = self.parse_1f()?;
                Ok(SdfNode::TruncatedOctahedron { radius: r })
            }
            "truncated_icosahedron" => {
                let r = self.parse_1f()?;
                Ok(SdfNode::TruncatedIcosahedron { radius: r })
            }
            "diamond_surface" => {
                let (s, t) = self.parse_2f()?;
                Ok(SdfNode::DiamondSurface {
                    scale: s,
                    thickness: t,
                })
            }
            "neovius" => {
                let (s, t) = self.parse_2f()?;
                Ok(SdfNode::Neovius {
                    scale: s,
                    thickness: t,
                })
            }
            "lidinoid" => {
                let (s, t) = self.parse_2f()?;
                Ok(SdfNode::Lidinoid {
                    scale: s,
                    thickness: t,
                })
            }
            "iwp" => {
                let (s, t) = self.parse_2f()?;
                Ok(SdfNode::IWP {
                    scale: s,
                    thickness: t,
                })
            }
            "frd" => {
                let (s, t) = self.parse_2f()?;
                Ok(SdfNode::FRD {
                    scale: s,
                    thickness: t,
                })
            }
            "fischer_koch_s" => {
                let (s, t) = self.parse_2f()?;
                Ok(SdfNode::FischerKochS {
                    scale: s,
                    thickness: t,
                })
            }
            "pmy" => {
                let (s, t) = self.parse_2f()?;
                Ok(SdfNode::PMY {
                    scale: s,
                    thickness: t,
                })
            }
            "circle_2d" => {
                let (r, h) = self.parse_2f()?;
                Ok(SdfNode::Circle2D {
                    radius: r,
                    half_height: h,
                })
            }
            "rect_2d" => {
                let (hx, hy, h) = self.parse_3f()?;
                Ok(SdfNode::Rect2D {
                    half_extents: Vec2::new(hx, hy),
                    half_height: h,
                })
            }
            "segment_2d" => {
                let (ax, ay, bx, by, t, h) = self.parse_6f()?;
                Ok(SdfNode::Segment2D {
                    a: Vec2::new(ax, ay),
                    b: Vec2::new(bx, by),
                    thickness: t,
                    half_height: h,
                })
            }
            "rounded_rect_2d" => {
                let (hx, hy, r, h) = self.parse_4f()?;
                Ok(SdfNode::RoundedRect2D {
                    half_extents: Vec2::new(hx, hy),
                    round_radius: r,
                    half_height: h,
                })
            }
            "annular_2d" => {
                let (r, t, h) = self.parse_3f()?;
                Ok(SdfNode::Annular2D {
                    outer_radius: r,
                    thickness: t,
                    half_height: h,
                })
            }
            "terrain" => {
                let (s, a) = self.parse_2f()?;
                Ok(SdfNode::Terrain {
                    scale: s,
                    amplitude: a,
                })
            }
            // ── v1.0 モディファイア ──
            "sweep_bezier" => {
                let (p0x, p0y, p1x, p1y, p2x, p2y, child) = self.parse_6f_child()?;
                Ok(SdfNode::SweepBezier {
                    child: Arc::new(child),
                    p0: Vec2::new(p0x, p0y),
                    p1: Vec2::new(p1x, p1y),
                    p2: Vec2::new(p2x, p2y),
                })
            }

            // ── Phase 5.1 高階 primitive (stdlib::hardsurface 完成 pattern) ──
            // ALICE 三相原理 Phase 2 Law 経路 (SDF+DC)、pipeline: LOL DSL → SdfNode → DC → 3MF
            // 詳細: docs/PIPELINE_COMPLETE.md
            "shopping_cart_coin" => {
                let (dia, thickness) = self.parse_2f()?;
                Ok(crate::stdlib::hardsurface::thin_sdf::shopping_cart_coin_sdf(dia, thickness))
            }
            "skadis_panel" => {
                let (size, thickness, corner_r) = self.parse_3f()?;
                Ok(crate::stdlib::hardsurface::skadis_sdf::skadis_panel_sdf(
                    size, thickness, corner_r,
                ))
            }
            "skadis_hook_l" => {
                self.expect_rparen()?;
                Ok(crate::stdlib::hardsurface::skadis_sdf::skadis_hook_l_sdf())
            }
            "skadis_hook_j" => {
                self.expect_rparen()?;
                Ok(crate::stdlib::hardsurface::skadis_sdf::skadis_hook_j_sdf())
            }
            "skadis_hook_s" => {
                self.expect_rparen()?;
                Ok(crate::stdlib::hardsurface::skadis_sdf::skadis_hook_s_sdf())
            }
            "skadis_container" => {
                self.expect_rparen()?;
                Ok(crate::stdlib::hardsurface::skadis_sdf::skadis_container_sdf())
            }
            "skadis_clip" => {
                self.expect_rparen()?;
                Ok(crate::stdlib::hardsurface::skadis_sdf::skadis_clip_sdf())
            }
            "skadis_shelf" => {
                self.expect_rparen()?;
                Ok(crate::stdlib::hardsurface::skadis_sdf::skadis_shelf_sdf())
            }
            "skadis_elastic_cord" => {
                self.expect_rparen()?;
                Ok(crate::stdlib::hardsurface::skadis_sdf::skadis_elastic_cord_sdf())
            }

            // ── Phase B.1.b 高階 primitive (stdlib::hardsurface::pattern_sdf) ──
            // Bamboo Rust generator 由来の 4 pattern (wall_hook / gridfinity_bin /
            // drawer_organizer / shelf_divider) を DSL syntax に expose
            // wall_hook / drawer_organizer / shelf_divider は param なし = spec default
            // gridfinity_bin は 3 param (units_x, units_y, height_u) basic 版
            "gridfinity_bin" => {
                let (ux, uy, hu) = self.parse_3f()?;
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let spec = crate::stdlib::hardsurface::pattern_sdf::GridfinitySpec {
                    units_x: ux as u32,
                    units_y: uy as u32,
                    height_u: hu as u32,
                    dividers: None,
                    wall_thickness: 1.2,
                    floor_thickness: 1.5,
                };
                Ok(crate::stdlib::hardsurface::pattern_sdf::gridfinity_bin(
                    &spec,
                ))
            }
            "gridfinity_bin_ex" => {
                // gridfinity_bin_ex(ux, uy, hu, divx, divy, wall, floor) 7 param full spec
                // divx>=1 && divy>=1 → dividers=Some((divx, divy))、それ以外 → None
                // wall <= 0 → default 1.2、floor <= 0 → default 1.5
                let (ux, uy, hu, divx, divy, wall, floor) = self.parse_7f()?;
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let dividers = if divx >= 1.0 && divy >= 1.0 {
                    Some((divx as u32, divy as u32))
                } else {
                    None
                };
                let wall_thickness = if wall > 0.0 { wall } else { 1.2 };
                let floor_thickness = if floor > 0.0 { floor } else { 1.5 };
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let spec = crate::stdlib::hardsurface::pattern_sdf::GridfinitySpec {
                    units_x: ux as u32,
                    units_y: uy as u32,
                    height_u: hu as u32,
                    dividers,
                    wall_thickness,
                    floor_thickness,
                };
                Ok(crate::stdlib::hardsurface::pattern_sdf::gridfinity_bin(
                    &spec,
                ))
            }
            "wall_hook" => {
                self.expect_rparen()?;
                Ok(crate::stdlib::hardsurface::pattern_sdf::wall_hook(
                    &crate::stdlib::hardsurface::pattern_sdf::WallHookSpec::pla_1kgf(),
                ))
            }
            "drawer_organizer" => {
                self.expect_rparen()?;
                Ok(crate::stdlib::hardsurface::pattern_sdf::drawer_organizer(
                    &crate::stdlib::hardsurface::pattern_sdf::DrawerSpec::default_chopsticks_set(),
                ))
            }
            "shelf_divider" => {
                self.expect_rparen()?;
                Ok(crate::stdlib::hardsurface::pattern_sdf::shelf_divider(
                    &crate::stdlib::hardsurface::pattern_sdf::ShelfDividerSpec::field_tested_560x250x120(),
                ))
            }

            // ── organizer-gridfinity-desk PART 2 archetypes (Phase B、2026-08-19) ──
            "sticky_note_holder" => {
                // sticky_note_holder(pad_w, pad_d, height) 3 param、wall/floor は default 1.5
                let (pw, pd, h) = self.parse_3f()?;
                let spec = crate::stdlib::hardsurface::pattern_sdf::StickyNoteHolderSpec {
                    pad_width: pw,
                    pad_depth: pd,
                    height: h,
                    wall_thickness: 1.5,
                    floor_thickness: 1.5,
                };
                Ok(crate::stdlib::hardsurface::pattern_sdf::sticky_note_holder(
                    &spec,
                ))
            }
            "business_card_holder" => {
                // business_card_holder(card_w, card_h, slot_thickness) 3 param
                let (cw, ch, st) = self.parse_3f()?;
                let spec = crate::stdlib::hardsurface::pattern_sdf::BusinessCardHolderSpec {
                    card_width: cw,
                    card_height: ch,
                    slot_thickness: st,
                    slot_depth: ch * 0.6,
                    wall_thickness: 1.5,
                    floor_thickness: 2.0,
                };
                Ok(crate::stdlib::hardsurface::pattern_sdf::business_card_holder(&spec))
            }
            "pen_cup" => {
                // pen_cup(inner_dia, height) 2 param、wall=2.0、floor=2.0 default
                let (dia, h) = self.parse_2f()?;
                let spec = crate::stdlib::hardsurface::pattern_sdf::PenCupSpec {
                    inner_diameter: dia,
                    height: h,
                    wall_thickness: 2.0,
                    floor_thickness: 2.0,
                };
                Ok(crate::stdlib::hardsurface::pattern_sdf::pen_cup(&spec))
            }
            "phone_stand" => {
                // phone_stand(slot_width, back_height, cable_hole_dia) 3 param
                // cable_hole_dia <= 0 → None (穴なし)、それ以外 → Some(dia)
                let (sw, bh, chd) = self.parse_3f()?;
                let cable_hole_dia = if chd > 0.0 { Some(chd) } else { None };
                let spec = crate::stdlib::hardsurface::pattern_sdf::PhoneStandSpec {
                    slot_width: sw,
                    slot_depth: 6.0,
                    base_width: 90.0,
                    base_depth: 90.0,
                    base_thickness: 6.0,
                    back_height: bh,
                    back_thickness: 5.0,
                    cable_hole_dia,
                };
                Ok(crate::stdlib::hardsurface::pattern_sdf::phone_stand(&spec))
            }

            // ── organizer-gridfinity-desk PART 2 B2 archetypes (2026-08-19) ──
            "headphone_holder" => {
                // headphone_holder(arm_length, headband_width, mount_width) 3 param
                // mount_hole_dia は default M4 (4.5mm) 固定、他は spec default
                let (al, hw, mw) = self.parse_3f()?;
                let spec = crate::stdlib::hardsurface::pattern_sdf::HeadphoneHolderSpec {
                    arm_length: al,
                    arm_thickness: 6.0,
                    arm_width: hw,
                    mount_width: mw,
                    mount_height: 68.0,
                    mount_thickness: 6.0,
                    hook_tip_up: 18.0,
                    mount_hole_dia: Some(4.5),
                };
                Ok(crate::stdlib::hardsurface::pattern_sdf::headphone_holder(
                    &spec,
                ))
            }
            "under_desk_mount" => {
                // under_desk_mount(desk_thickness, clamp_width, screw_dia) 3 param
                // screw_dia <= 0 → None (両面テープ想定)、それ以外 → Some(dia)
                let (dt, cw, sd) = self.parse_3f()?;
                let screw_hole_dia = if sd > 0.0 { Some(sd) } else { None };
                let spec = crate::stdlib::hardsurface::pattern_sdf::UnderDeskMountSpec {
                    desk_thickness: dt,
                    clamp_width: cw,
                    clamp_depth: 50.0,
                    clamp_wall_thickness: 4.0,
                    screw_hole_dia,
                };
                Ok(crate::stdlib::hardsurface::pattern_sdf::under_desk_mount(
                    &spec,
                ))
            }
            "desk_shelf" => {
                // desk_shelf(shelf_width, shelf_depth, leg_height) 3 param
                let (sw, sd, lh) = self.parse_3f()?;
                let spec = crate::stdlib::hardsurface::pattern_sdf::DeskShelfSpec {
                    shelf_width: sw,
                    shelf_depth: sd,
                    shelf_thickness: 5.0,
                    leg_height: lh,
                    leg_thickness: 20.0,
                };
                Ok(crate::stdlib::hardsurface::pattern_sdf::desk_shelf(&spec))
            }
            "monitor_riser" => {
                // monitor_riser(width, depth, height) 3 param、cable_hole default Some(40)
                let (w, d, h) = self.parse_3f()?;
                let spec = crate::stdlib::hardsurface::pattern_sdf::MonitorRiserSpec {
                    width: w,
                    depth: d,
                    height: h,
                    platform_thickness: 8.0,
                    leg_thickness: 25.0,
                    cable_hole_dia: Some(40.0),
                };
                Ok(crate::stdlib::hardsurface::pattern_sdf::monitor_riser(
                    &spec,
                ))
            }

            // ── household.md archetypes (Sprint 4、2026-08-19) ──
            "coaster" => {
                // coaster(diameter, thickness) 2 param、lip は default (2.5mm 幅 / 1.5mm 高)
                let (dia, t) = self.parse_2f()?;
                let spec = crate::stdlib::hardsurface::pattern_sdf::CoasterSpec {
                    diameter: dia,
                    thickness: t,
                    lip_width: 2.5,
                    lip_height: 1.5,
                };
                Ok(crate::stdlib::hardsurface::pattern_sdf::coaster(&spec))
            }
            "tissue_box_cover" => {
                // tissue_box_cover(internal_l, internal_w, internal_h) 3 param
                // wall + slot は default (1.6mm 壁 / 80×30mm slot)
                let (il, iw, ih) = self.parse_3f()?;
                let spec = crate::stdlib::hardsurface::pattern_sdf::TissueBoxCoverSpec {
                    internal_length: il,
                    internal_width: iw,
                    internal_height: ih,
                    wall_thickness: 1.6,
                    slot_length: 80.0,
                    slot_width: 30.0,
                };
                Ok(crate::stdlib::hardsurface::pattern_sdf::tissue_box_cover(
                    &spec,
                ))
            }
            "storage_box" => {
                // storage_box(internal_l, internal_w, internal_h) 3 param
                // wall + floor は default (2.0mm 各)、lid + hinge なし (future sprint)
                let (il, iw, ih) = self.parse_3f()?;
                let spec = crate::stdlib::hardsurface::pattern_sdf::StorageBoxSpec {
                    internal_length: il,
                    internal_width: iw,
                    internal_height: ih,
                    wall_thickness: 2.0,
                    floor_thickness: 2.0,
                };
                Ok(crate::stdlib::hardsurface::pattern_sdf::storage_box(&spec))
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

    // ── v1.0 新プリミティブ パーステスト ──

    #[test]
    fn test_v10_simple_prims() {
        // 1-param prims
        for (name, args) in [
            ("infinite_cylinder", "0.5"),
            ("infinite_cone", "0.3"),
            ("dodecahedron", "1.0"),
            ("icosahedron", "1.0"),
            ("truncated_octahedron", "1.0"),
            ("truncated_icosahedron", "1.0"),
        ] {
            let input = format!("{name}({args})");
            assert!(parse_lol(&input).is_ok(), "failed: {input}");
        }
    }

    #[test]
    fn test_v10_2f_prims() {
        for (name, args) in [
            ("triangular_prism", "0.5, 1.0"),
            ("cut_sphere", "1.0, 0.3"),
            ("solid_angle", "0.5, 1.0"),
            ("vesica", "1.0, 0.5"),
            ("gyroid", "3.0, 0.1"),
            ("schwarz_p", "3.0, 0.1"),
            ("blobby_cross", "0.5, 1.0"),
            ("diamond_surface", "3.0, 0.1"),
            ("neovius", "3.0, 0.1"),
            ("lidinoid", "3.0, 0.1"),
            ("iwp", "3.0, 0.1"),
            ("frd", "3.0, 0.1"),
            ("fischer_koch_s", "3.0, 0.1"),
            ("pmy", "3.0, 0.1"),
            ("circle_2d", "0.5, 1.0"),
        ] {
            let input = format!("{name}({args})");
            assert!(parse_lol(&input).is_ok(), "failed: {input}");
        }
    }

    #[test]
    fn test_v10_3f_prims() {
        for (name, args) in [
            ("cut_hollow_sphere", "1.0, 0.3, 0.1"),
            ("death_star", "1.0, 0.5, 0.8"),
            ("rounded_x", "0.5, 0.1, 1.0"),
            ("pie", "0.5, 1.0, 0.5"),
            ("tunnel", "0.5, 0.8, 1.0"),
            ("parabola_segment", "0.5, 0.8, 1.0"),
            ("regular_polygon", "1.0, 6.0, 0.5"),
            ("rect_2d", "0.5, 0.3, 1.0"),
            ("annular_2d", "1.0, 0.1, 0.5"),
        ] {
            let input = format!("{name}({args})");
            assert!(parse_lol(&input).is_ok(), "failed: {input}");
        }
    }

    #[test]
    fn test_v10_4f_prims() {
        for (name, args) in [
            ("rhombus", "0.5, 0.3, 1.0, 0.05"),
            ("chamfered_cube", "0.5, 0.5, 0.5, 0.1"),
            ("trapezoid", "0.5, 0.3, 1.0, 0.5"),
            ("parallelogram", "0.5, 1.0, 0.2, 0.5"),
            ("uneven_capsule", "0.3, 0.5, 1.0, 0.5"),
            ("arc_shape", "0.5, 1.0, 0.1, 0.5"),
            ("moon", "0.5, 1.0, 0.8, 0.5"),
            ("stairs_prim", "0.3, 0.2, 5.0, 0.5"),
            ("rounded_rect_2d", "0.5, 0.3, 0.1, 1.0"),
        ] {
            let input = format!("{name}({args})");
            assert!(parse_lol(&input).is_ok(), "failed: {input}");
        }
    }

    #[test]
    fn test_v10_5f_prims() {
        assert!(parse_lol("horseshoe(0.5, 1.0, 0.3, 0.1, 0.05)").is_ok());
        assert!(parse_lol("superellipsoid(0.5, 0.5, 0.5, 1.0, 1.0)").is_ok());
    }

    #[test]
    fn test_v10_segment_2d() {
        assert!(parse_lol("segment_2d(0.0, 0.0, 1.0, 1.0, 0.1, 0.5)").is_ok());
    }

    #[test]
    fn test_v10_triangle_bezier() {
        assert!(parse_lol("triangle(0.0,0.0,0.0, 1.0,0.0,0.0, 0.5,1.0,0.0)").is_ok());
        assert!(parse_lol("bezier(0.0,0.0,0.0, 0.5,1.0,0.0, 1.0,0.0,0.0, 0.1)").is_ok());
    }

    #[test]
    fn test_v10_sweep_bezier() {
        assert!(
            parse_lol("sweep_bezier(0.0, 0.0, 0.5, 1.0, 1.0, 0.0, circle_2d(0.2, 0.5))").is_ok()
        );
    }

    // ── Phase 5.1 高階 primitive tests ──

    #[test]
    fn test_shopping_cart_coin() {
        let node = parse_lol("shopping_cart_coin(22.8, 1.7)").unwrap();
        match node {
            SdfNode::Cylinder {
                radius,
                half_height,
            } => {
                assert!((radius - 11.4).abs() < 1e-4);
                assert!((half_height - 0.85).abs() < 1e-4);
            }
            _ => panic!("expected Cylinder"),
        }
    }

    #[test]
    fn test_skadis_panel() {
        let node = parse_lol("skadis_panel(300, 5, 5)").unwrap();
        assert!(matches!(node, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn test_skadis_hook_l_no_args() {
        let node = parse_lol("skadis_hook_l()").unwrap();
        assert!(matches!(node, SdfNode::Union { .. }));
    }

    #[test]
    fn test_skadis_hook_j_no_args() {
        let node = parse_lol("skadis_hook_j()").unwrap();
        assert!(matches!(node, SdfNode::Union { .. }));
    }

    #[test]
    fn test_skadis_hook_s_no_args() {
        let node = parse_lol("skadis_hook_s()").unwrap();
        assert!(matches!(node, SdfNode::Union { .. }));
    }

    #[test]
    fn test_phase_5_1_high_level_primitives_eval_correctly() {
        // Phase 5.1 追加 5 primitive の parse + eval sanity check
        use alice_sdf::eval;
        for lol in [
            "shopping_cart_coin(22.8, 1.7)",
            "skadis_panel(300, 5, 5)",
            "skadis_hook_l()",
            "skadis_hook_j()",
            "skadis_hook_s()",
        ] {
            let node = parse_lol(lol).unwrap_or_else(|e| panic!("{lol}: {e:?}"));
            let d = eval(&node, Vec3::new(0.1, 0.1, 0.1));
            assert!(d.is_finite(), "{lol}: non-finite SDF at (0.1,0.1,0.1)");
        }
    }

    // ── Phase B.1.b 高階 primitive tests (pattern_sdf 経路) ──

    #[test]
    fn test_gridfinity_bin_2x2_6u() {
        let node = parse_lol("gridfinity_bin(2, 2, 6)").unwrap();
        assert!(matches!(node, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn test_wall_hook_no_args() {
        let node = parse_lol("wall_hook()").unwrap();
        assert!(matches!(node, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn test_drawer_organizer_no_args() {
        let node = parse_lol("drawer_organizer()").unwrap();
        assert!(matches!(node, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn test_shelf_divider_no_args() {
        let node = parse_lol("shelf_divider()").unwrap();
        assert!(matches!(node, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn test_phase_b_1_b_pattern_sdf_primitives_eval_correctly() {
        // Phase B.1.b 追加 4 primitive の parse + eval sanity check
        use alice_sdf::eval;
        for lol in [
            "gridfinity_bin(2, 2, 6)",
            "wall_hook()",
            "drawer_organizer()",
            "shelf_divider()",
        ] {
            let node = parse_lol(lol).unwrap_or_else(|e| panic!("{lol}: {e:?}"));
            let d = eval(&node, Vec3::new(0.1, 0.1, 0.1));
            assert!(d.is_finite(), "{lol}: non-finite SDF at (0.1,0.1,0.1)");
        }
    }

    // ── Phase C: gridfinity_bin_ex (7 param advanced) tests ──

    #[test]
    fn test_gridfinity_bin_ex_no_dividers_no_walls() {
        // divx=0/divy=0 → dividers=None、wall=0/floor=0 → default (1.2, 1.5)
        let node = parse_lol("gridfinity_bin_ex(2, 2, 6, 0, 0, 0, 0)").unwrap();
        assert!(matches!(node, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn test_gridfinity_bin_ex_with_dividers_and_walls() {
        // 3×3 grid × 6U + dividers 2×2 + wall=1.5 + floor=2.0
        let node = parse_lol("gridfinity_bin_ex(3, 3, 6, 2, 2, 1.5, 2.0)").unwrap();
        assert!(matches!(node, SdfNode::Subtraction { .. }));
    }

    // ── Phase B: PART 2 archetype tests ──

    #[test]
    fn test_sticky_note_holder_small_square() {
        let node = parse_lol("sticky_note_holder(76, 76, 30)").unwrap();
        assert!(matches!(node, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn test_business_card_holder_jp_meishi() {
        let node = parse_lol("business_card_holder(91, 55, 22)").unwrap();
        assert!(matches!(node, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn test_pen_cup_standard() {
        let node = parse_lol("pen_cup(75, 100)").unwrap();
        assert!(matches!(node, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn test_phone_stand_with_cable_hole() {
        let node = parse_lol("phone_stand(14, 100, 18)").unwrap();
        assert!(matches!(node, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn test_phone_stand_no_cable_hole() {
        // cable_hole_dia=0 で穴なし、それでも slot subtract で Subtraction
        let node = parse_lol("phone_stand(14, 100, 0)").unwrap();
        assert!(matches!(node, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn test_phase_b_part2_archetypes_eval_correctly() {
        // organizer-gridfinity-desk PART 2 追加 4 archetype の parse + eval sanity check
        use alice_sdf::eval;
        for lol in [
            "sticky_note_holder(76, 76, 30)",
            "business_card_holder(91, 55, 22)",
            "pen_cup(75, 100)",
            "phone_stand(14, 100, 18)",
            "gridfinity_bin_ex(3, 3, 6, 2, 2, 1.5, 2.0)",
        ] {
            let node = parse_lol(lol).unwrap_or_else(|e| panic!("{lol}: {e:?}"));
            let d = eval(&node, Vec3::new(0.1, 0.1, 0.1));
            assert!(d.is_finite(), "{lol}: non-finite SDF at (0.1,0.1,0.1)");
        }
    }

    // ── Phase B2: PART 2 完成 4 archetype tests ──

    // 2026-08-20 (v2): archetype 別 print-optimal 方針
    // - headphone_holder / under_desk_mount: unwrap (Subtraction/SmoothUnion)
    // - desk_shelf / tissue_box_cover: to_z_up_flipped (Rotate)

    #[test]
    fn test_headphone_holder_wall_mount() {
        let node = parse_lol("headphone_holder(80, 50, 100)").unwrap();
        // unwrap 済、mount_hole あり → Subtraction (mount plate flat 印刷姿勢)
        assert!(matches!(node, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn test_under_desk_mount_standard() {
        let node = parse_lol("under_desk_mount(25, 40, 4)").unwrap();
        // unwrap 済、screw_hole あり → Subtraction
        assert!(matches!(node, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn test_under_desk_mount_no_screw() {
        // screw=0 で穴なし、unwrap 済 → SmoothUnion (両面テープ想定)
        let node = parse_lol("under_desk_mount(25, 40, 0)").unwrap();
        assert!(matches!(node, SdfNode::SmoothUnion { .. }));
    }

    #[test]
    fn test_desk_shelf_desktop() {
        let node = parse_lol("desk_shelf(400, 200, 100)").unwrap();
        // to_z_up_flipped で Rotate top-level (shelf 下 印刷)
        assert!(matches!(node, SdfNode::Rotate { .. }));
    }

    #[test]
    fn test_monitor_riser_compact() {
        let node = parse_lol("monitor_riser(250, 180, 90)").unwrap();
        // cable_hole 有 → Subtraction
        assert!(matches!(node, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn test_phase_b2_part2_archetypes_eval_correctly() {
        // organizer-gridfinity-desk PART 2 B2 追加 4 archetype の parse + eval sanity
        use alice_sdf::eval;
        for lol in [
            "headphone_holder(80, 50, 100)",
            "under_desk_mount(25, 40, 4)",
            "desk_shelf(400, 200, 100)",
            "monitor_riser(250, 180, 90)",
        ] {
            let node = parse_lol(lol).unwrap_or_else(|e| panic!("{lol}: {e:?}"));
            let d = eval(&node, Vec3::new(0.1, 0.1, 0.1));
            assert!(d.is_finite(), "{lol}: non-finite SDF at (0.1,0.1,0.1)");
        }
    }

    // ── Sprint 4: household.md 3 archetype tests ──

    #[test]
    fn test_coaster_round() {
        let node = parse_lol("coaster(95, 5)").unwrap();
        assert!(matches!(node, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn test_tissue_box_cover_rectangular_us() {
        let node = parse_lol("tissue_box_cover(231, 116, 53)").unwrap();
        // to_z_up_flipped wrap で Rotate top-level (slot 下 印刷 upside-down)
        assert!(matches!(node, SdfNode::Rotate { .. }));
    }

    #[test]
    fn test_storage_box_medium() {
        let node = parse_lol("storage_box(150, 100, 60)").unwrap();
        // to_z_up wrap で Rotate top-level (内部 Subtraction)
        assert!(matches!(node, SdfNode::Rotate { .. }));
    }

    #[test]
    fn test_sprint4_household_archetypes_eval_correctly() {
        // household.md 追加 3 archetype の parse + eval sanity
        use alice_sdf::eval;
        for lol in [
            "coaster(95, 5)",
            "tissue_box_cover(231, 116, 53)",
            "storage_box(150, 100, 60)",
        ] {
            let node = parse_lol(lol).unwrap_or_else(|e| panic!("{lol}: {e:?}"));
            let d = eval(&node, Vec3::new(0.1, 0.1, 0.1));
            assert!(d.is_finite(), "{lol}: non-finite SDF at (0.1,0.1,0.1)");
        }
    }
}
