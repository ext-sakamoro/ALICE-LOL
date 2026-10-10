#!/usr/bin/env python3
"""alice-lol/src/runtime_parser.rs の SDF keyword dispatch を独立に再抽出し、
alice-lol/src/syntax_table.rs の `SDF_SYNTAX` と突き合わせる

alice-lol/src/syntax_table.rs は同じ突合せを **Rust の `#[cfg(test)]` として**既に
行っている (`fn_body` / `dispatched` によるテキスト抽出) が、それは test build でしか
実行されず、Python の grid test (`scripts/degenerate_parameter_grid.py`、別途実装) から
再利用できない 本 script は**同じ抽出技法を Python へ独立に移植した**もの
(syntax_table.rs の `#[cfg(test)]` の可視性は変えない、二重の実装を比較することで
どちらか一方の抽出ロジックの誤りも検出できる)

両者の集合が食い違えば fail (差分を印字)、どちらも空でも fail (比較 0 件を green に
しない) 名前を keyword 足す / 消す時に、runtime_parser.rs と syntax_table.rs の片方
だけ更新すると red になる (既存の Rust test と同じ契約を Python 側にも持たせる)

usage: python3 scripts/runtime_keywords.py [--check]
--check: 差分があれば exit 1 (既定は常にこれと同じ、引数なしでも検査する)
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from utf8_stdio import fix_encoding  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
PARSER = ROOT / "alice-lol" / "src" / "runtime_parser.rs"
SYNTAX_TABLE = ROOT / "alice-lol" / "src" / "syntax_table.rs"


def fn_body(src: str, name: str) -> str:
    """`fn <name>(` の本体 (次の同じ字下げの `fn` まで)、syntax_table.rs の
    `#[cfg(test)]` 版 `fn_body` と同じ抽出"""
    head = f"fn {name}("
    start = src.find(head)
    if start < 0:
        raise SystemExit(f"runtime_keywords: runtime_parser.rs に `{head}` が無い")
    rest = src[start:]
    idx = rest[len(head) :].find("\n    fn ")
    end = len(rest) if idx < 0 else idx + len(head)
    return rest[:end]


def dispatched(body: str) -> list[str]:
    """関数本体の、直下の `"name" =>` / `"a" | "b" =>` の腕の名前 (ネストした match
    は字下げが深いので対象外)、syntax_table.rs の `#[cfg(test)]` 版 `dispatched` と同じ"""
    out: list[str] = []
    for line in body.splitlines():
        if not line.startswith('            "'):
            continue
        arm = line[len("            ") :]
        if "=>" not in arm:
            continue
        names_part = arm.split("=>", 1)[0]
        for m in re.finditer(r'"([^"]*)"', names_part):
            out.append(m.group(1))
    return out


def runtime_dispatch_keywords() -> set[str]:
    src = PARSER.read_text(encoding="utf-8")
    return set(dispatched(fn_body(src, "parse_expr_inner")))


def syntax_table_keywords() -> dict[str, set[str]]:
    """`SDF_SYNTAX` の (group, names) tuple 配列を group ごとの集合として読む"""
    src = SYNTAX_TABLE.read_text(encoding="utf-8")
    start = src.find("const SDF_SYNTAX")
    if start < 0:
        raise SystemExit("runtime_keywords: syntax_table.rs に `SDF_SYNTAX` が無い")
    # 配列 literal の終わり (`];` が行頭に来る所) まで
    end_marker = "\n];\n"
    end = src.find(end_marker, start)
    if end < 0:
        raise SystemExit("runtime_keywords: SDF_SYNTAX の終端 `];` が見つからない")
    body = src[start:end]

    groups: dict[str, set[str]] = {}
    for group_match in re.finditer(r'"([a-z_]+)",\s*&\[(.*?)\],\s*\),', body, re.DOTALL):
        group, names_blob = group_match.group(1), group_match.group(2)
        names = set(re.findall(r'"([a-z_0-9]+)"', names_blob))
        groups.setdefault(group, set()).update(names)
    return groups


def main() -> int:
    fix_encoding()

    parser_names = runtime_dispatch_keywords()
    table_groups = syntax_table_keywords()
    table_names: set[str] = set().union(*table_groups.values()) if table_groups else set()

    if not parser_names:
        print("runtime_keywords: runtime_parser.rs の parse_expr_inner から keyword を"
              " 1 件も読めない (比較 0 件)", file=sys.stderr)
        return 2
    if not table_names:
        print("runtime_keywords: syntax_table.rs の SDF_SYNTAX から keyword を"
              " 1 件も読めない (比較 0 件)", file=sys.stderr)
        return 2

    missing = sorted(parser_names - table_names)  # parser にあり表に無い
    extra = sorted(table_names - parser_names)  # 表にあり parser に無い

    for group, names in sorted(table_groups.items()):
        print(f"runtime_keywords: group {group}: {len(names)}")
    print(f"runtime_keywords: parser {len(parser_names)} / table {len(table_names)}")

    if missing or extra:
        print("runtime_keywords: VIOLATIONS (parser と syntax_table.rs の keyword 集合が"
              " 食い違う、片方だけ更新した可能性):")
        if missing:
            print(f"  parser にあり SDF_SYNTAX に無い: {missing}")
        if extra:
            print(f"  SDF_SYNTAX にあり parser に無い: {extra}")
        return 1

    print(f"runtime_keywords: ok (parser と SDF_SYNTAX は {len(parser_names)} 件で一致)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
