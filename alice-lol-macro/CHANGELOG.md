# Changelog (alice-lol-macro)

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
The changes of the whole repository, `alice-lol` included, are in the root `CHANGELOG.md`.

## [Unreleased]

## [0.2.1] - 未公開 (公開日を入れる)

`alice-lol` 0.4.0 と同時に公開する (公開の順は macro 0.2.1 → `alice-lol` 0.4.0) `alice-lol` 0.4.0 は macro 0.2.1 を要求する

### Fixed

- `taper` が `SdfNode::Taper { child, factor }` の struct literal を生成していた `alice-sdf` 5.x の `Taper` は field `reach` を持つので、0.2.0 で `taper(..)` を書くと compile できない 0.2.1 は `SdfNode::taper(child, factor)` を呼ぶ
- 4 つの keyword (`rect2d` / `segment2d` / `rounded_rect2d` / `sweep_bezier`) が `::glam::Vec2` を生成していた `glam` に直接依存しない crate では compile できない 0.2.1 は `::alice_lol::Vec2` を生成する (`alice-lol` 0.4.0 が `Vec2` を再公開する)
- 0.2.0 の package は license の text を含んでいなかった 0.2.1 の package は `LICENSE-APACHE` / `NOTICE` / `TRADEMARK_NOTICE` を含む

### Changed

- ライセンスは Apache-2.0 のみ (0.2.0 以前は MIT OR Apache-2.0 で公開しており、その版の条件は変わらない)

## [0.2.0] - 2026-07-23

最初の公開版 内容は root の `CHANGELOG.md` の `alice-lol` 0.2.0 / 0.3.0 の節を参照

[Unreleased]: https://github.com/ext-sakamoro/ALICE-LOL/compare/alice-lol-macro-v0.2.1...HEAD
[0.2.1]: https://github.com/ext-sakamoro/ALICE-LOL/compare/alice-lol-macro-v0.2.0...alice-lol-macro-v0.2.1
[0.2.0]: https://github.com/ext-sakamoro/ALICE-LOL/releases/tag/alice-lol-macro-v0.2.0
