#!/usr/bin/env python3
"""`-(x.mul_add(y, z))` (結果全体を negate する fma) を src/ 全体で禁止する.

実測 (2026-10-12、`rustc 1.98.1`、LLVM 22.1.8、`aarch64-apple-darwin`): この形は
`-C opt-level=0` と `opt-level>=1` で signed zero の結果が割れる LLVM が
`fneg(fma(x,y,z)) -> fma(x,-y,-z)` という `nsz` (no-signed-zeros) 専用の sink を、
Rust の既定 (nsz を立てない) codegen に対しても適用するため、cancellation
(`x*y+z` が厳密に 0 に丸まる入力) で debug と release の bit が一致しない

`mul_add` 自体は禁止しない (1 回の丸めが規定された厳密な演算、
`scripts/det_math_guard.py` も deterministic 側として扱う) 禁止するのは
**呼び出し結果全体を negate する形だけ** 引数を negate してから渡す形
(`a.mul_add(b, -c)`、FMSUB 相当) やネストした引数内の negate は対象外
(詳細 memory [[feedback_fma_negation_debug_release_divergence]] /
[[feedback_fneg_of_fma_signed_zero_debug_release_split]])

⚠️ **全部を禁止しない**: 既知の許容は `scripts/no-outer-fneg-of-fma-allowlist.txt` に
`<file>:<line>` + 理由で記載 (`(-a).mul_add(b, -c)` 形への書き換えが Law の検討対象に
なるまでの一時許容、または mesh 抽出の中間量で persisted rotate と無関係と判定済みの箇所)

⚠️ 検査対象の file が 1 つも読めなければ fail する (path が壊れて空振りするのを防ぐ)
同じ理由で、スキャンした `.mul_add(` 呼び出しが 0 件でも fail する
(0 件は file 一覧か正規表現が壊れた合図、0 違反と空振りを区別する)

Usage: `python3 scripts/no_outer_fneg_of_fma.py` (exit 1 on any finding)
`--root DIR` で別の tree を検査する
"""

from __future__ import annotations

import os
import sys

ALLOWLIST = "scripts/no-outer-fneg-of-fma-allowlist.txt"
SRC_DIRS = ("alice-lol/src",)
IDENT_CHARS = set("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_.[]")


def read_allowlist(root: str) -> set[str]:
    path = os.path.join(root, ALLOWLIST)
    keys: set[str] = set()
    if not os.path.exists(path):
        return keys
    with open(path, encoding="utf-8") as f:
        for raw in f:
            line = raw.split("#", 1)[0].strip()
            if line:
                keys.add(line)
    return keys


def strip_comments_and_strings(text: str) -> str:
    """行 comment / 文字列 literal を空白に潰す (括弧の深さ計算を壊さないため)."""
    out = []
    i = 0
    n = len(text)
    while i < n:
        c = text[i]
        if c == "/" and i + 1 < n and text[i + 1] == "/":
            while i < n and text[i] != "\n":
                out.append(" ")
                i += 1
            continue
        if c == '"':
            out.append(" ")
            i += 1
            while i < n and text[i] != '"':
                if text[i] == "\\" and i + 1 < n:
                    out.append("  ")
                    i += 2
                    continue
                out.append(" ")
                i += 1
            if i < n:
                out.append(" ")
                i += 1
            continue
        if c == "'" and i + 2 < n and text[i + 2] == "'":
            # char literal 'x' (lifetime の ' と区別、3 文字固定の形のみ)
            out.append("   ")
            i += 3
            continue
        out.append(c)
        i += 1
    return "".join(out)


def line_of(text: str, pos: int) -> int:
    return text.count("\n", 0, pos) + 1


def scan_file(path: str, rel: str) -> tuple[list[tuple[int, str]], int]:
    """(違反 (行, 抜粋) の一覧, スキャンした mul_add 呼び出し数)."""
    with open(path, encoding="utf-8") as f:
        raw = f.read()
    text = strip_comments_and_strings(raw)
    mul_add_calls = text.count(".mul_add(")
    violations: list[tuple[int, str]] = []
    n = len(text)
    i = 0
    while i < n:
        if text[i] != "-":
            i += 1
            continue
        # unary context かどうか: 直前の非空白文字が識別子/数字/閉じ括弧でない
        j = i - 1
        while j >= 0 and text[j] in " \t\n":
            j -= 1
        if j >= 0 and (text[j].isalnum() or text[j] in "_)]"):
            i += 1
            continue  # 二項減算
        k = i + 1
        while k < n and text[k] in " \t\n":
            k += 1
        if k >= n or text[k] != "(":
            i += 1
            continue
        # 対応する閉じ括弧を探す
        depth = 1
        p = k + 1
        while p < n and depth > 0:
            if text[p] == "(":
                depth += 1
            elif text[p] == ")":
                depth -= 1
            p += 1
        if depth != 0:
            i += 1
            continue  # 壊れた入力、この `-(` は諦める
        inner = text[k + 1 : p - 1].strip()
        # inner 全体が「識別子/パス.mul_add(...)」で、mul_add の呼び出しが
        # inner の末尾ちょうどで閉じているか (= 他に何も付いていないか)
        idx = inner.find(".mul_add(")
        if idx > 0 and all(ch in IDENT_CHARS for ch in inner[:idx]):
            depth2 = 1
            q = idx + len(".mul_add(")
            while q < len(inner) and depth2 > 0:
                if inner[q] == "(":
                    depth2 += 1
                elif inner[q] == ")":
                    depth2 -= 1
                q += 1
            if depth2 == 0 and q == len(inner):
                ln = line_of(text, i)
                excerpt = raw.split("\n")[ln - 1].strip()
                violations.append((ln, excerpt))
        i = k + 1
    return violations, mul_add_calls


def scan(root: str) -> tuple[list[str], int, int]:
    found: list[str] = []
    files = 0
    calls = 0
    for src_dir in SRC_DIRS:
        base = os.path.join(root, src_dir)
        if not os.path.isdir(base):
            continue
        for dirpath, _dirnames, filenames in os.walk(base):
            for name in filenames:
                if not name.endswith(".rs"):
                    continue
                path = os.path.join(dirpath, name)
                rel = os.path.relpath(path, root)
                files += 1
                violations, mul_add_calls = scan_file(path, rel)
                calls += mul_add_calls
                for ln, excerpt in violations:
                    found.append(f"{rel}:{ln}: {excerpt}")
    return found, files, calls


def check(root: str) -> tuple[list[str], dict[str, int]]:
    errors: list[str] = []
    found, files, calls = scan(root)
    allowlist = read_allowlist(root)

    if files == 0:
        errors.append(
            f"error: src file を 1 つも読めなかった — 検査が空振りしている (対象: {', '.join(SRC_DIRS)})"
        )
        return errors, {"files": files, "calls": calls, "violations": len(found)}
    if calls == 0:
        errors.append(f"error: .mul_add( 呼び出しが 0 件 — file 一覧か scan が壊れている (読めた file {files} 件)")

    for entry in found:
        key = entry.split(": ", 1)[0]
        if key not in allowlist:
            errors.append(f"error: {entry}  (呼び出し結果全体を negate する fma、禁止形)")

    for key in sorted(allowlist - {e.split(": ", 1)[0] for e in found}):
        errors.append(f"error: {key}: allowlist にあるが実物が無い — 行を消す")

    return errors, {"files": files, "calls": calls, "violations": len(found)}


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
    print(f"compared: files {counts['files']}, mul_add calls {counts['calls']}, outer-fneg sites {counts['violations']}")
    if errors:
        print(f"no-outer-fneg-of-fma: {len(errors)} violation(s)")
        return 1
    print("no-outer-fneg-of-fma: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
