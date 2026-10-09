#!/usr/bin/env python3
"""判定経路に platform 依存の超越関数が入るのを止める (ラチェット).

Law は「何が成立すべきか」を述べるので、同じ Law に同じ実測を与えたら**どの機械でも
同じ判定**が出なければならない ところが `f64::sin` 等の超越関数は IEEE 754 が値を
規定しておらず、実装 (platform の libm) ごとに最後の 1 bit が違いうる 判定の分岐が
その値に依ると、同じ入力で `Proven` と `Undecided` が割れる

そこで判定を出す file だけを対象に、**実装依存の関数の直呼び**を検査する
決定論が要る計算は `alice_det_math` の関数 (bit 一致を契約にしている) を通す

⚠️ **全部を禁止しない** IEEE 754 が正しい丸めを要求する演算は platform に依らない:

  * `sqrt`    — 正しい丸めが要求されている (実装依存ではない)
  * `mul_add` — 1 回の丸めで計算することが規定されている
  * `powi`    — 乗算の繰り返しで、乗算自体は正しく丸められる
  * `abs` / `floor` / `ceil` / `round` / `trunc` / `signum` / `min` / `max` — 厳密

既存分は `scripts/det-math-baseline.txt` に理由付きで置く 新規の直呼びは fail、
baseline にあるのに実物が消えていても fail (解消したら行を消す)

⚠️ 検査対象の file が 1 つも読めなければ fail する (path を変えて空振りするのを防ぐ)
同じ理由で、検査した呼び出しが 0 件でも fail する (判定 file は数値計算を含むので、
0 件は file 一覧か正規表現が壊れた合図)

Usage: `python3 scripts/det_math_guard.py` (exit 1 on any finding)
`--root DIR` で別の tree を検査する (scripts/test_det_math_guard.py が使う)
"""

from __future__ import annotations

import os
import re
import sys

# 判定を出す file 判定経路を足したらここに足す
JUDGMENT_FILES = (
    "alice-lol/src/law.rs",
    "alice-lol/src/audit_law.rs",
    "alice-lol/src/research_law.rs",
)
BASELINE = "scripts/det-math-baseline.txt"

# platform の libm 実装ごとに値が違いうる関数 (IEEE 754 が値を規定していない)
IMPL_DEFINED = (
    "sin", "cos", "tan", "asin", "acos", "atan", "atan2",
    "sinh", "cosh", "tanh", "asinh", "acosh", "atanh",
    "exp", "exp2", "exp_m1", "ln", "ln_1p", "log", "log2", "log10",
    "powf", "cbrt", "hypot",
)
# 値が規定されている / 厳密な演算 (検査するが落とさない、件数の母数になる)
EXACT = (
    "sqrt", "mul_add", "powi", "abs", "floor", "ceil", "round", "trunc",
    "signum", "min", "max", "recip",
)
CALL_RE = re.compile(r"\.(" + "|".join(IMPL_DEFINED + EXACT) + r")\s*\(")
FN_RE = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)")
LINE_COMMENT_RE = re.compile(r"//.*$")


def strip_comment(line: str) -> str:
    """行 comment を落とす (doc comment の中の `.sin()` を拾わないため)."""
    return LINE_COMMENT_RE.sub("", line)


def read_baseline(root: str) -> set[str]:
    path = os.path.join(root, BASELINE)
    keys: set[str] = set()
    if not os.path.exists(path):
        return keys
    with open(path, encoding="utf-8") as f:
        for raw in f:
            line = raw.split("#", 1)[0].strip()
            if line:
                keys.add(line)
    return keys


def scan(root: str) -> tuple[list[str], int, int]:
    """(実装依存の呼び出しの key 一覧, 読めた file 数, 検査した呼び出し数)."""
    found: list[str] = []
    files = 0
    calls = 0
    for rel in JUDGMENT_FILES:
        path = os.path.join(root, rel)
        if not os.path.exists(path):
            continue
        with open(path, encoding="utf-8") as f:
            text = f.read()
        files += 1
        fn = "(top level)"
        for line in text.split("\n"):
            code = strip_comment(line)
            m = FN_RE.search(code)
            if m:
                fn = m.group(1)
            for call in CALL_RE.finditer(code):
                calls += 1
                if call.group(1) in IMPL_DEFINED:
                    found.append(f"{rel}::{fn}::{call.group(1)}")
    return found, files, calls


def check(root: str) -> tuple[list[str], dict[str, int]]:
    errors: list[str] = []
    found, files, calls = scan(root)
    baseline = read_baseline(root)

    if files == 0:
        errors.append(
            "error: 判定 file を 1 つも読めなかった — 検査が空振りしている "
            f"(対象: {', '.join(JUDGMENT_FILES)})"
        )
        return errors, {"files": files, "calls": calls, "impl_defined": len(found)}
    if calls == 0:
        errors.append(
            "error: 検査した呼び出しが 0 件 — file 一覧か正規表現が壊れている "
            f"(読めた file {files} 件)"
        )

    for key in sorted(set(found)):
        if key not in baseline:
            errors.append(
                f"error: {key}: platform 依存の関数を判定経路で直呼びしている "
                "(決定論が要るなら alice_det_math を通す、要らないなら baseline に理由付きで足す)"
            )
    for key in sorted(baseline - set(found)):
        errors.append(f"error: {key}: baseline にあるが実物が無い — 行を消す")

    return errors, {"files": files, "calls": calls, "impl_defined": len(set(found))}


def main() -> int:
    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(encoding="utf-8")
        except (AttributeError, ValueError):
            pass
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    if "--root" in sys.argv:
        root = sys.argv[sys.argv.index("--root") + 1]
    errors, counts = check(root)
    for e in errors:
        print(e)
    print(
        f"compared: judgment files {counts['files']}, calls {counts['calls']}, "
        f"implementation-defined {counts['impl_defined']}"
    )
    if errors:
        print(f"det-math-guard: {len(errors)} violation(s)")
        return 1
    print("det-math-guard: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
