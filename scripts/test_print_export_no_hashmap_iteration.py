#!/usr/bin/env python3
"""Tests for scripts/print_export_no_hashmap_iteration.py.

`Evasions` each plants one of the forms the FAIL review found the previous
(declaration-shape-matching) version of this checker missed, as a self
-contained `print_export.rs`-shaped fragment, and asserts it is still
rejected under the current (type-closed + name-closed) design. `Control`
is the inverse: a well-behaved fragment (and a rename of it) must come out
clean or rejected exactly as expected, so the test suite is not only a list
of things that must go red."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import print_export_no_hashmap_iteration as h  # noqa: E402

IMPORT_LINE = "use std::collections::{HashMap, HashSet};\n"


def wrap(body: str) -> str:
    """A minimal file shape: the real import line, then `body` verbatim."""
    return IMPORT_LINE + body


REAL_SHAPE = wrap(
    "fn weld_coincident_vertices(raw: &Mesh) -> Mesh {\n"
    "    let mut id: HashMap<[u32; 3], usize> = HashMap::with_capacity(raw.vertices.len());\n"
    "    for v in &raw.vertices {\n"
    "        let next = id.len();\n"
    "        let slot = *id\n"
    "            .entry(v.position.to_array().map(f32::to_bits))\n"
    "            .or_insert(next);\n"
    "    }\n"
    "}\n"
    "\n"
    "fn euler_characteristic(m: &Mesh) -> i64 {\n"
    "    let mut edges: HashSet<(u32, u32)> = HashSet::with_capacity(m.indices.len());\n"
    "    edges.insert((1, 2));\n"
    "    let e = edges.len();\n"
    "}\n"
)


class Control(unittest.TestCase):
    def test_the_real_shape_is_clean(self):
        names, violations = h.scan(REAL_SHAPE)
        self.assertEqual(names, ["edges", "id"])
        self.assertEqual(violations, [])

    def test_prose_mentioning_the_english_word_edges_in_a_comment_is_not_a_hit(self):
        src = wrap(
            "/// Marching Cubes leaves 6177 non-manifold edges on thin walls.\n"
        ) + REAL_SHAPE[len(IMPORT_LINE) :]
        names, violations = h.scan(src)
        self.assertEqual(names, ["edges", "id"])
        self.assertEqual(violations, [])

    def test_renaming_the_pinned_local_is_a_violation(self):
        """Even an equally well-behaved rename must fail: the check is
        pinned to the two specific names, not to "whichever local holds the
        map" -- widening it is a conscious edit, not a side effect of a
        rename."""
        renamed = REAL_SHAPE.replace("id", "vertex_ids")
        names, violations = h.scan(renamed)
        self.assertNotIn("vertex_ids", names)
        self.assertTrue(
            any("vertex_ids" in v or "outside the use import" in v for v in violations)
        )
        self.assertTrue(violations)


class Evasions(unittest.TestCase):
    """Each of these reproduces one of the 9 forms the FAIL review listed as
    slipping past the previous version's `let name: HashMap<` matching."""

    def test_untyped_declaration_with_for_in(self):
        src = wrap(
            "fn weld_coincident_vertices(raw: &Mesh) -> Mesh {\n"
            "    let mut id = HashMap::<[u32; 3], usize>::new();\n"
            "    for (k, v) in id.iter() {\n"
            "        println!(\"{k:?} {v}\");\n"
            "    }\n"
            "}\n"
        )
        _names, violations = h.scan(src)
        self.assertTrue(violations, "untyped `HashMap::<..>::new()` must still be caught")

    def test_full_path_type(self):
        src = wrap(
            "fn weld_coincident_vertices(raw: &Mesh) -> Mesh {\n"
            "    let mut id: std::collections::HashMap<[u32; 3], usize> =\n"
            "        std::collections::HashMap::new();\n"
            "    let n = id.len();\n"
            "}\n"
        )
        _names, violations = h.scan(src)
        self.assertTrue(violations, "a full-path HashMap type must still be caught")

    def test_helper_fn_taking_a_hashmap_reference_parameter(self):
        src = wrap(
            "fn weld_coincident_vertices(raw: &Mesh) -> Mesh {\n"
            "    let mut id: HashMap<[u32; 3], usize> = HashMap::with_capacity(8);\n"
            "    let n = id.len();\n"
            "    helper(&id);\n"
            "}\n"
            "\n"
            "fn helper(m: &HashMap<[u32; 3], usize>) -> usize {\n"
            "    m.len()\n"
            "}\n"
        )
        _names, violations = h.scan(src)
        self.assertTrue(violations, "a &HashMap<...> parameter type must still be caught")

    def test_alias_by_reference(self):
        src = wrap(
            "fn weld_coincident_vertices(raw: &Mesh) -> Mesh {\n"
            "    let mut id: HashMap<[u32; 3], usize> = HashMap::with_capacity(8);\n"
            "    let n = id.len();\n"
            "    let a = &id;\n"
            "}\n"
        )
        _names, violations = h.scan(src)
        self.assertTrue(violations, "`let a = &id;` must still be caught")

    def test_multi_line_for_loop(self):
        src = wrap(
            "fn weld_coincident_vertices(raw: &Mesh) -> Mesh {\n"
            "    let mut id: HashMap<[u32; 3], usize> = HashMap::with_capacity(8);\n"
            "    let n = id.len();\n"
            "    for (k, v)\n"
            "        in\n"
            "        &id\n"
            "    {\n"
            "        println!(\"{k:?} {v}\");\n"
            "    }\n"
            "}\n"
        )
        _names, violations = h.scan(src)
        self.assertTrue(violations, "a for-loop split across lines must still be caught")

    def test_intoiterator_into_iter_called_by_path(self):
        src = wrap(
            "fn weld_coincident_vertices(raw: &Mesh) -> Mesh {\n"
            "    let mut id: HashMap<[u32; 3], usize> = HashMap::with_capacity(8);\n"
            "    let n = id.len();\n"
            "    let it = IntoIterator::into_iter(&id);\n"
            "}\n"
        )
        _names, violations = h.scan(src)
        self.assertTrue(
            violations, "IntoIterator::into_iter(&id) (bypassing `.into_iter()`) must still be caught"
        )

    def test_extend_consuming_a_reference(self):
        src = wrap(
            "fn weld_coincident_vertices(raw: &Mesh) -> Mesh {\n"
            "    let mut id: HashMap<[u32; 3], usize> = HashMap::with_capacity(8);\n"
            "    let n = id.len();\n"
            "    let mut v: Vec<([u32; 3], usize)> = Vec::new();\n"
            "    v.extend(&id);\n"
            "}\n"
        )
        _names, violations = h.scan(src)
        self.assertTrue(violations, "`v.extend(&id)` must still be caught")

    def test_type_alias(self):
        src = wrap(
            "type MapT = HashMap<[u32; 3], usize>;\n"
            "\n"
            "fn weld_coincident_vertices(raw: &Mesh) -> Mesh {\n"
            "    let mut id: MapT = MapT::new();\n"
            "    let n = id.len();\n"
            "}\n"
        )
        _names, violations = h.scan(src)
        self.assertTrue(violations, "a type alias hiding HashMap must still be caught")

    def test_use_as_renamed_import(self):
        """`use ... as Hm;` is a textually different line from the one pinned
        import, so it fails rule 1 on its own; `Hm` is then never tracked by
        rule 2 either -- both facts matter, this test only needs the net
        effect (a violation) to hold."""
        src = (
            "use std::collections::HashMap as Hm;\n"
            "\n"
            "fn f() {\n"
            "    let m: Hm<u32, u32> = Hm::new();\n"
            "    for _ in &m {}\n"
            "}\n"
        )
        _names, violations = h.scan(src)
        self.assertTrue(violations, "`use ... as Hm;` must still be caught")

    def test_a_second_use_statement_naming_hashmap(self):
        src = wrap("use std::collections::HashMap;\n") + REAL_SHAPE[len(IMPORT_LINE) :]
        _names, violations = h.scan(src)
        self.assertTrue(violations, "a second `use` naming HashMap must still be caught")

    def test_struct_field(self):
        src = wrap(
            "struct Wrapper {\n"
            "    id: HashMap<[u32; 3], usize>,\n"
            "}\n"
            "\n"
            "fn weld_coincident_vertices(w: &Wrapper) {\n"
            "    for (k, v) in w.id.iter() {\n"
            "        println!(\"{k:?} {v}\");\n"
            "    }\n"
            "}\n"
        )
        _names, violations = h.scan(src)
        self.assertTrue(violations, "a HashMap struct field must still be caught")


class ZeroDeclarations(unittest.TestCase):
    def test_zero_pinned_declarations_is_reported_as_zero_names(self):
        names, violations = h.scan(wrap("fn f() {}\n"))
        self.assertEqual(names, [])
        self.assertEqual(violations, [])


class Main(unittest.TestCase):
    def test_real_file_passes_and_declares_both_pinned_locals(self):
        self.assertEqual(h.main([]), 0)

    def test_missing_target_fails(self):
        old = h.TARGET_REL
        h.TARGET_REL = "does/not/exist.rs"
        try:
            self.assertEqual(h.main([]), 1)
        finally:
            h.TARGET_REL = old


if __name__ == "__main__":
    unittest.main()
