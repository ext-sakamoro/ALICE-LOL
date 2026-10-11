//! `lol!` マクロの TPMS (`gyroid`/等 9 primitive) 検証が、literal な
//! scale/thickness では compile-time に `compile_error!` を出すことを確かめる
//!
//! `#[test]` の中で compile error を起こすとそのテスト binary 自体が
//! ビルドできなくなるため (他の test も巻き込む)、trybuild で別プロセス
//! コンパイルする ⚠️ `.stderr` は rustc の出力そのものなので toolchain の
//! version で変わりうる (CI で既知のリスク、変わったら `TRYBUILD=overwrite
//! cargo test --test compile_fail` で更新する)
#[test]
fn tpms_literal_scale_and_thickness_are_checked_at_compile_time() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/*.rs");
}
