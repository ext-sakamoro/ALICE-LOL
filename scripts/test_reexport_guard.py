#!/usr/bin/env python3
"""tests for scripts/reexport_guard.py over synthetic rustdoc JSON: a re-export of an unstamped alice-sdf writer
is a finding under any name, as a module, a glob, the crate root or an ancestor module; a stamped writer, another
alice-sdf module and a local item are not; nothing read or nothing into the rule's crate exits 2; an unknown
format version exits 2; the repository's configuration loads

    python3 scripts/test_reexport_guard.py
"""
from __future__ import annotations

import importlib.util
import io
import json
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("reexport_guard", HERE / "reexport_guard.py")
g = importlib.util.module_from_spec(spec)
spec.loader.exec_module(g)

RULES = [{"crate": "alice_sdf", "module": ["alice_sdf", "io"], "function": "^export_", "allow": "_with_stamp$",
          "reason": "unstamped"}]
TARGETS = {
    "stl": (["alice_sdf", "io", "stl", "export_stl"], "function"),
    "stl_stamped": (["alice_sdf", "io", "stl", "export_stl_with_stamp"], "function"),
    "obj": (["alice_sdf", "io", "obj", "export_obj"], "function"),
    "io": (["alice_sdf", "io"], "module"),
    "io_stl": (["alice_sdf", "io", "stl"], "module"),
    "root": (["alice_sdf"], "module"),
    "mesh": (["alice_sdf", "mesh"], "module"),
    "node": (["alice_sdf", "SdfNode"], "struct"),
    "other_crate_fn": (["alice_zip", "io", "export_stl"], "function"),
    "reader": (["alice_sdf", "io", "stl", "import_stl"], "function"),
}


def doc(*uses, version=57):
    """a rustdoc JSON with one public `use` item per (source, target key or None for a local item, is_glob)"""
    paths, index = {}, {}
    for i, (key, (path, kind)) in enumerate(TARGETS.items()):
        paths[str(1000 + i)] = {"crate_id": 9 if path[0] == "alice_zip" else 7, "path": path, "kind": kind}
    ids = {key: 1000 + i for i, key in enumerate(TARGETS)}
    for n, (source, key, glob) in enumerate(uses):
        index[str(n)] = {"id": n, "crate_id": 0, "visibility": "public",
                         "span": {"filename": "alice-lol/src/x.rs", "begin": [n + 1, 0]},
                         "inner": {"use": {"source": source, "name": source.split("::")[-1],
                                           "id": ids[key] if key else 500 + n, "is_glob": glob}}}
    index["99"] = {"id": 99, "crate_id": 0, "visibility": "default", "inner": {"use": {
        "source": "alice_sdf::io::export_stl", "name": "export_stl", "id": ids["stl"], "is_glob": False}}}
    return {"format_version": version, "index": index, "paths": paths,
            "external_crates": {"7": {"name": "alice_sdf"}, "9": {"name": "alice_zip"}}}


ALWAYS = ("alice_sdf::SdfNode", "node", False)


class Findings(unittest.TestCase):
    def found(self, *uses):
        return g.findings(doc(ALWAYS, *uses), RULES)[0]

    def test_a_clean_api_passes(self):
        out, read, into = g.findings(doc(ALWAYS, ("alice_sdf::io::export_stl_with_stamp", "stl_stamped", False),
                                         ("alice_sdf::mesh", "mesh", False), ("crate::x::Local", None, False)), RULES)
        self.assertEqual((out, read, into), ([], 4, {"alice_sdf": 3}))

    def test_an_unstamped_writer_under_its_own_name(self):
        self.assertEqual(len(self.found(("alice_sdf::io::export_stl", "stl", False))), 1)

    def test_an_unstamped_writer_renamed_or_through_a_local_module(self):
        self.assertEqual(len(self.found(("alice_sdf::io::export_stl", "stl", False),
                                        ("inner::write_stl", "stl", False))), 2)

    def test_any_export_function_of_the_module_not_only_the_four(self):
        self.assertIn("export_obj", self.found(("alice_sdf::io::export_obj", "obj", False))[0])

    def test_the_module_a_submodule_a_glob_and_the_crate_root(self):
        for use in [("alice_sdf::io", "io", False), ("alice_sdf::io::stl", "io_stl", False),
                    ("alice_sdf::io", "io", True), ("alice_sdf", "root", False)]:
            with self.subTest(use=use):
                self.assertEqual(len(self.found(use)), 1)

    def test_another_crate_with_the_same_path_shape_is_not_the_rule(self):
        out, _, into = g.findings(doc(ALWAYS, ("alice_zip::io::export_stl", "other_crate_fn", False)), RULES)
        self.assertEqual((out, into), ([], {"alice_sdf": 1}))

    def test_a_function_of_the_module_that_is_not_a_writer_passes(self):
        self.assertEqual(self.found(("alice_sdf::io::import_stl", "reader", False)), [])

    def test_a_non_public_use_is_not_read(self):
        _, read, _ = g.findings(doc(ALWAYS), RULES)
        self.assertEqual(read, 1)

    def test_an_unknown_format_version_is_refused(self):
        with self.assertRaises(ValueError):
            g.findings(doc(ALWAYS, version=12), RULES)


class Main(unittest.TestCase):
    def run_main(self, d):
        with tempfile.TemporaryDirectory() as t:
            p = Path(t) / "x.json"
            p.write_text(json.dumps(d))
            out = io.StringIO()
            with redirect_stdout(out), redirect_stderr(out):
                rc = g.main(["--json", str(p)])
        return rc, out.getvalue()

    def test_a_finding_exits_1(self):
        self.assertEqual(self.run_main(doc(ALWAYS, ("alice_sdf::io", "io", False)))[0], 1)

    def test_clean_exits_0(self):
        self.assertEqual(self.run_main(doc(ALWAYS))[0], 0)

    def test_no_public_use_is_compared_nothing(self):
        d = doc()
        self.assertEqual(self.run_main(d)[0], 2)

    def test_no_use_into_the_rule_crate_is_compared_nothing(self):
        self.assertEqual(self.run_main(doc(("crate::x::Local", None, False)))[0], 2)

    def test_a_missing_json_exits_2(self):
        out = io.StringIO()
        with redirect_stdout(out), redirect_stderr(out):
            self.assertEqual(g.main(["--json", "/nonexistent/alice_lol.json"]), 2)

    def test_the_repository_configuration_loads(self):
        cfg = g.load_config()
        self.assertTrue(cfg["nightly"].startswith("nightly-"))
        self.assertEqual(cfg["forbid"][0]["module"], ["alice_sdf", "io"])


if __name__ == "__main__":
    unittest.main()
