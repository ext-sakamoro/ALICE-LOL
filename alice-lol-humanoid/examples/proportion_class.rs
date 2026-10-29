//! Classify a figure's proportions from its height in head lengths and its eye height over its
//! height (both measured in one space)
//!
//! ```text
//! proportion_class <heads> <eye_ratio>
//! ```
//!
//! Prints the class and the bands it was judged by. Exit status: 0 classified, 2 usage error or
//! ratios that are not ratios.

use std::process::ExitCode;

use alice_lol_humanoid::measure::ProportionClass;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let parsed = match args.as_slice() {
        [h, e] => h.parse::<f64>().ok().zip(e.parse::<f64>().ok()),
        _ => None,
    };
    let Some((heads, eye_ratio)) = parsed else {
        eprintln!("usage: proportion_class <heads> <eye_ratio>");
        return ExitCode::from(2);
    };
    match ProportionClass::of(heads, eye_ratio) {
        Ok(c) => {
            println!(
                "{c:?} (adult: heads >= {} and eye ratio >= {}; youthful: heads <= {} or eye ratio <= {})",
                ProportionClass::ADULT_MIN_HEADS,
                ProportionClass::ADULT_MIN_EYE_RATIO,
                ProportionClass::YOUTHFUL_MAX_HEADS,
                ProportionClass::YOUTHFUL_MAX_EYE_RATIO
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(2)
        }
    }
}
