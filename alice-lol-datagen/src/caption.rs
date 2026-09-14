//! caption 部品 (英 / 日) + 構造列挙 captioner
//!
//! family は parameter から文を組み立てる 数値は `fmt_mm` で「切りのいい」表記
//! (`25` / `12.5`) にして、LOL 側の `25.0` と対応が取れるようにする
//! [`describe`] は任意の `SdfNode` を英語で構造列挙する (`random_tree` 用、
//! 幾何的に正しいが自然な文ではない)

use alice_lol::emit::fmt_f32;
use alice_lol::SdfNode;
use glam::{EulerRot, Vec3};

/// mm 値の caption 表記 (`25.0` → `25`、`12.5` → `12.5`)
#[must_use]
pub fn fmt_mm(v: f32) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        fmt_f32(v)
    }
}

/// `(x, y, z)` 表記
#[must_use]
pub fn fmt_pos(p: Vec3) -> String {
    format!("({}, {}, {})", fmt_mm(p.x), fmt_mm(p.y), fmt_mm(p.z))
}

/// 英語の位置句 (原点なら "at the origin")
#[must_use]
pub fn at_en(p: Vec3) -> String {
    if p == Vec3::ZERO {
        "at the origin".to_string()
    } else {
        format!("centered at {}", fmt_pos(p))
    }
}

/// 日本語の位置句
#[must_use]
pub fn at_ja(p: Vec3) -> String {
    if p == Vec3::ZERO {
        "原点に".to_string()
    } else {
        format!("中心 {} に", fmt_pos(p))
    }
}

/// 任意の `SdfNode` を英語で構造列挙 (`random_tree` 用)
#[must_use]
#[allow(clippy::too_many_lines)] // variant 別の文 template
pub fn describe(node: &SdfNode) -> String {
    match node {
        SdfNode::Sphere { radius } => format!("a sphere of radius {}", fmt_mm(*radius)),
        SdfNode::Box3d { half_extents } => format!(
            "a box {} wide (X), {} tall (Y), {} deep (Z)",
            fmt_mm(half_extents.x * 2.0),
            fmt_mm(half_extents.y * 2.0),
            fmt_mm(half_extents.z * 2.0)
        ),
        SdfNode::RoundedBox {
            half_extents,
            round_radius,
        } => format!(
            "a rounded box {} x {} x {} with corner radius {}",
            fmt_mm(half_extents.x * 2.0),
            fmt_mm(half_extents.y * 2.0),
            fmt_mm(half_extents.z * 2.0),
            fmt_mm(*round_radius)
        ),
        SdfNode::Cylinder {
            radius,
            half_height,
        } => format!(
            "a cylinder of radius {} and total height {} along Y",
            fmt_mm(*radius),
            fmt_mm(half_height * 2.0)
        ),
        SdfNode::Cone {
            radius,
            half_height,
        } => format!(
            "a cone of base radius {} and total height {}",
            fmt_mm(*radius),
            fmt_mm(half_height * 2.0)
        ),
        SdfNode::Torus {
            major_radius,
            minor_radius,
        } => format!(
            "a torus in the XZ plane with major radius {} and minor radius {}",
            fmt_mm(*major_radius),
            fmt_mm(*minor_radius)
        ),
        SdfNode::Capsule {
            point_a,
            point_b,
            radius,
        } => format!(
            "a capsule of radius {} from {} to {}",
            fmt_mm(*radius),
            fmt_pos(*point_a),
            fmt_pos(*point_b)
        ),
        SdfNode::Ellipsoid { radii } => format!(
            "an ellipsoid with radii {}, {}, {}",
            fmt_mm(radii.x),
            fmt_mm(radii.y),
            fmt_mm(radii.z)
        ),
        SdfNode::HexPrism {
            hex_radius,
            half_height,
        } => format!(
            "a hexagonal prism of radius {} and total height {}",
            fmt_mm(*hex_radius),
            fmt_mm(half_height * 2.0)
        ),
        SdfNode::Translate { child, offset } => {
            format!("{} moved by {}", describe(child), fmt_pos(*offset))
        }
        SdfNode::Rotate { child, rotation } => {
            let (rx, ry, rz) = rotation.to_euler(EulerRot::XYZ);
            format!(
                "{} rotated by {} deg about X, {} deg about Y, {} deg about Z",
                describe(child),
                fmt_mm((rx.to_degrees() * 1000.0).round() / 1000.0),
                fmt_mm((ry.to_degrees() * 1000.0).round() / 1000.0),
                fmt_mm((rz.to_degrees() * 1000.0).round() / 1000.0)
            )
        }
        SdfNode::Scale { child, factor } => {
            format!(
                "{} scaled uniformly by {}",
                describe(child),
                fmt_mm(*factor)
            )
        }
        SdfNode::ScaleNonUniform { child, factors } => format!(
            "{} scaled by {}, {}, {} along X, Y, Z",
            describe(child),
            fmt_mm(factors.x),
            fmt_mm(factors.y),
            fmt_mm(factors.z)
        ),
        SdfNode::Union { a, b } => format!("the union of [{}] and [{}]", describe(a), describe(b)),
        SdfNode::Intersection { a, b } => {
            format!(
                "the intersection of [{}] and [{}]",
                describe(a),
                describe(b)
            )
        }
        SdfNode::Subtraction { a, b } => {
            format!("[{}] with [{}] subtracted", describe(a), describe(b))
        }
        SdfNode::SmoothUnion { a, b, k } => format!(
            "the smooth union (blend {}) of [{}] and [{}]",
            fmt_mm(*k),
            describe(a),
            describe(b)
        ),
        SdfNode::SmoothSubtraction { a, b, k } => format!(
            "[{}] with [{}] smoothly subtracted (blend {})",
            describe(a),
            describe(b),
            fmt_mm(*k)
        ),
        SdfNode::PolarRepeat { child, count } => format!(
            "{} repeated {count} times around the Y axis",
            describe(child)
        ),
        SdfNode::RepeatFinite {
            child,
            count,
            spacing,
        } => format!(
            "{} repeated {}x{}x{} times with spacing {}, {}, {}",
            describe(child),
            count[0],
            count[1],
            count[2],
            fmt_mm(spacing.x),
            fmt_mm(spacing.y),
            fmt_mm(spacing.z)
        ),
        SdfNode::Round { child, radius } => {
            format!(
                "{} with edges rounded by {}",
                describe(child),
                fmt_mm(*radius)
            )
        }
        SdfNode::Onion { child, thickness } => {
            format!(
                "a {} thick shell of {}",
                fmt_mm(*thickness),
                describe(child)
            )
        }
        SdfNode::Twist { child, strength } => {
            format!(
                "{} twisted with strength {}",
                describe(child),
                fmt_mm(*strength)
            )
        }
        SdfNode::Mirror { child, .. } => format!("{} mirrored", describe(child)),
        other => {
            // 構文名だけは LOL text から取れる (先頭の識別子)
            let text = alice_lol::emit::to_lol(other).unwrap_or_default();
            let name = text.split('(').next().unwrap_or("shape").replace('_', " ");
            format!("a {name} shape")
        }
    }
}
