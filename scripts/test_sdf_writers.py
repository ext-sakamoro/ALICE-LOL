#!/usr/bin/env python3
"""tests for scripts/sdf_writers.py: the classification of alice-sdf's writers against its own count, the
re-export check over alice-lol's rustdoc JSON, the lock check (a moved pin, a new writer in alice-sdf's count,
other features, a clippy block or canary that is not the lock's), the reachable-path walk, the rustdoc flags
that make hidden and private re-exports visible, and the repository's own files agreeing with the lock

    python3 scripts/test_sdf_writers.py
"""
from __future__ import annotations

import importlib.util
import io
import json
import re
import tempfile
import tomllib
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("sdf_writers", HERE / "sdf_writers.py")
g = importlib.util.module_from_spec(spec)
spec.loader.exec_module(g)

CANDS = {
    "src/io/stl.rs::export_stl": {"path": "alice_sdf::io::export_stl", "call": 'alice_sdf::io::export_stl(todo!(), "x")'},
    "src/io/stl.rs::export_stl_with_stamp": {"path": "alice_sdf::io::export_stl_with_stamp", "call": "c"},
    "src/neural.rs::save": {"path": "alice_sdf::neural::NeuralSdf::save", "call": "c"},
    "src/glsl.rs::export_shader": {"path": "alice_sdf::glsl::Shader::export_shader", "call": "c"},
}
COUNT = ["src/io/stl.rs::export_stl", "src/io/stl.rs::export_stl_with_stamp", "src/volume/export.rs::export_raw"]
OUTSIDE = [
    {"key": "src/neural.rs::save", "kind": "writer", "reason": "r"},
    {"key": "src/glsl.rs::export_shader", "kind": "not-a-writer", "reason": "r"},
    {"key": "src/volume/export.rs::export_raw", "kind": "not-built", "reason": "r"},
]


class Classify(unittest.TestCase):
    def test_both_sources_and_the_outside_entries(self):
        writers, found = g.classify(CANDS, COUNT, OUTSIDE)
        self.assertEqual(found, [])
        self.assertEqual([w["key"] for w in writers], ["src/io/stl.rs::export_stl", "src/neural.rs::save"])

    def test_a_writer_only_in_the_api_is_a_finding(self):
        _, found = g.classify(CANDS, COUNT, OUTSIDE[1:])
        self.assertTrue(any("src/neural.rs::save: in alice-sdf API but not in its writer count" in f for f in found))

    def test_a_new_writer_in_alice_sdfs_count_is_a_finding(self):
        _, found = g.classify(CANDS, COUNT + ["src/io/new.rs::export_new"], OUTSIDE)
        self.assertTrue(any("src/io/new.rs::export_new: counted by alice-sdf" in f for f in found))

    def test_an_outside_entry_both_sources_agree_on_is_stale(self):
        _, found = g.classify(CANDS, COUNT + ["src/neural.rs::save"], OUTSIDE)
        self.assertTrue(any("src/neural.rs::save" in f and "stale" in f for f in found))

    def test_not_built_but_now_in_the_api(self):
        cands = dict(CANDS, **{"src/volume/export.rs::export_raw": {"path": "alice_sdf::volume::export_raw", "call": "c"}})
        # alice-lol turned the feature on: both sources have it now, so the not-built entry is stale
        _, found = g.classify(cands, COUNT, OUTSIDE)
        self.assertTrue(any("src/volume/export.rs::export_raw" in f and "stale" in f for f in found))
        # in the API only (alice-sdf's count dropped it) while still marked not-built
        _, found = g.classify(cands, [k for k in COUNT if "volume" not in k], OUTSIDE)
        self.assertTrue(any("not-built, but alice-sdf's API has it now" in f for f in found))

    def test_an_outside_entry_naming_nothing(self):
        _, found = g.classify(CANDS, COUNT, OUTSIDE + [{"key": "src/x.rs::gone", "kind": "writer", "reason": "r"}])
        self.assertTrue(any("src/x.rs::gone: names nothing" in f for f in found))


def lol_doc(*uses):
    """alice-lol rustdoc JSON with public `use` items (source, target path, kind, is_glob)"""
    paths, index = {}, {}
    for n, (source, path, kind, glob) in enumerate(uses):
        tid = str(1000 + n)
        paths[tid] = {"crate_id": 7 if path[0] == "alice_sdf" else 9, "path": path, "kind": kind}
        index[str(n)] = {"id": n, "crate_id": 0, "visibility": "public",
                         "span": {"filename": "alice-lol/src/x.rs", "begin": [n + 1, 0]},
                         "inner": {"use": {"source": source, "name": path[-1], "id": int(tid), "is_glob": glob}}}
    return {"format_version": 57, "index": index, "paths": paths,
            "external_crates": {"7": {"name": "alice_sdf"}, "9": {"name": "alice_zip"}}}


WRITERS = [{"key": "k1", "path": "alice_sdf::io::export_stl", "canonical": "alice_sdf::io::stl::export_stl", "call": "c"},
           {"key": "k2", "path": "alice_sdf::neural::NeuralSdf::save", "canonical": "alice_sdf::neural::NeuralSdf::save",
            "call": "c"}]
NODE = ("alice_sdf::SdfNode", ["alice_sdf", "SdfNode"], "struct", False)


class Reexports(unittest.TestCase):
    def found(self, *uses):
        return g.reexports(lol_doc(NODE, *uses), WRITERS)[0]

    def test_clean(self):
        out, read, into = g.reexports(lol_doc(NODE, ("alice_sdf::mesh", ["alice_sdf", "mesh"], "module", False),
                                              ("alice_zip::io::export_stl", ["alice_zip", "io", "stl", "export_stl"],
                                               "function", False)), WRITERS)
        self.assertEqual((out, read, into), ([], 3, 2))

    def test_a_writer_under_any_name(self):
        self.assertEqual(len(self.found(("x::write_stl", ["alice_sdf", "io", "stl", "export_stl"], "function", False))), 1)

    def test_the_module_an_ancestor_a_glob_and_the_root(self):
        for use in [("alice_sdf::io::stl", ["alice_sdf", "io", "stl"], "module", False),
                    ("alice_sdf::io", ["alice_sdf", "io"], "module", False),
                    ("alice_sdf::io", ["alice_sdf", "io"], "module", True),
                    ("alice_sdf", ["alice_sdf"], "module", False)]:
            with self.subTest(use=use):
                self.assertEqual(len(self.found(use)), 1)

    def test_a_type_with_a_writer_method(self):
        self.assertIn("writer method", self.found(("alice_sdf::neural::NeuralSdf", ["alice_sdf", "neural", "NeuralSdf"],
                                                   "struct", False))[0])

    def test_a_non_public_use_is_not_read(self):
        doc = lol_doc(NODE, ("alice_sdf::io", ["alice_sdf", "io"], "module", False))
        doc["index"]["1"]["visibility"] = "default"
        self.assertEqual(g.reexports(doc, WRITERS)[:2], ([], 1))


def sdf_doc():
    """alice-sdf rustdoc JSON: root -> pub mod io (private mod stl holds export_stl, re-exported by io) and a
    type with a writer method; an unreachable public fn in a private module"""
    idx = {
        "0": {"name": "alice_sdf", "crate_id": 0, "visibility": "public", "inner": {"module": {"items": [1, 20, 30]}}},
        "1": {"name": "io", "crate_id": 0, "visibility": "public", "inner": {"module": {"items": [2, 5]}}},
        "2": {"name": "stl", "crate_id": 0, "visibility": "default", "inner": {"module": {"items": [3, 4]}}},
        "3": {"name": "export_stl", "crate_id": 0, "visibility": "public", "span": {"filename": "/x/ALICE-SDF/src/io/stl.rs"},
              "inner": {"function": {"sig": {"inputs": [["mesh", {"borrowed_ref": {"type": {"resolved_path": {}}}}],
                                                        ["path", {"impl_trait": [{"trait_bound": {"trait": {"path": "AsRef"}}}]}]]}}}},
        "4": {"name": "export_hidden", "crate_id": 0, "visibility": "public", "span": {"filename": "src/io/stl.rs"},
              "inner": {"function": {"sig": {"inputs": []}}}},
        "5": {"name": None, "crate_id": 0, "visibility": "public", "inner": {"use": {"name": "export_stl", "id": 3, "is_glob": False}}},
        "20": {"name": "Net", "crate_id": 0, "visibility": "public", "inner": {"struct": {}}},
        "21": {"name": None, "crate_id": 0, "visibility": "default", "inner": {"impl": {"trait": None, "for": {"resolved_path": {"id": 20}}, "items": [22]}}},
        "22": {"name": "save", "crate_id": 0, "visibility": "public", "span": {"filename": "src/neural.rs"},
               "inner": {"function": {"sig": {"inputs": [["self", {"borrowed_ref": {"is_mutable": False, "type": {"generic": "Self"}}}],
                                                         ["w", {"borrowed_ref": {"is_mutable": True, "type": {"generic": "W"}}}]]}}}},
        "30": {"name": "helper", "crate_id": 0, "visibility": "public", "inner": {"function": {"sig": {"inputs": []}}}},
    }
    return {"root": 0, "index": idx, "paths": {"3": {"path": ["alice_sdf", "io", "stl", "export_stl"]}}}


class Candidates(unittest.TestCase):
    def test_reachable_paths_calls_and_methods(self):
        cands = g.candidates(sdf_doc(), re.compile(r"(?:export|save|write)\w*|to_bytes"))
        self.assertEqual(cands["src/io/stl.rs::export_stl"],
                         {"path": "alice_sdf::io::export_stl", "canonical": "alice_sdf::io::stl::export_stl",
                          "call": 'alice_sdf::io::export_stl(todo!(), "x")'})
        self.assertEqual(cands["src/neural.rs::save"],
                         {"path": "alice_sdf::Net::save", "canonical": "alice_sdf::Net::save",
                          "call": "alice_sdf::Net::save(todo!(), &mut std::io::sink())"})
        self.assertNotIn("src/io/stl.rs::export_hidden", cands, "public in a private module and not re-exported")


class Lock(unittest.TestCase):
    def setUp(self):
        self.writers = [{"key": "k", "path": "alice_sdf::io::export_stl", "canonical": "alice_sdf::io::stl::export_stl",
                         "call": "alice_sdf::io::export_stl(x)"}]
        self.lock = tomllib.loads(g.render_lock("a" * 40, ["glsl"], COUNT, self.writers))
        self.clippy = "x\n" + g.render_clippy_block(self.writers) + "\n"
        self.canary = "fn a() {}\n" + g.render_canary_block(self.writers) + "\n"

    def check(self, lock=None, count=COUNT, pin="a" * 40, clippy=None, canary=None, feats=("glsl",)):
        with mock.patch.object(g, "sdf_features", return_value=list(feats)):
            return g.check_lock({}, lock or self.lock, count, pin, clippy or self.clippy, canary or self.canary)

    def test_in_step(self):
        self.assertEqual(self.check(), [])

    def test_a_moved_pin(self):
        self.assertTrue(any("pins bbbbbbbbbbbb" in f for f in self.check(pin="b" * 40)))

    def test_a_new_writer_in_alice_sdfs_count(self):
        self.assertTrue(any("new ['src/io/new.rs::export_new']" in f for f in self.check(count=COUNT + ["src/io/new.rs::export_new"])))

    def test_other_features(self):
        self.assertTrue(any("features" in f for f in self.check(feats=("glsl", "hlsl"))))

    def test_a_clippy_block_that_is_not_the_locks(self):
        self.assertTrue(any("clippy_lib_gate.toml" in f for f in self.check(clippy=self.clippy.replace("export_stl", "export_ply", 1))))

    def test_a_canary_that_is_not_the_locks(self):
        self.assertTrue(any("clippy-lib-canary" in f for f in self.check(canary=self.canary.replace("export_stl(x)", "export_stl(y)"))))


class Rustdoc(unittest.TestCase):
    def test_hidden_items_and_for_alice_lol_private_items_are_documented(self):
        calls = []

        def run(cmd, **kw):
            calls.append(cmd)
            m = mock.Mock(returncode=0, stdout=json.dumps({"target_directory": "/nonexistent"}), stderr="")
            return m
        with mock.patch.object(g.subprocess, "run", side_effect=run), \
                mock.patch.object(g.Path, "read_text", return_value=json.dumps({"format_version": 57})):
            g.rustdoc("alice-lol", "nightly-x", [])
            g.rustdoc("alice-sdf", "nightly-x", [])
        self.assertIn("--document-hidden-items", calls[0])
        self.assertIn("--document-private-items", calls[0])
        self.assertIn("--document-hidden-items", calls[2])

    def test_an_unknown_format_version_is_refused(self):
        def run(cmd, **kw):
            return mock.Mock(returncode=0, stdout=json.dumps({"target_directory": "/nonexistent"}), stderr="")
        with mock.patch.object(g.subprocess, "run", side_effect=run), \
                mock.patch.object(g.Path, "read_text", return_value=json.dumps({"format_version": 3})):
            with self.assertRaises(ValueError):
                g.rustdoc("alice-lol", "nightly-x", [])


class Main(unittest.TestCase):
    def run_main(self, doc):
        with tempfile.TemporaryDirectory() as t:
            p = Path(t) / "lol.json"
            p.write_text(json.dumps(doc))
            out = io.StringIO()
            with redirect_stdout(out), redirect_stderr(out), \
                    mock.patch.object(g, "check_lock", return_value=[]), \
                    mock.patch.object(g, "sdf_count", return_value=COUNT), mock.patch.object(g, "sdf_pin", return_value="p"):
                return g.main(["--lol-json", str(p)]), out.getvalue()

    def test_a_reexport_finding_exits_1(self):
        self.assertEqual(self.run_main(lol_doc(NODE, ("alice_sdf::io", ["alice_sdf", "io"], "module", False)))[0], 1)

    def test_nothing_into_alice_sdf_is_compared_nothing(self):
        self.assertEqual(self.run_main(lol_doc(("z", ["alice_zip", "z"], "struct", False)))[0], 2)

    def test_no_public_use_is_compared_nothing(self):
        self.assertEqual(self.run_main(lol_doc())[0], 2)


class Repository(unittest.TestCase):
    def test_the_config_loads_and_the_lock_has_writers(self):
        cfg = g.load_config()
        self.assertTrue(cfg["nightly"].startswith("nightly-"))
        lock = tomllib.loads(g.LOCK.read_text(encoding="utf-8"))
        self.assertGreater(len(lock["writer"]), 0)
        self.assertFalse(any(g.STAMPED.search(w["key"]) for w in lock["writer"]))

    def test_a_reexport_is_matched_by_the_path_rustdoc_records_for_the_lock_writers(self):
        # alice-lol's rustdoc names a re-export's target by where it is defined; a lock that matched by the
        # callable path (alice_sdf::io::export_stl) instead of alice_sdf::io::stl::export_stl let every one pass
        lock = tomllib.loads(g.LOCK.read_text(encoding="utf-8"))
        stl = next(w for w in lock["writer"] if w["key"] == "src/io/stl.rs::export_stl")
        self.assertEqual(stl["canonical"], "alice_sdf::io::stl::export_stl")
        out, _, _ = g.reexports(lol_doc(NODE, ("x::write_stl", stl["canonical"].split("::"), "function", False)),
                                lock["writer"])
        self.assertEqual(len(out), 1)

    def test_the_generated_blocks_are_the_locks(self):
        lock = tomllib.loads(g.LOCK.read_text(encoding="utf-8"))
        self.assertEqual(g.block_of(g.CLIPPY_DATA.read_text(encoding="utf-8"), g.BEGIN, g.END),
                         g.render_clippy_block(lock["writer"]))
        self.assertEqual(g.block_of(g.CANARY_SRC.read_text(encoding="utf-8"), g.RS_BEGIN, g.RS_END),
                         g.render_canary_block(lock["writer"]))
        self.assertEqual(lock["sdf-revision"], g.sdf_pin())


if __name__ == "__main__":
    unittest.main()
