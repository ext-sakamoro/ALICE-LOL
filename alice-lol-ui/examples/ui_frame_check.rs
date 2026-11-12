//! The frame law on one hand-built frame: a phone screen whose tilt knob covers a quarter of the
//! part it turns and whose header takes a quarter of the screen, checked with the strict (48 px)
//! touch target.
//!
//! `cargo run -p alice-lol-ui --example ui_frame_check`

use alice_lol_ui::frame::{FrameItem, FrameLaw, FrameReport, Viewport, REPORT_VERSION};
use alice_lol_ui::UiLaw;

fn item(id: &str, role: &str, rect: [f32; 4], interactive: bool) -> FrameItem {
    FrameItem {
        id: id.into(),
        source: "dom".into(),
        role: role.into(),
        rect,
        hit: interactive.then_some(rect),
        interactive,
        text: None,
        in_overlay: false,
        pinned: false,
        inline_text: false,
    }
}

fn main() {
    let law = FrameLaw::from_ui_law(&UiLaw::strict_wcag_aaa());
    println!(
        "{} checks, touch target {} px phone / {} px desktop, control cover {:.0} %",
        law.checks().len(),
        law.min_touch_target(true),
        law.min_touch_target(false),
        law.max_control_cover() * 100.0
    );
    let report = FrameReport {
        version: REPORT_VERSION,
        viewport: Viewport {
            w: 390.0,
            h: 844.0,
            dpr: 3.0,
            coarse: Some(true),
            scrolls_y: false,
            screen_w: Some(390.0),
        },
        items: vec![
            item("header", "chrome", [0.0, 0.0, 390.0, 200.0], false),
            item("plank", "subject", [100.0, 300.0, 75.0, 6.0], false),
            item("knob", "control", [157.0, 285.0, 36.0, 36.0], true),
            item("play", "control", [20.0, 760.0, 64.0, 48.0], true),
        ],
        samples: vec![],
    };
    let verdict = law.check(&report);
    for (check, n) in &verdict.compared {
        println!("{:<16} compared {n}", check.name());
    }
    for v in &verdict.violations {
        println!("  {v}");
    }
    println!("{}", if verdict.passed() { "PASS" } else { "FAIL" });
}
