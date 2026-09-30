//! `alice-lol-humanoid` の閉形式 oracle
//!
//! この crate には `tests/` が無く、unit test 36 本はいずれも「joint が 16 個ある」
//! 「parse が通る」といった構造の確認に寄っていた 骨格生成 (`from_morphology`) と
//! BVH の回転合成 (`compose_rotation`) はどちらも**式が閉じている**ので、
//! 教科書の値と直接突き合わせる
//!
//! | 対象 | oracle の出所 |
//! |---|---|
//! | 骨格 | `from_morphology` が宣言している比率の定義そのもの (頭部長 = `height/頭身`、腕長 = `height·arm_ratio`、肘 = 肩と手首の中点、肩幅 = `height·shoulder_ratio`) |
//! | 相似性 | 長さ parameter は 1 次の量なので `height` を α 倍すると全 joint 座標が α 倍になる |
//! | 対称性 | T-pose は左右鏡像 (x のみ符号反転) |
//! | BVH 回転 | 単軸回転の四元数 `(sin(θ/2)·axis, cos(θ/2))` と、チャネル順の右乗算合成 |
//!
//! 期待値はすべて式から書いており、実装を呼んで作った値は無い

use alice_lol::{Quat, Vec3};
use alice_lol_humanoid::bvh::BvhFile;
use alice_lol_humanoid::{HumanoidTemplate, Joint, MorphologyParams};

/// f32 の相対許容 (座標は高々 10 のオーダー、演算は数回)
fn close(got: f32, want: f32) -> bool {
    (got - want).abs() <= 1e-5 * want.abs().max(1.0)
}

fn joint(template: &HumanoidTemplate, j: Joint) -> [f32; 3] {
    *template
        .joints
        .get(&j)
        .unwrap_or_else(|| panic!("{j:?} が骨格に無い"))
}

/// 検査に使う morphology 一式 (既定 + 3 preset + 極端な 1 つ)
fn all_morphologies() -> Vec<(&'static str, MorphologyParams)> {
    vec![
        ("default", MorphologyParams::default()),
        ("adult", MorphologyParams::adult()),
        ("chibi", MorphologyParams::chibi()),
        ("hero", MorphologyParams::hero()),
        (
            "tall",
            MorphologyParams {
                height: 12.5,
                head_body_ratio: 9.0,
                arm_ratio: 0.42,
                shoulder_ratio: 0.3,
                hip_ratio: 0.2,
                leg_ratio: 0.55,
            },
        ),
    ]
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  骨格 — from_morphology
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// oracle: 「n 頭身」の定義どおり、頭頂から首までが `height / head_body_ratio`
#[test]
fn head_span_equals_height_over_head_body_ratio() {
    for (name, params) in all_morphologies() {
        let t = HumanoidTemplate::from_morphology(&params);
        let head_top = joint(&t, Joint::Head)[1];
        let neck = joint(&t, Joint::Neck)[1];
        let want = params.height / params.head_body_ratio;
        assert!(
            close(head_top - neck, want),
            "{name}: 頭部長 {} が height/頭身 = {want} と違う",
            head_top - neck
        );
    }
}

/// oracle: 腕長 (肩→手首) は `height · arm_ratio`、肘はその中点
#[test]
fn arm_chain_matches_the_arm_ratio_and_the_elbow_bisects_it() {
    for (name, params) in all_morphologies() {
        let t = HumanoidTemplate::from_morphology(&params);
        for (shoulder, elbow, wrist) in [
            (Joint::RShoulder, Joint::RElbow, Joint::RWrist),
            (Joint::LShoulder, Joint::LElbow, Joint::LWrist),
        ] {
            let s = joint(&t, shoulder);
            let e = joint(&t, elbow);
            let w = joint(&t, wrist);
            let arm = (w[0] - s[0]).abs();
            assert!(
                close(arm, params.height * params.arm_ratio),
                "{name}: 腕長 {arm} が height·arm_ratio = {} と違う",
                params.height * params.arm_ratio
            );
            assert!(
                close(e[0], f32::midpoint(s[0], w[0])),
                "{name}: 肘の x {} が肩 {} と手首 {} の中点でない",
                e[0],
                s[0],
                w[0]
            );
            // T-pose なので腕は水平 (肩と同じ高さ)
            assert!(
                close(e[1], s[1]) && close(w[1], s[1]),
                "{name}: T-pose の腕が水平でない (肩 {} / 肘 {} / 手首 {})",
                s[1],
                e[1],
                w[1]
            );
        }
    }
}

/// oracle: 肩幅と骨盤幅はそれぞれ `height · shoulder_ratio` / `height · hip_ratio`
#[test]
fn shoulder_and_hip_spans_match_their_ratios() {
    for (name, params) in all_morphologies() {
        let t = HumanoidTemplate::from_morphology(&params);
        let shoulder_span = joint(&t, Joint::RShoulder)[0] - joint(&t, Joint::LShoulder)[0];
        let hip_span = joint(&t, Joint::RHip)[0] - joint(&t, Joint::LHip)[0];
        assert!(
            close(shoulder_span, params.height * params.shoulder_ratio),
            "{name}: 肩幅 {shoulder_span} が height·shoulder_ratio = {} と違う",
            params.height * params.shoulder_ratio
        );
        assert!(
            close(hip_span, params.height * params.hip_ratio),
            "{name}: 骨盤幅 {hip_span} が height·hip_ratio = {} と違う",
            params.height * params.hip_ratio
        );
    }
}

/// oracle: 足首は腰から `height · leg_ratio` 下、膝はその 52%
#[test]
fn leg_chain_follows_the_leg_ratio() {
    for (name, params) in all_morphologies() {
        let t = HumanoidTemplate::from_morphology(&params);
        let waist = joint(&t, Joint::Waist)[1];
        let ankle = joint(&t, Joint::RAnkle)[1];
        let knee = joint(&t, Joint::RKnee)[1];
        assert!(close(waist, 0.0), "{name}: 腰が原点でない ({waist})");
        assert!(
            close(ankle, -params.height * params.leg_ratio),
            "{name}: 足首 {ankle} が −height·leg_ratio = {} と違う",
            -params.height * params.leg_ratio
        );
        assert!(
            close(knee, ankle * 0.52),
            "{name}: 膝 {knee} が足首の 52% = {} と違う",
            ankle * 0.52
        );
        assert!(
            knee > ankle && knee < waist,
            "{name}: 膝 {knee} が腰 {waist} と足首 {ankle} の間に無い"
        );
    }
}

/// oracle: T-pose は左右鏡像 — 対になる joint は x だけ符号が反転し y/z は等しい
#[test]
fn skeleton_is_left_right_mirror_symmetric() {
    const PAIRS: [(Joint, Joint); 6] = [
        (Joint::LShoulder, Joint::RShoulder),
        (Joint::LElbow, Joint::RElbow),
        (Joint::LWrist, Joint::RWrist),
        (Joint::LHip, Joint::RHip),
        (Joint::LKnee, Joint::RKnee),
        (Joint::LAnkle, Joint::RAnkle),
    ];
    for (name, params) in all_morphologies() {
        let t = HumanoidTemplate::from_morphology(&params);
        for (left, right) in PAIRS {
            let l = joint(&t, left);
            let r = joint(&t, right);
            assert!(
                close(l[0], -r[0]) && close(l[1], r[1]) && close(l[2], r[2]),
                "{name}: {left:?} {l:?} と {right:?} {r:?} が鏡像でない"
            );
            assert!(r[0] > 0.0, "{name}: {right:?} が右 (x>0) に無い");
        }
        // 正中線上の joint は x = 0
        for center in [Joint::Head, Joint::Neck, Joint::Chest, Joint::Waist] {
            assert!(
                close(joint(&t, center)[0], 0.0),
                "{name}: {center:?} が正中線から外れている"
            );
        }
    }
}

/// oracle: 長さはすべて `height` の 1 次式なので、`height` を α 倍した骨格は
/// 元の骨格の α 倍の相似形になる (比率 parameter は無次元)
#[test]
fn morphology_scales_similarly_with_height() {
    const ALL: [Joint; 16] = [
        Joint::Head,
        Joint::Neck,
        Joint::Chest,
        Joint::Waist,
        Joint::LShoulder,
        Joint::RShoulder,
        Joint::LElbow,
        Joint::RElbow,
        Joint::LWrist,
        Joint::RWrist,
        Joint::LHip,
        Joint::RHip,
        Joint::LKnee,
        Joint::RKnee,
        Joint::LAnkle,
        Joint::RAnkle,
    ];
    let base = MorphologyParams::default();
    let unit = HumanoidTemplate::from_morphology(&base);
    for alpha in [0.5_f32, 2.0, 7.3] {
        let scaled = HumanoidTemplate::from_morphology(&MorphologyParams {
            height: base.height * alpha,
            ..base
        });
        for j in ALL {
            let a = joint(&unit, j);
            let b = joint(&scaled, j);
            for axis in 0..3 {
                assert!(
                    close(b[axis], a[axis] * alpha),
                    "α={alpha} {j:?} 軸{axis}: {} が {} の α 倍でない",
                    b[axis],
                    a[axis]
                );
            }
        }
    }
}

/// `head_body_ratio` が指す「頭身」と、実際のシルエット (頭頂〜足首) から測った
/// 頭身が一致するのは `leg_ratio = 0.5` の時だけ、という現状を定量的に固定する
///
/// ⚠️ `height` の doc は「全身高」だが、頭頂は `height/2`、足首は `−height·leg_ratio`
/// なので実際の全高は `height·(0.5 + leg_ratio)` `leg_ratio` が 0.5 から離れるほど
/// 宣言頭身と見た目の頭身がずれる (chibi は宣言 3.0 に対し実測 2.7)
/// 仕様として意図されたものかは要判断 — 変えるなら preset の意味が動く
#[test]
fn declared_head_body_ratio_matches_the_silhouette_only_when_legs_are_half() {
    for (name, params) in all_morphologies() {
        let t = HumanoidTemplate::from_morphology(&params);
        let head_top = joint(&t, Joint::Head)[1];
        let ankle = joint(&t, Joint::RAnkle)[1];
        let silhouette = head_top - ankle;
        // 全高の閉形式
        assert!(
            close(silhouette, params.height * (0.5 + params.leg_ratio)),
            "{name}: 全高 {silhouette} が height·(0.5+leg_ratio) と違う"
        );
        let head_span = params.height / params.head_body_ratio;
        let actual_ratio = silhouette / head_span;
        let matches = close(actual_ratio, params.head_body_ratio);
        assert_eq!(
            matches,
            close(params.leg_ratio, 0.5),
            "{name}: 宣言頭身 {} と実測頭身 {actual_ratio} の一致が leg_ratio={} の条件と食い違う",
            params.head_body_ratio,
            params.leg_ratio
        );
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  BVH — 回転チャネルの合成
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Z / X / Y の 3 チャネルを持つ 1 関節だけの BVH
/// frame 0 は 3 軸とも 90°、frame 1 は Z のみ 30°
const TRIAXIAL_BVH: &str = "\
HIERARCHY
ROOT Hips
{
    OFFSET 0.0 0.0 0.0
    CHANNELS 3 Zrotation Xrotation Yrotation
    End Site
    {
        OFFSET 0.0 1.0 0.0
    }
}
MOTION
Frames: 3
Frame Time: 0.033333
90.0 90.0 90.0
30.0 0.0 0.0
0.0 0.0 0.0
";

/// 位置チャネルを BVH で許される別の並び (Z, X, Y) で書いた root
const SHUFFLED_POSITION_BVH: &str = "\
HIERARCHY
ROOT Hips
{
    OFFSET 0.0 0.0 0.0
    CHANNELS 6 Zposition Xposition Yposition Zrotation Xrotation Yrotation
    End Site
    {
        OFFSET 0.0 1.0 0.0
    }
}
MOTION
Frames: 1
Frame Time: 0.033333
3.0 1.0 2.0 0.0 0.0 0.0
";

fn hips_rotation(source: &str, frame: usize) -> Quat {
    let bvh: BvhFile = source.parse().expect("fixture BVH がパースできる");
    let rotations = bvh.frame_rotations_named(frame, |name| Some(name.to_string()));
    *rotations.get("Hips").expect("Hips の回転が返る")
}

fn quat_close(got: Quat, want: Quat) -> bool {
    // q と −q は同じ回転
    let direct = (got.x - want.x).abs()
        + (got.y - want.y).abs()
        + (got.z - want.z).abs()
        + (got.w - want.w).abs();
    let flipped = (got.x + want.x).abs()
        + (got.y + want.y).abs()
        + (got.z + want.z).abs()
        + (got.w + want.w).abs();
    direct.min(flipped) < 1e-5
}

/// oracle: 単軸回転の四元数は `(sin(θ/2)·axis, cos(θ/2))`
/// Z 軸 30° なら `(0, 0, sin15°, cos15°)`
#[test]
fn single_axis_channel_matches_the_exact_quaternion() {
    let got = hips_rotation(TRIAXIAL_BVH, 1);
    let half = 15.0_f32.to_radians();
    let want = Quat::from_xyzw(0.0, 0.0, half.sin(), half.cos());
    assert!(
        quat_close(got, want),
        "Z 30° の四元数が {got:?}、閉形式は {want:?}"
    );
}

/// oracle: BVH のチャネル値は**度**なので、30° の回転は X 軸を 30° 動かす
#[test]
fn channel_values_are_degrees_not_radians() {
    let q = hips_rotation(TRIAXIAL_BVH, 1);
    let rotated = q * Vec3::X;
    let angle = rotated.y.atan2(rotated.x).to_degrees();
    assert!(
        (angle - 30.0).abs() < 1e-3,
        "Z 30° 指定で X 軸が {angle}° しか回っていない (度/ラジアンの取り違え)"
    );
}

/// oracle: 全チャネル 0 の frame は恒等回転
#[test]
fn zero_frame_is_the_identity_rotation() {
    let q = hips_rotation(TRIAXIAL_BVH, 2);
    assert!(
        quat_close(q, Quat::IDENTITY),
        "0° の frame が恒等でない ({q:?})"
    );
}

/// oracle: チャネルは file に書かれた順に右から掛かる (`q = q_Z · q_X · q_Y`)
///
/// 回転は非可換なので、順序が入れ替わると別の姿勢になる ここでは 3 軸すべてに
/// 90° を与えて、閉形式の積と一致することと、**逆順の積とは一致しないこと**の
/// 両方を見る (後者が無いと「順序を守っている」ことの証拠にならない)
#[test]
fn rotation_channels_compose_in_file_order() {
    let got = hips_rotation(TRIAXIAL_BVH, 0);
    let quarter = 90.0_f32.to_radians();
    let qz = Quat::from_axis_angle(Vec3::Z, quarter);
    let qx = Quat::from_axis_angle(Vec3::X, quarter);
    let qy = Quat::from_axis_angle(Vec3::Y, quarter);

    assert!(
        quat_close(got, qz * qx * qy),
        "合成順が file 順 (Z·X·Y) でない: got {got:?}"
    );
    assert!(
        !quat_close(qz * qx * qy, qy * qx * qz),
        "この fixture では順序が結果を変えないので、順序の検証になっていない"
    );
}

/// `compose_rotation` の doc が主張する「`v' = q·v·q⁻¹` では最後のチャネルが
/// 最初に v へ適用される」を、実際にベクトルを回して確かめる
///
/// 3 軸 90°、`v = X` のとき Y→X→Z の順に 90° ずつ適用すると
/// `X → −Z → Y → −X` になるので、答えは `−X`
#[test]
fn the_last_channel_is_applied_to_the_vector_first() {
    let q = hips_rotation(TRIAXIAL_BVH, 0);
    let got = q * Vec3::X;

    let quarter = 90.0_f32.to_radians();
    let step_by_step = Quat::from_axis_angle(Vec3::Z, quarter)
        * (Quat::from_axis_angle(Vec3::X, quarter)
            * (Quat::from_axis_angle(Vec3::Y, quarter) * Vec3::X));

    assert!(
        (got - step_by_step).length() < 1e-5,
        "段階適用 {step_by_step:?} と一括適用 {got:?} が違う"
    );
    assert!(
        (got - Vec3::NEG_X).length() < 1e-5,
        "X を Y→X→Z の順に 90° ずつ回すと −X になるはずが {got:?}"
    );
}

/// oracle: 位置チャネルは**名前**で対応付くので、書かれた順に依らず
/// `Xposition` の値が x に入る
#[test]
fn root_position_maps_channels_by_name_not_by_order() {
    let bvh: BvhFile = SHUFFLED_POSITION_BVH
        .parse()
        .expect("fixture BVH がパースできる");
    let position = bvh
        .frame_root_position(0)
        .expect("position チャネルがあるので Some");
    // file の並びは Z, X, Y で値は 3, 1, 2
    assert!(
        (position - Vec3::new(1.0, 2.0, 3.0)).length() < 1e-6,
        "並び替えた位置チャネルが {position:?} になった (X=1, Y=2, Z=3 のはず)"
    );
}
