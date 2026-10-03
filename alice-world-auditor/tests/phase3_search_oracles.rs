//! Phase 3 search oracles (`project_alice_world_model_mvp_plan` §3 Phase 3)
//! — written and pinned **before** the search body exists (Phase 4).
//!
//! All three scenes call a real public entry point ([`lower_bound_frames`]
//! for (a)/(b), [`plan`] for (c)) and are `#[ignore = "src gap: ..."]`
//! because the body each calls is `todo!()` (oracle-first discipline: pin
//! the expected answer now, implement later, remove `#[ignore]` when the
//! `src gap:` closes).
//!
//! ⚠️ **(c) used to compute its closed-form expectation and stop there,
//! never calling `plan`** — that tested nothing about this crate (an
//! "入口が内側" oracle, `ys-1f` 2026-10-02 review): a `plan` that
//! implements naive kinetic-energy dominance pruning could ship and this
//! test would stay green forever. It now calls `plan` and is red for the
//! same `todo!()` reason as (a)/(b); the closed-form derivation stays as
//! the comment explaining *why* the expected value is what it is.
//!
//! `ceil()` results here are always non-negative and small (well under
//! `u32::MAX`, these are frame counts in the hundreds), so the `as u32`
//! truncating casts are intentional.
#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use alice_world_auditor::{lower_bound_frames, Aabb, AxisState, Goal, Params};

/// (a) Closed-form minimal step count for 1-D rest-to-rest.
///
/// `t* = 2*sqrt(d/a)`, minimal integer frame count = `ceil(t*/dt)`.
///
/// ⚠️ **`goal` here is `PositionWithinAndAtRest`** — this is what selects
/// the rest-to-rest closed form (`lib.rs::lower_bound_frames` doc). The
/// `target` bounds are a placeholder (the heuristic reads `state`'s
/// already-resolved `distance_to_goal`, not `target`, for the distance);
/// reused from (c)'s safe window for consistency, not because it matters
/// here (`feedback_world_auditor_phase3_oracle_contradictions`, `ys-3a`
/// 2026-10-03).
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
    let goal = Goal::PositionWithinAndAtRest {
        target: Aabb {
            min: glam::Vec3::new(d - 0.1, -0.5, -0.5),
            max: glam::Vec3::new(d + 0.1, 0.5, 0.5),
        },
    };
    let params = Params::new(a, dt, 100_000);
    let frames = lower_bound_frames(state, &goal, &params);
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
///
/// ⚠️ **`goal` here is `PositionWithin`** (position-only, no `AtRest`) — the
/// scene's documented intent. Mixing this with (a)'s `PositionWithinAndAtRest`
/// value under a goal-blind `lower_bound_frames` was unsatisfiable: no
/// constant `C` in `ceil(C*sqrt(d/a)/dt)` passes both scenes simultaneously
/// (`feedback_world_auditor_phase3_oracle_contradictions`, `ys-3a`
/// 2026-10-03). `target` is a placeholder for the same reason as (a)'s.
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
    // 2026-10-03 ys-3a 実測: 旧値 49 は sqrt(2*20/12)=1.8257s → 110 frame の計算を
    // 誤っていた (closed form コード自体は正しい、assert の pin 値が狂っていた)
    assert_eq!(
        true_frames, 110,
        "closed form 自体の計算が狂っている (oracle の空振り確認)"
    );

    let state = AxisState {
        distance_to_goal: drop_height,
        velocity: 0.0,
    };
    let goal = Goal::PositionWithin {
        target: Aabb {
            min: glam::Vec3::new(drop_height - 0.1, -0.5, -0.5),
            max: glam::Vec3::new(drop_height + 0.1, 0.5, 0.5),
        },
    };

    // red 版: 重力加勢を見落とす (a_max_axis = a_input のみ)
    let params_without_gravity = Params::new(a_input, dt, 100_000);
    let h_without_gravity = lower_bound_frames(state, &goal, &params_without_gravity);
    assert!(
        h_without_gravity > true_frames,
        "a_input だけの heuristic が true cost を超えない (= この scene では加勢の見落としが \
         観測可能になっていない、scene を選び直す)"
    );

    // green 版: 重力加勢を含める (a_max_axis = a_input + |g|)
    let params_with_gravity = Params::new(a_input + g, dt, 100_000);
    let h_with_gravity = lower_bound_frames(state, &goal, &params_with_gravity);
    assert!(
        h_with_gravity <= true_frames,
        "a_input + |g| の heuristic が admissible でない (true cost を超えている)"
    );
}

/// (c) `plan` は運動エネルギーでの naive dominance pruning に落ちない
///
/// 「2 つの完了済み経路を比べて運動エネルギー (= ピーク速度) の低い方を
/// 優先する」は bang-bang 最適制御では不健全 この scene (`d=100, a=5`) の
/// bang-bang 最適経路はピーク速度が**比較的高い** (`v_peak = sqrt(d*a)
/// ≈ 22.36`) — naive な KE dominance を実装した `plan` は、この高速な
/// 最適経路をより低速な (= KE が低い、しかし所要時間が長い) 経路に劣後
/// すると誤判定して枝刈りし、**非最適な frame 数を返す**
///
/// ⚠️ **閉形式だけで判定すると「入口が内側」になる** (`ys-1f` 2026-10-02
/// review) — `plan` を実際に呼び、返ってきた `frames` を閉形式の真の最適
/// 値と比較することで初めて oracle になる 閉形式の導出 (`t* = 2*sqrt(d/a)`)
/// はコメントとして残す (期待値 537 の出所)
#[cfg(feature = "physics")]
#[test]
#[ignore = "src gap: plan body is todo!() (Phase 4, project_alice_world_model_mvp_plan §3 Phase 4.2)"]
fn c_plan_does_not_fall_for_naive_kinetic_energy_dominance() {
    use alice_physics::{Fix128, PhysicsConfig, PhysicsWorld, RigidBody, Vec3Fix};

    let d = 100.0_f32;
    let a = 5.0_f32;
    let dt = 1.0 / 60.0_f32;

    // 真の最適 (bang-bang): t* = 2*sqrt(d/a) = 2*sqrt(20) = 8.94427191
    // → /dt(1/60) = 536.656... → ceil 537 (a_closed_form_minimal_step_count
    // と同じ scene・同じ値 — ここでは heuristic でなく plan 自体の答えを問う)
    //
    // ⚠️ **537 は AABB の下端 (`target.min.x`) が特定の窓にある時だけ正しい**
    // (`ys-ba` 2026-10-03 検算、[[reference_bangbang_integer_lattice_closed_form]])
    // 整数格子では到達可能な位置が 1 刻みの全整数 (granularity 1) なので、
    // 最小 frame 数は「position(r) が target AABB の **下端以上**になる最小 r」
    // で決まる (この scene は下端側から接近するので) 下端を `d - 0.5 = 99.5`
    // に取ると実測 536 になる (位置格子単位 `D = d/(a*dt²) = 72000` に対し窓は `x_lo ∈
    // (99.755556, 100.127778]` の時のみ 537、検算: `verify_bangbang_lattice.py`)
    // ⇒ 下端を安全な窓の内側 (`99.9`) に取る
    let t_star = 2.0 * (d / a).sqrt();
    let optimal_frames = (t_star / dt).ceil() as u32;
    assert_eq!(
        optimal_frames, 537,
        "closed form 自体の計算が狂っている (oracle の空振り確認)"
    );

    // ⚠️ **既定 config (`substeps: 8` / `damping: 0.99`) では 537 frame に
    // 物理的に到達できない** (`ys-3a` 2026-10-03 実測、
    // `feedback_world_auditor_phase3_oracle_contradictions`) — frame 毎の
    // 0.99 速度減衰は終端速度を `a*dt/(1-0.99) ≈ 8.3 m/s` に抑え、bang-bang
    // 最適が要求する peak `sqrt(d*a) ≈ 22.36 m/s` の 4 割弱しか出せない
    // (全力加速だけでも 99.9 m に 819〜822 frame 要する、537 の 1.53 倍)
    // `damping: ONE` で消し、`substeps: 1` で §6 の整数格子モデル
    // (`v += a*dt` が厳密 / `x += v*dt`) と一致させる (`substeps` は damping
    // と違い frame 頭の速度 impulse 注入で吸収できない位置格子単位のズレを
    // 生むため、1 に揺らす必要がある) — この scene の目的は「理想 bang-bang
    // 模型と `plan` の出力を比較する解析解突合」であって「既定の使われ方」
    // ではないため、[[feedback_physics_analytic_oracle_rule_2026_09_15]] の
    // 「既定 config で書く」規律の対象外として扱う
    let mut world = PhysicsWorld::new(PhysicsConfig {
        gravity: Vec3Fix::ZERO,
        damping: Fix128::ONE,
        substeps: 1,
        ..PhysicsConfig::default()
    });
    world.add_body(RigidBody::new_dynamic(Vec3Fix::ZERO, Fix128::ONE));

    let goal = Goal::PositionWithinAndAtRest {
        target: Aabb {
            min: glam::Vec3::new(d - 0.1, -0.5, -0.5),
            max: glam::Vec3::new(d + 0.1, 0.5, 0.5),
        },
    };
    let params = Params::new(a, dt, 1_000_000);

    let result = alice_world_auditor::plan(&mut world, &goal, &params);
    let frames = match result {
        Ok(optimal) => optimal.frames,
        Err(best) => panic!(
            "budget exhausted before finding the optimal plan (best so far: {} frames) — \
             a dominance rule that prunes the high-velocity optimal branch would show up \
             exactly like this",
            best.frames
        ),
    };
    assert_eq!(
        frames, optimal_frames,
        "plan が bang-bang 最適 ({optimal_frames} frame) と異なる値を返した — \
         naive kinetic-energy dominance がこの高速な最適経路を誤って枝刈りしている可能性"
    );
}
