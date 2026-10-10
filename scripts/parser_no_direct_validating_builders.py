#!/usr/bin/env python3
"""alice-lol/src/runtime_parser.rs が validating な infallible builder を直接呼んでいないか検査する

2026-10-10 の実測: `gridfinity_bin_ex` は divider の各軸を per-axis の上限
(`MAX_STDLIB_COUNT`) まで検査していたが、積は検査しておらず、infallible な
`gridfinity_bin` (= `spec.validate().expect(..)` を冒頭で呼ぶ) を直接呼んでいた
`gridfinity_bin_ex(1,1,1,101,100,0,0)` (各軸は上限内、積は上限超過) が
untrusted な `.lol` text からそのまま panic した (base では Ok、6MB)

対象の builder は Spec を検査してから確保する設計 (`validate()` / `try_*` 対)
untrusted な入力 (parser が読んだ値) はこの fallible な `try_*` 経由でしか
呼んではいけない、この gate はそれを静的に確認する (呼出を数えられなければ fail、
比較 0 件を green にしない)

usage: python3 scripts/parser_no_direct_validating_builders.py
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

TARGET = Path(__file__).resolve().parents[1] / "alice-lol" / "src" / "runtime_parser.rs"

# validate() を持つ Spec の infallible builder (呼ぶなら try_<name> を使うこと)
# 新しい Spec/validate()/try_* の対を増やしたら、ここにも名前を足す
VALIDATING_BUILDERS = ["gridfinity_bin", "skadis_panel_sdf", "shelf_divider"]


def find_violations(text: str) -> list[tuple[str, int, str]]:
    violations: list[tuple[str, int, str]] = []
    for name in VALIDATING_BUILDERS:
        # `::` を直前に要求する: 実際の呼出 (`pattern_sdf::gridfinity_bin(..)`) は
        # 常に完全修飾 path で、`::` が無いヒットは `.lol` text の文字列 literal
        # (test fixture の keyword 名) なので誤検出になる \b は "_" を word 文字
        # として扱うので、try_gridfinity_bin( の内側の "gridfinity_bin(" には
        # \b が立たず正しく除外される
        for m in re.finditer(rf"::{re.escape(name)}\(", text):
            line_no = text.count("\n", 0, m.start()) + 1
            line_start = text.rfind("\n", 0, m.start()) + 1
            line_end = text.find("\n", m.start())
            line = text[line_start:line_end].strip()
            if line.startswith("//"):
                continue
            violations.append((name, line_no, line))
    return violations


def main() -> int:
    if not TARGET.is_file():
        print(f"parser_no_direct_validating_builders: {TARGET} が読めない", file=sys.stderr)
        return 2
    text = TARGET.read_text(encoding="utf-8")
    if not text:
        print(f"parser_no_direct_validating_builders: {TARGET} が空 (比較 0 件)", file=sys.stderr)
        return 2

    violations = find_violations(text)
    if violations:
        print("parser_no_direct_validating_builders: VIOLATIONS (parser が validating な "
              "infallible builder を直接呼んでいる、try_* を使うこと):")
        for name, line_no, line in violations:
            print(f"  {TARGET.name}:{line_no}: {name}(..) — {line}")
        return 1

    print(f"parser_no_direct_validating_builders: ok (検査した builder "
          f"{len(VALIDATING_BUILDERS)}: {', '.join(VALIDATING_BUILDERS)})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
