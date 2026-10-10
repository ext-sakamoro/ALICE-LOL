#!/usr/bin/env python3
"""Windows の既定 stdout/stderr (cp1252) で日本語の print が落ちるのを防ぐ、共有 1 行 helper

Windows の `python` は既定で stdout/stderr を cp1252 (locale 既定) で開く CI 検査器が
日本語の message を print すると `UnicodeEncodeError` になる (PYTHONIOENCODING=cp1252 で
Mac/Linux でも再現できる) 検査器ごとに同じ 6 行を複製せず、ここから import する

usage:
    from utf8_stdio import fix_encoding
    fix_encoding()  # print より前、main() の先頭で呼ぶ
"""
from __future__ import annotations

import sys


def fix_encoding() -> None:
    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(encoding="utf-8")
        except (AttributeError, ValueError):
            pass
