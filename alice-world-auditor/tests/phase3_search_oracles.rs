//! Phase 3 search oracles (`project_alice_world_model_mvp_plan` §3 Phase 3)
//! — written and pinned **before** the search body exists (Phase 4).
//!
//! Scenes (a)/(b) call the real public entry point [`lower_bound_frames`]
//! and are `#[ignore = "src gap: ..."]` because its body is `todo!()`
//! (oracle-first discipline: pin the expected answer now, implement later,
//! remove `#[ignore]` when the `src gap:` closes). Scene (c) is a pure
//! closed-form demonstration that does **not** call the stub — it is the
//! documented reason dominance pruning by kinetic energy is not planned,
//! and runs (green) today.
//!
//! `ceil()` results here are always non-negative and small (well under
//! `u32::MAX`, these are frame counts in the hundreds), so the `as u32`
//! truncating casts are intentional.
#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use alice_world_auditor::{lower_bound_frames, AxisState, Params};

/// (a) Closed-form minimal step count for 1-D rest-to-rest.
///
/// `t* = 2*sqrt(d/a)`, minimal integer frame count = `ceil(t*/dt)`.
#[test]
#[ignore = "src gap: lower_bound_frames body is todo!() (Phase 4, project_alice_world_model_mvp_plan §3 Phase 4.1)"]
fn a_closed_form_minimal_step_count_for_1d_rest_to_rest() {
    let d = 100.0_f32;
    let a = 5.0_f32;
    let dt = 1.0 / 60.0_f32;

    let t_star = 2.0 * (d / a).sqrt();
    let expected_frames = (t_star / dt).ceil() as u32;

    // 空振り防止: 手計算と閉形式コードの二重計算が一致することを別経路でも確認
    // (d=100, a=5 → t*=2*sqrt(20)=8.94427191 → /the dt(1/60)=536.656... → ceil 537)
    assert_eq!(
        expected_frames, 537,
        "closed form 自体の計算が狂っている (oracle の空振り確認)"
    );

    let state = AxisState {
        distance_to_goal: d,
        velocity: 0.0,
    };
    let params = Params::new(a, dt, 100_000);
    let frames = lower_bound_frames(state, &params);
    assert_eq!(
        frames, expected_frames,
        "lower_bound_frames が 1-D rest-to-rest の最小 step 数と一致しない"
    );
}

/// (b) Admissibility pair (scene B, gravity-assisted, position-only goal).
///
/// `a_input` 単独版は加勢を見落として true cost を**上回る**見積りを返す
/// (= inadmissible、red) `a_input + |g|` 版は true cost と一致する
/// (admissible、green) 両方を 1 test に収めて「対」であることを明示する
#[test]
#[ignore = "src gap: lower_bound_frames body is todo!() (Phase 4, project_alice_world_model_mvp_plan §3 Phase 4.1)"]
fn b_admissibility_pair_scene_gravity_assisted_position_only() {
    let drop_height = 20.0_f32;
    let g = 10.0_f32;
    let a_input = 2.0_f32;
    let dt = 1.0 / 60.0_f32;

    // true minimal time: 位置のみ goal、加速度 (a_input + g) での自由落下相当
    // t = sqrt(2h / a_total)
    let true_time = (2.0 * drop_height / (a_input + g)).sqrt();
    let true_frames = (true_time / dt).ceil() as u32;
    assert_eq!(
        true_frames, 49,
        "closed form 自体の計算が狂っている (oracle の空振り確認)"
    );

    let state = AxisState {
        distance_to_goal: drop_height,
        velocity: 0.0,
    };

    // red 版: 重力加勢を見落とす (a_max_axis = a_input のみ)
    let params_without_gravity = Params::new(a_input, dt, 100_000);
    let h_without_gravity = lower_bound_frames(state, &params_without_gravity);
    assert!(
        h_without_gravity > true_frames,
        "a_input だけの heuristic が true cost を超えない (= この scene では加勢の見落としが \
         観測可能になっていない、scene を選び直す)"
    );

    // green 版: 重力加勢を含める (a_max_axis = a_input + |g|)
    let params_with_gravity = Params::new(a_input + g, dt, 100_000);
    let h_with_gravity = lower_bound_frames(state, &params_with_gravity);
    assert!(
        h_with_gravity <= true_frames,
        "a_input + |g| の heuristic が admissible でない (true cost を超えている)"
    );
}

/// (c) 運動エネルギーでの naive dominance pruning が落ちる反例
///
/// 「2 つの完了済み経路を比べて運動エネルギー (= ピーク速度) の低い方を
/// 優先する」は bang-bang 最適制御では不健全 本 test は **閉形式のみ**で
/// 反例を示す (`lower_bound_frames` / `plan` を呼ばない — これは実装の
/// 正しさでなく力学の事実、dominance pruning を実装しない根拠として pin
/// する)
///
/// 反例の構成: 同じ 1-D rest-to-rest 距離 `d=100` を、2 通りの最大加速度
/// で対称 bang-bang (加速 t*/2 + 減速 t*/2) で走る
/// - 経路 A (`a = 5`、速い): ピーク速度 `v_peak(a) = sqrt(d*a)` が**大きい**が
///   所要時間 `t*(a) = 2*sqrt(d/a)` は**小さい**
/// - 経路 B (`a = 1`、遅い): ピーク速度が A より**小さい**が所要時間は A より
///   **大きい** (`v_peak` は `a` の増加関数、`t*` は `a` の減少関数、同じ
///   bang-bang 閉形式から従う代数的事実)
///
/// ⇒ 「ピーク速度 (運動エネルギー) が低い方を優先する」dominance は、所要
/// 時間で劣る B を A より好ましいと判定してしまう (KE が低い方が時間で
/// 劣る反例、両者とも同じ bang-bang 戦略なので「制御則が違うから」という
/// 反論も成立しない)
#[test]
fn c_naive_kinetic_energy_dominance_is_unsound_for_bang_bang() {
    let d = 100.0_f32;

    let bang_bang_peak_velocity = |a: f32| (d * a).sqrt();
    let bang_bang_time = |a: f32| 2.0 * (d / a).sqrt();

    let a_fast = 5.0_f32;
    let a_slow = 1.0_f32;

    let v_peak_fast = bang_bang_peak_velocity(a_fast);
    let v_peak_slow = bang_bang_peak_velocity(a_slow);
    let time_fast = bang_bang_time(a_fast);
    let time_slow = bang_bang_time(a_slow);

    assert!(
        v_peak_slow < v_peak_fast,
        "反例の前提: 経路 B (slow) のピーク速度が経路 A (fast) 未満でなければならない"
    );
    assert!(
        time_slow > time_fast,
        "反例の前提: 経路 B (slow) の所要時間が経路 A (fast) より長くなければならない \
         (= KE の低い B を優先する dominance が、より優れた A を誤って枝刈りする反例が成立しない)"
    );

    // 空振り防止: a_fast と a_slow を入れ替えると不等式が両方逆転するはず
    // (= 反例が「a が違えば答えも違う」という非自明な性質に依存していることの確認、
    // 恒等式 (常に成立する式) を oracle にしていないことの pin)
    assert!(
        v_peak_fast > v_peak_slow && time_fast < time_slow,
        "a_fast / a_slow を入れ替えた不等式が成立しない — 反例が a の値に依存しない恒等式になっている"
    );
}
