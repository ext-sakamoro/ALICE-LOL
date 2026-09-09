//! Card × 6 の grid layout demo
//!
//! 実行:
//! ```bash
//! cargo run --example ui_card_grid -p alice-lol-ui
//! ```

use alice_lol_ui::{layout, Card, Theme, UiLaw};

fn main() {
    println!("=== alice-lol-ui: Card × 6 grid demo ===\n");

    let theme = Theme::light();
    let law = UiLaw::default_wcag_aa();

    // Card 300×200 × 6 個
    let card_w = 300.0;
    let card_h = 200.0;
    let card_r = theme.radius(2); // golden ratio step 2 ≈ 10.5 px
    let shadow = 4.0;

    let cards: Vec<_> = (0..6)
        .map(|_| {
            (
                Card::new(card_w, card_h, card_r, shadow).build(),
                card_w,
                card_h,
            )
        })
        .collect();

    // 2 列 × 3 行 grid、gap = spacing(2)
    let gap = theme.spacing(2);
    println!(
        "Card: {card_w}×{card_h}, radius={card_r:.2} px (golden step 2), shadow={shadow} px, gap={gap:.2} px"
    );

    let grid = layout::grid(&cards, 2, gap).unwrap();
    println!("grid(2 cols, 6 cards) → Union tree built");
    let _ = grid;

    // law check
    for v in &law.check_card(card_w, card_h, card_r) {
        println!("law violation: [{}] {}", v.rule, v.detail);
    }
    println!(
        "card law check: {}",
        if law.check_card(card_w, card_h, card_r).is_empty() {
            "PASS"
        } else {
            "FAIL"
        }
    );

    // theme contrast
    for v in &law.check_theme_contrast(&theme, false) {
        println!("theme contrast violation: [{}] {}", v.rule, v.detail);
    }
    let fg_bg = alice_lol_ui::theme::contrast_ratio(theme.fg, theme.bg);
    println!("theme contrast: fg/bg = {fg_bg:.2}:1 (WCAG AA min 4.5)");

    println!("\n=== done ===");
}
