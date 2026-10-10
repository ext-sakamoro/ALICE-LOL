# ALICE-LOL

[![crates.io](https://img.shields.io/crates/v/alice-lol.svg)](https://crates.io/crates/alice-lol)
[![docs.rs](https://img.shields.io/docsrs/alice-lol)](https://docs.rs/alice-lol)
[![MSRV](https://img.shields.io/crates/msrv/alice-lol)](#msrv)
[![CI](https://github.com/ext-sakamoro/ALICE-LOL/actions/workflows/ci.yml/badge.svg)](https://github.com/ext-sakamoro/ALICE-LOL/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/crates/l/alice-lol.svg)](#license)

English | [日本語](README_JP.md)

A text language for signed distance fields. The `lol!` macro (compile time) and
the runtime parser turn the same text into an
[ALICE-SDF](https://github.com/ext-sakamoro/ALICE-SDF) `SdfNode` tree, which can
be evaluated on the CPU, transpiled to GLSL / WGSL / HLSL, exported as a mesh
for 3D printing, and checked against geometric constraints whose verdict is
three-valued (satisfied / violated / undecided). A separate module,
`research_law`, holds unit-checked formulas with valid ranges, residuals and
provenance.

## What it is for, and what it is not for

It is for writing geometry as short text that both people and language models
can produce: the grammar shipped with the crate ([`alice-lol/lol.gbnf`](alice-lol/lol.gbnf))
can constrain a model's decoding so that only parseable text comes out, and the
parser turns that text into a field you can evaluate, render or print.

It is not a CAD kernel: there is no boundary representation, no assembly
mating and no STEP output. Distance evaluation, compiled backends, shader
transpilers and meshing belong to ALICE-SDF; this crate is the language on top
of it. The geometric law checker answers questions about a field on a sampling
grid and reports what it could not decide instead of passing it; it is not a
finite-element solver.

## Installation

```bash
cargo add alice-lol
```

The path dependencies of this repository (`alice-sdf`, `alice-zip`, and the
optional `alice-physics` / `alice-llm`) are expected as sibling checkouts when
building from source; see [Building and testing](#building-and-testing).

## Example

```rust
use alice_lol::lol;
use alice_lol::runtime_parser::parse_lol;
use alice_lol::Vec3;

// Compile time: the macro builds the tree
let scene = lol! {
    smooth_union(0.2,
        sphere(1.0),
        translate(2.0, 0.0, 0.0, box3d(0.5, 0.5, 0.5))
    )
};

// Run time: the same text through the parser (from a file or a language model)
let parsed = parse_lol(
    "smooth_union(0.2, sphere(1.0), translate(2.0, 0.0, 0.0, box3d(0.5, 0.5, 0.5)))",
)
.unwrap();

// Both are the same field; the origin is inside the sphere
let p = Vec3::new(0.3, 0.1, 0.0);
assert_eq!(alice_lol::eval(&scene, p), alice_lol::eval(&parsed, p));
assert!(alice_lol::eval(&scene, Vec3::ZERO) < 0.0);

// GLSL source for a shader (default feature `glsl`)
let glsl = alice_lol::to_glsl(&scene);
assert!(!glsl.is_empty());
```

The two front ends differ in one convention: `SdfNode::box3d` takes **full**
extents, the DSL's `box3d` takes **half** extents. Both notations of the same
shape are compared in [`alice-lol/tests/readme_parity.rs`](alice-lol/tests/readme_parity.rs).

## Where it sits

| Crate | Owns |
|-------|------|
| ALICE-LOL | the language (macro, runtime parser, grammar, `SdfNode` → text emitter) and the law checkers |
| [ALICE-SDF](https://github.com/ext-sakamoro/ALICE-SDF) | distance functions, compiled backends (scalar / SIMD / BVH), shader transpilers, meshing |
| [ALICE-Physics](https://github.com/ext-sakamoro/ALICE-Physics) | fixed-point rigid bodies and the heat solver behind `Constraint::ThermalField` (feature `physics`) |
| [ALICE-DetMath](https://github.com/ext-sakamoro/ALICE-DetMath) | `sin` / `cos` / `exp` / `ln` … with a bit-exact contract, shared instead of platform libm |
| [ALICE-Zip](https://github.com/ext-sakamoro/ALICE-Zip) | the `law` vocabulary (valid range, residual statistics, provenance, ingest policy) that `research_law` shares |

Keep `alice-det-math` unified across the resolved dependency graph: two
versions in one tree are two implementations of the same function. Check with
`cargo tree -i alice-det-math`.

## Syntax

Every name below is accepted by `alice_lol::runtime_parser::parse_lol`. The
`lol!` macro accepts the same names except the `stdlib` group and
`capsule_ab`. Arguments are numbers (or, in the macro, `{rust_expr}` / bare
identifiers) followed by child expressions; the argument order of each name is
in [LLM_REFERENCE.md](LLM_REFERENCE.md) and in the grammar. Text may be wrapped
as `field Name { ... }`. The runtime parser accepts `//` comments; the grammar,
written for model decoding, does not.

Primitives:

<!-- readme-sync: syntax-primitives -->
```text
sphere box3d rounded_box cylinder torus cone capsule capsule_ab ellipsoid
plane octahedron rounded_cone pyramid hex_prism link capped_cone capped_torus
rounded_cylinder tube barrel heart egg helix tetrahedron box_frame diamond
star_polygon cross_shape triangle bezier triangular_prism cut_sphere
cut_hollow_sphere death_star solid_angle rhombus horseshoe vesica
infinite_cylinder infinite_cone gyroid chamfered_cube schwarz_p superellipsoid
rounded_x pie trapezoid parallelogram tunnel uneven_capsule arc_shape moon
blobby_cross parabola_segment regular_polygon stairs_prim dodecahedron
icosahedron truncated_octahedron truncated_icosahedron diamond_surface neovius
lidinoid iwp frd fischer_koch_s pmy circle_2d rect_2d segment_2d
rounded_rect_2d annular_2d terrain
```

CSG operations (the variadic ones fold left):

<!-- readme-sync: syntax-csg -->
```text
union smooth_union intersection smooth_intersection subtract smooth_subtract
chamfer_union chamfer_intersection chamfer_subtraction stairs_union
stairs_intersection stairs_subtraction xor pipe engrave groove tongue
columns_union columns_intersection columns_subtraction exp_smooth_union
exp_smooth_intersection exp_smooth_subtraction
```

Transforms:

<!-- readme-sync: syntax-transforms -->
```text
translate rotate scale scale_non_uniform
```

Modifiers:

<!-- readme-sync: syntax-modifiers -->
```text
round onion twist bend mirror repeat elongate revolution extrude taper
displacement polar_repeat shear noise repeat_finite octant_mirror
icosahedral_symmetry with_material surface_roughness sweep_bezier
```

3D-print infill (shell plus lattice):

<!-- readme-sync: syntax-print -->
```text
lattice_infill diamond_infill schwarz_infill
```

Time:

<!-- readme-sync: syntax-time -->
```text
animate morph
```

<details>
<summary>Product and mechanical shortcuts (runtime parser only)</summary>

<!-- readme-sync: syntax-stdlib -->
```text
shopping_cart_coin skadis_panel skadis_hook_l skadis_hook_j skadis_hook_s
skadis_container skadis_clip skadis_shelf skadis_elastic_cord mug
gridfinity_bin gridfinity_bin_ex wall_hook drawer_organizer shelf_divider
sticky_note_holder business_card_holder pen_cup phone_stand headphone_holder
under_desk_mount desk_shelf monitor_riser coaster tissue_box_cover storage_box
cable_clip led_channel card_tray token_well wrench_holder socket_rail
hex_bit_holder raspi_case esp32_enclosure battery_18650_holder
toothbrush_holder drill_bit_holder pliers_rack spice_rack egg_tray
utensil_caddy filament_spool_holder nozzle_holder build_plate_rack
cutlery_tray pill_organizer magnetic_strip hairdryer_holder kcup_holder
hex_key_holder wrap_holder sock_divider soap_tray razor_holder
chopstick_holder swatch_holder tp_holder sd_card_holder driver_rack
cotton_dispenser sink_caddy clamp_rack dry_box outdoor_enclosure jewelry_stand
phone_dock cutting_board_rack tape_dispenser shower_caddy caliper_holder
bag_clip_org can_rack led_hub_box makeup_organizer vesa_mount l_bracket
t_slot_bracket_2020 raspi_mount_plate heat_set_array flange_mount
dovetail_pair profile_extrusion snap_fit_pair boss_array screw_hole tap_hole
counterbore countersink heat_set_hole bolt bracket_l flange_circular
t_slot_2020 profile_2020 profile_3030 dovetail slot snap_fit_annular
pin_hinge_knuckle boss rib bearing_seat rack_shelf cable_grommet
curtain_rod_bracket dowel_hole wood_screw_pilot arduino_mount_plate
pixhawk_mount servo_mount jst_ph_slot
```

</details>

`runtime_parser::parse_program` also reads
`program(<sdf>, entities(<sdf>, ...), <intent>)`, where the intent is built
from these verbs (`turn` is the text form of the rotate verb, since `rotate`
is the SDF transform):

<!-- readme-sync: syntax-intent -->
```text
grasp release catch walk gaze point throw push pull turn align follow avoid
rest latent seq par music
```

## Audit laws (`audit_law`)

An audit law states what a check must find, never how to measure it. One clause
per line; `parse_law` reads them and `AuditLaw::evaluate` answers with a
six-valued verdict (`Supports` / `NoEvidence` / `Breaks` / `OutOfRange` /
`ParameterUpdate` / `Undecided`). Whoever runs the check supplies the numbers,
so the same law can be fed from Rust or from another language.

<!-- readme-sync: syntax-law -->
```text
audit evidence expect range
```

`evidence <metric>` requires that the metric was measured at all — a check that
compared nothing answers `NoEvidence` instead of passing. Only a finite number
greater than 0 is evidence; 0, a negative number, NaN and infinity are not.
`expect <metric> == <value> [within <tol>]` is the expectation, and
`range <key> <value>...` records a known violation: it is allowed while the
measurement matches, becomes `OutOfRange` once the row no longer applies, and
`ParameterUpdate` when the values move. The values are compared as a set (order
and duplicates are ignored). Evidence is judged before expectations, so `0 == 0` cannot pass by
accident on an empty measurement.

## Law identity (`law_id`)

A stored law and an evaluated law are the same law only if they carry the same
name. `AuditLaw::law_id` derives a 32-byte identifier from the domain, the kind
of law, the arithmetic generation and the claim itself. Every field is written
with its type and its length, so `["ab", "c"]` and `["a", "bc"]`, `u32 5` and
`u64 5`, and an empty field and a missing field all get different identifiers.

The arithmetic generation is part of the name: the same text evaluated with
different arithmetic can answer differently, so it is a different law.
`LOL_SEMANTICS_ID` is the fold of `LOL_SEMANTICS_PINS`, and the pins are
computed from behaviour (`audit_verdict_order_fingerprint`,
`law_input::input_reading_fingerprint`,
`research_law::expression_functions_fingerprint`,
`law_input::derivation_fingerprint`) rather than written down, so changing
the order in which verdicts are reached, or how a request's inputs are read,
makes a pin disagree. An audit law's identifier also covers the type of each
`x-input` line (`law_input::audit_law_from_file`). Geometric and research laws
do not have identifiers yet.

Example: [`law_id_demo`](alice-lol/examples/law_id_demo.rs); oracle:
[`alice-lol/tests/law_id_oracle.rs`](alice-lol/tests/law_id_oracle.rs).

## Geometric laws (`law`)

`law::Constraint` declares a property of a field; `LawSet` collects them with
a hard or soft priority and checks them on a grid. Each sample point ends as
satisfied, violated or **undecided**, and undecided points are reported in
`LawReport::unresolved` instead of being counted as passes. `LawReport::hard_verdict`
returns `Proven` / `Violated` / `Undecided` for the hard constraints.

The distance-dependent constraints do not read the field value as a distance
(that is wrong for TPMS surfaces and inside unions): a box is proved free of
surface by interval arithmetic, and an upper bound on the distance to the
surface comes from a sign change between two evaluated points.

Each constraint has an evidence class (`Constraint::evidence_class`).
`Modelled` constraints rest on a model (grid resolution, a threshold, a
proxy) and cannot be added with `Priority::Hard`.

<!-- readme-sync: laws -->
| Constraint | Checks | Evidence |
|------------|--------|----------|
| `NonOverlap` | two fields do not overlap | `Witnessed` |
| `Containment` | one field lies inside another | `Witnessed` |
| `MinThickness` | the wall is at least a given thickness | `Witnessed` |
| `Stress` | wall thickness near load points scales with the load (geometric proxy) | `Modelled` |
| `Thermal` | surface-to-volume ratio near heat sources (geometric proxy) | `Modelled` |
| `Contact` | the gap between two fields lies in a range | `Witnessed` |
| `Continuity` | the interior is one connected region (flood fill) | `Modelled` |
| `GradientBound` | the field gradient stays under a bound | `Witnessed` |
| `Reachable` | two interior points are connected through the interior | `Proved` |
| `VolumeConservation` | volume before and after a change agrees within a tolerance | `Modelled` |
| `ThermalField` (feature `physics`) | the peak temperature of a solved heat field stays under a limit, bracketed between an insulated and an isothermal surface | `Modelled` |

Closed-form oracles: [`alice-lol/tests/analytic_law.rs`](alice-lol/tests/analytic_law.rs),
[`alice-lol/tests/test_field_law_oracle.rs`](alice-lol/tests/test_field_law_oracle.rs);
the grammar corpus against a brute-force refuter:
[`alice-lol/tests/law_corpus_oracle.rs`](alice-lol/tests/law_corpus_oracle.rs).

## Research laws (`research_law`)

`research_law` is unrelated to the geometric `law` module. A `ResearchLaw` is
a claim about data, `output = f(inputs; parameters)`, written as text
(`n*R*T/V`) together with:

- **units and dimensions**: inputs, output and parameters carry units
  (`Pa`, `kPa`, `J/(mol*K)`, `m^3`, `L`, …); the dimension of the expression
  is checked when the law is built (`+` / `-` need equal dimensions, `exp` /
  `ln` / `sin` / `cos` need a dimensionless argument, the result must have the
  output's dimension)
- **valid ranges**: evaluation outside the range of an input is refused
  (`OutOfRange`); the law does not extrapolate, and a non-finite intermediate
  value is an error rather than a NaN
- **residuals**: measured from the stored observations, not reported numbers
- **provenance** and **oracles**: where the law comes from, and reference values
  it has to reproduce within a tolerance (`check_oracles`)
- **comparison**: `compare` evaluates two laws written in different units under
  the same conditions through a `Bridge` (input-name mapping with unit conversion)
- **new evidence**: `ingest` judges new observations, in the rule order of
  `alice_zip::law::SignalLaw::ingest`; a parameter update is a least-squares
  refit of all parameters (Gauss–Newton with a fixed iteration limit and
  tolerance)

<!-- readme-sync: research-verdicts -->
| Verdict | Meaning |
|---------|---------|
| `NoEvidence` | no observations were given |
| `OutOfRange` | some observations lie where the law cannot be evaluated; nothing was judged |
| `Supports` | the new observations agree with the law within the band |
| `ParameterUpdate` | the same expression fits stored and new evidence with refitted parameters |
| `ResidualGrew` | the deviation exceeds the band but not the break threshold |
| `Breaks` | the evidence is not described by this law |

The transcendental functions of an expression (`exp`, `ln`, `sqrt`, powers,
`sin`, `cos`) are evaluated through `alice-det-math`'s `f64` functions instead
of the platform math library, and the evaluation is a fixed sequence of `f64`
operations without fused multiply-add, so the same inputs give the same bits on
every platform. Range, residual, provenance and policy types are those of
`alice_zip::law`. Example: [`research_law_demo`](alice-lol/examples/research_law_demo.rs);
oracle: [`alice-lol/tests/analytic_research_law.rs`](alice-lol/tests/analytic_research_law.rs).

## Crates in this workspace

| Crate | Role |
|-------|------|
| `alice-lol-macro` | the `lol!` proc-macro |
| `alice-lol` | runtime parser, emitter, transpile and export functions, law checkers, intent IR, stdlib shapes |
| `alice-lol-humanoid` | parametric humanoid template (joint FK, VRM / BVH import) |
| `alice-lol-robot` | intent verbs to humanoid FK and an 8-byte kinematics packet, with a safety law |
| `alice-lol-ui` | UI shapes (button, card, panel …), flex / grid / stack layout, contrast law |
| `alice-lol-datagen` | synthetic (caption, LOL) pair generator with self-check (not published) |
| `alice-world-auditor-types` | goal and verdict types shared by law checkers and planners |
| `alice-world-auditor` | planner over `alice-physics` worlds with three-valued verdicts (AGPL-3.0-or-later or commercial) |

## Features

<!-- readme-sync: features -->
| Feature | Default | Description |
|---------|---------|-------------|
| `glsl` | yes | GLSL transpilation (`to_glsl`, `to_glsl_dynamic`, `to_glsl_full`) |
| `wgsl` | no | WGSL transpilation |
| `hlsl` | no | HLSL transpilation |
| `physics` | no | `Constraint::ThermalField` and material data from `alice-physics` (AGPL-3.0-or-later; enabling it applies that license downstream) |
| `roblox` | no | OBJ / FBX export within Roblox mesh limits (`roblox_export`) |
| `llm-bridge` | no | grammar-constrained generation through `alice-llm` (`bridge`; AGPL-3.0-or-later, applies downstream) |

## Examples

| Example | Shows |
|---------|-------|
| [`basic`](alice-lol/examples/basic.rs) | the macro and GLSL output |
| [`showcase`](alice-lol/examples/showcase.rs) | a tour of the syntax, variable capture, autodiff, `CompiledSdf` |
| [`law_demo`](alice-lol/examples/law_demo.rs) | declaring and checking geometric laws |
| [`research_law_demo`](alice-lol/examples/research_law_demo.rs) | the ideal-gas law in SI and in (kPa, L): re-evaluation, comparison, oracles, new evidence |
| [`audit_law_demo`](alice-lol/examples/audit_law_demo.rs) | writing a check as an audit law and reaching the six verdicts |
| [`law_id_demo`](alice-lol/examples/law_id_demo.rs) | what a law's identifier is derived from, and what changes it |
| [`audit_conformance`](alice-lol/examples/audit_conformance.rs) | the program contract of `conformance/TASK.md` for the audit laws, built on `law_input` (checked against `conformance/probes.json`) |
| [`export_formats`](alice-lol/examples/export_formats.rs) | STL / 3MF / FBX written from the same mesh, and the resolution presets side by side |
| [`pruning_demo`](alice-lol/examples/pruning_demo.rs) | interval-arithmetic pruning per grid cell |
| [`autodiff_demo`](alice-lol/examples/autodiff_demo.rs) | gradients, curvatures, Hessian |
| [`compiled_demo`](alice-lol/examples/compiled_demo.rs) | compiled evaluation (single point, SIMD batch, normals) |
| [`print_export`](alice-lol/examples/print_export.rs) | STL / 3MF export |
| [`print_verify`](alice-lol/examples/print_verify.rs) | numerical check of a lattice infill against the mesh |
| [`roblox_accessory`](alice-lol/examples/roblox_accessory.rs) | Roblox accessory export (feature `roblox`) |
| [`prompt_to_sword`](alice-lol/examples/prompt_to_sword.rs) | prompt → grammar-constrained LOL → mesh (feature `llm-bridge`) |
| [`alice_coaster`](alice-lol/examples/alice_coaster.rs) | a 10 cm round coaster built from an SDF pattern |
| [`coin_dc_vs_mc`](alice-lol/examples/coin_dc_vs_mc.rs) | marching cubes against dual contouring on a 1.7 mm coin |
| [`complete_pipeline_output`](alice-lol/examples/complete_pipeline_output.rs) | 3MF export of every catalogue item in one run |
| [`hardsurface_bolt_plate`](alice-lol/examples/hardsurface_bolt_plate.rs) | the fastener primitives |
| [`hardsurface_snap_case`](alice-lol/examples/hardsurface_snap_case.rs) | the joint primitives |
| [`hardsurface_ribbed_bracket`](alice-lol/examples/hardsurface_ribbed_bracket.rs) | the reinforcement primitives |
| [`hardsurface_wall_bracket`](alice-lol/examples/hardsurface_wall_bracket.rs) | fasteners, joints, reinforcement and mounts in one part |
| [`pattern_catalog`](alice-lol/examples/pattern_catalog.rs) | the pattern registry as a catalogue |
| [`print_demo`](alice-lol/examples/print_demo.rs) | structural intent for 3D printing |
| [`royal_crown`](alice-lol/examples/royal_crown.rs) | an ornate crown exported as OBJ |
| [`skadis_panel_dc_vs_mc`](alice-lol/examples/skadis_panel_dc_vs_mc.rs) | marching cubes against dual contouring on a perforated panel |
| [`verify_customizers`](alice-lol/examples/verify_customizers.rs) | the customizer archetypes render the expected shape |
| [`llm_bench`](alice-lol/examples/llm_bench.rs) | pass rate of grammar-constrained decoding against a think-then-grammar prompt (feature `llm-bridge`) |
| [`gridfinity_untrusted_input`](alice-lol/examples/gridfinity_untrusted_input.rs) | the fallible `try_gridfinity_bin` entry point refusing a degenerate dividers count instead of panicking |

Run one with `cargo run -p alice-lol --example <name>`.

## MSRV

Minimum supported Rust version: **1.90** <!-- readme-sync: msrv -->. CI checks
the whole workspace with that toolchain.

## Building and testing

The crates depend on sibling repositories by path, so clone them next to this
one:

```bash
git clone https://github.com/ext-sakamoro/ALICE-LOL
git clone https://github.com/ext-sakamoro/ALICE-SDF
git clone https://github.com/ext-sakamoro/ALICE-Zip
git clone https://github.com/ext-sakamoro/ALICE-Physics     # feature `physics`
git clone https://github.com/ext-sakamoro/ALICE-LLM         # feature `llm-bridge`
git clone https://github.com/ext-sakamoro/ALICE-Kinematics  # alice-lol-robot
cd ALICE-LOL
cargo test
scripts/law_tests.sh            # the law oracles, failing if any of them ran zero tests
python scripts/readme_sync.py --check
python scripts/docs_lint.py --check
scripts/preflight.sh --quick    # the CI gates that do not run the test suites
```

## Related crates

- [ALICE-SDF](https://github.com/ext-sakamoro/ALICE-SDF): the field the language describes
- [ALICE-Physics](https://github.com/ext-sakamoro/ALICE-Physics): deterministic physics
- [ALICE-DetMath](https://github.com/ext-sakamoro/ALICE-DetMath): bit-exact math functions
- [ALICE-Zip](https://github.com/ext-sakamoro/ALICE-Zip): the `law` vocabulary shared with `research_law`
- [ALICE-Synth](https://github.com/ext-sakamoro/ALICE-Synth): defines the 8-byte payload of the `music` intent
- [ALICE-View](https://github.com/ext-sakamoro/ALICE-View): wgpu renderer

## License

Apache-2.0 ([LICENSE-APACHE](LICENSE-APACHE), with [NOTICE](NOTICE) and
[TRADEMARK_NOTICE](TRADEMARK_NOTICE)) from 0.4.0; 0.3.x and earlier were released
under MIT OR Apache-2.0 and keep those terms. The exception is
`alice-world-auditor` (AGPL-3.0-or-later or commercial). The `physics`
and `llm-bridge` features pull in AGPL-3.0-or-later dependencies.

A redistribution (source or binary) must include [LICENSE-APACHE](LICENSE-APACHE) and
[NOTICE](NOTICE); the notice can sit with the other licence texts and does not need
to appear in a user interface. The names "ALICE" and "ALICE-*" are trademarks: see
[TRADEMARK_NOTICE](TRADEMARK_NOTICE), which applies whatever the licence.

The DSL exposes the ALICE-SDF primitive set, so that crate's attribution carries
over: many distance-function forms follow Inigo Quilez's published articles,
the stairs / columns / chamfer operators follow Mercury's hg_sdf, and the noise
gradient table follows Ken Perlin. The implementations are ALICE-SDF's own Rust
code; the full list is in
[ALICE-SDF/THIRD-PARTY-NOTICES.md](https://github.com/ext-sakamoro/ALICE-SDF/blob/main/THIRD-PARTY-NOTICES.md).
