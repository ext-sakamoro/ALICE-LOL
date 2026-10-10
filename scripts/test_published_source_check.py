#!/usr/bin/env python3
"""scripts/published_source_check.py の試験.

小さな workspace (架空の crate `sample-core` / `sample-app`) と、crates.io の応答の代わりの
offline index を一時 directory に作り、1 箇所だけ崩して検査器の出口を確かめる
"""
from __future__ import annotations

import io
import json
import os
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import published_source_check as psc  # noqa: E402

LIB = b"pub fn f() -> u32 { 1 }\n"


def crate(name: str, version: str, files: dict[str, bytes]) -> bytes:
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w:gz") as t:
        for rel, data in files.items():
            info = tarfile.TarInfo(f"{name}-{version}/{rel}")
            info.size = len(data)
            t.addfile(info, io.BytesIO(data))
    return buf.getvalue()


class Workspace:
    def __init__(self, d: Path, req: str = "0.2.1", local_lib: bytes = LIB, with_dep: bool = True):
        self.root, self.index = d / "ws", d / "index"
        (self.root / "core" / "src").mkdir(parents=True)
        (self.root / "app" / "src").mkdir(parents=True)
        self.index.mkdir()
        (self.root / "Cargo.toml").write_text('[workspace]\nmembers = ["core", "app"]\n', encoding="utf-8")
        (self.root / "core" / "Cargo.toml").write_text('[package]\nname = "sample-core"\nversion = "0.2.1"\n', encoding="utf-8")
        (self.root / "core" / "src" / "lib.rs").write_bytes(local_lib)
        dep = f'sample-core = {{ path = "../core", version = "{req}" }}\n' if with_dep else ""
        (self.root / "app" / "Cargo.toml").write_text(
            f'[package]\nname = "sample-app"\nversion = "0.1.0"\n\n[dependencies]\n{dep}', encoding="utf-8")
        (self.root / "app" / "src" / "lib.rs").write_bytes(b"")

    def publish(self, versions: list[str], lib: bytes = LIB, yanked: tuple[str, ...] = ()) -> None:
        (self.index / "sample-core.json").write_text(
            json.dumps({"versions": [{"num": v, "yanked": v in yanked} for v in versions]}), encoding="utf-8")
        for v in versions:
            (self.index / f"sample-core-{v}.crate").write_bytes(crate("sample-core", v, {"src/lib.rs": lib, "Cargo.toml": b"x"}))

    def run(self) -> int:
        return psc.main(["--root", str(self.root), "--offline-index", str(self.index)])


class PublishedSource(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.d = Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def test_the_same_source_as_the_published_version_passes(self):
        ws = Workspace(self.d)
        ws.publish(["0.2.0", "0.2.1"])
        self.assertEqual(ws.run(), 0)

    def test_a_source_changed_under_a_published_version_fails(self):
        ws = Workspace(self.d, local_lib=b"pub fn f() -> u32 { 2 }\n")
        ws.publish(["0.2.1"])
        self.assertEqual(ws.run(), 1)

    def test_a_new_file_under_a_published_version_fails(self):
        ws = Workspace(self.d)
        (ws.root / "core" / "src" / "extra.rs").write_bytes(b"")
        ws.publish(["0.2.1"])
        self.assertEqual(ws.run(), 1)

    def test_the_requirement_resolves_to_the_newest_matching_version(self):
        # `0.2.0` admits 0.2.1 (caret), so the 0.2.1 package is the one compared
        ws = Workspace(self.d, req="0.2.0")
        ws.publish(["0.2.0", "0.2.1"])
        (ws.index / "sample-core-0.2.0.crate").write_bytes(crate("sample-core", "0.2.0", {"src/lib.rs": b"old"}))
        self.assertEqual(ws.run(), 0)

    def test_a_yanked_version_is_not_what_the_requirement_resolves_to(self):
        ws = Workspace(self.d, req="0.2.0")
        ws.publish(["0.2.0", "0.2.1"], yanked=("0.2.1",))
        (ws.index / "sample-core-0.2.0.crate").write_bytes(crate("sample-core", "0.2.0", {"src/lib.rs": b"old"}))
        self.assertEqual(ws.run(), 1)

    def test_a_version_not_yet_published_is_pending(self):
        ws = Workspace(self.d)
        ws.publish(["0.2.0"])  # 0.2.0 is not ^0.2.1
        self.assertEqual(ws.run(), 0)

    def test_no_dependency_between_members_fails(self):
        ws = Workspace(self.d, with_dep=False)
        ws.publish(["0.2.1"])
        self.assertEqual(ws.run(), 2)

    def test_caret(self):
        m = psc.caret_matches
        self.assertTrue(m("0.2.1", "0.2.5"))
        self.assertFalse(m("0.2.1", "0.3.0"))
        self.assertFalse(m("0.2.1", "0.2.0"))
        self.assertTrue(m("0.4", "0.4.9"))
        self.assertFalse(m("0.0.3", "0.0.4"))
        self.assertTrue(m("5.1", "5.9.0"))
        self.assertFalse(m("5.1", "6.0.0"))
        self.assertFalse(m("0.2.1", "0.2.2-beta.1"))


if __name__ == "__main__":
    unittest.main()
