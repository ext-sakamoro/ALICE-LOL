# Changelog (alice-lol-macro)

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
The changes of the whole repository, `alice-lol` included, are in the root `CHANGELOG.md`.

## [Unreleased]

0.4.0 として公開する (公開の順は macro 0.4.0 → `alice-lol` 0.5.0) `alice-lol` 0.5.0 は macro 0.4 を要求する

### Changed

- 破壊的変更: 展開は毎回 `::alice_lol::__expect_macro_id!("<識別子>")` を呼び、この macro の識別子 (`EXPANSION_ID_HEX`、probe corpus の展開を正規化した token 列の SHA-256) を `alice-lol` に照合させる (ALICE-Law G7) `__expect_macro_id!` を持たない `alice-lol` 0.4.x とは組めないので minor を上げる `alice-lol` は自分の値と違う識別子を両方を名指す compile error にする

## [0.3.0] - 2026-10-13

`alice-lol` 0.4.0 より前に公開する (公開の順は macro 0.3.0 → `alice-lol` 0.4.0) `alice-lol` 0.4.0 は macro 0.3 を要求する このバージョン自身は `alice-lol` を含むどの ALICE-* crate にも依存しない (`syn` / `quote` / `proc-macro2` のみ)

### Fixed

- `taper` が `SdfNode::Taper { child, factor }` の struct literal を生成していた `alice-sdf` 5.x の `Taper` は field `reach` を持つので、0.2.0 で `taper(..)` を書くと compile できない 0.3.0 は `SdfNode::taper(child, factor)` を呼ぶ
- 4 つの keyword (`rect2d` / `segment2d` / `rounded_rect2d` / `sweep_bezier`) が `::glam::Vec2` を生成していた `glam` に直接依存しない crate では compile できない 0.3.0 は `::alice_lol::Vec2` を生成する (`alice-lol` 0.4.0 が `Vec2` を再公開する)
- 0.2.0 の package は license の text を含んでいなかった 0.3.0 の package は `LICENSE-APACHE` / `NOTICE` / `TRADEMARK_NOTICE` を含む

### Changed

- ライセンスは Apache-2.0 のみ (0.2.0 以前は MIT OR Apache-2.0 で公開しており、その版の条件は変わらない)

## [0.2.0] - 2026-07-23

最初の公開版 内容は root の `CHANGELOG.md` の `alice-lol` 0.2.0 / 0.3.0 の節を参照

[Unreleased]: https://github.com/ext-sakamoro/ALICE-LOL/compare/alice-lol-macro-v0.3.0...HEAD
[0.3.0]: https://github.com/ext-sakamoro/ALICE-LOL/compare/alice-lol-macro-v0.2.0...alice-lol-macro-v0.3.0
[0.2.0]: https://github.com/ext-sakamoro/ALICE-LOL/releases/tag/alice-lol-macro-v0.2.0
