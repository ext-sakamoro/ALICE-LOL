//! product shortcut — `pen_cup(50,100)` 等の高階 construct をそのまま学習 target に
//!
//! text-to-print の system prompt が案内する 2-param / 3-param product を dims 付きで
//! 生成 caption は製品名 + 寸法、`lol` は shortcut 呼び出し (展開形は
//! `lol_canonical`) oracle は持たない (製品 geometry の内外は shortcut 実装依存)

use crate::caption::fmt_mm;
use crate::rng::Rng;
use crate::sample::{Sample, VerifyError};

/// (construct, 英名, 日名, 引数の意味 en, 引数の意味 ja)
type TwoParam = (
    &'static str,
    &'static str,
    &'static str,
    [&'static str; 2],
    [&'static str; 2],
);
const TWO: &[TwoParam] = &[
    (
        "pen_cup",
        "pen cup",
        "ペン立て",
        ["diameter", "height"],
        ["直径", "高さ"],
    ),
    (
        "coaster",
        "coaster",
        "コースター",
        ["diameter", "thickness"],
        ["直径", "厚さ"],
    ),
    (
        "mug",
        "mug",
        "マグカップ",
        ["diameter", "height"],
        ["直径", "高さ"],
    ),
];

const THREE: &[(&str, &str, &str)] = &[
    ("phone_stand", "phone stand", "スマホスタンド"),
    ("business_card_holder", "business card holder", "名刺立て"),
    ("desk_shelf", "desk shelf", "卓上棚"),
    ("storage_box", "storage box", "収納箱"),
    ("card_tray", "card tray", "カードトレイ"),
    ("headphone_holder", "headphone holder", "ヘッドホンホルダー"),
    ("tissue_box_cover", "tissue box cover", "ティッシュ箱カバー"),
    ("soap_tray", "soap tray", "石鹸トレイ"),
    ("sd_card_holder", "SD card holder", "SD カードホルダー"),
    ("toothbrush_holder", "toothbrush holder", "歯ブラシ立て"),
];

pub(crate) fn generate(rng: &mut Rng) -> Result<Sample, VerifyError> {
    if rng.chance(0.4) {
        let (name, en, ja, arg_en, arg_ja) = *rng.pick(TWO);
        let a = rng.stepped(30.0, 100.0, 5.0);
        let b = rng.stepped(5.0, 120.0, 5.0);
        let lol = format!("{name}({a}, {b})");
        Ok(Sample::new(
            "product_shortcut",
            format!(
                "A {en}, {} {} and {} {} (use the {name} shortcut).",
                arg_en[0],
                fmt_mm(a),
                arg_en[1],
                fmt_mm(b)
            ),
            format!(
                "{ja}: {} {}、{} {} ({name} ショートカットを使う)。",
                arg_ja[0],
                fmt_mm(a),
                arg_ja[1],
                fmt_mm(b)
            ),
            &lol,
            Vec::new(),
        )?)
    } else {
        let (name, en, ja) = *rng.pick(THREE);
        let (w, d, h) = (
            rng.stepped(40.0, 160.0, 10.0),
            rng.stepped(30.0, 120.0, 10.0),
            rng.stepped(20.0, 120.0, 10.0),
        );
        let lol = format!("{name}({w}, {d}, {h})");
        Ok(Sample::new(
            "product_shortcut",
            format!(
                "A {en} {} wide, {} deep, {} tall (use the {name} shortcut with width, depth, height).",
                fmt_mm(w),
                fmt_mm(d),
                fmt_mm(h)
            ),
            format!("{ja}: 幅 {}、奥行 {}、高さ {} ({name} ショートカットに幅, 奥行, 高さで渡す)。", fmt_mm(w), fmt_mm(d), fmt_mm(h)),
            &lol,
            Vec::new(),
        )?)
    }
}
