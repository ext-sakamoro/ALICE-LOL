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
        self.clippy = g.render_writers_config(self.writers)
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

    def test_a_writers_config_missing_a_writer(self):
        found = self.check(clippy=g.render_writers_config([]))
        self.assertTrue(any("clippy-writers" in f and "missing ['alice_sdf::io::export_stl']" in f for f in found), found)

    def test_a_writers_config_with_an_extra_entry(self):
        extra = self.clippy.replace("]\n", '    { path = "alice_sdf::io::extra", reason = "x" },\n]\n')
        self.assertTrue(any("extra ['alice_sdf::io::extra']" in f for f in self.check(clippy=extra)))

    def test_a_canary_that_is_not_the_locks(self):
        self.assertTrue(any("clippy-lib-canary" in f for f in self.check(canary=self.canary.replace("export_stl(x)", "export_stl(y)"))))


SRC = (
    "pub fn node_to_obj_roblox(a: u8) {\n"   # line 1
    '    let s = "}";\n'                     # line 2: a brace in a string does not close the fn
    "    export_obj(a);\n"                  # line 3
    "}\n"                                   # line 4
    "pub fn other() {\n"                    # line 5
    "    export_obj(1);\n"                  # line 6
    "}\n"
)
OPEN = [{"path": "alice_sdf::io::export_obj", "file": "alice-lol/src/roblox_export.rs", "function": "node_to_obj_roblox"}]


def diag(file, line, path):
    return {"reason": "compiler-message", "message": {"code": {"code": "clippy::disallowed_methods"},
                                                     "message": f"use of a disallowed method `{path}`",
                                                     "spans": [{"is_primary": True, "file_name": file, "line_start": line}]}}


def artifact(name, fresh=False):
    return {"reason": "compiler-artifact", "package_id": f"path+file:///x/{name}#0.1.0", "target": {"name": name.replace("-", "_")},
            "fresh": fresh}


class SecondPass(unittest.TestCase):
    def judge(self, msgs):
        return g.judge(msgs, {"alice-lol"}, OPEN, read=lambda p: SRC)

    def test_the_open_call_once_and_a_fresh_crate(self):
        self.assertEqual(self.judge([artifact("alice-lol"), diag("alice-lol/src/roblox_export.rs", 3, "alice_sdf::io::export_obj")]),
                         ([], 1, 1))

    def test_any_other_report_is_a_finding(self):
        found, _, _ = self.judge([artifact("alice-lol"), diag("alice-lol/src/roblox_export.rs", 3, "alice_sdf::io::export_obj"),
                                  diag("alice-lol/src/print_export.rs", 9, "alice_sdf::io::export_stl")])
        self.assertTrue(any("print_export.rs:9" in f for f in found), found)

    def test_the_open_call_outside_its_function_is_a_finding(self):
        found, _, _ = self.judge([diag("alice-lol/src/roblox_export.rs", 3, "alice_sdf::io::export_obj"),
                                  diag("alice-lol/src/roblox_export.rs", 6, "alice_sdf::io::export_obj")])
        self.assertTrue(any("roblox_export.rs:6" in f for f in found), found)

    def test_another_writer_inside_the_open_calls_function_is_a_finding(self):
        found, _, _ = self.judge([diag("alice-lol/src/roblox_export.rs", 3, "alice_sdf::io::export_obj"),
                                  diag("alice-lol/src/roblox_export.rs", 3, "alice_sdf::io::export_stl")])
        self.assertTrue(any("export_stl" in f for f in found), found)

    def test_the_open_call_twice_is_a_finding(self):
        found, _, _ = self.judge([diag("alice-lol/src/roblox_export.rs", 3, "alice_sdf::io::export_obj")] * 2)
        self.assertTrue(any("seen 2 time(s)" in f for f in found), found)

    def test_the_open_call_missing_is_a_finding(self):
        found, _, _ = self.judge([artifact("alice-lol")])
        self.assertTrue(any("seen 0 time(s)" in f for f in found), found)

    def test_a_cached_crate_does_not_count_as_linted(self):
        self.assertEqual(self.judge([artifact("alice-lol", fresh=True)])[1], 0)

    def test_an_unexpected_cfg_is_a_finding(self):
        m = diag("alice-lol/src/x.rs", 7, "x")
        m["message"]["code"]["code"] = "unexpected_cfgs"
        m["message"]["message"] = "unexpected `cfg` condition value: `nonexistent`"
        found, _, _ = self.judge([m, diag("alice-lol/src/roblox_export.rs", 3, "alice_sdf::io::export_obj")])
        self.assertTrue(any("x.rs:7" in f and "does not declare" in f for f in found), found)

    def test_another_lint_is_not_read(self):
        m = diag("alice-lol/src/x.rs", 1, "x")
        m["message"]["code"]["code"] = "clippy::max"
        self.assertEqual(self.judge([m, diag("alice-lol/src/roblox_export.rs", 3, "alice_sdf::io::export_obj")])[0], [])

    def test_fn_lines_skips_braces_in_strings(self):
        self.assertEqual(g.fn_lines(SRC, "node_to_obj_roblox"), (1, 4))


class CfgCoverage(unittest.TestCase):
    DEFAULTS = {"alice-lol/": {"glsl"}}

    def found(self, src, rel="alice-lol/src/x.rs"):
        return g.cfg_findings({rel: src}, self.DEFAULTS)

    def test_features_on_test_and_non_default_features_off_pass(self):
        src = ('#[cfg(feature = "roblox")]\nfn a() {}\n#[cfg(test)]\nmod t {}\n#[cfg(any(test, feature = "x"))]\nfn b() {}\n'
               '#[cfg(not(feature = "roblox"))]\nfn c() {}\n#[cfg(not(test))]\nfn d() {}\n'
               '#[cfg_attr(feature = "x", derive(Debug))]\nstruct S;\nfn e() -> bool { cfg!(feature = "y") }\n')
        self.assertEqual(self.found(src), ([], 7))

    def test_what_the_second_pass_never_compiles(self):
        for pred, word in [("not(clippy)", "not(clippy)"), ("clippy", "clippy"), ('target_os = "windows"', "target_os=windows"),
                           ('target_arch = "wasm32"', "target_arch=wasm32"), ("windows", "windows"), ("unix", "unix"),
                           ("not(debug_assertions)", "not(debug_assertions)"), ("my_custom_cfg", "my_custom_cfg"),
                           ('all(feature = "a", not(feature = "b"))', "both on and under not"),
                           ('not(feature = "glsl")', 'a default feature')]:
            with self.subTest(pred=pred):
                out, n = self.found(f"#[cfg({pred})]\nfn w() {{}}\n")
                self.assertEqual(n, 1)
                self.assertTrue(out and word in out[0], out)

    def test_cfg_attr_and_cfg_macro_and_inner_attributes(self):
        for src in ['#[cfg_attr(windows, path = "x.rs")]\nmod m;\n', "fn f() -> bool { cfg!(clippy) }\n",
                    "#![cfg(target_os = \"linux\")]\n"]:
            with self.subTest(src=src):
                self.assertEqual(len(self.found(src)[0]), 1)

    def test_comments_and_strings_are_not_read(self):
        self.assertEqual(self.found('// #[cfg(windows)]\n/// #[cfg(not(clippy))]\nconst S: &str = "#[cfg(unix)]";\n'), ([], 0))

    def test_the_default_of_another_member_does_not_apply(self):
        self.assertEqual(self.found('#[cfg(not(feature = "glsl"))]\nfn w() {}\n', rel="alice-lol-humanoid/src/x.rs")[0], [])


class DepInfo(unittest.TestCase):
    def test_the_files_rustc_read_including_include_and_path(self):
        with tempfile.TemporaryDirectory() as t:
            t = Path(t)
            (t / "deps").mkdir()
            rmeta = t / "deps" / "libalice_lol-abc.rmeta"
            rmeta.write_text("")
            src = g.ROOT / "alice-lol" / "src" / "lib.rs"
            hidden = g.ROOT / "scripts" / "sdf_writers.py"   # any workspace file stands for one pulled in by include!
            outside = g.ROOT.parent / "ALICE-SDF" / "src" / "lib.rs"
            (t / "deps" / "alice_lol-abc.d").write_text(
                f"{rmeta}: {src} {outside} {hidden}\n\n{src}:\n")
            msgs = [{"reason": "compiler-artifact", "fresh": False, "package_id": "path+file:///x/alice-lol#0.5.0",
                     "filenames": [str(rmeta)]}]
            files, units = g.dep_info_files(msgs, {"alice-lol"})
        self.assertEqual(units, 1)
        self.assertEqual(sorted(files), ["alice-lol/src/lib.rs"], "only .rs files of the workspace")

    def test_a_missing_dep_info_is_an_error(self):
        msgs = [{"reason": "compiler-artifact", "fresh": False, "package_id": "path+file:///x/alice-lol#0.5.0",
                 "filenames": [str(g.ROOT / "target" / "nowhere" / "libalice_lol-zzz.rmeta")]}]
        with self.assertRaises(ValueError):
            g.dep_info_files(msgs, {"alice-lol"})

    def test_cached_and_foreign_units_are_not_read(self):
        msgs = [{"reason": "compiler-artifact", "fresh": True, "package_id": "path+file:///x/alice-lol#0.5.0", "filenames": ["/x"]},
                {"reason": "compiler-artifact", "fresh": False, "package_id": "registry+https://x#glam@0.29", "filenames": ["/y"]}]
        self.assertEqual(g.dep_info_files(msgs, {"alice-lol"}), ({}, 0))


class SecondPassWiring(unittest.TestCase):
    def test_the_check_runs_the_second_pass(self):
        with mock.patch.object(g, "check_lock", return_value=[]), mock.patch.object(g, "sdf_count", return_value=COUNT), \
                mock.patch.object(g, "sdf_pin", return_value="p"), \
                mock.patch.object(g, "rustdoc", return_value=lol_doc(NODE)), \
                mock.patch.object(g, "second_pass", return_value=["x: a writer call"]) as sp, \
                redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
            rc = g.main([])
        sp.assert_called_once()
        self.assertEqual(rc, 1)

    def test_a_second_pass_that_linted_nothing_afresh_exits_2(self):
        cached = [artifact("alice-lol", fresh=True)]
        meta = json.dumps({"packages": [{"id": "a", "name": "alice-lol", "manifest_path": str(g.ROOT / "Cargo.toml")}],
                           "workspace_members": ["a"]})
        with mock.patch.object(g, "clippy_json", return_value=cached), mock.patch.object(g, "clean"), \
                mock.patch.object(g, "dep_info_files", return_value=({"a.rs": "#[cfg(test)] mod t {}"}, 1)), \
                mock.patch.object(g.subprocess, "run", return_value=mock.Mock(stdout=meta, returncode=0)):
            with self.assertRaises(ValueError) as e:
                g.second_pass({"open": []}, {"writer": []})
        self.assertIn("linted no workspace crate afresh", str(e.exception))

    def test_both_legs_run_and_the_default_leg_needs_a_fresh_crate(self):
        meta = json.dumps({"packages": [{"id": "a", "name": "alice-lol", "manifest_path": str(g.ROOT / "Cargo.toml")}],
                           "workspace_members": ["a"]})
        calls = []

        def cj(args, target, manifest=g.ROOT / "Cargo.toml"):
            calls.append(args)
            return [artifact("alice-lol")] if len(calls) == 1 else [artifact("alice-lol", fresh=True)]
        with mock.patch.object(g, "clippy_json", side_effect=cj), mock.patch.object(g, "clean"), \
                mock.patch.object(g, "dep_info_files", return_value=({"a.rs": "#[cfg(test)] mod t {}"}, 1)), \
                mock.patch.object(g.subprocess, "run", return_value=mock.Mock(stdout=meta, returncode=0)):
            with self.assertRaises(ValueError) as e:
                g.second_pass({"open": []}, {"writer": []})
        self.assertIn("default-feature leg", str(e.exception))
        self.assertIn("--all-features", calls[0])
        self.assertIn("--examples", calls[0])
        self.assertNotIn("--all-features", calls[1])
        self.assertNotIn("--no-default-features", calls[1])

    def test_no_cfg_read_in_the_library_sources_is_compared_nothing(self):
        meta = json.dumps({"packages": [{"id": "a", "name": "alice-lol", "manifest_path": str(g.ROOT / "Cargo.toml")}],
                           "workspace_members": ["a"]})
        with mock.patch.object(g, "clippy_json", return_value=[artifact("alice-lol")]), mock.patch.object(g, "clean"), \
                mock.patch.object(g, "dep_info_files", return_value=({"a.rs": "fn f() {}"}, 1)), \
                mock.patch.object(g.subprocess, "run", return_value=mock.Mock(stdout=meta, returncode=0)):
            with self.assertRaises(ValueError) as e:
                g.second_pass({"open": []}, {"writer": []})
        self.assertIn("no cfg predicate or no dep-info read", str(e.exception))

    def test_the_second_pass_cleans_the_members_first(self):
        meta = json.dumps({"packages": [{"id": "a", "name": "alice-lol", "manifest_path": str(g.ROOT / "Cargo.toml")}],
                           "workspace_members": ["a"]})
        with mock.patch.object(g, "clippy_json", return_value=[]), mock.patch.object(g, "clean") as cl, \
                mock.patch.object(g, "dep_info_files", return_value=({"a.rs": "#[cfg(test)] mod t {}"}, 1)), \
                mock.patch.object(g.subprocess, "run", return_value=mock.Mock(stdout=meta, returncode=0)):
            with self.assertRaises(ValueError):
                g.second_pass({"open": []}, {"writer": []})
        self.assertEqual(cl.call_args_list[0].args[0], ["alice-lol"])

    def test_the_second_pass_forces_the_lint_on(self):
        with mock.patch.object(g.subprocess, "run", return_value=mock.Mock(stdout="", returncode=0)) as run:
            g.clippy_json(["-p", "x"], g.ROOT / "target")
        cmd = run.call_args.args[0]
        self.assertEqual(cmd[-4:], ["--force-warn", "clippy::disallowed_methods", "--force-warn", "unexpected_cfgs"])
        self.assertEqual(run.call_args.kwargs["env"]["CLIPPY_CONF_DIR"], str(g.WRITERS_CONF_DIR))


class Canary(unittest.TestCase):
    W = [{"path": "alice_sdf::io::export_stl", "call": "alice_sdf::io::export_stl(x)"},
         {"path": "alice_sdf::io::save", "call": "alice_sdf::io::save(y)"}]
    TEXT = "pub fn f() {\n    let _ = alice_sdf::io::export_stl(x);\n    let _ = alice_sdf::io::save(y);\n}\n"

    def test_each_writer_reported_once_by_its_path(self):
        self.assertEqual(g.canary_findings([diag("src/lib.rs", 2, "alice_sdf::io::export_stl"),
                                            diag("src/lib.rs", 3, "alice_sdf::io::save")], self.W, self.TEXT), [])

    def test_a_writer_not_reported(self):
        out = g.canary_findings([diag("src/lib.rs", 2, "alice_sdf::io::export_stl")], self.W, self.TEXT)
        self.assertEqual(out, ["canary line 3: alice_sdf::io::save not reported by the second pass"])

    def test_a_report_by_another_path(self):
        out = g.canary_findings([diag("src/lib.rs", 2, "alice_sdf::io::export_stl"), diag("src/lib.rs", 3, "alice_sdf::io::other")],
                                self.W, self.TEXT)
        self.assertTrue(any("line 3" in o for o in out), out)

    def test_an_unexpected_report(self):
        out = g.canary_findings([diag("src/lib.rs", 2, "alice_sdf::io::export_stl"), diag("src/lib.rs", 3, "alice_sdf::io::save"),
                                 diag("src/lib.rs", 1, "alice_sdf::io::save")], self.W, self.TEXT)
        self.assertTrue(any("unexpected" in o for o in out), out)


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

    def test_the_generated_files_are_the_locks(self):
        lock = tomllib.loads(g.LOCK.read_text(encoding="utf-8"))
        self.assertEqual(g.WRITERS_CONFIG.read_text(encoding="utf-8"), g.render_writers_config(lock["writer"]))
        # the first pass (the library-only configuration) does not hold the writers
        self.assertNotIn("alice_sdf::", (g.ROOT / ".github" / "clippy-lib" / "clippy.toml").read_text(encoding="utf-8"))
        self.assertEqual(g.block_of(g.CANARY_SRC.read_text(encoding="utf-8"), g.RS_BEGIN, g.RS_END),
                         g.render_canary_block(lock["writer"]))
        self.assertEqual(lock["sdf-revision"], g.sdf_pin())


if __name__ == "__main__":
    unittest.main()
