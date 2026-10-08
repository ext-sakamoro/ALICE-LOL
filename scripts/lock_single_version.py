#!/usr/bin/env python3
"""版ガード: 同じ `alice-*` crate が `Cargo.lock` に 2 版以上入っているのを止める検査器.

semver 非互換な要求 (`^0.3` と `^0.4` 等) が混在すると、cargo はどちらも正当な解決として
1 つの build に両方を link する 法則を評価する crate と法則を保存する crate が別の版の
算術 crate を引くと、同じ法則が 2 通りに評価されるが、片方だけを相手にした bit 一致試験は
green のまま通るので差が出口に現れない third-party の重複 (`syn` / `thiserror` 等) は
対象にしない (上流の都合で日常的に起きるので、ここで止めると歯が鈍る)

検査 A (duplicate): `alice-` で始まる package が 2 版以上あれば fail
    既知の分は baseline の行に `<crate 名> <版> <版> …` を書く (版は昇順)
検査 B (stale): baseline の行が解消済 (1 版だけになった / 版の組が変わった) なら fail
    ラチェットなので、解消したら行を消す 版が動いたら行を書き替える
検査 C (nothing compared): 読めた package が 0 件なら fail
    lock を読めなかった時に「重複 0 件」として green になるのを防ぐ

usage: scripts/lock_single_version.py [--lock <path>] [--baseline <path>]
"""

from __future__ import annotations

import argparse
import os
import re
import sys

PREFIX = "alice-"
PACKAGE_RE = re.compile(r"^name = \"(?P<name>[^\"]+)\"\s*$\n^version = \"(?P<version>[^\"]+)\"\s*$", re.M)


def read_text(path: str) -> str | None:
    """file を UTF-8 として読む 読めなければ None.

    `open(..., "r")` の既定 encoding は platform の locale なので、Windows では
    UTF-8 の file が読めずに例外になる bytes で読んで明示的に decode する
    """
    try:
        with open(path, "rb") as f:
            data = f.read()
    except OSError:
        return None
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError:
        return None


def version_key(v: str) -> tuple:
    """版を数値として比べる key 数値でない部分は文字列のまま後ろに置く."""
    parts: list = []
    for chunk in re.split(r"[.\-+]", v):
        parts.append((0, int(chunk), "") if chunk.isdigit() else (1, 0, chunk))
    return tuple(parts)


def packages(lock_text: str) -> dict[str, list[str]]:
    """`Cargo.lock` の package 名 -> 版の一覧 (昇順、重複は畳む)."""
    found: dict[str, set[str]] = {}
    for m in PACKAGE_RE.finditer(lock_text):
        found.setdefault(m.group("name"), set()).add(m.group("version"))
    return {k: sorted(v, key=version_key) for k, v in found.items()}


def parse_baseline(text: str) -> dict[str, list[str]]:
    """baseline の `<crate 名> <版> <版> …` を名前 -> 版の一覧 (昇順) に."""
    out: dict[str, list[str]] = {}
    for line in text.splitlines():
        line = line.split("#", 1)[0].strip()
        if not line:
            continue
        name, *vers = line.split()
        out[name] = sorted(vers, key=version_key)
    return out


def check(lock_path: str, baseline_path: str) -> tuple[list[str], dict[str, int]]:
    errors: list[str] = []
    counts = {"packages": 0, "alice packages": 0, "duplicates": 0, "baseline rows": 0}

    lock_text = read_text(lock_path)
    if lock_text is None:
        errors.append(f"{lock_path}: UTF-8 の text として読めない")
        return errors, counts

    pkgs = packages(lock_text)
    counts["packages"] = len(pkgs)
    alice = {k: v for k, v in pkgs.items() if k.startswith(PREFIX)}
    counts["alice packages"] = len(alice)
    dup = {k: v for k, v in alice.items() if len(v) > 1}
    counts["duplicates"] = len(dup)

    baseline_text = read_text(baseline_path)
    if baseline_text is None:
        baseline: dict[str, list[str]] = {}
        if os.path.exists(baseline_path):
            errors.append(f"{baseline_path}: UTF-8 の text として読めない")
    else:
        baseline = parse_baseline(baseline_text)
    counts["baseline rows"] = len(baseline)

    # 検査 A: baseline に無い重複
    for name, vers in sorted(dup.items()):
        want = baseline.get(name)
        if want is None:
            errors.append(
                f"{name}: {len(vers)} 版が同じ build に入っている ({' '.join(vers)}) "
                f"— 要求を 1 つの版に揃えるか、既知として {os.path.basename(baseline_path)} に "
                f"`{name} {' '.join(vers)}` を足す"
            )
        elif want != vers:
            errors.append(
                f"{name}: 版の組が baseline と違う (lock {' '.join(vers)} / "
                f"baseline {' '.join(want)}) — baseline の行を書き替える"
            )

    # 検査 B: 解消済なのに残っている baseline の行 (ラチェット)
    for name in sorted(baseline):
        if name not in dup:
            vers = alice.get(name) or pkgs.get(name)
            state = f"{len(vers)} 版 ({' '.join(vers)})" if vers else "lock に無い"
            errors.append(f"{name}: baseline に行があるが解消済 ({state}) — 行を消す")

    # 検査 C: 1 件も比べていないなら fail
    for key in ("packages", "alice packages"):
        if counts[key] == 0:
            errors.append(f"検査 `{key}` が 1 件も比べていない")

    return errors, counts


def main(argv: list[str] | None = None) -> int:
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--lock", default=os.path.join(root, "Cargo.lock"))
    ap.add_argument("--baseline", default=os.path.join(root, "scripts", "lock-duplicates-baseline.txt"))
    a = ap.parse_args(argv)

    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(encoding="utf-8")
        except (AttributeError, ValueError):
            pass

    errors, counts = check(a.lock, a.baseline)
    print("compared: " + ", ".join(f"{k} {v}" for k, v in counts.items()))
    for e in errors:
        print(f"error: {e}", file=sys.stderr)
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
