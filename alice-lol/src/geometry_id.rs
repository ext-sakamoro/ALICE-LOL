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
use crate::law_id::{hex32, DET_MATH_PINS};
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
/// `[-40, 40]^3` の 17^3 点の格子で `alice_sdf::eval` を呼んで評価した距離の bit を
/// hash した digest
///
/// 超越関数を platform libm に戻す・別の実装に替える等で形状の bit が変われば
/// この digest が動く (`alice-det-math` の pin は算術そのものの世代を、こちらは
/// 「stdlib がその算術を通っているか」を畳む) ⚠️ 超越関数の経路以外にも、
/// `alice_sdf::eval` 自体の評価アルゴリズムが変わる (距離計算の近似精度・
/// primitive の合成方法等) 変更でもこの digest は動く — ここで挙がっている
/// probe 5 件 (`skadis_hook_s`/`_j`、`countersink`、`dovetail`、`phone_dock`) は
/// いずれも `alice_sdf::eval` を直接呼んで bit を取るため、alice-sdf 側の
/// 挙動変化がこの digest の drift として現れることが前提の設計 (alice-sdf の
/// 版を上げた直後にこの test が red になったら、まずそちら側の変更を疑う)
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

/// `stdlib` の template を呼ぶ LOL の文 (parser が `stdlib` に渡す呼び出しの全部)
///
/// 1 組目は名前 1 つにつき 1 つ (引数は parse corpus の値か文書の例)、2 組目は引数のある template に
/// 1 つずつ (1 組目の数値を 10 倍、断られたら 4 倍 1 組目と違う木になる組を選んだ) 1 組の引数では、
/// 別の分岐 (厚い板・現実的な個数・上限で丸められる値) だけを変える変更が識別子に出ない
///
/// 一覧と parser の対応は `tests/stdlib_template_probe_coverage.rs` が `runtime_parser.rs` の
/// 呼び出しの分岐を数えて突き合わせる (probe の無い分岐・分岐の無い probe・比較 0 件は失敗)
pub const STDLIB_TEMPLATE_PROBE_LOL: &[&str] = &[
    "rounded_box(40, 20, 10, 2)",
    "shopping_cart_coin(22.8, 1.7)",
    "skadis_panel(300, 5, 5)",
    "skadis_hook_l()",
    "skadis_hook_j()",
    "skadis_hook_s()",
    "skadis_container()",
    "skadis_clip()",
    "skadis_shelf()",
    "skadis_elastic_cord()",
    "mug(3.0, 2.0)",
    "gridfinity_bin(3.0, 2.0, 1.0)",
    "gridfinity_bin_ex(3.0, 2.0, 1.0, 4.0, 2.0, 3.0, 2.0)",
    "wall_hook()",
    "drawer_organizer()",
    "shelf_divider()",
    "sticky_note_holder(3.0, 2.0, 1.0)",
    "business_card_holder(3.0, 2.0, 1.0)",
    "pen_cup(3.0, 2.0)",
    "phone_stand(3.0, 2.0, 1.0)",
    "headphone_holder(3.0, 2.0, 1.0)",
    "under_desk_mount(3.0, 2.0, 1.0)",
    "desk_shelf(3.0, 2.0, 1.0)",
    "monitor_riser(3.0, 2.0, 1.0)",
    "coaster(3.0, 2.0)",
    "tissue_box_cover(3.0, 2.0, 1.0)",
    "storage_box(3.0, 2.0, 1.0)",
    "cable_clip(3.0, 2.0)",
    "led_channel(3.0, 2.0)",
    "card_tray(3.0, 2.0, 1.0)",
    "token_well(3.0, 2.0, 1.0)",
    "wrench_holder(3.0, 2.0, 1.0)",
    "socket_rail(3.0, 2.0, 1.0)",
    "hex_bit_holder(3.0, 2.0, 1.0)",
    "sbc_case(3.0, 2.0, 1.0)",
    "esp32_enclosure(51.6, 28.4, 15)",
    "battery_18650_holder(3.0, 2.0, 1.0)",
    "toothbrush_holder(3.0, 2.0, 1.0)",
    "drill_bit_holder(3.0, 2.0, 1.0)",
    "pliers_rack(3.0, 2.0, 1.0)",
    "spice_rack(3.0, 2.0, 1.0)",
    "egg_tray(3.0, 2.0, 1.0)",
    "utensil_caddy(3.0, 2.0, 1.0)",
    "filament_spool_holder(3.0, 2.0, 1.0)",
    "nozzle_holder(3.0, 2.0, 1.0)",
    "build_plate_rack(3.0, 2.0, 1.0)",
    "cutlery_tray(3.0, 2.0, 1.0)",
    "pill_organizer(3.0, 2.0, 1.0)",
    "magnetic_strip(3.0, 2.0, 1.0)",
    "hairdryer_holder(3.0, 2.0, 1.0)",
    "kcup_holder(3.0, 2.0, 1.0)",
    "hex_key_holder(3.0, 2.0, 1.0)",
    "wrap_holder(3.0, 2.0, 1.0)",
    "sock_divider(3.0, 2.0, 1.0)",
    "soap_tray(3.0, 2.0, 1.0)",
    "razor_holder(3.0, 2.0, 1.0)",
    "chopstick_holder(3.0, 2.0, 1.0)",
    "swatch_holder(3.0, 2.0, 1.0)",
    "tp_holder(3.0, 2.0, 1.0)",
    "sd_card_holder(3.0, 2.0, 1.0)",
    "driver_rack(3.0, 2.0, 1.0)",
    "cotton_dispenser(3.0, 2.0, 1.0)",
    "sink_caddy(3.0, 2.0, 1.0)",
    "clamp_rack(3.0, 2.0, 1.0)",
    "dry_box(3.0, 2.0, 1.0)",
    "outdoor_enclosure(3.0, 2.0, 1.0)",
    "jewelry_stand(3.0, 2.0, 1.0)",
    "phone_dock(3.0, 2.0, 1.0)",
    "cutting_board_rack(3.0, 2.0, 1.0)",
    "tape_dispenser(3.0, 2.0, 1.0)",
    "shower_caddy(3.0, 2.0, 1.0)",
    "caliper_holder(3.0, 2.0, 1.0)",
    "bag_clip_org(3.0, 2.0, 1.0)",
    "can_rack(3.0, 2.0, 1.0)",
    "led_hub_box(3.0, 2.0, 1.0)",
    "makeup_organizer(3.0, 2.0, 1.0)",
    "vesa_mount(3.0, 2.0, 1.0)",
    "l_bracket(3.0, 2.0, 1.0, 4.0, 2.0)",
    "t_slot_bracket_2020(3.0, 2.0)",
    "sbc_mount_plate(3.0, 2.0)",
    "heat_set_array(3.0, 2.0, 1.0, 4.0, 2.0)",
    "flange_mount(3.0, 2.0, 1.0)",
    "dovetail_pair(3.0, 2.0, 1.0, 4.0)",
    "profile_extrusion(3.0, 2.0)",
    "snap_fit_pair(3.0, 2.0, 1.0, 4.0)",
    "boss_array(3.0, 2.0, 1.0, 4.0, 2.0, 3.0)",
    "screw_hole(3.0, 2.0)",
    "tap_hole(3.0, 2.0)",
    "counterbore(3.0, 2.0)",
    "countersink(3.0, 2.0)",
    "heat_set_hole(3.0)",
    "bolt(3.0, 2.0)",
    "bracket_l(60, 40, 4, 40, 3)",
    "flange_circular(3.0, 2.0, 1.0, 4.0, 2.0, 3.0)",
    "t_slot_2020(3.0)",
    "profile_2020(3.0)",
    "profile_3030(3.0)",
    "dovetail(3.0, 2.0, 1.0)",
    "slot(3.0, 2.0, 1.0)",
    "snap_fit_annular(3.0, 2.0, 1.0, 4.0)",
    "pin_hinge_knuckle(3.0, 2.0, 1.0)",
    "boss(3.0, 2.0)",
    "rib(3.0, 2.0, 1.0)",
    "bearing_seat(3.0, 2.0, 1.0)",
    "rack_shelf(3.0, 2.0, 1.0, 4.0, 2.0, 3.0)",
    "cable_grommet(3.0, 2.0)",
    "curtain_rod_bracket(3.0, 2.0)",
    "dowel_hole(3.0, 2.0)",
    "wood_screw_pilot(3.0, 2.0)",
    "arduino_mount_plate(3.0, 2.0)",
    "pixhawk_mount(3.0, 2.0)",
    "servo_mount(3.0)",
    "jst_ph_slot(3.0)",
    // 2 組目: 引数のある template は 1 組目の数値を 10 倍 (断られたら 4 倍) した文 (現実の寸法に近い分岐と
    // 個数を通す)、servo_mount は別の種類 (2 = MG996R、1 組目の 3 は SG90 に丸められる)
    "rounded_box(400, 200, 100, 20)",
    "shopping_cart_coin(228, 17)",
    "skadis_panel(1200, 20, 20)",
    "mug(30, 20)",
    "gridfinity_bin(30, 20, 10)",
    "gridfinity_bin_ex(30, 20, 10, 40, 20, 30, 20)",
    "sticky_note_holder(30, 20, 10)",
    "business_card_holder(30, 20, 10)",
    "pen_cup(30, 20)",
    "phone_stand(30, 20, 10)",
    "headphone_holder(30, 20, 10)",
    "under_desk_mount(30, 20, 10)",
    "desk_shelf(30, 20, 10)",
    "monitor_riser(30, 20, 10)",
    "coaster(30, 20)",
    "tissue_box_cover(30, 20, 10)",
    "storage_box(30, 20, 10)",
    "cable_clip(30, 20)",
    "led_channel(30, 20)",
    "card_tray(30, 20, 10)",
    "token_well(30, 20, 10)",
    "wrench_holder(30, 20, 10)",
    "socket_rail(30, 20, 10)",
    "hex_bit_holder(30, 20, 10)",
    "sbc_case(30, 20, 10)",
    "esp32_enclosure(516, 284, 150)",
    "battery_18650_holder(30, 20, 10)",
    "toothbrush_holder(30, 20, 10)",
    "drill_bit_holder(30, 20, 10)",
    "pliers_rack(30, 20, 10)",
    "spice_rack(30, 20, 10)",
    "egg_tray(30, 20, 10)",
    "utensil_caddy(30, 20, 10)",
    "filament_spool_holder(30, 20, 10)",
    "nozzle_holder(30, 20, 10)",
    "build_plate_rack(30, 20, 10)",
    "cutlery_tray(30, 20, 10)",
    "pill_organizer(30, 20, 10)",
    "magnetic_strip(30, 20, 10)",
    "hairdryer_holder(30, 20, 10)",
    "kcup_holder(30, 20, 10)",
    "hex_key_holder(30, 20, 10)",
    "wrap_holder(30, 20, 10)",
    "sock_divider(30, 20, 10)",
    "soap_tray(30, 20, 10)",
    "razor_holder(30, 20, 10)",
    "chopstick_holder(30, 20, 10)",
    "swatch_holder(30, 20, 10)",
    "tp_holder(30, 20, 10)",
    "sd_card_holder(30, 20, 10)",
    "driver_rack(30, 20, 10)",
    "cotton_dispenser(30, 20, 10)",
    "sink_caddy(30, 20, 10)",
    "clamp_rack(12, 8, 4)",
    "dry_box(30, 20, 10)",
    "outdoor_enclosure(30, 20, 10)",
    "jewelry_stand(30, 20, 10)",
    "phone_dock(30, 20, 10)",
    "cutting_board_rack(30, 20, 10)",
    "tape_dispenser(30, 20, 10)",
    "shower_caddy(30, 20, 10)",
    "caliper_holder(30, 20, 10)",
    "bag_clip_org(30, 20, 10)",
    "can_rack(30, 20, 10)",
    "led_hub_box(30, 20, 10)",
    "makeup_organizer(30, 20, 10)",
    "vesa_mount(30, 20, 10)",
    "l_bracket(30, 20, 10, 40, 20)",
    "t_slot_bracket_2020(30, 20)",
    "sbc_mount_plate(30, 20)",
    "heat_set_array(30, 20, 10, 40, 20)",
    "flange_mount(30, 20, 10)",
    "dovetail_pair(30, 20, 10, 40)",
    "profile_extrusion(30, 20)",
    "snap_fit_pair(30, 20, 10, 40)",
    "boss_array(30, 20, 10, 40, 20, 30)",
    "screw_hole(30, 20)",
    "tap_hole(30, 20)",
    "counterbore(30, 20)",
    "countersink(30, 20)",
    "heat_set_hole(30)",
    "bolt(30, 20)",
    "bracket_l(600, 400, 40, 400, 30)",
    "flange_circular(30, 20, 10, 40, 20, 30)",
    "t_slot_2020(30)",
    "profile_2020(30)",
    "profile_3030(30)",
    "dovetail(30, 20, 10)",
    "slot(30, 20, 10)",
    "snap_fit_annular(30, 20, 10, 40)",
    "pin_hinge_knuckle(30, 20, 10)",
    "boss(30, 20)",
    "rib(30, 20, 10)",
    "bearing_seat(30, 20, 10)",
    "rack_shelf(30, 20, 10, 40, 20, 30)",
    "cable_grommet(30, 20)",
    "curtain_rod_bracket(30, 20)",
    "dowel_hole(30, 20)",
    "wood_screw_pilot(30, 20)",
    "arduino_mount_plate(30, 20)",
    "pixhawk_mount(30, 20)",
    "servo_mount(2.0)",
    "jst_ph_slot(30)",
];

/// `stdlib` の template が作る木の pin: [`STDLIB_TEMPLATE_PROBE_LOL`] の各文を parse し、
/// 返る `SdfNode` を正規形の文 ([`crate::emit::to_lol_unstamped`]) に書いて hash した digest
///
/// 依存するもの: template の木の構造と数値 (既定の引数・寸法・穴の数と位置・CSG の組み方)
/// と、正規形の書き方 (`fmt_f32` は別の pin が固定する) template の中で超越関数を呼ぶ物は、その
/// 値も木の数値に入る ⚠️ `alice_sdf::eval` は呼ばないので、alice-sdf の距離の評価や
/// alice-det-math の関数が変わってもこの digest は動かない (それらは alice-det-math の関数の pin
/// と [`stdlib_transcendental_probe_digest`] が畳む) 動くのは template の作る形が変わった時と、
/// `SdfNode` の型 (variant と field) が変わり正規形の文が変わった時
///
/// # Errors
///
/// [`TemplateProbeError`]: probe の文が parse できない、または正規形に書けない (probe の一覧の誤り
/// `tests/geometry_id_oracle.rs` が pin と突き合わせるので build の度に検出される)
pub fn stdlib_template_probe_digest() -> Result<[u8; 32], TemplateProbeError> {
    let mut h = Sha256::new();
    for &lol in STDLIB_TEMPLATE_PROBE_LOL {
        let node = crate::runtime_parser::parse_lol(lol)
            .map_err(|_| TemplateProbeError::DoesNotParse(lol))?;
        let text = crate::emit::to_lol_unstamped(&node)
            .map_err(|_| TemplateProbeError::NoCanonicalText(lol))?;
        length_prefixed(&mut h, lol.as_bytes());
        length_prefixed(&mut h, text.as_bytes());
    }
    Ok(h.finalize().into())
}

/// [`stdlib_template_probe_digest`] が digest を作れなかった probe
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateProbeError {
    /// probe の文が parse できない
    DoesNotParse(&'static str),
    /// parse した木を正規形の文に書けない
    NoCanonicalText(&'static str),
}

impl std::fmt::Display for TemplateProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DoesNotParse(lol) => write!(f, "stdlib template probe {lol:?} does not parse"),
            Self::NoCanonicalText(lol) => {
                write!(f, "stdlib template probe {lol:?} has no canonical text")
            }
        }
    }
}

impl std::error::Error for TemplateProbeError {}

/// 幾何評価の算術・受理境界・canonical form の pin
///
/// ⚠️ 値は振る舞いから計算する (各 probe digest 関数の現在の出力)
/// `tests/geometry_id_oracle.rs` が各 entry を再計算して突合する
pub const LOL_GEOMETRY_SEMANTICS_PINS: &[(&str, [u8; 32])] = &[
    // この crate が呼ぶ alice-det-math の関数 (`law_id::DET_MATH_PINS`、同じ順、
    // `law_id::LOL_SEMANTICS_PINS` と同じ行) 呼ぶ関数の振る舞いが変われば幾何評価の識別子も変わる
    ("alice-det-math::atan2", DET_MATH_PINS[0].1),
    ("alice-det-math::atan2_64", DET_MATH_PINS[1].1),
    ("alice-det-math::cos64", DET_MATH_PINS[2].1),
    ("alice-det-math::exp64", DET_MATH_PINS[3].1),
    ("alice-det-math::ln64", DET_MATH_PINS[4].1),
    ("alice-det-math::powf64", DET_MATH_PINS[5].1),
    ("alice-det-math::powi", DET_MATH_PINS[6].1),
    ("alice-det-math::sin64", DET_MATH_PINS[7].1),
    ("alice-det-math::sin_cos", DET_MATH_PINS[8].1),
    ("alice-det-math::sqrt64", DET_MATH_PINS[9].1),
    ("alice-det-math::tan", DET_MATH_PINS[10].1),
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
        hex32("35eb5b0f75a33dd33dcfe81a559c57286f3ec12bc2c5fc70e02a1ec0d9819c05"),
    ),
    (
        "stdlib: template trees (canonical text)",
        hex32("0286cc07db92bd5da0f934557722f47d6ce316d39e0f17924afcff053e5d83cc"),
    ),
    (
        "emit: fmt_f32 round-trip fidelity class",
        hex32("4744995b02057f35d13e18e149c8f1d9880cf0b3f4ebab3515f5529d99a70553"),
    ),
    // alice-sdf の形の意味 (`alice_sdf::SDF_SEMANTICS_ID`): LOL の幾何は alice-sdf の木として評価される
    // ので、alice-sdf の形の意味が変わる (例: repeat_finite の奇数個の数え方) と同じ LOL の文が別の形に
    // なる 値は literal で持ち、試験が解決した alice-sdf の値と突き合わせる (上げた時に必ず red になる)
    (
        "alice-sdf::SDF_SEMANTICS_ID",
        hex32("443238e4a561285522719dad6a7cec11e988f39406108593c375cd5762cb133d"),
    ),
];

/// [`LOL_GEOMETRY_SEMANTICS_PINS`] の fold
/// ⚠️ 定数と fold の一致は `tests/geometry_id_oracle.rs` と `tests/semantics_id_from_function_pins.rs` が検査する
pub const LOL_GEOMETRY_SEMANTICS_ID: [u8; 32] =
    hex32("9b82c3c330accd463b34ae863c109e9adc5bbd65fc0893e78570687bd072d3fe");

/// 以前の世代の [`LOL_GEOMETRY_SEMANTICS_ID`] のうち、今と同じ bit を出すと確かめたもの
///
/// [`crate::law_id::PREVIOUS_LOL_SEMANTICS_IDS`] と同じ規則・同じ理由で literal にする
/// alice-det-math の識別子全体を畳んでいた世代 (0.5.0 / 0.5.1 / 0.5.2 at 2d01c63f の `251e73bf…` /
/// `d1cbba69…` / `8ee0c1a3…`) は入れない: 版をまたぐ oracle が無い 戻すのはその試験を置いた時だけ
pub const PREVIOUS_LOL_GEOMETRY_SEMANTICS_IDS: &[[u8; 32]] = &[];

/// 保存された幾何の識別子を分類する ([`crate::law_id::check_lol_semantics`] と同じ形)
pub fn check_lol_geometry_semantics(id: &[u8; 32]) -> alice_det_math::SemanticsCheck {
    crate::law_id::classify(
        id,
        &LOL_GEOMETRY_SEMANTICS_ID,
        PREVIOUS_LOL_GEOMETRY_SEMANTICS_IDS,
    )
}

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
    #[allow(
        clippy::disallowed_methods,
        reason = "test-probe-alternative: the platform libm is the alternative under test, not production code"
    )]
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
