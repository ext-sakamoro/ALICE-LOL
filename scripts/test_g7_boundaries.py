#!/usr/bin/env python3
"""Oracle of scripts/g7_boundaries.py and scripts/g7_macro_pairing.py (the checks' own tests, no cargo)."""
from __future__ import annotations

import importlib.util
import re
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent


def load(name: str):
    spec = importlib.util.spec_from_file_location(name, HERE / f"{name}.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


g7 = load("g7_boundaries")
pairing = load("g7_macro_pairing")
STRIP = re.compile(r'//[^\n]*|/\*.*?\*/|(?<![\w])b?r(#*)".*?"\1|"(?:\\.|[^"\\])*"', re.S)

STR = {"borrowed_ref": {"is_mutable": False, "type": {"primitive": "str"}}}
U8S = {"borrowed_ref": {"is_mutable": False, "type": {"slice": {"primitive": "u8"}}}}
F32 = {"primitive": "f32"}


def fn(inputs, generics=None):
    return {"function": {"sig": {"inputs": inputs}, "generics": generics or {"params": [], "where_predicates": []}}}


def doc():
    """a crate root with module `m`: fn takes_str(&str), fn takes_f32(f32), fn reader<R: Read>(R),
    a private fn(&str), struct S with method name(&self, &str) and a FromStr-like trait impl"""
    index = {
        "0": {"crate_id": 0, "name": "c", "visibility": "public", "inner": {"module": {"items": [1]}}},
        "1": {"crate_id": 0, "name": "m", "visibility": "public", "inner": {"module": {"items": [2, 3, 4, 5, 6]}}},
        "2": {"crate_id": 0, "name": "takes_str", "visibility": "public", "inner": fn([["s", STR]])},
        "3": {"crate_id": 0, "name": "takes_f32", "visibility": "public", "inner": fn([["x", F32]])},
        "4": {"crate_id": 0, "name": "reader", "visibility": "public", "inner": fn(
            [["r", {"generic": "R"}]],
            {"params": [{"name": "R", "kind": {"type": {"bounds": [
                {"trait_bound": {"trait": {"path": "Read", "id": 90, "args": None}}}]}}}],
             "where_predicates": []})},
        "5": {"crate_id": 0, "name": "hidden", "visibility": "crate", "inner": fn([["s", STR]])},
        "6": {"crate_id": 0, "name": "S", "visibility": "public", "inner": {"struct": {}}},
        "7": {"crate_id": 0, "name": None, "visibility": "default", "inner": {"impl": {
            "for": {"resolved_path": {"path": "S", "id": 6}}, "trait": None, "items": [8, 9],
            "is_synthetic": False, "blanket_impl": None}}},
        "8": {"crate_id": 0, "name": "name", "visibility": "public", "inner": fn([["self", {}], ["n", STR]])},
        "9": {"crate_id": 0, "name": "private_name", "visibility": "crate", "inner": fn([["n", STR]])},
        "10": {"crate_id": 0, "name": None, "visibility": "default", "inner": {"impl": {
            "for": {"resolved_path": {"path": "S", "id": 6}},
            "trait": {"path": "FromStr", "id": 91, "args": None}, "items": [11],
            "is_synthetic": False, "blanket_impl": None}}},
        "11": {"crate_id": 0, "name": "from_str", "visibility": "default", "inner": fn([["s", STR]])},
        "12": {"crate_id": 0, "name": None, "visibility": "default", "inner": {"impl": {
            "for": {"resolved_path": {"path": "S", "id": 6}},
            "trait": {"path": "Borrow", "id": 92, "args": None}, "items": [13],
            "is_synthetic": False, "blanket_impl": {"generic": "T"}}}},
        "13": {"crate_id": 0, "name": "borrow", "visibility": "default", "inner": fn([["b", U8S]])},
    }
    return {"root": 0, "index": index, "paths": {}}


def reach(d):
    sw = load("sdf_writers")
    return sw.reachable_paths(d)


class EntryPoints(unittest.TestCase):
    def found(self):
        d = doc()
        return g7.entry_points(d, reach(d))

    def test_text_bytes_and_reader_parameters_are_found(self):
        found = self.found()
        self.assertEqual(set(found), {"c::m::takes_str", "c::m::reader", "c::m::S::name",
                                      "<c::m::S as FromStr>::from_str"})
        self.assertIn("R: Read", found["c::m::reader"])

    def test_a_private_function_a_number_and_a_blanket_impl_are_not(self):
        found = self.found()
        for path in ("c::m::takes_f32", "c::m::hidden", "c::m::S::private_name", "<c::m::S as Borrow>::borrow"):
            self.assertNotIn(path, found)

    def test_type_rendering(self):
        self.assertEqual(g7.type_text(U8S, {}), "&[u8]")
        vec_u8 = {"resolved_path": {"path": "Vec", "id": 1, "args": {"angle_bracketed": {
            "args": [{"type": {"primitive": "u8"}}], "constraints": []}}}}
        self.assertTrue(g7.ERASED.search(g7.type_text(vec_u8, {})))
        imp = {"impl_trait": [{"trait_bound": {"trait": {"path": "AsRef", "id": 2, "args": {"angle_bracketed": {
            "args": [{"type": {"resolved_path": {"path": "std::path::Path", "id": 3, "args": None}}}],
            "constraints": []}}}}}]}
        self.assertEqual(g7.type_text(imp, {}), "impl AsRef<std::path::Path>")
        self.assertTrue(g7.ERASED.search(g7.type_text(imp, {})))
        self.assertFalse(g7.ERASED.search(g7.type_text({"primitive": "f64"}, {})))


class Table(unittest.TestCase):
    FOUND = {"c::a": "s: &str", "c::b": "s: &str"}

    def rows(self, **over):
        rows = [{"path": "c::a", "class": "local", "reason": "a name"},
                {"path": "c::b", "class": "authoring", "reason": "source"}]
        for r in rows:
            r.update(over.get(r["path"], {}))
        return rows

    def test_a_complete_table_has_no_finding(self):
        self.assertEqual(g7.judge_entries(self.FOUND, self.rows()), [])

    def test_an_unlisted_entry_point_is_a_finding(self):
        found = dict(self.FOUND, **{"c::new": "s: &str"})
        out = g7.judge_entries(found, self.rows())
        self.assertEqual(len(out), 1)
        self.assertIn("c::new", out[0])

    def test_a_stale_row_is_a_finding(self):
        out = g7.judge_entries({"c::a": "s: &str"}, self.rows())
        self.assertEqual(len(out), 1)
        self.assertIn("no longer", out[0])

    def test_a_row_without_class_or_reason_is_a_finding(self):
        self.assertTrue(g7.judge_entries(self.FOUND, self.rows(**{"c::a": {"class": "maybe"}})))
        self.assertTrue(g7.judge_entries(self.FOUND, self.rows(**{"c::a": {"reason": ""}})))

    def test_an_exchange_row_names_an_existing_test(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "t.rs").write_text("#[test]\nfn mismatch_named() {}\n", encoding="utf-8")
            ok = self.rows(**{"c::b": {"class": "exchange", "test": "t.rs::mismatch_named"}})
            self.assertEqual(g7.judge_entries(self.FOUND, ok, root), [])
            for test in ("t.rs::absent", "missing.rs::mismatch_named", ""):
                bad = self.rows(**{"c::b": {"class": "exchange", "test": test}})
                self.assertEqual(len(g7.judge_entries(self.FOUND, bad, root)), 1, test)


class Forms(unittest.TestCase):
    def findings(self, code: str) -> list[str]:
        return g7.form_findings({"src/x.rs": code}, STRIP)

    def test_each_form_is_found(self):
        for code in ('extern "C" { fn f(); }', 'pub extern "C" fn f() {}', 'extern "system" fn f() {}',
                     "#[no_mangle]\npub fn f() {}", '#[unsafe(no_mangle)]\nfn f() {}',
                     '#[export_name = "g"]\nfn f() {}', "#[wasm_bindgen]\npub fn f() {}",
                     "#[pyfunction]\nfn f() {}", "use pyo3::prelude::*;", "fn f(x: &dyn Any) {}",
                     "fn f(x: Box<dyn std::any::Any>) {}", "x.downcast_ref::<u8>()"):
            self.assertEqual(len(self.findings(code)), 1, code)

    def test_comments_and_string_literals_are_not_code(self):
        for code in ('// extern "C" and #[no_mangle]', "/* dyn Any */", 'let s = "no_mangle downcast";',
                     "/// wasm_bindgen in a doc", 'fn f() -> &str { "dyn Any" }'):
            self.assertEqual(self.findings(code), [], code)

    def test_the_line_is_reported(self):
        out = self.findings('fn a() {}\n// x\nextern "C" { fn f(); }\n')
        self.assertTrue(out[0].startswith("src/x.rs:3:"), out)


class Targets(unittest.TestCase):
    def meta(self, macro_kinds=("proc-macro",), lol_kinds=("lib",)):
        return {"packages": [
            {"name": "alice-lol-macro", "targets": [{"kind": list(macro_kinds)}]},
            {"name": "alice-lol", "targets": [{"kind": list(lol_kinds)}]},
            {"name": "other", "targets": [{"kind": ["custom-build"]}]},
        ]}

    def table(self, root: Path):
        (root / "c.rs").write_text("", encoding="utf-8")
        (root / "p.py").write_text("", encoding="utf-8")
        return {"macro": [{"crate": "alice-lol-macro", "compared_in": "c.rs", "paired_by": "p.py"}]}

    def test_the_listed_proc_macro_and_a_lib_pass(self):
        with tempfile.TemporaryDirectory() as tmp:
            out, seen = g7.target_findings(self.meta(), self.table(Path(tmp)), Path(tmp))
            self.assertEqual((out, seen), ([], 2))

    def test_a_build_script_or_a_foreign_crate_type_is_a_finding(self):
        with tempfile.TemporaryDirectory() as tmp:
            for kinds in (("lib", "custom-build"), ("cdylib",), ("staticlib",)):
                out, _ = g7.target_findings(self.meta(lol_kinds=kinds), self.table(Path(tmp)), Path(tmp))
                self.assertEqual(len(out), 1, kinds)

    def test_an_unlisted_proc_macro_is_a_finding(self):
        with tempfile.TemporaryDirectory() as tmp:
            out, _ = g7.target_findings(self.meta(lol_kinds=("proc-macro",)), self.table(Path(tmp)), Path(tmp))
            self.assertEqual(len(out), 1)
            out, _ = g7.target_findings(self.meta(), {"macro": []}, Path(tmp))
            self.assertEqual(len(out), 1)

    def test_a_listed_file_that_is_missing_is_a_finding(self):
        with tempfile.TemporaryDirectory() as tmp:
            t = {"macro": [{"crate": "alice-lol-macro", "compared_in": "nope.rs", "paired_by": "nope.py"}]}
            out, _ = g7.target_findings(self.meta(), t, Path(tmp))
            self.assertEqual(len(out), 2)


class Pairing(unittest.TestCase):
    def test_the_identifier_patterns_read_the_sources(self):
        root = HERE.parent
        ids = pairing.read_ids()
        self.assertIsNotNone(ids, "EXPANSION_ID_HEX or EXPECTED_MACRO_EXPANSION_ID not read")
        self.assertEqual(ids[0], ids[1])
        self.assertTrue((root / "alice-lol-macro" / "src" / "expansion_id.rs").is_file())

    def test_the_identifier_substitution_changes_only_the_value(self):
        text = 'pub const EXPANSION_ID_HEX: &str =\n    "' + "a" * 64 + '";\n'
        out = pairing.ID_RE.sub('pub const EXPANSION_ID_HEX: &str = "' + "b" * 64 + '";', text)
        self.assertEqual(pairing.ID_RE.findall(out), ["b" * 64])

    def test_extra_arguments_exit_2(self):
        self.assertEqual(pairing.main(["--x"]), 2)


if __name__ == "__main__":
    sys.exit(0 if unittest.main(exit=False).result.wasSuccessful() else 1)
