//! # alice-lol-ui
//!
//! UI / UX template for [`alice_lol`] DSL — parametric Button / Card / Panel / Icon /
//! Divider / `InputField` / Badge / Chip → [`SdfNode`] + Flex / Grid / Stack layout +
//! WCAG a11y law checker (contrast + touch target + radius sanity)
//!
//! # 三相原理での位置付け
//!
//! Phase 2 Law (parametric UI 数式) → SDF shader / mesh 描画への合流層
//! Web / UI / UX domain の共通 primitive 抽出、`extoria-website-sdf-v2` 等の実装参考
//!
//! # 3 module 分割
//!
//! - `primitives`: 8 UI primitive (`Button` / `Panel` / `Card` / `Icon` / `Divider` /
//!   `InputField` / `Badge` / `Chip`) → `alice_sdf::types::SdfNode` builder
//! - `layout`: 5 composition (`flex_row` / `flex_col` / `stack` / `grid` / `padding`)
//! - `theme`: `Theme` (bg/fg/accent) + golden ratio spacing / radius scale
//! - `law`: `UiLaw` (WCAG AA contrast + touch target + radius sanity)
//! - `error`: `UiError` (`LayoutInvalid` / `PrimitiveInvalid`)
//!
//! # Quick start
//!
//! ```ignore
//! use alice_lol_ui::{Button, Card, Theme, UiLaw, layout};
//!
//! let theme = Theme::light();
//! let button = Button::new(120.0, 44.0, 8.0).build();
//! let card = Card::new(300.0, 200.0, 12.0, 4.0).build();
//! let form = layout::flex_col(&[card, button], theme.spacing(1), layout::CrossAlign::Center);
//!
//! let law = UiLaw::default_wcag_aa();
//! let violations = law.check_button(120.0, 44.0, 8.0);
//! assert!(violations.is_empty()); // 44px = min touch target OK
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod law;
pub mod layout;
pub mod primitives;
pub mod theme;

pub use error::{UiError, UiViolation};
pub use law::UiLaw;
pub use primitives::{Badge, Button, Card, Chip, Divider, Icon, InputField, Panel};
pub use theme::Theme;

/// UI primitive の Z-extrude 半深さ (canonical、shader 側で depth 使わない場合の default)
///
/// 全 primitive が 2D SDF に見えるよう十分薄く (0.5) 固定 3D 空間に配置する時のみ
/// caller 側で Translate で Z 移動する
pub const UI_HALF_HEIGHT: f32 = 0.5;
