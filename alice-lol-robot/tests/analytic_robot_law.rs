//! `SafetyLaw` の閉形式突合 oracle (ISO 10218 参考の静的 safety 検査)
//!
//! 閉形式との突合 既存の unit test 23 本は
//! 「違反が 1 件出ること」「rule 名が一致すること」までしか見ておらず、
//! **判定器が危険側に緩んでも green のまま**通る経路がある
//!
//! 被覆の実測 (grep、2026-09-30): 既存 23 test が構築する verb は
//! `walk` 8 / `latent_intent` 8 / `grasp` 3 / `sequence` 2 / `rotate` `push` `point` `gaze` 各 1 で、
//! **`throw` と `parallel` は 0 回** (18 verb のうち 10 verb を 1 度も構築しない)
//! workspace で破られる面も `x>max` `y>max` `y<min` `z<min` の 4 面だけで、
//! **`x<min` と `z>max` は 1 度も破られない**
//! ⇒ 「x 下限の節を落とす」「z 上限の節を落とす」「`Throw` を workspace 腕から外す」
//! 「`Parallel` を無検査にする」の 4 つの危険側変異は、既存 23 test では
//! **その入力を誰も作らないので原理的に捕まえられない** (変異注入の完走測定は未了)
//!
//! 本 file は判定を閉形式 / 代数的不変量と突合する:
//!
//! | 分類 | 突合先 |
//! |---|---|
//! | workspace | 閉じた AABB の membership (`cmpge` / `cmple` mask 形、実装の 6 節連鎖とは独立な表現) |
//! | 閾値の厳格さ | 閾値ちょうどは pass / 1 ulp 上は fail (比較を両方向から固定) |
//! | 角度則 | `abs` 契約 = 角度の偶関数であること |
//! | latent norm | ピタゴラス数 (3, 4, 5) と一様ベクトルの厳密値 + 絶対斉次性 |
//! | 単調性 | 入力について単調非減少 / 閾値について単調非増加 |
//! | 合成 | `Sequence` / `Parallel` は子の違反列の連結 (構造帰納の homomorphism) |
//! | 散文の検証 | module doc が名指しした verb / rule が実際に到達可能か |
//!
//! # `gap_*` test について (直したら red になる pin)
//!
//! 末尾の `gap_*` 5 本は **現在の判定器が危険側に緩い箇所**を明示的に pin している
//! 仕様判断 (緩さを許容するか閉じるか) が未了なので実装は変更していない
//! 緩さを閉じると `gap_*` が red になる = 意図した警報であり、その時は
//! 該当 test を「危険側を fail にする」向きに書き換える
//!
//! 浮動小数の境界主張は全て 2 進小数 (1.5 / 150.0 / 3.0 / 2.0 / 3-4-5) に限定して
//! 丸め誤差を排除している 非 2 進小数 (0.3 等) は境界 oracle に使わない

use alice_lol::intent::{
    gaze, grasp, latent_intent, parallel, point, pull, push, rotate, sequence, throw, walk,
    HandSide, IntentNode, ProgramBuilder,
};
use alice_lol::SdfNode;
use alice_lol_robot::law::{SafetyLaw, SafetyViolation};
use alice_lol_robot::RobotError;
use glam::Vec3;

const OVERSPEED: &str = "Overspeed";
const OUT_OF_WORKSPACE: &str = "OutOfWorkspace";
const OVERFORCE: &str = "Overforce";
const NON_FINITE: &str = "NonFiniteLatent";
const NORM_EXCEEDED: &str = "LatentNormExceeded";

const fn law() -> SafetyLaw {
    SafetyLaw::default_collab()
}

fn has_rule(violations: &[SafetyViolation], rule: &str) -> bool {
    violations.iter().any(|v| v.rule == rule)
}

/// 閉じた AABB の membership を実装とは独立な mask 形で書いた参照実装
///
/// 実装 (`check_workspace`) は 6 節の or 連鎖 こちらは per-component 比較 mask の全件 and
fn inside_closed_box(p: Vec3, lo: Vec3, hi: Vec3) -> bool {
    p.cmpge(lo).all() && p.cmple(hi).all()
}

/// ラベル付きの verb 構築子 (`Vec3` target を 1 つ取る verb)
type TargetVerb = (&'static str, fn(Vec3) -> IntentNode);
/// ラベル付きの verb 構築子 (スカラーの力を 1 つ取る verb)
type ForceVerb = (&'static str, fn(f32) -> IntentNode);

fn verbs_with_target() -> Vec<TargetVerb> {
    vec![
        ("Walk::destination", |p| walk(p, 0.5)),
        ("Point::target", |p| point(p, HandSide::Right)),
        ("Throw::target", |p| throw(p, 1.0, HandSide::Right)),
        ("Gaze::target", |p| gaze(p, 100)),
    ]
}

// ══════════════════════════════════════════════════════════════════════════
// A. workspace — 閉じた AABB の membership と全件突合
// ══════════════════════════════════════════════════════════════════════════

/// 各軸 7 点 (外・境界・内・中心・内・境界・外) の格子 343 点を、
/// `Vec3` target を持つ 4 verb すべてで参照実装と突合する
///
/// oracle: 軸ごとに内側 5 点 / 外側 2 点なので内側は厳密に 125 点
/// (1 節でも落とすと内側が 150 点等になり件数が合わなくなる)
#[test]
fn workspace_membership_matches_closed_box_on_the_whole_lattice() {
    let l = law();
    let (lo, hi) = (l.workspace_min, l.workspace_max);
    let axis = |a: f32, b: f32| [a - 1.0, a, a + 0.5, 0.0, b - 0.5, b, b + 1.0];
    let xs = axis(lo.x, hi.x);
    let ys = axis(lo.y, hi.y);
    let zs = axis(lo.z, hi.z);

    for (label, make) in verbs_with_target() {
        let mut inside = 0_u32;
        let mut outside = 0_u32;
        for &x in &xs {
            for &y in &ys {
                for &z in &zs {
                    let p = Vec3::new(x, y, z);
                    let expect_inside = inside_closed_box(p, lo, hi);
                    let flagged = has_rule(&l.check_intent(&make(p)), OUT_OF_WORKSPACE);
                    assert_eq!(
                        expect_inside,
                        !flagged,
                        "{label}: target {p:?} — 参照は inside={expect_inside} だが flagged={flagged}"
                    );
                    if expect_inside {
                        inside += 1;
                    } else {
                        outside += 1;
                    }
                }
            }
        }
        // 空振り検出: 内外の件数は格子の構成から閉形式で決まる
        assert_eq!(inside, 125, "{label}: 内側点の数が 5^3 にならない");
        assert_eq!(outside, 218, "{label}: 外側点の数が 7^3 - 5^3 にならない");
    }
}

/// 6 面それぞれが独立に guard されているか (他 5 面は内部のまま 1 面だけ破る)
#[test]
fn each_of_the_six_faces_is_guarded_independently() {
    let l = law();
    let (lo, hi) = (l.workspace_min, l.workspace_max);
    let c = Vec3::new(0.0, 1.0, 0.0); // 内部の基準点
    assert!(
        inside_closed_box(c, lo, hi),
        "基準点が内部でないと 1 面だけ破る検査が成立しない"
    );
    let cases = [
        ("x 下限", Vec3::new(lo.x - 1.0, c.y, c.z)),
        ("x 上限", Vec3::new(hi.x + 1.0, c.y, c.z)),
        ("y 下限", Vec3::new(c.x, lo.y - 1.0, c.z)),
        ("y 上限", Vec3::new(c.x, hi.y + 1.0, c.z)),
        ("z 下限", Vec3::new(c.x, c.y, lo.z - 1.0)),
        ("z 上限", Vec3::new(c.x, c.y, hi.z + 1.0)),
    ];
    for (face, p) in cases {
        assert!(
            has_rule(&l.check_intent(&walk(p, 0.5)), OUT_OF_WORKSPACE),
            "{face} を破った target {p:?} が OutOfWorkspace にならない"
        );
    }
}

/// 境界ちょうど (8 頂点 + 6 面心) は閉区間なので pass
#[test]
fn boundary_surface_points_are_inside_the_closed_box() {
    let l = law();
    let (lo, hi) = (l.workspace_min, l.workspace_max);
    let mid = (lo + hi) * 0.5;
    let mut pts = Vec::new();
    for &x in &[lo.x, hi.x] {
        for &y in &[lo.y, hi.y] {
            for &z in &[lo.z, hi.z] {
                pts.push(Vec3::new(x, y, z));
            }
        }
    }
    pts.push(Vec3::new(lo.x, mid.y, mid.z));
    pts.push(Vec3::new(hi.x, mid.y, mid.z));
    pts.push(Vec3::new(mid.x, lo.y, mid.z));
    pts.push(Vec3::new(mid.x, hi.y, mid.z));
    pts.push(Vec3::new(mid.x, mid.y, lo.z));
    pts.push(Vec3::new(mid.x, mid.y, hi.z));
    assert_eq!(pts.len(), 14, "頂点 8 + 面心 6 を測っていない");
    for p in pts {
        assert!(
            !has_rule(&l.check_intent(&walk(p, 0.5)), OUT_OF_WORKSPACE),
            "境界上の {p:?} が OutOfWorkspace になった (閉区間でなく開区間になっている)"
        );
    }
}

/// 境界の 1 ulp 外は fail (上の test と対で比較の厳格さを両方向から固定)
#[test]
fn one_ulp_outside_each_face_is_flagged() {
    let l = law();
    let (lo, hi) = (l.workspace_min, l.workspace_max);
    let c = Vec3::new(0.0, 1.0, 0.0);
    let cases = [
        Vec3::new(lo.x.next_down(), c.y, c.z),
        Vec3::new(hi.x.next_up(), c.y, c.z),
        Vec3::new(c.x, lo.y.next_down(), c.z),
        Vec3::new(c.x, hi.y.next_up(), c.z),
        Vec3::new(c.x, c.y, lo.z.next_down()),
        Vec3::new(c.x, c.y, hi.z.next_up()),
    ];
    for p in cases {
        assert!(
            !inside_closed_box(p, lo, hi),
            "参照実装が {p:?} を内側と判定した (test の前提が壊れている)"
        );
        assert!(
            has_rule(&l.check_intent(&walk(p, 0.5)), OUT_OF_WORKSPACE),
            "境界 +1 ulp の {p:?} が OutOfWorkspace にならない"
        );
    }
}

/// doc の「workspace: 3 m cube」は実際には y 下限だけ −1.0 で非対称
///
/// 非対称は安全側 (床下を許さない) なので実装が正 doc の語が緩い側にずれている
#[test]
fn default_workspace_is_asymmetric_in_y_not_a_symmetric_cube() {
    let l = law();
    assert!(
        has_rule(
            &l.check_intent(&walk(Vec3::new(0.0, -2.0, 0.0), 0.5)),
            OUT_OF_WORKSPACE
        ),
        "y = -2.0 (対称 cube なら内側) が外側判定されない = y 下限が -3 に緩んでいる"
    );
    assert!(
        !has_rule(
            &l.check_intent(&walk(Vec3::new(0.0, -1.0, 0.0), 0.5)),
            OUT_OF_WORKSPACE
        ),
        "y = -1.0 (宣言された下限そのもの) が外側判定された"
    );
}

// ══════════════════════════════════════════════════════════════════════════
// B. スカラー閾値 — 1 ulp 厳格性
// ══════════════════════════════════════════════════════════════════════════

/// `default_collab` の定数を doc の宣言値と突合 (単位換算 / scale 誤りの検出)
#[test]
fn default_collab_constants_match_the_documented_values() {
    let l = law();
    assert!(
        (l.max_linear_velocity - 1.5).abs() < f32::EPSILON,
        "max_linear_velocity が 1.5 m/s でない: {}",
        l.max_linear_velocity
    );
    assert!(
        (l.max_angular_step - core::f32::consts::FRAC_PI_2).abs() < f32::EPSILON,
        "max_angular_step が pi/2 でない: {}",
        l.max_angular_step
    );
    assert!(
        (l.max_force - 150.0).abs() < f32::EPSILON,
        "max_force が 150 N でない: {}",
        l.max_force
    );
    assert!(
        (l.max_latent_norm - 1.5).abs() < f32::EPSILON,
        "max_latent_norm が 1.5 でない: {}",
        l.max_latent_norm
    );
    assert_eq!(l.workspace_min, Vec3::new(-3.0, -1.0, -3.0));
    assert_eq!(l.workspace_max, Vec3::new(3.0, 3.0, 3.0));
}

#[test]
fn walk_speed_threshold_is_exact_to_one_ulp() {
    let l = law();
    let t = l.max_linear_velocity;
    let inside = Vec3::new(1.0, 0.0, 0.0);
    assert!(
        !has_rule(&l.check_intent(&walk(inside, t)), OVERSPEED),
        "速度が閾値ちょうど ({t}) で Overspeed になった (比較が緩い側に倒れている)"
    );
    assert!(
        has_rule(&l.check_intent(&walk(inside, t.next_up())), OVERSPEED),
        "速度が閾値 +1 ulp で Overspeed にならない (閾値が上にずれている)"
    );
}

#[test]
fn force_threshold_is_exact_to_one_ulp_for_every_force_verb() {
    let l = law();
    let t = l.max_force;
    let verbs: [ForceVerb; 3] = [
        ("Grasp", |f| grasp(0, HandSide::Right, f)),
        ("Push", |f| push(0, Vec3::X, f)),
        ("Pull", |f| pull(0, Vec3::X, f)),
    ];
    for (label, make) in verbs {
        assert!(
            !has_rule(&l.check_intent(&make(t)), OVERFORCE),
            "{label}: 力が閾値ちょうど ({t} N) で Overforce になった"
        );
        assert!(
            has_rule(&l.check_intent(&make(t.next_up())), OVERFORCE),
            "{label}: 力が閾値 +1 ulp で Overforce にならない"
        );
    }
}

#[test]
fn angular_step_threshold_is_exact_to_one_ulp() {
    let l = law();
    let t = l.max_angular_step;
    assert!(
        !has_rule(&l.check_intent(&rotate(0, Vec3::Y, t)), OVERSPEED),
        "回転角が pi/2 ちょうどで Overspeed になった"
    );
    assert!(
        has_rule(&l.check_intent(&rotate(0, Vec3::Y, t.next_up())), OVERSPEED),
        "回転角が pi/2 +1 ulp で Overspeed にならない"
    );
    assert!(
        has_rule(
            &l.check_intent(&rotate(0, Vec3::Y, core::f32::consts::PI)),
            OVERSPEED
        ),
        "180 度回転が Overspeed にならない"
    );
}

/// 角度則の契約は `abs` = 角度の偶関数 符号を反転しても判定は変わらない
#[test]
fn rotate_rule_is_an_even_function_of_the_angle() {
    let l = law();
    let t = l.max_angular_step;
    let angles = [
        0.0,
        t * 0.5,
        t,
        t.next_up(),
        t * 1.5,
        core::f32::consts::PI,
        core::f32::consts::TAU,
    ];
    let mut flagged = 0_u32;
    let mut clean = 0_u32;
    for a in angles {
        let pos = has_rule(&l.check_intent(&rotate(0, Vec3::Y, a)), OVERSPEED);
        let neg = has_rule(&l.check_intent(&rotate(0, Vec3::Y, -a)), OVERSPEED);
        assert_eq!(
            pos, neg,
            "角度 {a} と その符号反転で判定が違う (abs が落ちて逆回転が素通りする)"
        );
        if pos {
            flagged += 1;
        } else {
            clean += 1;
        }
    }
    // 空振り検出: 両方の結果が実際に出ていること
    assert!(flagged >= 3, "違反側の観測が {flagged} 件しかない");
    assert!(clean >= 3, "合格側の観測が {clean} 件しかない");
}

// ══════════════════════════════════════════════════════════════════════════
// C. 単調性 (閾値比較器の閉形式性質)
// ══════════════════════════════════════════════════════════════════════════

/// 速度を上げて違反が消えてはならない かつ 遷移点は厳密に閾値の直上
#[test]
fn overspeed_is_monotone_in_speed_with_the_crossover_at_the_threshold() {
    let l = law();
    let t = l.max_linear_velocity;
    let speeds = [
        0.0,
        t * 0.5,
        t.next_down(),
        t,
        t.next_up(),
        t * 2.0,
        t * 100.0,
    ];
    let mut seen_violation = false;
    for s in speeds {
        let v = has_rule(
            &l.check_intent(&walk(Vec3::new(1.0, 0.0, 0.0), s)),
            OVERSPEED,
        );
        assert_eq!(v, s > t, "速度 {s} の判定 {v} が閉形式の比較と一致しない");
        assert!(!seen_violation || v, "速度 {s} で違反が消えた (非単調)");
        seen_violation |= v;
    }
    assert!(seen_violation, "違反が 1 件も出ていない (空振り)");
}

#[test]
fn overforce_is_monotone_in_force_with_the_crossover_at_the_threshold() {
    let l = law();
    let t = l.max_force;
    let forces = [0.0, t * 0.5, t.next_down(), t, t.next_up(), t * 10.0];
    let mut seen_violation = false;
    for f in forces {
        let v = has_rule(&l.check_intent(&grasp(0, HandSide::Right, f)), OVERFORCE);
        assert_eq!(v, f > t, "力 {f} N の判定 {v} が閉形式の比較と一致しない");
        assert!(!seen_violation || v, "力 {f} N で違反が消えた (非単調)");
        seen_violation |= v;
    }
    assert!(seen_violation, "違反が 1 件も出ていない (空振り)");
}

/// 法則を厳しくすると違反は増えるだけ (閾値について単調非増加)
#[test]
fn tightening_the_threshold_never_removes_a_violation() {
    let program = ProgramBuilder::new()
        .with_sdf(SdfNode::sphere(0.5))
        .with_intent(sequence(vec![
            walk(Vec3::new(1.0, 0.0, 0.0), 1.0),
            walk(Vec3::new(2.0, 0.0, 0.0), 2.0),
            walk(Vec3::new(0.5, 0.0, 0.0), 4.0),
        ]))
        .build();
    let mut prev = 0;
    // 閾値を下げていく = 法則を厳しくしていく
    for limit in [8.0_f32, 4.0, 3.0, 2.0, 1.5, 0.5, 0.0] {
        let law_at = SafetyLaw {
            max_linear_velocity: limit,
            ..SafetyLaw::default_collab()
        };
        let n = law_at.check_program(&program).len();
        assert!(
            n >= prev,
            "閾値 {limit} で違反数が減った (閾値について非単調)"
        );
        prev = n;
    }
    assert_eq!(prev, 3, "閾値 0 なら 3 verb すべてが違反するはず");
}

// ══════════════════════════════════════════════════════════════════════════
// D. latent L2 norm — 厳密値との突合
// ══════════════════════════════════════════════════════════════════════════

const fn law_with_latent_norm(max: f32) -> SafetyLaw {
    SafetyLaw {
        max_latent_norm: max,
        ..SafetyLaw::default_collab()
    }
}

/// ピタゴラス数 (3, 4, 5) で norm の厳密値を突合
///
/// 先頭 3 要素は decoded target として workspace check に回るので原点に置き、
/// 3 と 4 を後方に配置して norm rule だけを単離する
#[test]
fn latent_norm_matches_the_pythagorean_closed_form() {
    let v = vec![0.0, 0.0, 0.0, 3.0, 4.0]; // norm = 5 (厳密)
    let at = law_with_latent_norm(5.0);
    assert!(
        !has_rule(&at.check_intent(&latent_intent(v.clone())), NORM_EXCEEDED),
        "norm ちょうど 5.0 が閾値 5.0 で違反になった"
    );
    let over = law_with_latent_norm(5.0_f32.next_down());
    assert!(
        has_rule(&over.check_intent(&latent_intent(v)), NORM_EXCEEDED),
        "norm 5.0 が閾値 5.0 の 1 ulp 下で違反にならない (L2 が過小評価されている)"
    );
}

/// 一様ベクトルの norm は成分値と次元の平方根の積 2 進小数で厳密に 2.0 になる 4 組
#[test]
fn latent_norm_matches_the_uniform_vector_closed_form() {
    let l = law_with_latent_norm(2.0);
    // (n, c): c * sqrt(n) = 2 が厳密に成り立つ組
    for (n, c) in [(4_usize, 1.0_f32), (16, 0.5), (64, 0.25), (256, 0.125)] {
        let at = vec![c; n];
        assert!(
            !has_rule(&l.check_intent(&latent_intent(at)), NORM_EXCEEDED),
            "dim {n} / c {c}: norm 2.0 ちょうどが違反になった"
        );
        // 摂動は成分側でなく閾値側に置く
        // 成分を 1 ulp 上げる書き方は誤り: 1 成分の 1 ulp (c = 1.0 なら 2^-23) が
        // 二乗和に与える増分は 和 4.0 の 1 ulp (2^-21) より小さく、累算の丸めで消える
        // (実測: sum([1, 1, 1, next_up(1)]^2) は f32 で厳密に 4.0、sqrt も 2.0)
        // 閾値を 1 ulp 下げる形なら norm が厳密値 2.0 のまま比較だけが動く
        let tight = law_with_latent_norm(2.0_f32.next_down());
        assert!(
            has_rule(
                &tight.check_intent(&latent_intent(vec![c; n])),
                NORM_EXCEEDED
            ),
            "dim {n} / c {c}: 閾値を 2.0 の 1 ulp 下にしても違反にならない"
        );
    }
}

/// L2 norm は絶対斉次 閾値を同じ倍率で動かせば判定は不変
#[test]
fn latent_norm_is_absolutely_homogeneous() {
    let base = [0.0_f32, 0.0, 0.0, 3.0, 4.0]; // norm = 5
    for k in [0.5_f32, 1.0, 2.0, 4.0, -2.0] {
        let scaled: Vec<f32> = base.iter().map(|x| x * k).collect();
        let exact = law_with_latent_norm(5.0 * k.abs());
        assert!(
            !has_rule(
                &exact.check_intent(&latent_intent(scaled.clone())),
                NORM_EXCEEDED
            ),
            "k = {k}: 倍率を掛けた norm ちょうどが違反になった (斉次性が崩れている)"
        );
        let tight = law_with_latent_norm((5.0 * k.abs()).next_down());
        assert!(
            has_rule(&tight.check_intent(&latent_intent(scaled)), NORM_EXCEEDED),
            "k = {k}: 閾値を 1 ulp 下げても違反にならない"
        );
    }
}

/// L2 norm は成分の置換と符号反転で不変 (対称かつ偶) norm rule のみを見る
#[test]
fn latent_norm_is_invariant_under_permutation_and_sign_flip() {
    let l = law_with_latent_norm(2.0);
    let base = vec![0.0_f32, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]; // norm = sqrt(6) > 2
    assert!(
        has_rule(&l.check_intent(&latent_intent(base.clone())), NORM_EXCEEDED),
        "norm sqrt(6) が閾値 2.0 で違反にならない (空振り)"
    );
    for shift in 1..base.len() {
        let mut rotated = base.clone();
        rotated.rotate_left(shift);
        // 符号反転も同時に掛ける (偶関数性)
        let signed: Vec<f32> = rotated
            .iter()
            .enumerate()
            .map(|(i, x)| if i % 2 == 0 { -x } else { *x })
            .collect();
        assert!(
            has_rule(&l.check_intent(&latent_intent(signed)), NORM_EXCEEDED),
            "shift {shift} の置換 + 符号反転で norm 判定が変わった"
        );
    }
}

// ══════════════════════════════════════════════════════════════════════════
// E. 合成の代数 (構造帰納の homomorphism)
// ══════════════════════════════════════════════════════════════════════════

fn mixed_children() -> Vec<IntentNode> {
    vec![
        walk(Vec3::new(1.0, 0.0, 0.0), 5.0),        // Overspeed
        grasp(0, HandSide::Right, 500.0),           // Overforce
        walk(Vec3::new(100.0, 0.0, 0.0), 0.5),      // OutOfWorkspace
        rotate(0, Vec3::Y, -core::f32::consts::PI), // Overspeed (負角)
        walk(Vec3::new(1.0, 0.0, 0.0), 0.5),        // 合格
    ]
}

/// `Sequence` の違反列は子の違反列の連結と厳密一致 (順序も保つ)
#[test]
fn sequence_violations_are_the_concatenation_of_the_children() {
    let l = law();
    let kids = mixed_children();
    let expected: Vec<SafetyViolation> = kids.iter().flat_map(|k| l.check_intent(k)).collect();
    assert_eq!(
        expected.len(),
        4,
        "子の違反総数が想定と違う (前提が壊れている)"
    );
    assert_eq!(
        l.check_intent(&sequence(kids)),
        expected,
        "Sequence が子の違反列の連結になっていない"
    );
}

/// `Parallel` は `Sequence` と同じ違反意味論を持つ (並列だから緩む は無い)
#[test]
fn parallel_has_the_same_violation_semantics_as_sequence() {
    let l = law();
    let kids = mixed_children();
    let seq = l.check_intent(&sequence(kids.clone()));
    let par = l.check_intent(&parallel(kids));
    assert!(!par.is_empty(), "Parallel の子が 1 件も検査されていない");
    assert_eq!(seq, par, "Parallel と Sequence で違反列が違う");
}

/// 入れ子の形を変えても違反列は変わらない (flatten 不変)
#[test]
fn nesting_does_not_change_the_violation_sequence() {
    let l = law();
    let kids = mixed_children();
    let flat = l.check_intent(&sequence(kids.clone()));
    let nested = sequence(vec![
        sequence(vec![kids[0].clone(), kids[1].clone()]),
        parallel(vec![kids[2].clone(), sequence(vec![kids[3].clone()])]),
        kids[4].clone(),
    ]);
    assert!(!flat.is_empty(), "前提の違反列が空 (空振り)");
    assert_eq!(l.check_intent(&nested), flat, "入れ子で違反列が変わった");
}

#[test]
fn empty_composites_report_nothing_but_non_empty_ones_do() {
    let l = law();
    assert!(l.check_intent(&sequence(vec![])).is_empty());
    assert!(l.check_intent(&parallel(vec![])).is_empty());
    assert!(
        !l.check_intent(&sequence(mixed_children())).is_empty(),
        "空でない合成が違反 0 件 = 検査が空振りしている"
    );
}

// ══════════════════════════════════════════════════════════════════════════
// F. module doc が名指しした散文の検証
// ══════════════════════════════════════════════════════════════════════════

/// doc の `OutOfWorkspace` 節が名指しした 4 verb すべてが実際に到達可能か
#[test]
fn every_workspace_verb_named_in_the_module_doc_is_reachable() {
    let l = law();
    let far = Vec3::new(500.0, 500.0, 500.0);
    for (label, make) in verbs_with_target() {
        assert!(
            has_rule(&l.check_intent(&make(far)), OUT_OF_WORKSPACE),
            "doc は {label} を OutOfWorkspace 対象と書いているが到達しない"
        );
    }
}

/// doc が名指しした全 rule 名が到達可能か (rule そのものの死活)
#[test]
fn every_rule_name_in_the_module_doc_is_reachable() {
    let l = law();
    let reach: [(&str, IntentNode); 5] = [
        (OVERSPEED, walk(Vec3::new(1.0, 0.0, 0.0), 500.0)),
        (OUT_OF_WORKSPACE, walk(Vec3::new(500.0, 0.0, 0.0), 0.5)),
        (OVERFORCE, grasp(0, HandSide::Right, 5_000.0)),
        (NON_FINITE, latent_intent(vec![f32::NAN, 0.0, 0.0, 0.0])),
        (NORM_EXCEEDED, latent_intent(vec![0.0, 0.0, 0.0, 9.0])),
    ];
    for (rule, intent) in reach {
        assert!(
            has_rule(&l.check_intent(&intent), rule),
            "rule {rule} が到達不能になっている"
        );
    }
}

/// safety rule なしと doc が宣言した verb は本当に無検査か (scope の pin)
#[test]
fn verbs_declared_out_of_scope_produce_no_violation() {
    use alice_lol::intent::{align, avoid, catch, follow, music_intent, release, rest};
    let l = law();
    let out_of_scope = [
        release(0),
        catch(0),
        align(0, Vec3::X),
        follow(0, -1.0e6),
        avoid(0, -1.0e6),
        rest(0),
        music_intent([0xFF; 8]),
    ];
    for intent in out_of_scope {
        assert!(
            l.check_intent(&intent).is_empty(),
            "MVP scope 外と宣言された verb が違反を出した: {intent:?}"
        );
    }
}

// ══════════════════════════════════════════════════════════════════════════
// G. 危険側の緩みを閉じたことの確認 (2026-10-01)
//
// ⚠️ 以下 4 件は当初 **緩さを pin する** 形で書かれていた (直すと red になる警報)
//    実装を直したので、**危険側の入力が違反になる**向きへ書き換えてある
// ══════════════════════════════════════════════════════════════════════════

/// `Throw::force` も Overforce の対象 (module doc と `max_force` doc の宣言どおり)
///
/// ⚠️ 2026-09-30 時点の実装は `walk_intent` の `Throw` 腕が workspace 節にしか無く
/// **force を `..` で捨てていた** = doc が宣言した検査が存在しない状態だった
#[test]
fn throw_force_is_checked_like_the_other_force_verbs() {
    let l = law();
    let inside = Vec3::new(1.0, 0.0, 0.0);
    let huge = l.max_force * 1_000.0;
    assert!(
        has_rule(
            &l.check_intent(&throw(inside, huge, HandSide::Right)),
            OVERFORCE
        ),
        "Throw::force が検査されていない (doc は Overforce 対象と宣言している)"
    );
    // 他の force verb と同じ扱いであること
    for other in [
        grasp(0, HandSide::Right, huge),
        push(0, Vec3::X, huge),
        pull(0, Vec3::X, huge),
    ] {
        assert!(
            has_rule(&l.check_intent(&other), OVERFORCE),
            "対照の force verb が違反にならない (前提が壊れている)"
        );
    }
    // ⚠️ 到達域の検査は失われていない (force を足したことで target 側が抜けないこと)
    let outside = Vec3::new(100.0, 0.0, 0.0);
    assert!(
        has_rule(
            &l.check_intent(&throw(outside, 1.0, HandSide::Right)),
            OUT_OF_WORKSPACE
        ),
        "Throw::target の到達域検査が失われた"
    );
}

/// 速度 / 力 / 角度は **大きさ**で判定する (負の大きさも違反)
///
/// ⚠️ 符号なし比較だと `-5000` が素通りする 当初は `Rotate` だけが `abs` を
/// 取っており、同 file 内で契約が不統一だった
#[test]
fn negative_magnitudes_are_violations_for_every_scalar_verb() {
    let l = law();
    let inside = Vec3::new(1.0, 0.0, 0.0);
    let cases = [
        ("Walk::speed", walk(inside, -500.0), OVERSPEED),
        (
            "Grasp::force",
            grasp(0, HandSide::Right, -5_000.0),
            OVERFORCE,
        ),
        ("Push::force", push(0, Vec3::X, -5_000.0), OVERFORCE),
        ("Pull::force", pull(0, Vec3::X, -5_000.0), OVERFORCE),
        (
            "Rotate::angle_rad",
            rotate(0, Vec3::Y, -core::f32::consts::PI),
            OVERSPEED,
        ),
    ];
    for (label, intent, rule) in cases {
        assert!(
            has_rule(&l.check_intent(&intent), rule),
            "{label} が負の大きさを違反にしない"
        );
    }
    // ⚠️ 上限以内の負値は違反にしない (符号そのものを禁じたのではない)
    assert!(
        l.check_intent(&walk(inside, -0.5)).is_empty(),
        "上限以内の負の速度まで違反にした (大きさで見る契約を超えている)"
    );
}

/// `NaN` はすべての verb で違反 (判定できない入力は安全側に倒す)
///
/// ⚠️ `NaN` は比較が全部 false になるので、素朴な `>` / `||` 連鎖では
/// **「違反 0 件」が「安全」ではなく「比較が成立しなかった」になる**
/// 当初は `LatentIntent` だけが `is_finite` 検査を持っていた
#[test]
fn nan_scalars_and_coordinates_are_violations_everywhere() {
    let l = law();
    let inside = Vec3::new(1.0, 0.0, 0.0);
    let cases = [
        ("Walk::speed NaN", walk(inside, f32::NAN), OVERSPEED),
        (
            "Walk::destination NaN",
            walk(Vec3::splat(f32::NAN), 0.5),
            OUT_OF_WORKSPACE,
        ),
        (
            "Grasp::force NaN",
            grasp(0, HandSide::Right, f32::NAN),
            OVERFORCE,
        ),
        ("Rotate::angle NaN", rotate(0, Vec3::Y, f32::NAN), OVERSPEED),
        (
            "Throw::force NaN",
            throw(inside, f32::NAN, HandSide::Right),
            OVERFORCE,
        ),
    ];
    for (label, intent, rule) in cases {
        assert!(
            has_rule(&l.check_intent(&intent), rule),
            "{label} が違反にならない (比較が成立していないだけの可能性)"
        );
    }
    // 対照: LatentIntent の finite 検査は従来どおり
    assert!(
        has_rule(
            &l.check_intent(&latent_intent(vec![f32::NAN, 0.0, 0.0, 0.0])),
            NON_FINITE
        ),
        "LatentIntent の finite 検査が失われた"
    );
}

/// `±Inf` は座標でも大きさでも違反
///
/// ⚠️ 当初 `-Inf` の速度は符号なし比較で素通りしていた (負値 gap と同根)
#[test]
fn infinite_speeds_and_coordinates_are_violations_in_both_signs() {
    let l = law();
    let inside = Vec3::new(1.0, 0.0, 0.0);
    for p in [
        Vec3::new(f32::INFINITY, 0.0, 0.0),
        Vec3::new(f32::NEG_INFINITY, 0.0, 0.0),
    ] {
        assert!(
            has_rule(&l.check_intent(&walk(p, 0.5)), OUT_OF_WORKSPACE),
            "無限大の座標 {p:?} が OutOfWorkspace にならない"
        );
    }
    for v in [f32::INFINITY, f32::NEG_INFINITY] {
        assert!(
            has_rule(&l.check_intent(&walk(inside, v)), OVERSPEED),
            "速度 {v} が Overspeed にならない"
        );
        assert!(
            has_rule(&l.check_intent(&grasp(0, HandSide::Right, v)), OVERFORCE),
            "力 {v} が Overforce にならない"
        );
    }
}

/// `guard_program` は `check_program` の違反を 1 件目以外捨てる
#[test]
fn gap_guard_program_reports_only_the_first_of_several_violations() {
    let l = law();
    let program = ProgramBuilder::new()
        .with_sdf(SdfNode::sphere(0.5))
        .with_intent(sequence(mixed_children()))
        .build();
    let all = l.check_program(&program);
    assert_eq!(all.len(), 4, "前提の違反数が想定と違う");
    assert!(all.len() > 1, "違反が 1 件だと捨てている事実を示せない");
    let err = l
        .guard_program(&program)
        .expect_err("違反があるのに Ok が返った");
    let RobotError::SafetyViolated { rule, detail } = err else {
        panic!("SafetyViolated 以外が返った");
    };
    assert_eq!(rule, all[0].rule, "1 件目以外が報告された");
    assert_eq!(detail, all[0].detail);
}
