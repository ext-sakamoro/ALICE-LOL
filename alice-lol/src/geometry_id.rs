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
//!
//! # What the identifier does not attest: routing
//!
//! The identifier moves if and only if a probed result's bits move. It does
//! not attest *how* a result was computed: replacing an `alice_det_math` call
//! with the platform's libm leaves every pin unchanged wherever the two agree
//! on the probed inputs. Measured on macOS (aarch64 and `x86_64`), reverting
//! one routed call at a time to the platform function: `atan2` in the hook
//! profile moved the `stdlib` pin, while a `sin_cos` in the hook centreline, the
//! dovetail taper's `sin_cos` and the countersink's `tan` left it unchanged (the
//! platform result equals `alice_det_math` there; another libm may not).
//!
//! Routing is therefore enforced by the gates, not by this identifier:
//! `clippy.toml`'s `disallowed-methods` rejects a platform transcendental call
//! anywhere in this crate, and the oracles behind these pins run on Linux
//! `x86_64`, Windows and macOS aarch64 in both debug and release, so a result
//! that depends on the platform shows up as a digest mismatch between them.

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

/// The signed-zero and exact-cancellation lattice for Euler angles: every
/// `(x, y, z)` drawn from `{+0, -0, ±pi/6, ±pi/4, ±pi/2, ±pi}` (10^3 = 1,000
/// triples).
///
/// The dense grid above samples the interior, but the arithmetic choices that
/// matter most for a persisted rotation sit on this lattice: an input of `-0`
/// or a pair of angles whose products cancel exactly makes a quaternion
/// component an exact zero, and *which* zero (`+0` or `-0`) depends on the
/// arithmetic form (an fma with negated operands, a negated fma result, or an
/// unfused combine each give a different sign there). Hashing this lattice is
/// what makes the pin move for such a change; the grid alone does not reliably
/// hit a cancellation.
fn signed_zero_euler_lattice() -> impl Iterator<Item = (f32, f32, f32)> {
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, FRAC_PI_6, PI};
    const L: [f32; 10] = [
        0.0, -0.0, FRAC_PI_6, -FRAC_PI_6, FRAC_PI_4, -FRAC_PI_4, FRAC_PI_2, -FRAC_PI_2, PI, -PI,
    ];
    L.into_iter().flat_map(|x| {
        L.into_iter()
            .flat_map(move |y| L.into_iter().map(move |z| (x, y, z)))
    })
}

/// The Euler probe digest for any quaternion builder with
/// [`quat_from_euler_xyz`]'s signature; the pin passes the real function and
/// the unit tests pass alternative arithmetic to prove the pin moves for it.
fn euler_xyz_quat_probe_digest_of(f: fn(f32, f32, f32) -> glam::Quat) -> [u8; 32] {
    let mut h = Sha256::new();
    let mut put = |q: glam::Quat| {
        h.update(q.x.to_bits().to_le_bytes());
        h.update(q.y.to_bits().to_le_bytes());
        h.update(q.z.to_bits().to_le_bytes());
        h.update(q.w.to_bits().to_le_bytes());
    };
    for &((x, y, z), _) in EULER_XYZ_PROBE_TABLE {
        put(f(x, y, z));
    }
    for (x, y, z) in dense_euler_grid(15) {
        put(f(x, y, z));
    }
    for (x, y, z) in signed_zero_euler_lattice() {
        put(f(x, y, z));
    }
    h.finalize().into()
}

/// `quat_from_euler_xyz` の固定 probe set (`angle::EULER_XYZ_PROBE_TABLE` の
/// 特別な角度 8 件 + `[-pi, pi]^3` の 15^3 = 3,375 件の密な格子 + 符号付き 0 と
/// 打ち消しの格子 1,000 件) に対する出力を hash した digest — **振る舞いから計算する**
///
/// 格子を足す理由は [`dense_euler_grid`] と [`signed_zero_euler_lattice`] の doc
/// 参照 (8 件の特別な角度だけでは捕まらない算術のみの変更が実測で見つかった、
/// 打ち消しで 0 になる成分の符号は算術の形で変わる)
#[must_use]
pub fn euler_xyz_quat_probe_digest() -> [u8; 32] {
    euler_xyz_quat_probe_digest_of(quat_from_euler_xyz)
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

/// The `stdlib` constructs whose geometry is built with transcendental
/// functions (the hook centrelines, the countersink cone, the dovetail taper,
/// the phone dock's tilt): each routes `sin`/`cos`/`tan`/`atan2` through
/// `alice_det_math` (the platform libm is not bit-identical across targets or
/// even between `-C opt-level=0` and `>=1`, since release builds constant-fold
/// it with the build machine's libm).
const STDLIB_TRANSCENDENTAL_PROBE_LOL: &[&str] = &[
    "skadis_hook_s()",
    "skadis_hook_j()",
    "countersink(4, 6)",
    "dovetail(10, 6, 8)",
    "phone_dock(80, 100, 6)",
];

/// `stdlib` の超越関数の経路の pin: [`STDLIB_TRANSCENDENTAL_PROBE_LOL`] の各構成を
/// `[-40, 40]^3` の 17^3 点の格子で評価した距離の bit を hash した digest
///
/// 超越関数を platform libm に戻す・別の実装に替える等で形状の bit が変われば
/// この digest が動く (`alice-det-math` の pin は算術そのものの世代を、こちらは
/// 「stdlib がその算術を通っているか」を畳む)
///
/// # Panics
///
/// probe の LOL text が parse できない時 (probe set の誤り、build の度に検出される)
#[must_use]
pub fn stdlib_transcendental_probe_digest() -> [u8; 32] {
    #[allow(clippy::cast_precision_loss)] // 17 steps, far below f32's exact-integer range
    let coord = |i: usize| -> f32 { 80.0_f32.mul_add(i as f32 / 16.0, -40.0) };
    let mut h = Sha256::new();
    for lol in STDLIB_TRANSCENDENTAL_PROBE_LOL {
        let node = crate::runtime_parser::parse_lol(lol)
            .unwrap_or_else(|e| panic!("stdlib probe {lol:?} does not parse: {e:?}"));
        length_prefixed(&mut h, lol.as_bytes());
        for i in 0..17 {
            for j in 0..17 {
                for k in 0..17 {
                    let p = glam::Vec3::new(coord(i), coord(j), coord(k));
                    h.update(alice_sdf::eval(&node, p).to_bits().to_le_bytes());
                }
            }
        }
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
        hex32("c9a026178c53c29c8ccdb5413fd1d66ce51e0703ea7ec4e3c82d86ef505030de"),
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
        "stdlib: transcendental routing through alice_det_math",
        hex32("e161067d3da77c1ad3449f03a5108f8124e9a352513ad922e99fda7429f50d44"),
    ),
    (
        "emit: fmt_f32 round-trip fidelity class",
        hex32("4744995b02057f35d13e18e149c8f1d9880cf0b3f4ebab3515f5529d99a70553"),
    ),
];

/// [`LOL_GEOMETRY_SEMANTICS_PINS`] の fold
/// ⚠️ 定数と fold の一致は `tests/geometry_id_oracle.rs` が検査する
pub const LOL_GEOMETRY_SEMANTICS_ID: [u8; 32] =
    hex32("251e73bf84e00c4c1916184bdcbbb75e5cf3f74b62fd4adcbc3922dd18df907d");

#[cfg(test)]
mod tests {
    //! The Euler pin must move for every arithmetic change that alters a
    //! persisted bit: these alternatives each differ from
    //! [`quat_from_euler_xyz`] in exactly one way, and the probe digest has to
    //! see each of them.

    use super::{euler_xyz_quat_probe_digest, euler_xyz_quat_probe_digest_of};
    use std::hint::black_box;

    /// The shared part of every alternative below (the same half-angle
    /// products [`crate::angle::quat_from_euler_xyz`] forms).
    fn parts(x: f32, y: f32, z: f32) -> (f32, f32, f32, f32, f32, f32) {
        let (si, ci) = alice_det_math::sin_cos(x * 0.5);
        let (sj, cj) = alice_det_math::sin_cos((-y) * 0.5);
        let (sh, ch) = alice_det_math::sin_cos(z * 0.5);
        (cj, sj, ci * ch, ci * sh, si * ch, si * sh)
    }

    /// `y` as the negated result of the fma (the shape LLVM rewrites only at
    /// `opt-level >= 1`); `black_box` keeps the literal negation, so the
    /// exact-cancellation zero is `-0` at every optimisation level.
    fn outer_negated_fma(x: f32, y: f32, z: f32) -> glam::Quat {
        let (cj, sj, cc, cs, sc, ss) = parts(x, y, z);
        glam::Quat::from_xyzw(
            cj.mul_add(sc, -(sj * cs)),
            -black_box(cj.mul_add(ss, sj * cc)),
            cj.mul_add(cs, -(sj * sc)),
            cj.mul_add(cc, sj * ss),
        )
    }

    /// `y` unfused (two roundings, and `-0` at cancellation).
    #[allow(clippy::suboptimal_flops)] // the unfused combine is the alternative under test
    fn unfused_y(x: f32, y: f32, z: f32) -> glam::Quat {
        let (cj, sj, cc, cs, sc, ss) = parts(x, y, z);
        glam::Quat::from_xyzw(
            cj.mul_add(sc, -(sj * cs)),
            -(cj * ss + sj * cc),
            cj.mul_add(cs, -(sj * sc)),
            cj.mul_add(cc, sj * ss),
        )
    }

    /// `x` unfused (the measured case: 22.4% of random triples change, none of
    /// the 8 curated rows).
    #[allow(clippy::suboptimal_flops)] // the unfused combine is the alternative under test
    fn unfused_x(x: f32, y: f32, z: f32) -> glam::Quat {
        let (cj, sj, cc, cs, sc, ss) = parts(x, y, z);
        glam::Quat::from_xyzw(
            cj * sc - sj * cs,
            (-cj).mul_add(ss, -(sj * cc)),
            cj.mul_add(cs, -(sj * sc)),
            cj.mul_add(cc, sj * ss),
        )
    }

    /// The same Law form but with the platform's `sin_cos` in place of
    /// `alice_det_math` (the routing the identifier has to see).
    #[allow(clippy::disallowed_methods)] // the platform libm is the alternative under test
    fn platform_trig(x: f32, y: f32, z: f32) -> glam::Quat {
        let (si, ci) = black_box(x * 0.5).sin_cos();
        let (sj, cj) = black_box((-y) * 0.5).sin_cos();
        let (sh, ch) = black_box(z * 0.5).sin_cos();
        let (cc, cs, sc, ss) = (ci * ch, ci * sh, si * ch, si * sh);
        glam::Quat::from_xyzw(
            cj.mul_add(sc, -(sj * cs)),
            (-cj).mul_add(ss, -(sj * cc)),
            cj.mul_add(cs, -(sj * sc)),
            cj.mul_add(cc, sj * ss),
        )
    }

    #[test]
    fn the_euler_pin_is_the_digest_of_the_real_builder() {
        // guards the refactor: the parameterised digest with the real function
        // is the pin's digest (otherwise the alternatives below prove nothing)
        assert_eq!(
            euler_xyz_quat_probe_digest_of(crate::angle::quat_from_euler_xyz),
            euler_xyz_quat_probe_digest()
        );
    }

    #[test]
    fn the_euler_pin_moves_for_each_arithmetic_change() {
        let real = euler_xyz_quat_probe_digest();
        for (name, alt) in [
            (
                "outer-negated fma (signed zero)",
                outer_negated_fma as fn(f32, f32, f32) -> glam::Quat,
            ),
            ("unfused y (signed zero + double rounding)", unfused_y),
            ("unfused x (fused -> unfused)", unfused_x),
            ("platform trig instead of alice_det_math", platform_trig),
        ] {
            assert_ne!(
                euler_xyz_quat_probe_digest_of(alt),
                real,
                "{name}: the Euler pin did not move"
            );
        }
    }

    #[test]
    fn the_signed_zero_lattice_alone_sees_the_zero_sign() {
        // the lattice is what carries the sign: with only the table and the
        // grid an outer-negated fma must still be distinguishable here
        let lattice_only = |f: fn(f32, f32, f32) -> glam::Quat| -> Vec<u32> {
            super::signed_zero_euler_lattice()
                .map(|(x, y, z)| f(x, y, z).y.to_bits())
                .collect()
        };
        let real = lattice_only(crate::angle::quat_from_euler_xyz);
        let alt = lattice_only(outer_negated_fma);
        let differ = real.iter().zip(&alt).filter(|(a, b)| a != b).count();
        assert!(differ > 0, "no lattice point separates +0 from -0");
        assert!(
            real.iter()
                .zip(&alt)
                .all(|(a, b)| a == b || (a ^ b) == 0x8000_0000),
            "outside the zero sign the two forms should agree on the lattice"
        );
    }
}
