//! Reference frames of a moving word: pixels worked out by hand, the rules of the raster, and
//! the golden frame hashes every platform must reproduce
//!
//! The golden hashes are recorded constants: every leg of the CI matrix (Linux `x86_64`,
//! Windows, macOS aarch64, debug and release) compares against the same values, so a platform
//! that draws different bits goes red.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use alice_lol::motion_frame::{
    frame_hash, frame_semantics_fingerprints, measured_frame_semantics_id, probe_word,
    render_frame, Frame, FrameError, Glyph, Word, FRAME_PROBE_LAW, FRAME_SEMANTICS_ID,
    FRAME_SEMANTICS_PINS, PX_PER_EM,
};
use alice_lol::motion_law::{MotionLaw, MOTION_SEMANTICS_ID};
use alice_sdf::sdf2d::Sdf2dNode;

fn hex(b: &[u8]) -> String {
    use std::fmt::Write as _;
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}

fn still(channels: &str) -> MotionLaw {
    MotionLaw::from_file(&format!("law still\nkind motion\nperiod 1000\n{channels}")).unwrap()
}

/// A circle of radius 63/256 em whose centre is level with row 64's pixel centres, so the
/// pixel 63/256 em to its right has its centre exactly on the edge
fn disc(alpha_lum: &str) -> (Word, MotionLaw) {
    let word = Word {
        glyphs: vec![Glyph {
            shape: Sdf2dNode::circle(63.0 / 256.0),
            origin: [0.5, 1.0 - 64.5 / 128.0],
        }],
        width_em: 1.0,
        height_em: 1.0,
    };
    (word, still(alpha_lum))
}

fn at(f: &Frame, x: u32, y: u32) -> u8 {
    f.pixels[(y * f.width + x) as usize]
}

#[test]
fn pixels_are_coverage_times_alpha_and_luminance_stored_halves_up() {
    let (word, law) = disc("");
    let f = render_frame(&word, &law.bind(&[]).unwrap(), 0).unwrap();
    assert_eq!((f.width, f.height, f.pixels.len()), (128, 128, 128 * 128));
    // the centre is inside, far corners outside
    assert_eq!(at(&f, 64, 64), 255);
    assert_eq!(at(&f, 0, 0), 0);
    // pixel 95's centre is 95.5/128 - 0.5 = 63/256 em right of the centre: on the edge, so
    // coverage 0.5, stored floor(0.5 * 255 + 0.5) = 128
    assert_eq!(at(&f, 95, 64), 128);
    // the next pixel out is 1/128 em outside: coverage 0.5 - 1 < 0
    assert_eq!(at(&f, 96, 64), 0);

    let (word, law) = disc("channel alpha = 0.5\nchannel luminance = 0.9\n");
    let f = render_frame(&word, &law.bind(&[]).unwrap(), 0).unwrap();
    // inside: 1 * 0.5 * 0.9 = 0.45, stored floor(0.45 * 255 + 0.5) = floor(115.25) = 115
    assert_eq!(at(&f, 64, 64), 115);
    // on the edge: 0.5 * 0.45 = 0.225, floor(57.875) = 57
    assert_eq!(at(&f, 95, 64), 57);
}

#[test]
fn channels_move_scale_and_dilate_the_glyph() {
    // an offset of one pixel moves the edge by one pixel
    let (word, law) = disc(&format!(
        "channel offset_x = {}\n",
        1.0 / f64::from(PX_PER_EM)
    ));
    let f = render_frame(&word, &law.bind(&[]).unwrap(), 0).unwrap();
    assert_eq!((at(&f, 96, 64), at(&f, 97, 64)), (128, 0));
    // dilating by one pixel grows the radius by one pixel
    let (word, law) = disc(&format!(
        "channel dilate = {}\n",
        1.0 / f64::from(PX_PER_EM)
    ));
    let f = render_frame(&word, &law.bind(&[]).unwrap(), 0).unwrap();
    assert_eq!((at(&f, 96, 64), at(&f, 97, 64)), (128, 0));
    // scale 2 doubles the radius about the glyph's origin to 126/256 em: pixel 126's centre is
    // 125/256 em out (half a pixel inside, coverage 1), pixel 127's is 127/256 em out (half a
    // pixel outside, coverage 0)
    let (word, law) = disc("channel scale = 2\n");
    let f = render_frame(&word, &law.bind(&[]).unwrap(), 0).unwrap();
    assert_eq!((at(&f, 126, 64), at(&f, 127, 64)), (255, 0));
    assert_eq!(at(&f, 95, 64), 255);
}

#[test]
fn the_order_of_the_glyphs_does_not_change_a_still_frame() {
    let law = still("");
    let b = law.bind(&[]).unwrap();
    let mut word = probe_word();
    let a = render_frame(&word, &b, 0).unwrap();
    word.glyphs.reverse();
    assert_eq!(render_frame(&word, &b, 0).unwrap(), a);
}

#[test]
fn frames_that_cannot_be_drawn_say_why() {
    let law = still("");
    let b = law.bind(&[]).unwrap();
    let mut word = probe_word();
    for w in [0.0, -1.0, f32::NAN, 33.0] {
        word.width_em = w;
        assert_eq!(
            render_frame(&word, &b, 0).unwrap_err(),
            FrameError::Size,
            "{w}"
        );
    }
    let empty = Word {
        glyphs: vec![],
        width_em: 1.0,
        height_em: 1.0,
    };
    assert_eq!(
        render_frame(&empty, &b, 0).unwrap_err(),
        FrameError::GlyphCount
    );
    let zero = still("channel scale = phase\n");
    assert!(matches!(
        render_frame(&probe_word(), &zero.bind(&[]).unwrap(), 0).unwrap_err(),
        FrameError::Motion(_)
    ));
}

/// The heartbeat law on the probe word at four instants of a beat: the hashes every platform
/// must reproduce
#[test]
fn the_heartbeat_frames_are_the_recorded_ones() {
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../laws/motion/heartbeat.law"),
    )
    .unwrap();
    let law = MotionLaw::from_file(&text).unwrap();
    let b = law.bind(&[("bpm", 60.0), ("depth", 0.1)]).unwrap();
    let word = probe_word();
    let got: Vec<String> = [0, 100, 300, 600]
        .iter()
        .map(|&t| hex(&frame_hash(&render_frame(&word, &b, t).unwrap())))
        .collect();
    // printed so a run on another target shows the values it computed (scripts/frame_targets.sh
    // runs this test with --nocapture and checks the line)
    println!("heartbeat frame hashes: {}", got.join(" "));
    assert_eq!(got, GOLDEN_HEARTBEAT, "{got:#?}");
    // the swell is visible: the frames at the two swells differ from the rest frame, and the
    // rest frames at the start and between beats are the same
    assert_ne!(got[1], got[3]);
    assert_ne!(got[2], got[3]);
    assert_eq!(got[0], got[3]);
}

const GOLDEN_HEARTBEAT: [&str; 4] = [
    "a268690d57a112b2afa51b806cee19424dbca56e43bd10dcdc904ac2453f1997",
    "dd902017fd91c96dd8c2dc2d646e53e400a51e2220b6950f7dad54389a15165e",
    "77335a837d425a77077f0c531c7f5d88c489a82341f998091b32b057a8148415",
    "a268690d57a112b2afa51b806cee19424dbca56e43bd10dcdc904ac2453f1997",
];

#[test]
fn the_frame_semantics_are_their_own_and_measured() {
    assert_ne!(FRAME_SEMANTICS_ID, MOTION_SEMANTICS_ID);
    let measured = frame_semantics_fingerprints();
    assert_eq!(measured.len(), FRAME_SEMANTICS_PINS.len());
    for ((name, pinned), (m_name, m)) in FRAME_SEMANTICS_PINS.iter().zip(&measured) {
        assert_eq!(name, m_name);
        assert_eq!(
            hex(m),
            hex(pinned),
            "pin `{name}` is not the measured behaviour"
        );
    }
    assert_eq!(
        alice_lol::law_id::fold_semantics_pins(FRAME_SEMANTICS_PINS),
        FRAME_SEMANTICS_ID
    );
    assert_eq!(measured_frame_semantics_id(), FRAME_SEMANTICS_ID);
    assert!(MotionLaw::from_file(FRAME_PROBE_LAW).is_ok());
}
