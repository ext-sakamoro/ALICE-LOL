//! Law の識別子 — 同じ主張に同じ名前を付ける.
//!
//! Law は「何が成立すべきか」の記述なので、**保存した Law と評価する Law が同じものか**を
//! 言えなければ、保存・配布・検証が成立しない その名前を作るのが [`LawIdHasher`] で、
//! `alice-zip` が信号の法則に対して既に持っている仕組みと同じ形に揃えている
//! ([`LAW_ID_DOMAIN`] を共有し、種類ごとの tag を足す)
//!
//! # 識別子に何を含めるか
//!
//! 1. **domain** ([`LAW_ID_DOMAIN`]) — 他の用途の hash と衝突させない
//! 2. **種類** (`*_LAW_KIND`) — 別の種類の Law が偶然同じ byte 列を書いても分かれる
//! 3. **算術の世代** ([`LOL_SEMANTICS_ID`]) — ⚠️ **同じ text でも評価する算術が違えば
//!    別の答えが出る**ので、別の Law として扱う
//! 4. **主張そのもの** — 名前・項・値・単位
//!
//! # なぜ長さを前置するか
//!
//! ⚠️ 長さを付けずに byte 列を繋ぐと `["ab", "c"]` と `["a", "bc"]` が同じ hash になり、
//! **別の Law が同じ名前を名乗る** [`LawIdHasher`] は値ごとに型 tag と長さを書くので、
//! `u32 5` と `u64 5`、空の項と項の不在も分かれる
//! (`tests/law_id_oracle.rs` がこの 4 つを固定している)
//!
//! # 識別子が振る舞いから drift しないこと
//!
//! [`LOL_SEMANTICS_ID`] は定数だが、[`LOL_SEMANTICS_PINS`] の fold と一致することを
//! oracle が検査する ⚠️ さらに pin の値自体を**振る舞いから計算**している
//! ([`audit_verdict_order_fingerprint`]) ので、判定順序を変えると pin が合わなくなり
//! 定数を直すまで red になる **文字列の hash を pin にすると、振る舞いを変えても
//! 気付けない**ので採らない
//!
//! ## 再記録する順序
//!
//! 意図して振る舞いを変えた時は **pin の値を先に**直し、そのあとで
//! [`LOL_SEMANTICS_ID`] を再計算する 逆順にすると、fold が古い pin を読むので
//! 同じ値が出てしまい「識別子が変化に気付かなかった」ように見える
//!
//! # 既知の限界
//!
//! ⚠️ **式は text として識別される**ので、`a*b + a*c` と `a*(b+c)` は別の名前になる
//! 意味が同じ式に同じ名前を付けるには式の正規形が要るが、それは**この識別子の上に載る**
//! 層で、ここでは扱わない
//!
//! # まだ識別子を持たない 2 種類と、その理由
//!
//! ⚠️ **幾何の制約** ([`crate::law::Constraint`]) — `SdfNode` の正規形は `emit::to_lol` だが
//! これは失敗しうるので戻り値が `Result` になり、他と形が変わる 設計を分けて別途足す
//!
//! ⚠️ **研究の法則** ([`crate::research_law::ResearchLaw`]) — 式 (`LawExpr`) に canonical な
//! text 形が無い `describe()` は**提示用**の文字列なので、label を変えただけで識別子が
//! 変わる脆い名前になる ⇒ **式の正規形を決めてから**足す
//! (同じ意味の式に同じ名前を付ける話と同じ問題なので、まとめて扱うのが筋)

use sha2::{Digest, Sha256};

use crate::audit_law::{AuditLaw, Clause, Measurements};

/// 信号の法則と共有する domain 分離子 (`alice-zip` の定義をそのまま使う)
pub use alice_zip::law::LAW_ID_DOMAIN;

/// 監査の Law の種類 tag
pub const AUDIT_LAW_KIND: &[u8] = b"lol.law.audit";

/// 値の型 tag `u32 5` と `u64 5` を分けるために、長さの前に 1 byte 書く
mod tag {
    pub const BYTES: u8 = 1;
    pub const U32: u8 = 2;
    pub const U64: u8 = 3;
    pub const F64: u8 = 4;
    pub const STR: u8 = 5;
}

/// 算術と判定の意味論の pin `(名前, その振る舞いの指紋)`
///
/// ⚠️ 値は**振る舞いから計算**する (oracle が各 entry を再計算して突合する)
/// 名前を足したら [`LOL_SEMANTICS_ID`] を再計算する (module doc の「再記録する順序」)
pub const LOL_SEMANTICS_PINS: &[(&str, [u8; 32])] = &[
    // 上流の算術世代 これが変われば本 crate の識別子も変わる
    ("alice-det-math", alice_det_math::SEMANTICS_ID),
    // 監査の判定順序 (証拠 -> 成立範囲 -> 期待値) 順序は verdict を変えるので意味論
    (
        "audit verdict order: evidence -> ranges -> expectations",
        hex32("14e30eaa97ad76260f022b28f4fc15f06f9df779256439078289400b8ce11e64"),
    ),
    // 入力の読み方 (型を持つ x-input、型に合わない値は入力全体が合わない) 読み方は
    // 同じ request に対する判定を変えるので意味論
    (
        "input reading: whole-input typed x-input",
        hex32("1bf362f10e2fff2a3a620df006515f08940c4770037cfe70ee3354a5c1466f50"),
    ),
];

/// [`LOL_SEMANTICS_PINS`] の fold ⚠️ 定数と fold の一致は oracle が検査する
pub const LOL_SEMANTICS_ID: [u8; 32] =
    hex32("e1b05c475eaa4209264d7e74211baa68f8d5e550359bfa05f812d645af9a6e3c");

/// 16 進 64 文字を 32 byte に (const 文脈で書けるようにするため)
///
/// # Panics
///
/// 64 文字の 16 進でない時に compile 時 panic する
#[must_use]
pub const fn hex32(s: &str) -> [u8; 32] {
    let b = s.as_bytes();
    assert!(b.len() == 64, "32 byte = 16 進 64 文字");
    let mut out = [0_u8; 32];
    let mut i = 0;
    while i < 32 {
        out[i] = (nibble(b[i * 2]) << 4) | nibble(b[i * 2 + 1]);
        i += 1;
    }
    out
}

const fn nibble(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => panic!("16 進でない文字"),
    }
}

/// [`LOL_SEMANTICS_PINS`] から意味論の識別子を作る
#[must_use]
pub fn fold_semantics_pins(pins: &[(&str, [u8; 32])]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(LAW_ID_DOMAIN);
    h.update(b"lol.semantics");
    for (name, id) in pins {
        length_prefixed(&mut h, name.as_bytes());
        h.update(id);
    }
    h.finalize().into()
}

/// 監査の判定順序の指紋 — **振る舞いから計算する**
///
/// 固定の Law と固定の実測列に対する判定の並びを hash する
/// ⚠️ 判定順序 (証拠 -> 成立範囲 -> 期待値) を入れ替えると並びが変わるので、
/// この値も変わる = pin が振る舞いを本当に名乗っている
///
/// 実測列は判定順序のほかに、**証拠の読み方** (有限で 0 より大きい数) と
/// **成立範囲の読み方** (集合) を区別する case を含む どちらかの読み方を変えると
/// 判定の並びが変わり、pin が合わなくなる
#[must_use]
pub fn audit_verdict_order_fingerprint() -> [u8; 32] {
    let law = AuditLaw::new(
        "fingerprint".to_owned(),
        vec![
            Clause::Evidence {
                metric: "n".to_owned(),
            },
            Clause::Expect {
                metric: "bad".to_owned(),
                value: 0.0,
                tolerance: 0.0,
            },
            Clause::Range {
                key: "k".to_owned(),
                values: vec!["1".to_owned(), "2".to_owned()],
            },
        ],
    );
    // 「どの不成立を先に報告するか」が出る実測列
    let cases = [
        // 何も測れていない (証拠なし)
        Measurements::new(),
        // 数えたが 0 件 + 期待値も 0 ⚠️ 順序が逆なら Supports に倒れる
        Measurements::new()
            .with_number("n", 0.0)
            .with_number("bad", 0.0)
            .with_range("k", &["1", "2"]),
        // 成立範囲が消えている + 期待値も外れている (どちらを先に報告するか)
        Measurements::new()
            .with_number("n", 3.0)
            .with_number("bad", 1.0),
        // すべて満たす
        Measurements::new()
            .with_number("n", 3.0)
            .with_number("bad", 0.0)
            .with_range("k", &["1", "2"]),
        // 負の件数は証拠にならない (有限で 0 より大きい数だけが証拠)
        // ⚠️ `!= 0` を証拠とする読み方では Supports に倒れる
        Measurements::new()
            .with_number("n", -1.0)
            .with_number("bad", 0.0)
            .with_range("k", &["1", "2"]),
        // 成立範囲は集合 (重複は判定に影響しない)
        // ⚠️ 重複を残す読み方では ParameterUpdate に倒れる
        Measurements::new()
            .with_number("n", 3.0)
            .with_number("bad", 0.0)
            .with_range("k", &["2", "1", "2"]),
        // 期待値の実測が有限でない (NaN) のは測られていないのと同じ
        // ⚠️ 検査しない読み方では `(NaN - 0).abs() > 0` が false で Supports に倒れる
        Measurements::new()
            .with_number("n", 3.0)
            .with_number("bad", f64::NAN)
            .with_range("k", &["1", "2"]),
    ];
    let mut h = Sha256::new();
    h.update(LAW_ID_DOMAIN);
    h.update(b"lol.audit.verdict-order");
    for m in &cases {
        length_prefixed(&mut h, law.evaluate(m).to_string().as_bytes());
    }
    h.finalize().into()
}

fn length_prefixed(h: &mut Sha256, bytes: &[u8]) {
    h.update((bytes.len() as u64).to_le_bytes());
    h.update(bytes);
}

/// Law の識別子を組み立てる **道具** (識別子そのものではない)
///
/// 値ごとに型 tag と長さを書くので、連結が曖昧にならない (module doc 参照)
/// 下流が自分の種類の Law に識別子を付ける時も、同じ domain で作れるよう公開している
///
/// # 実装していない trait とその理由
///
/// * `PartialEq` / `Eq` / `Ord` — 途中状態の道具なので比較に意味が無い
///   (比べるのは [`finish`](Self::finish) が返す `[u8; 32]`)
/// * `Display` — 人間に見せる形が無い (見せるのは確定した識別子)
/// * `Default` — 種類と算術の世代を宣言しない識別子は意味を持たないので、
///   既定値を作れてはいけない
/// * `From` / `AsRef` / `Deref` — 内側の hash 実装を API に出さないため
///   (出すと hash を差し替えられなくなる)
#[derive(Debug, Clone)]
pub struct LawIdHasher(Sha256);

impl LawIdHasher {
    /// 種類と算術の世代を宣言して始める
    #[must_use]
    pub fn new(kind: &[u8], semantics_id: &[u8; 32]) -> Self {
        let mut h = Sha256::new();
        h.update(LAW_ID_DOMAIN);
        length_prefixed(&mut h, kind);
        h.update(semantics_id);
        Self(h)
    }

    /// 生の byte 列
    #[must_use]
    pub fn bytes(mut self, v: &[u8]) -> Self {
        self.0.update([tag::BYTES]);
        length_prefixed(&mut self.0, v);
        self
    }

    /// 文字列 (`bytes` と別の tag を使うので、同じ中身でも混ざらない)
    #[must_use]
    pub fn str(mut self, v: &str) -> Self {
        self.0.update([tag::STR]);
        length_prefixed(&mut self.0, v.as_bytes());
        self
    }

    /// 32 bit 整数
    #[must_use]
    pub fn u32(mut self, v: u32) -> Self {
        self.0.update([tag::U32]);
        self.0.update(v.to_le_bytes());
        self
    }

    /// 64 bit 整数
    #[must_use]
    pub fn u64(mut self, v: u64) -> Self {
        self.0.update([tag::U64]);
        self.0.update(v.to_le_bytes());
        self
    }

    /// 浮動小数点 bit で識別する
    ///
    /// ⚠️ `-0.0` と `0.0` は `==` では等しいが、値としては別物なので分ける
    /// ⚠️ 逆に NaN は payload が違っても「判定できない」という同じ意味なので揃える
    #[must_use]
    pub fn f64(mut self, v: f64) -> Self {
        self.0.update([tag::F64]);
        let bits = if v.is_nan() {
            f64::NAN.to_bits()
        } else {
            v.to_bits()
        };
        self.0.update(bits.to_le_bytes());
        self
    }

    /// 識別子を確定する
    #[must_use]
    pub fn finish(self) -> [u8; 32] {
        self.0.finalize().into()
    }
}

impl AuditLaw {
    /// この監査 Law の識別子
    ///
    /// 名前と項 (順序を含む) と型を持つ入力と算術の世代から決まる
    ///
    /// 型を持つ入力が無い Law では、同じ `semantics_id` のもとで入力の型を入れる前と同じ値
    /// (入力の列は項の後に、1 つ以上ある時だけ tag 3 で書く 項の数を先に書いているので
    /// 境界は曖昧にならない) ⚠️ `semantics_id` が変われば、どの Law の識別子も変わる
    #[must_use]
    pub fn law_id(&self, semantics_id: &[u8; 32]) -> [u8; 32] {
        let mut e = LawIdHasher::new(AUDIT_LAW_KIND, semantics_id).str(self.name());
        e = e.u64(self.clauses().len() as u64);
        for c in self.clauses() {
            e = match c {
                Clause::Evidence { metric } => e.u32(0).str(metric),
                Clause::Expect {
                    metric,
                    value,
                    tolerance,
                } => e.u32(1).str(metric).f64(*value).f64(*tolerance),
                Clause::Range { key, values } => {
                    let mut e = e.u32(2).str(key).u64(values.len() as u64);
                    for v in values {
                        e = e.str(v);
                    }
                    e
                }
            };
        }
        for (name, ty) in self.inputs() {
            e = e.u32(3).str(name).str(&ty.canonical());
        }
        e.finish()
    }
}
