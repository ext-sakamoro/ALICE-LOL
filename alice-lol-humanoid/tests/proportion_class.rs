//! The proportion class: each boundary of the two bands, approached by one ulp from either side,
//! the gap between the bands, a figure whose two ratios disagree, and ratios that are no ratios
use alice_lol_humanoid::measure::{MeasureError, ProportionClass as C};

const ADULT_EYES: f64 = 0.95;
const ADULT_HEADS: f64 = 7.5;

#[test]
fn the_adult_band_is_closed_at_its_bounds() {
    assert_eq!(C::of(6.5, 0.92), Ok(C::Adult), "both on the adult bounds");
    assert_eq!(C::of(6.5, ADULT_EYES), Ok(C::Adult));
    assert_eq!(
        C::of(f64::next_down(6.5), ADULT_EYES),
        Ok(C::Indeterminate),
        "one ulp under the adult heads"
    );
    assert_eq!(C::of(ADULT_HEADS, 0.92), Ok(C::Adult));
    assert_eq!(
        C::of(ADULT_HEADS, f64::next_down(0.92)),
        Ok(C::Indeterminate),
        "one ulp under the adult eye ratio"
    );
}

#[test]
fn the_youthful_band_is_closed_at_its_bounds() {
    assert_eq!(
        C::of(5.0, ADULT_EYES),
        Ok(C::Youthful),
        "on the youthful heads bound"
    );
    assert_eq!(
        C::of(f64::next_up(5.0), ADULT_EYES),
        Ok(C::Indeterminate),
        "one ulp over it"
    );
    assert_eq!(
        C::of(ADULT_HEADS, 0.89),
        Ok(C::Youthful),
        "on the youthful eye-ratio bound"
    );
    assert_eq!(
        C::of(ADULT_HEADS, f64::next_up(0.89)),
        Ok(C::Indeterminate),
        "one ulp over it, under the adult band"
    );
}

#[test]
fn the_gap_and_disagreeing_ratios() {
    assert_eq!(C::of(6.0, 0.905), Ok(C::Indeterminate), "both in the gap");
    assert_eq!(
        C::of(ADULT_HEADS, 0.905),
        Ok(C::Indeterminate),
        "adult heads, eye ratio in the gap"
    );
    assert_eq!(
        C::of(6.0, ADULT_EYES),
        Ok(C::Indeterminate),
        "heads in the gap, adult eye ratio"
    );
    assert_eq!(
        C::of(ADULT_HEADS, 0.86),
        Ok(C::Youthful),
        "a youthful eye ratio decides, whatever the heads"
    );
    assert_eq!(
        C::of(4.5, ADULT_EYES),
        Ok(C::Youthful),
        "youthful heads decide, whatever the eye ratio"
    );
}

#[test]
fn ratios_that_are_no_ratios_are_refused() {
    for (h, e) in [
        (f64::NAN, 0.9),
        (7.0, f64::NAN),
        (f64::INFINITY, 0.9),
        (0.0, 0.9),
        (-7.0, 0.9),
        (7.0, 0.0),
        (7.0, -0.5),
        (7.0, 1.2),
    ] {
        assert_eq!(
            C::of(h, e),
            Err(MeasureError::BadUnit),
            "heads {h}, eye ratio {e}"
        );
    }
    assert_eq!(
        C::of(7.0, 1.0),
        Ok(C::Adult),
        "eyes at the top is the limit, not beyond it"
    );
}
