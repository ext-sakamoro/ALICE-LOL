//! 8 primitive の単体表示 + `Theme` light/dark + `UiLaw` 検査 demo
//!
//! 実行:
//! ```bash
//! cargo run --example ui_button_showcase -p alice-lol-ui
//! ```

use alice_lol_ui::{
    layout, Badge, Button, Card, Chip, Divider, Icon, InputField, Panel, Theme, UiLaw,
};

fn main() {
    println!("=== alice-lol-ui: 8 primitive showcase ===\n");

    // Theme 3 preset の contrast 一覧
    for (name, theme) in [
        ("light", Theme::light()),
        ("dark", Theme::dark()),
        ("high_contrast", Theme::high_contrast()),
    ] {
        let ratio = alice_lol_ui::theme::contrast_ratio(theme.fg, theme.bg);
        let accent_ratio = alice_lol_ui::theme::contrast_ratio(theme.accent, theme.bg);
        println!(
            "  theme={name:>14}  fg/bg={ratio:.2}:1  accent/bg={accent_ratio:.2}:1  spacing(0)={} px  radius(0)={} px",
            theme.spacing_base, theme.radius_base
        );
    }

    println!("\n--- 8 primitive samples ---");

    // 8 primitive の SdfNode 種別確認
    let btn = Button::new(120.0, 44.0, 8.0).build();
    let panel = Panel::new(300.0, 200.0, 12.0).build();
    let card = Card::new(300.0, 200.0, 12.0, 4.0).build();
    let icon_circle = Icon::circle(12.0);
    let icon_cross = Icon::cross(16.0);
    let icon_chevron = Icon::chevron(16.0);
    let divider_h = Divider::horizontal(280.0, 1.0);
    let input = InputField::new(240.0, 44.0, 6.0).build();
    let badge = Badge::new(6.0).build();
    let chip = Chip::new(80.0, 32.0).build();

    for (label, node) in [
        ("Button      120×44", &btn),
        ("Panel       300×200", &panel),
        ("Card        300×200+shadow4", &card),
        ("Icon.circle r=12", &icon_circle),
        ("Icon.cross  s=16", &icon_cross),
        ("Icon.chevron s=16", &icon_chevron),
        ("Divider.h   280×1", &divider_h),
        ("InputField  240×44", &input),
        ("Badge       r=6", &badge),
        ("Chip        80×32 pill", &chip),
    ] {
        let variant = match node {
            alice_sdf::types::SdfNode::RoundedRect2D { .. } => "RoundedRect2D",
            alice_sdf::types::SdfNode::Rect2D { .. } => "Rect2D",
            alice_sdf::types::SdfNode::Circle2D { .. } => "Circle2D",
            alice_sdf::types::SdfNode::Segment2D { .. } => "Segment2D",
            alice_sdf::types::SdfNode::Polygon2D { .. } => "Polygon2D",
            alice_sdf::types::SdfNode::Union { .. } => "Union (composite)",
            _ => "other",
        };
        println!("  {label:<30}  → {variant}");
    }

    // UiLaw AA 検査
    println!("\n--- UiLaw AA 検査 ---");
    let law = UiLaw::default_wcag_aa();

    for (label, violations) in [
        ("Button 120×44 r=8", law.check_button(120.0, 44.0, 8.0)),
        (
            "Button 120×32 r=8 (small)",
            law.check_button(120.0, 32.0, 8.0),
        ),
        (
            "Button 120×44 r=30 (over pill)",
            law.check_button(120.0, 44.0, 30.0),
        ),
        (
            "InputField 240×44 r=6",
            law.check_input_field(240.0, 44.0, 6.0),
        ),
        ("Chip 80×24 (small)", law.check_chip(80.0, 24.0)),
        ("Chip 80×48 (proper)", law.check_chip(80.0, 48.0)),
    ] {
        if violations.is_empty() {
            println!("  {label:<38}  → PASS");
        } else {
            for v in &violations {
                println!("  {label:<38}  → FAIL [{}] {}", v.rule, v.detail);
            }
        }
    }

    // Layout composition sanity
    println!("\n--- layout composition ---");
    let row = layout::flex_row(
        &[
            (btn, 120.0, 44.0),
            (Chip::new(60.0, 44.0).build(), 60.0, 44.0),
            (Chip::new(80.0, 44.0).build(), 80.0, 44.0),
        ],
        8.0,
        layout::CrossAlign::Center,
    )
    .unwrap();
    println!("  flex_row(3 chips + button, gap=8, Center) → Union tree constructed");
    let _ = row;

    println!("\n=== done ===");
}
