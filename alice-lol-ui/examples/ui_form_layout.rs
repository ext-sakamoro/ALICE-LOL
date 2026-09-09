//! Label (`Divider`) + `InputField` × 3 + `Button` の form layout demo
//!
//! 実行:
//! ```bash
//! cargo run --example ui_form_layout -p alice-lol-ui
//! ```

use alice_lol_ui::{layout, Button, Divider, InputField, Theme, UiLaw};

fn main() {
    println!("=== alice-lol-ui: form layout demo ===\n");

    let theme = Theme::dark();
    let law = UiLaw::default_wcag_aa();

    let form_w = 320.0;
    let field_h = 44.0;
    let btn_h = 44.0;
    let radius = theme.radius(1); // ≈ 6.5 px

    // Divider (label 相当)
    let divider1 = (Divider::horizontal(form_w, 1.0), form_w, 1.0);
    let divider2 = (Divider::horizontal(form_w, 1.0), form_w, 1.0);
    let divider3 = (Divider::horizontal(form_w, 1.0), form_w, 1.0);

    // InputField × 3
    let field1 = (
        InputField::new(form_w, field_h, radius).build(),
        form_w,
        field_h,
    );
    let field2 = (
        InputField::new(form_w, field_h, radius).build(),
        form_w,
        field_h,
    );
    let field3 = (
        InputField::new(form_w, field_h, radius).build(),
        form_w,
        field_h,
    );

    // Submit Button (accent 色想定)
    let btn = (Button::new(form_w, btn_h, radius).build(), form_w, btn_h);

    // flex_col で縦積み、gap = spacing(2) ≈ 10.5 px
    let gap = theme.spacing(2);
    let form = layout::flex_col(
        &[divider1, field1, divider2, field2, divider3, field3, btn],
        gap,
        layout::CrossAlign::Center,
    )
    .unwrap();
    println!("form: 7 children (3 dividers + 3 inputs + 1 button), gap={gap:.2} px");
    let _ = form;

    // 各要素 law check
    println!("\n--- law check ---");
    for (label, violations) in [
        ("InputField", law.check_input_field(form_w, field_h, radius)),
        ("Button", law.check_button(form_w, btn_h, radius)),
    ] {
        if violations.is_empty() {
            println!("  {label:<12}  → PASS");
        } else {
            for v in &violations {
                println!("  {label:<12}  → FAIL [{}] {}", v.rule, v.detail);
            }
        }
    }

    // theme accent contrast (button 表面テキスト想定 = large text 相当で AA)
    let violations = law.check_accent_contrast(&theme, true);
    if violations.is_empty() {
        println!("  dark theme accent → PASS (large text AA)");
    } else {
        for v in &violations {
            println!("  dark theme accent → FAIL [{}] {}", v.rule, v.detail);
        }
    }

    println!("\n=== done ===");
}
