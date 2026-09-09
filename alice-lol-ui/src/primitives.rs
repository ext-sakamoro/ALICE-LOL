//! UI primitive 8 種 — Button / Panel / Card / Icon / Divider / `InputField` / Badge / Chip
//!
//! 全 primitive は [`SdfNode`] を return、Z 方向は [`UI_HALF_HEIGHT`](crate::UI_HALF_HEIGHT)
//! で薄く extrude される 3D 空間配置は caller が Translate で行う

use alice_sdf::types::SdfNode;
use glam::{Vec2, Vec3};
use std::sync::Arc;

use crate::UI_HALF_HEIGHT;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Button — 角丸矩形の interactive element
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Button primitive (`RoundedRect2D` extruded、tap/click 対象)
#[derive(Debug, Clone, Copy)]
pub struct Button {
    /// full width [CSS px 相当]
    pub width: f32,
    /// full height [CSS px 相当]
    pub height: f32,
    /// 角丸半径
    pub corner_radius: f32,
}

impl Button {
    /// 新規 Button を構築
    #[must_use]
    pub const fn new(width: f32, height: f32, corner_radius: f32) -> Self {
        Self {
            width,
            height,
            corner_radius,
        }
    }

    /// [`SdfNode`] を build
    #[must_use]
    pub fn build(&self) -> SdfNode {
        SdfNode::RoundedRect2D {
            half_extents: Vec2::new(self.width * 0.5, self.height * 0.5),
            round_radius: self.corner_radius,
            half_height: UI_HALF_HEIGHT,
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Panel — flat 背景面 (Card の shadow なし版)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Panel primitive (`RoundedRect2D` flat、背景 surface)
#[derive(Debug, Clone, Copy)]
pub struct Panel {
    /// full width
    pub width: f32,
    /// full height
    pub height: f32,
    /// 角丸半径
    pub corner_radius: f32,
}

impl Panel {
    /// 新規 Panel を構築
    #[must_use]
    pub const fn new(width: f32, height: f32, corner_radius: f32) -> Self {
        Self {
            width,
            height,
            corner_radius,
        }
    }

    /// [`SdfNode`] を build
    #[must_use]
    pub fn build(&self) -> SdfNode {
        SdfNode::RoundedRect2D {
            half_extents: Vec2::new(self.width * 0.5, self.height * 0.5),
            round_radius: self.corner_radius,
            half_height: UI_HALF_HEIGHT,
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Card — Panel + drop shadow layer
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Card primitive (Panel + drop shadow の Union 合成)
#[derive(Debug, Clone, Copy)]
pub struct Card {
    /// full width
    pub width: f32,
    /// full height
    pub height: f32,
    /// 角丸半径
    pub corner_radius: f32,
    /// shadow の (x, y) offset (+y = 下、shader 側 rendering で blur)
    pub shadow_offset: f32,
}

impl Card {
    /// 新規 Card を構築
    #[must_use]
    pub const fn new(width: f32, height: f32, corner_radius: f32, shadow_offset: f32) -> Self {
        Self {
            width,
            height,
            corner_radius,
            shadow_offset,
        }
    }

    /// [`SdfNode`] を build (surface + shadow layer の Union)
    #[must_use]
    pub fn build(&self) -> SdfNode {
        let surface = SdfNode::RoundedRect2D {
            half_extents: Vec2::new(self.width * 0.5, self.height * 0.5),
            round_radius: self.corner_radius,
            half_height: UI_HALF_HEIGHT,
        };
        let shadow = SdfNode::Translate {
            child: Arc::new(SdfNode::RoundedRect2D {
                half_extents: Vec2::new(self.width * 0.5, self.height * 0.5),
                round_radius: self.corner_radius,
                half_height: UI_HALF_HEIGHT * 0.5, // shadow は薄く
            }),
            offset: Vec3::new(
                self.shadow_offset,
                -self.shadow_offset,
                -UI_HALF_HEIGHT * 0.5,
            ),
        };
        SdfNode::Union {
            a: Arc::new(surface),
            b: Arc::new(shadow),
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Icon — 5 種基本図形 (Circle / Square / Triangle / Cross / Chevron)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Icon primitive (5 種基本図形)
///
/// Circle / Square / Triangle / Cross / Chevron 各々 `SdfNode` を返す static method
pub struct Icon;

impl Icon {
    /// 円 icon
    #[must_use]
    pub const fn circle(radius: f32) -> SdfNode {
        SdfNode::Circle2D {
            radius,
            half_height: UI_HALF_HEIGHT,
        }
    }

    /// 正方形 icon (side は 1 辺)
    #[must_use]
    pub fn square(side: f32) -> SdfNode {
        SdfNode::Rect2D {
            half_extents: Vec2::splat(side * 0.5),
            half_height: UI_HALF_HEIGHT,
        }
    }

    /// 上向き三角 icon (等辺三角形、size は外接円半径)
    #[must_use]
    pub fn triangle(size: f32) -> SdfNode {
        // 等辺三角形 3 頂点 (上、左下、右下)、外接円半径 size
        let vertices = vec![
            Vec2::new(0.0, size),
            Vec2::new(-size * 0.866_025_4, -size * 0.5),
            Vec2::new(size * 0.866_025_4, -size * 0.5),
        ];
        SdfNode::Polygon2D {
            vertices,
            half_height: UI_HALF_HEIGHT,
        }
    }

    /// 十字 icon (2 本の `Segment2D` の Union、close / delete UI に使う)
    #[must_use]
    pub fn cross(size: f32) -> SdfNode {
        let thickness = size * 0.15;
        let arm1 = SdfNode::Segment2D {
            a: Vec2::new(-size * 0.5, -size * 0.5),
            b: Vec2::new(size * 0.5, size * 0.5),
            thickness,
            half_height: UI_HALF_HEIGHT,
        };
        let arm2 = SdfNode::Segment2D {
            a: Vec2::new(-size * 0.5, size * 0.5),
            b: Vec2::new(size * 0.5, -size * 0.5),
            thickness,
            half_height: UI_HALF_HEIGHT,
        };
        SdfNode::Union {
            a: Arc::new(arm1),
            b: Arc::new(arm2),
        }
    }

    /// 右向き Chevron icon (> 記号、arrow / expand UI 用)
    #[must_use]
    pub fn chevron(size: f32) -> SdfNode {
        let thickness = size * 0.15;
        let arm1 = SdfNode::Segment2D {
            a: Vec2::new(-size * 0.3, size * 0.5),
            b: Vec2::new(size * 0.3, 0.0),
            thickness,
            half_height: UI_HALF_HEIGHT,
        };
        let arm2 = SdfNode::Segment2D {
            a: Vec2::new(size * 0.3, 0.0),
            b: Vec2::new(-size * 0.3, -size * 0.5),
            thickness,
            half_height: UI_HALF_HEIGHT,
        };
        SdfNode::Union {
            a: Arc::new(arm1),
            b: Arc::new(arm2),
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Divider — 区切り線
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Divider primitive (水平 / 垂直の細線)
pub struct Divider;

impl Divider {
    /// 水平線 (Y=0 に沿って、length は X 方向)
    #[must_use]
    pub fn horizontal(length: f32, thickness: f32) -> SdfNode {
        SdfNode::Segment2D {
            a: Vec2::new(-length * 0.5, 0.0),
            b: Vec2::new(length * 0.5, 0.0),
            thickness: thickness * 0.5,
            half_height: UI_HALF_HEIGHT,
        }
    }

    /// 垂直線 (X=0 に沿って、length は Y 方向)
    #[must_use]
    pub fn vertical(length: f32, thickness: f32) -> SdfNode {
        SdfNode::Segment2D {
            a: Vec2::new(0.0, -length * 0.5),
            b: Vec2::new(0.0, length * 0.5),
            thickness: thickness * 0.5,
            half_height: UI_HALF_HEIGHT,
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// InputField — text 入力枠
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// `InputField` primitive (`RoundedRect2D` 内側 border 表現)
///
/// Panel と分離型で構造タグ付け (a11y 検査で input として認識できる)
#[derive(Debug, Clone, Copy)]
pub struct InputField {
    /// full width
    pub width: f32,
    /// full height
    pub height: f32,
    /// 角丸半径
    pub corner_radius: f32,
}

impl InputField {
    /// 新規 `InputField` を構築
    #[must_use]
    pub const fn new(width: f32, height: f32, corner_radius: f32) -> Self {
        Self {
            width,
            height,
            corner_radius,
        }
    }

    /// [`SdfNode`] を build
    #[must_use]
    pub fn build(&self) -> SdfNode {
        SdfNode::RoundedRect2D {
            half_extents: Vec2::new(self.width * 0.5, self.height * 0.5),
            round_radius: self.corner_radius,
            half_height: UI_HALF_HEIGHT,
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Badge — 小型丸 (notification dot 等)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Badge primitive (`Circle2D`、notification / status dot)
#[derive(Debug, Clone, Copy)]
pub struct Badge {
    /// 半径
    pub radius: f32,
}

impl Badge {
    /// 新規 Badge を構築
    #[must_use]
    pub const fn new(radius: f32) -> Self {
        Self { radius }
    }

    /// [`SdfNode`] を build
    #[must_use]
    pub const fn build(&self) -> SdfNode {
        SdfNode::Circle2D {
            radius: self.radius,
            half_height: UI_HALF_HEIGHT,
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Chip — pill 形 (tag、chip UI)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Chip primitive (pill 形 `RoundedRect2D`、`corner_radius = height/2`)
#[derive(Debug, Clone, Copy)]
pub struct Chip {
    /// full width
    pub width: f32,
    /// full height
    pub height: f32,
}

impl Chip {
    /// 新規 Chip を構築
    #[must_use]
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    /// [`SdfNode`] を build (corner は height 半分で pill 形)
    #[must_use]
    pub fn build(&self) -> SdfNode {
        SdfNode::RoundedRect2D {
            half_extents: Vec2::new(self.width * 0.5, self.height * 0.5),
            round_radius: self.height * 0.5,
            half_height: UI_HALF_HEIGHT,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_rounded_rect(node: &SdfNode) -> bool {
        matches!(node, SdfNode::RoundedRect2D { .. })
    }

    fn is_circle(node: &SdfNode) -> bool {
        matches!(node, SdfNode::Circle2D { .. })
    }

    fn is_segment(node: &SdfNode) -> bool {
        matches!(node, SdfNode::Segment2D { .. })
    }

    fn is_polygon(node: &SdfNode) -> bool {
        matches!(node, SdfNode::Polygon2D { .. })
    }

    fn is_union(node: &SdfNode) -> bool {
        matches!(node, SdfNode::Union { .. })
    }

    // ─── Button ───

    #[test]
    fn button_builds_rounded_rect() {
        let btn = Button::new(120.0, 44.0, 8.0).build();
        assert!(is_rounded_rect(&btn));
    }

    #[test]
    fn button_dimensions_preserved() {
        let SdfNode::RoundedRect2D {
            half_extents,
            round_radius,
            ..
        } = Button::new(120.0, 44.0, 8.0).build()
        else {
            panic!("expected RoundedRect2D");
        };
        assert!((half_extents.x - 60.0).abs() < 1e-5);
        assert!((half_extents.y - 22.0).abs() < 1e-5);
        assert!((round_radius - 8.0).abs() < 1e-5);
    }

    // ─── Panel ───

    #[test]
    fn panel_builds_rounded_rect() {
        assert!(is_rounded_rect(&Panel::new(300.0, 200.0, 12.0).build()));
    }

    // ─── Card ───

    #[test]
    fn card_builds_union_of_surface_and_shadow() {
        assert!(is_union(&Card::new(300.0, 200.0, 12.0, 4.0).build()));
    }

    // ─── Icon ───

    #[test]
    fn icon_circle_returns_circle2d() {
        assert!(is_circle(&Icon::circle(10.0)));
    }

    #[test]
    fn icon_square_returns_rect2d() {
        assert!(matches!(Icon::square(20.0), SdfNode::Rect2D { .. }));
    }

    #[test]
    fn icon_triangle_returns_polygon2d_with_3_vertices() {
        let SdfNode::Polygon2D { vertices, .. } = Icon::triangle(20.0) else {
            panic!("expected Polygon2D");
        };
        assert_eq!(vertices.len(), 3);
    }

    #[test]
    fn icon_triangle_is_polygon() {
        assert!(is_polygon(&Icon::triangle(20.0)));
    }

    #[test]
    fn icon_cross_returns_union_of_two_segments() {
        assert!(is_union(&Icon::cross(20.0)));
    }

    #[test]
    fn icon_chevron_returns_union_of_two_segments() {
        assert!(is_union(&Icon::chevron(20.0)));
    }

    // ─── Divider ───

    #[test]
    fn divider_horizontal_is_segment() {
        assert!(is_segment(&Divider::horizontal(200.0, 1.0)));
    }

    #[test]
    fn divider_vertical_is_segment() {
        assert!(is_segment(&Divider::vertical(100.0, 1.0)));
    }

    #[test]
    fn divider_horizontal_length_reflected_in_endpoints() {
        let SdfNode::Segment2D { a, b, .. } = Divider::horizontal(200.0, 1.0) else {
            panic!("expected Segment2D");
        };
        assert!((b.x - a.x - 200.0).abs() < 1e-5);
        assert!(a.y.abs() < 1e-5 && b.y.abs() < 1e-5);
    }

    // ─── InputField ───

    #[test]
    fn input_field_builds_rounded_rect() {
        assert!(is_rounded_rect(&InputField::new(200.0, 44.0, 6.0).build()));
    }

    // ─── Badge ───

    #[test]
    fn badge_builds_circle() {
        assert!(is_circle(&Badge::new(6.0).build()));
    }

    // ─── Chip ───

    #[test]
    fn chip_builds_rounded_rect_with_pill_shape() {
        let chip = Chip::new(80.0, 24.0).build();
        assert!(is_rounded_rect(&chip));
        let SdfNode::RoundedRect2D { round_radius, .. } = chip else {
            panic!("expected RoundedRect2D");
        };
        // corner_radius = height / 2 = 12.0 で pill 形
        assert!((round_radius - 12.0).abs() < 1e-5);
    }
}
