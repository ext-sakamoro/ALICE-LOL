//! Layout composition — flex / stack / grid / padding
//!
//! 全 function は `Vec<SdfNode>` を Translate + Union で合成した単一 [`SdfNode`] を return
//! shader / mesh renderer 側は単一 tree を評価するだけで済む
//!
//! # 座標系
//!
//! - 右手系 Y-up (alice-sdf canonical)
//! - `flex_row` は +X 方向、`flex_col` は -Y 方向 (Y-down UI 慣習)
//! - 子要素の origin は各要素の中心 (`Rect2D` / `RoundedRect2D` の center = origin)

use alice_sdf::types::SdfNode;
use glam::Vec3;
use std::sync::Arc;

use crate::error::UiError;

/// Cross-axis alignment (`flex_row` / `flex_col` の cross 軸整列)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossAlign {
    /// 起点側揃え (top / left)
    Start,
    /// 中央揃え (default、center)
    Center,
    /// 終点側揃え (bottom / right)
    End,
}

/// 子要素の cross-axis size (`flex_row` / `flex_col` の主軸配置に使う metadata)
///
/// primitive 単体では size 情報が失われるため、caller が (`SdfNode`, width, height) の
/// tuple で渡す
pub type Sized = (SdfNode, f32, f32);

/// X 方向並置 (row)
///
/// 各要素を `+X` に配置、`gap` 分ずつ間隔を空け、`cross_align` で Y 揃え
///
/// # Errors
///
/// - [`UiError::LayoutInvalid`]: `children` が空、または `gap` が負
pub fn flex_row(children: &[Sized], gap: f32, cross_align: CrossAlign) -> Result<SdfNode, UiError> {
    if children.is_empty() {
        return Err(UiError::LayoutInvalid("flex_row: children is empty"));
    }
    if gap < 0.0 {
        return Err(UiError::LayoutInvalid("flex_row: gap must be >= 0"));
    }
    let max_h = children.iter().map(|(_, _, h)| *h).fold(0.0_f32, f32::max);
    let mut cursor_x = 0.0_f32;
    let mut nodes: Vec<SdfNode> = Vec::with_capacity(children.len());
    for (node, w, h) in children {
        let center_x = cursor_x + w * 0.5;
        let center_y = cross_align_offset_y(cross_align, *h, max_h);
        nodes.push(SdfNode::Translate {
            child: Arc::new(node.clone()),
            offset: Vec3::new(center_x, center_y, 0.0),
        });
        cursor_x += w + gap;
    }
    Ok(union_all(nodes))
}

/// Y 方向並置 (col、Y-down)
///
/// 各要素を `-Y` 方向に配置、`gap` 分ずつ間隔、`cross_align` で X 揃え
///
/// # Errors
///
/// - [`UiError::LayoutInvalid`]: `children` が空、または `gap` が負
pub fn flex_col(children: &[Sized], gap: f32, cross_align: CrossAlign) -> Result<SdfNode, UiError> {
    if children.is_empty() {
        return Err(UiError::LayoutInvalid("flex_col: children is empty"));
    }
    if gap < 0.0 {
        return Err(UiError::LayoutInvalid("flex_col: gap must be >= 0"));
    }
    let max_w = children.iter().map(|(_, w, _)| *w).fold(0.0_f32, f32::max);
    let mut cursor_y = 0.0_f32;
    let mut nodes: Vec<SdfNode> = Vec::with_capacity(children.len());
    for (node, w, h) in children {
        let center_y = -(cursor_y + h * 0.5);
        let center_x = cross_align_offset_x(cross_align, *w, max_w);
        nodes.push(SdfNode::Translate {
            child: Arc::new(node.clone()),
            offset: Vec3::new(center_x, center_y, 0.0),
        });
        cursor_y += h + gap;
    }
    Ok(union_all(nodes))
}

/// Z-layer stack — 全要素を origin (0, 0, 0) に重ねる
///
/// 後の要素が Union の後段になるため、shader 側で material lookup すれば visual に手前配置可
///
/// # Errors
///
/// - [`UiError::LayoutInvalid`]: `children` が空
pub fn stack(children: &[SdfNode]) -> Result<SdfNode, UiError> {
    if children.is_empty() {
        return Err(UiError::LayoutInvalid("stack: children is empty"));
    }
    Ok(union_all(children.to_vec()))
}

/// n × m grid (row-major)
///
/// `cols` 列で並べ、余った要素は次行、`gap` は行間 = 列間
///
/// # Errors
///
/// - [`UiError::LayoutInvalid`]: `children` が空、`cols` = 0、`gap` < 0
pub fn grid(children: &[Sized], cols: usize, gap: f32) -> Result<SdfNode, UiError> {
    if children.is_empty() {
        return Err(UiError::LayoutInvalid("grid: children is empty"));
    }
    if cols == 0 {
        return Err(UiError::LayoutInvalid("grid: cols must be > 0"));
    }
    if gap < 0.0 {
        return Err(UiError::LayoutInvalid("grid: gap must be >= 0"));
    }
    let max_w = children.iter().map(|(_, w, _)| *w).fold(0.0_f32, f32::max);
    let max_h = children.iter().map(|(_, _, h)| *h).fold(0.0_f32, f32::max);
    let mut nodes: Vec<SdfNode> = Vec::with_capacity(children.len());
    for (i, (node, _, _)) in children.iter().enumerate() {
        let col = i % cols;
        let row = i / cols;
        #[allow(clippy::cast_precision_loss)]
        let col_f = col as f32;
        #[allow(clippy::cast_precision_loss)]
        let row_f = row as f32;
        let center_x = col_f.mul_add(max_w + gap, max_w * 0.5);
        let center_y = -row_f.mul_add(max_h + gap, max_h * 0.5);
        nodes.push(SdfNode::Translate {
            child: Arc::new(node.clone()),
            offset: Vec3::new(center_x, center_y, 0.0),
        });
    }
    Ok(union_all(nodes))
}

/// Padding wrapper — child を Translate で shift、余白をつける
///
/// 純粋な視覚 padding (SDF 上は shift のみ、外形は変わらない)
/// layout 主軸に組み込む時に caller 側の Sized (w, h) に padding 分足す必要
#[must_use]
pub fn padding(child: SdfNode, top: f32, right: f32, bottom: f32, left: f32) -> SdfNode {
    // horizontal 中心を padding 差の半分 shift、vertical 同様
    let shift_x = (left - right) * 0.5;
    let shift_y = (bottom - top) * 0.5;
    SdfNode::Translate {
        child: Arc::new(child),
        offset: Vec3::new(shift_x, shift_y, 0.0),
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Internal helpers
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 複数 `SdfNode` を左結合 Union で合成 (1 個の場合はそのまま return)
fn union_all(mut nodes: Vec<SdfNode>) -> SdfNode {
    debug_assert!(!nodes.is_empty(), "union_all: nodes empty");
    if nodes.len() == 1 {
        return nodes.remove(0);
    }
    let first = nodes.remove(0);
    nodes.into_iter().fold(first, |acc, next| SdfNode::Union {
        a: Arc::new(acc),
        b: Arc::new(next),
    })
}

/// cross-axis Y offset (`flex_row`)
fn cross_align_offset_y(align: CrossAlign, h: f32, max_h: f32) -> f32 {
    match align {
        CrossAlign::Center => 0.0,
        CrossAlign::Start => (max_h - h) * 0.5,
        CrossAlign::End => -(max_h - h) * 0.5,
    }
}

/// cross-axis X offset (`flex_col`)
fn cross_align_offset_x(align: CrossAlign, w: f32, max_w: f32) -> f32 {
    match align {
        CrossAlign::Center => 0.0,
        CrossAlign::Start => -(max_w - w) * 0.5,
        CrossAlign::End => (max_w - w) * 0.5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alice_sdf::types::SdfNode;
    use glam::Vec2;

    fn dummy(w: f32, h: f32) -> SdfNode {
        SdfNode::Rect2D {
            half_extents: Vec2::new(w * 0.5, h * 0.5),
            half_height: 0.5,
        }
    }

    fn sized(w: f32, h: f32) -> Sized {
        (dummy(w, h), w, h)
    }

    fn count_nodes(node: &SdfNode) -> usize {
        match node {
            SdfNode::Union { a, b } => count_nodes(a) + count_nodes(b),
            _ => 1,
        }
    }

    // ─── flex_row ───

    #[test]
    fn flex_row_empty_errors() {
        assert!(matches!(
            flex_row(&[], 4.0, CrossAlign::Center),
            Err(UiError::LayoutInvalid(_))
        ));
    }

    #[test]
    fn flex_row_negative_gap_errors() {
        assert!(matches!(
            flex_row(&[sized(10.0, 10.0)], -1.0, CrossAlign::Center),
            Err(UiError::LayoutInvalid(_))
        ));
    }

    #[test]
    fn flex_row_single_child_returns_translate() {
        let node = flex_row(&[sized(10.0, 10.0)], 4.0, CrossAlign::Center).unwrap();
        assert!(matches!(node, SdfNode::Translate { .. }));
    }

    #[test]
    fn flex_row_three_children_produces_three_translates_in_union_tree() {
        let node = flex_row(
            &[sized(10.0, 10.0), sized(20.0, 10.0), sized(15.0, 10.0)],
            4.0,
            CrossAlign::Center,
        )
        .unwrap();
        assert_eq!(count_nodes(&node), 3);
    }

    // ─── flex_col ───

    #[test]
    fn flex_col_empty_errors() {
        assert!(matches!(
            flex_col(&[], 4.0, CrossAlign::Center),
            Err(UiError::LayoutInvalid(_))
        ));
    }

    #[test]
    fn flex_col_two_children_are_stacked_vertically() {
        let node = flex_col(
            &[sized(20.0, 10.0), sized(20.0, 10.0)],
            4.0,
            CrossAlign::Center,
        )
        .unwrap();
        assert_eq!(count_nodes(&node), 2);
    }

    // ─── stack ───

    #[test]
    fn stack_empty_errors() {
        assert!(matches!(stack(&[]), Err(UiError::LayoutInvalid(_))));
    }

    #[test]
    fn stack_two_layers_produces_union() {
        let node = stack(&[dummy(10.0, 10.0), dummy(20.0, 20.0)]).unwrap();
        assert!(matches!(node, SdfNode::Union { .. }));
    }

    // ─── grid ───

    #[test]
    fn grid_empty_errors() {
        assert!(matches!(grid(&[], 2, 4.0), Err(UiError::LayoutInvalid(_))));
    }

    #[test]
    fn grid_zero_cols_errors() {
        assert!(matches!(
            grid(&[sized(10.0, 10.0)], 0, 4.0),
            Err(UiError::LayoutInvalid(_))
        ));
    }

    #[test]
    fn grid_negative_gap_errors() {
        assert!(matches!(
            grid(&[sized(10.0, 10.0)], 2, -1.0),
            Err(UiError::LayoutInvalid(_))
        ));
    }

    #[test]
    fn grid_2x2_produces_4_translates() {
        let node = grid(
            &[
                sized(10.0, 10.0),
                sized(10.0, 10.0),
                sized(10.0, 10.0),
                sized(10.0, 10.0),
            ],
            2,
            4.0,
        )
        .unwrap();
        assert_eq!(count_nodes(&node), 4);
    }

    // ─── padding ───

    #[test]
    fn padding_wraps_in_translate() {
        let node = padding(dummy(10.0, 10.0), 4.0, 4.0, 4.0, 4.0);
        assert!(matches!(node, SdfNode::Translate { .. }));
    }

    #[test]
    fn padding_asymmetric_shifts_center() {
        let SdfNode::Translate { offset, .. } = padding(dummy(10.0, 10.0), 0.0, 0.0, 0.0, 8.0)
        else {
            panic!("expected Translate");
        };
        // left=8 → 中心が +X に 4 shift
        assert!((offset.x - 4.0).abs() < 1e-5);
    }

    // ─── CrossAlign ───

    #[test]
    fn cross_align_center_yields_zero_offset() {
        assert!(cross_align_offset_y(CrossAlign::Center, 10.0, 20.0).abs() < 1e-6);
        assert!(cross_align_offset_x(CrossAlign::Center, 10.0, 20.0).abs() < 1e-6);
    }

    #[test]
    fn cross_align_start_shifts_smaller_element_up_in_row() {
        // 高い要素 (max_h=20) + 低い要素 (h=10) の row、Start = top 揃え
        // Y-up 系で top = +Y、center は 0、Start = (max_h - h)/2 = +5
        assert!((cross_align_offset_y(CrossAlign::Start, 10.0, 20.0) - 5.0).abs() < 1e-5);
    }
}
