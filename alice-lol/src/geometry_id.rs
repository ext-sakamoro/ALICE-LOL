//! The geometry evaluation's identifier — `.lol` text → `SdfNode` tree →
//! evaluation result, and the reverse (`emit`'s canonical form).
//!
//! [`crate::law_id::LOL_SEMANTICS_ID`] already pins a semantics identifier,
//! but it is scoped to audit law and research law: it does **not** cover
//! `SdfNode`'s own numeric evaluation (its module doc says so explicitly).
//! The `rotate_q` landing changed that evaluation twice in one series of
//! commits (the `rotate`/`rotate_q` quaternion arithmetic moved from
//! `glam`'s platform-dependent libm to `alice_det_math`, and `stdlib`'s own
//! rotation construction did the same) with no identifier to say so — this
//! module is that identifier.
//!
//! # Scope: what this pins, what it does not
//!
//! Pinned here: the arithmetic a construct's evaluation uses (e.g.
//! `rotate`/`rotate_q`'s quaternion construction), the accept/reject boundary
//! of a construct's inputs (e.g. `rotate_q`'s unit-length tolerance, TPMS's
//! `scale`/`thickness` validity range), and `emit`'s canonical text form
//! (e.g. `fmt_f32`'s round-trip fidelity class).
//!
//! **Not** pinned here (that is [`crate::law_id::LOL_SEMANTICS_ID`]'s job):
//! audit law verdict ordering, input reading, research law expression
//! semantics, audit derivation semantics. The two identifiers are
//! deliberately independent constants — a change to one must never silently
//! move the other, or a report naming "which identifier moved" stops being
//! informative.
//!
//! # Design (mirrors `law_id.rs`)
//!
//! Each pin in [`LOL_GEOMETRY_SEMANTICS_PINS`] is a behaviour digest: a fixed
//! probe set run through the real function, hashed. **Never** a hand-typed
//! constant — `tests/geometry_id_oracle.rs` recomputes every pin from
//! current behaviour and fails if it no longer matches the literal recorded
//! here (the same discipline `law_id_oracle.rs` and `alice-det-math`'s golden
//! tests already use: a probe digest moving without this file being
//! re-recorded in the same commit is exactly the failure mode this guards).
//!
//! ⚠️ A handful of hand-picked probe points is not enough on its own: an
//! arithmetic-only change (e.g. fusing/unfusing a multiply-add) can miss
//! every one of a small curated table while still changing a real fraction
//! of inputs (measured: an unfused combine in [`crate::angle::quat_from_euler_xyz`]
//! changed 22.4% of 2,000,000 random triples but 0 of 8 hand-picked rows).
//! Every arithmetic pin below therefore hashes a **dense, deterministic**
//! probe (a programmatically-generated grid or sweep, not a literal table)
//! in addition to any curated special-case rows reused from an existing
//! cross-architecture oracle.
//!
//! # First version, no history
//!
//! `alice-lol` 0.4.0 is unpublished as of this identifier's introduction, so
//! the three behavioural changes that already landed in this series
//! (`rotate_q` + Euler `alice_det_math` switch, the TPMS `scale`/`thickness`
//! validity range, `fmt_f32`'s full round-trip fidelity) are pre-history:
//! this identifier's first recorded value already reflects all three. From
//! this point on, every behavioural change among the probes below needs a
//! deliberate identifier move in the same commit, with a `CHANGELOG.md` line
//! saying so.

use sha2::{Digest, Sha256};

use crate::angle::{
    canon_angle_deg, quat_from_euler_xyz, quat_from_rotation_x, quat_from_rotation_y,
    quat_from_rotation_z, CANON_ANGLE_DEG_PROBE_VALUES, EULER_XYZ_PROBE_TABLE,
    ROTATION_AXIS_PROBE_TABLE,
};
use crate::emit::{fmt_f32, FMT_F32_FIDELITY_PROBE_VALUES};
use crate::law_id::hex32;
use crate::limits::{
    checked_rotation_quat, checked_tpms_fields, ROTATION_QUAT_BOUNDARY_PROBE_INPUTS,
    TPMS_VALIDITY_PROBE_INPUTS,
};

/// A separate domain tag from [`crate::law_id::fold_semantics_pins`]'s
/// `"lol.semantics"` — this identifier and [`crate::law_id::LOL_SEMANTICS_ID`]
/// must never collide, even if their pin lists happened to coincide in
/// length and order.
const GEOMETRY_DOMAIN_TAG: &[u8] = b"lol.geometry";

fn length_prefixed(h: &mut Sha256, bytes: &[u8]) {
    h.update((bytes.len() as u64).to_le_bytes());
    h.update(bytes);
}

/// [`LOL_GEOMETRY_SEMANTICS_PINS`] から意味論の識別子を作る
/// (`crate::law_id::fold_semantics_pins` と同じ組み立て、domain tag だけ別)
#[must_use]
pub fn fold_geometry_pins(pins: &[(&str, [u8; 32])]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(crate::law_id::LAW_ID_DOMAIN);
    h.update(GEOMETRY_DOMAIN_TAG);
    for (name, id) in pins {
        length_prefixed(&mut h, name.as_bytes());
        h.update(id);
    }
    h.finalize().into()
}

/// A dense, deterministic grid of `(x, y, z)` triples spanning `[-pi, pi]^3`,
/// generated programmatically (not stored as a literal table) so the probe
/// stays auditable and cheap to widen: `n` linearly-spaced steps per axis
/// (including both endpoints), `n^3` triples total.
///
/// Why this exists, not just [`EULER_XYZ_PROBE_TABLE`]'s 8 rows: that table
/// pins exact bits for a curated set of interesting angles (the cross-arch
/// oracle's job), but a handful of hand-picked rows can miss an
/// arithmetic-only change that doesn't happen to matter at those specific
/// points. Measured case: replacing [`quat_from_euler_xyz`]'s fused
/// `cj.mul_add(sc, -(sj * cs))` combine with the unfused `cj * sc - sj * cs`
/// changes 22.4% of 2,000,000 random triples, but 0 of the 8 probe-table
/// rows -- a dense grid (unlike 8 hand-picked points) is very unlikely to
/// land entirely on the no-difference set by chance.
fn dense_euler_grid(n: usize) -> impl Iterator<Item = (f32, f32, f32)> {
    debug_assert!(n >= 2, "need at least 2 steps to include both endpoints");
    // `n` is a small, caller-chosen step count (this module calls it with 15)
    // -- nowhere near f32's 2^24 exact-integer limit, so the cast is lossless
    // in practice even though clippy can't prove that from the type alone.
    #[allow(clippy::cast_precision_loss)]
    let step = move |i: usize| -> f32 {
        let t = i as f32 / (n - 1) as f32; // 0.0..=1.0
        (2.0 * std::f32::consts::PI).mul_add(t, -std::f32::consts::PI)
    };
    (0..n).flat_map(move |i| {
        (0..n).flat_map(move |j| (0..n).map(move |k| (step(i), step(j), step(k))))
    })
}

/// `quat_from_euler_xyz` の固定 probe set (`angle::EULER_XYZ_PROBE_TABLE` の
/// 特別な角度 8 件 + `[-pi, pi]^3` の 15^3 = 3,375 件の密な格子) に対する
/// 出力を hash した digest — **振る舞いから計算する**
///
/// 格子を足す理由は [`dense_euler_grid`] の doc 参照 (8 件の特別な角度だけでは
/// 捕まらない算術のみの変更が実測で見つかった)
#[must_use]
pub fn euler_xyz_quat_probe_digest() -> [u8; 32] {
    let mut h = Sha256::new();
    for &((x, y, z), _) in EULER_XYZ_PROBE_TABLE {
        let q = quat_from_euler_xyz(x, y, z);
        h.update(q.x.to_bits().to_le_bytes());
        h.update(q.y.to_bits().to_le_bytes());
        h.update(q.z.to_bits().to_le_bytes());
        h.update(q.w.to_bits().to_le_bytes());
    }
    for (x, y, z) in dense_euler_grid(15) {
        let q = quat_from_euler_xyz(x, y, z);
        h.update(q.x.to_bits().to_le_bytes());
        h.update(q.y.to_bits().to_le_bytes());
        h.update(q.z.to_bits().to_le_bytes());
        h.update(q.w.to_bits().to_le_bytes());
    }
    h.finalize().into()
}

/// A dense, deterministic sweep of degree values spanning `[-1080.0, 1080.0]`
/// (+/- 3 full turns, so it crosses the `(-180, 180]` wrap boundary several
/// times over), `n` linearly-spaced steps including both endpoints -- the
/// same "narrow hand-picked probe can miss an arithmetic-only change" risk
/// [`dense_euler_grid`] documents applies here too, so this pin does not
/// rely on [`CANON_ANGLE_DEG_PROBE_VALUES`] alone.
#[allow(clippy::cast_precision_loss)] // n is a small, caller-chosen step count
fn dense_canon_angle_deg_sweep(n: usize) -> impl Iterator<Item = f32> {
    debug_assert!(n >= 2, "need at least 2 steps to include both endpoints");
    (0..n).map(move |i| {
        let t = i as f32 / (n - 1) as f32; // 0.0..=1.0
        2160.0_f32.mul_add(t, -1080.0)
    })
}

/// IEEE 754 leaves a NaN result's sign bit unspecified for an operation that
/// produces NaN from a non-NaN input (here, `%` applied to +/-infinity) --
/// measured to differ between an arm64 and an `x86_64` build of this exact pin
/// (`+-infinity % 360.0` comes back as a positive-sign NaN on one, a
/// negative-sign NaN on the other, every finite input's bits agreeing).
/// `canon_angle_deg` already treats any NaN alike (neither `if` branch
/// fires), so the pin should too: collapse every NaN's bits to one
/// representative before hashing, the way `to_bits` would if `f32` only
/// ever produced a single canonical NaN.
const fn canonical_bits(x: f32) -> u32 {
    if x.is_nan() {
        0x7fc0_0000
    } else {
        x.to_bits()
    }
}

/// `canon_angle_deg` の pin.
///
/// 代表値 (`angle::CANON_ANGLE_DEG_PROBE_VALUES`、特殊値・巨大値・subnormal・
/// 非有限) + `[-1080, 1080]` 度の密な sweep ([`dense_canon_angle_deg_sweep`]、
/// `(-180, 180]` の折返しを複数回跨ぐ) の出力 bit を hash した digest —
/// 度数の正規化範囲や巨大値の reduce の仕方が変われば、`rotate`/`rotate_q`
/// に渡る前の入力自体が変わる NaN は [`canonical_bits`] で符号を畳んでから
/// hash する (非有限入力が作る NaN の符号は IEEE 754 が規定しない)
#[must_use]
pub fn canon_angle_deg_probe_digest() -> [u8; 32] {
    let mut h = Sha256::new();
    for &deg in CANON_ANGLE_DEG_PROBE_VALUES {
        h.update(canonical_bits(canon_angle_deg(deg)).to_le_bytes());
    }
    for deg in dense_canon_angle_deg_sweep(2161) {
        h.update(canonical_bits(canon_angle_deg(deg)).to_le_bytes());
    }
    h.finalize().into()
}

/// `quat_from_rotation_x`/`_y`/`_z` の固定 probe set
/// (`angle::ROTATION_AXIS_PROBE_TABLE`) に対する出力を hash した digest
#[must_use]
pub fn rotation_axis_quat_probe_digest() -> [u8; 32] {
    let mut h = Sha256::new();
    for &(a, ..) in ROTATION_AXIS_PROBE_TABLE {
        for q in [
            quat_from_rotation_x(a),
            quat_from_rotation_y(a),
            quat_from_rotation_z(a),
        ] {
            h.update(q.x.to_bits().to_le_bytes());
            h.update(q.y.to_bits().to_le_bytes());
            h.update(q.z.to_bits().to_le_bytes());
            h.update(q.w.to_bits().to_le_bytes());
        }
    }
    h.finalize().into()
}

/// `checked_rotation_quat` の受理/拒否境界の固定 probe set
/// (`limits::ROTATION_QUAT_BOUNDARY_PROBE_INPUTS`) に対する判定を hash した
/// digest — accept/reject の別と、accept した値そのものを両方 hash する
/// (正規化していないことも、この digest が変われば分かる)
#[must_use]
pub fn rotation_quat_boundary_probe_digest() -> [u8; 32] {
    let mut h = Sha256::new();
    for &(x, y, z, w) in ROTATION_QUAT_BOUNDARY_PROBE_INPUTS {
        match checked_rotation_quat(x, y, z, w) {
            Ok(q) => {
                h.update([1u8]);
                h.update(q.x.to_bits().to_le_bytes());
                h.update(q.y.to_bits().to_le_bytes());
                h.update(q.z.to_bits().to_le_bytes());
                h.update(q.w.to_bits().to_le_bytes());
            }
            Err(_) => h.update([0u8]),
        }
    }
    h.finalize().into()
}

/// `checked_tpms_fields` の受理/拒否境界の固定 probe set
/// (`limits::TPMS_VALIDITY_PROBE_INPUTS`) に対する判定を hash した digest
#[must_use]
pub fn tpms_validity_probe_digest() -> [u8; 32] {
    let mut h = Sha256::new();
    for &(scale, thickness) in TPMS_VALIDITY_PROBE_INPUTS {
        match checked_tpms_fields(scale, thickness) {
            Ok((s, t)) => {
                h.update([1u8]);
                h.update(s.to_bits().to_le_bytes());
                h.update(t.to_bits().to_le_bytes());
            }
            Err(_) => h.update([0u8]),
        }
    }
    h.finalize().into()
}

/// `fmt_f32` の固定 probe set (`emit::FMT_F32_FIDELITY_PROBE_VALUES`) が書く
/// text を hash した digest — round-trip 忠実度の分類が変わればこの digest
/// も変わる (丸め方針の変更、指数表記の閾値の変更、符号付き零の扱いの変更 等)
#[must_use]
pub fn fmt_f32_fidelity_probe_digest() -> [u8; 32] {
    let mut h = Sha256::new();
    for &v in FMT_F32_FIDELITY_PROBE_VALUES {
        length_prefixed(&mut h, fmt_f32(v).as_bytes());
    }
    h.finalize().into()
}

/// 幾何評価の算術・受理境界・canonical form の pin
///
/// ⚠️ 値は振る舞いから計算する (各 probe digest 関数の現在の出力)
/// `tests/geometry_id_oracle.rs` が各 entry を再計算して突合する
pub const LOL_GEOMETRY_SEMANTICS_PINS: &[(&str, [u8; 32])] = &[
    // 上流の算術世代 これが変われば本 crate の幾何評価の識別子も変わる
    // (`law_id::LOL_SEMANTICS_PINS` の最初の entry と同じ理由、同じ形)
    ("alice-det-math", alice_det_math::SEMANTICS_ID),
    (
        "rotate: angle canonicalisation",
        hex32("ecff9d3055efcb57e6cac095b88ed1207310f6f36d42f4945ae07862a792b9a1"),
    ),
    (
        "rotate: euler-to-quat arithmetic",
        hex32("502373bfd649717d10189fd7ad9ffeb60b19080acacc59b7ffe763459995f080"),
    ),
    (
        "rotate: single-axis quaternion arithmetic",
        hex32("9100865661ac31560c6ded8464a4f5d906ce3cbb972730fc836e574f0ca55d14"),
    ),
    (
        "rotation quat: unit-length acceptance boundary",
        hex32("42bb92427d0f152ecffaab3f3e2e2eb56aac18bca3d7604c3ccbf227197fda41"),
    ),
    (
        "tpms: scale/thickness validity range",
        hex32("cfb95b75cfc0d47ed745c31e963d37bb6ec5d605d5584ea0b636413f72e008c1"),
    ),
    (
        "emit: fmt_f32 round-trip fidelity class",
        hex32("4744995b02057f35d13e18e149c8f1d9880cf0b3f4ebab3515f5529d99a70553"),
    ),
];

/// [`LOL_GEOMETRY_SEMANTICS_PINS`] の fold
/// ⚠️ 定数と fold の一致は `tests/geometry_id_oracle.rs` が検査する
pub const LOL_GEOMETRY_SEMANTICS_ID: [u8; 32] =
    hex32("88aa2a3d9e46266033d32c3278fd4e0c829ccbc8d6a1c31f22c8ccc30b14c04c");
