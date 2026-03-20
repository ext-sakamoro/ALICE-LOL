# ALICE-LOL — LLM Reference Guide

> This document teaches LLMs how to generate ALICE-LOL DSL code for 3D scene creation.
> Include this in your system prompt or RAG context when asking an LLM to create 3D scenes.
> For 3D printing–specific guidance (FDM/SLA/SLS material presets, anti-patterns), see also `LLM_PRINT_PROMPT.md`.

## What is ALICE-LOL?

ALICE-LOL (Law-Oriented Language) is a DSL for describing 3D scenes using Signed Distance Functions (SDF).
Instead of writing verbose JSON trees, you write concise function-call syntax:

```
smooth_union(0.3, sphere(1.0), box3d(0.5, 0.5, 0.5))
```

This compiles to an `SdfNode` tree that can be:
- Evaluated on CPU (SIMD 8-wide)
- Transpiled to GLSL/WGSL/HLSL shaders
- Meshed via Marching Cubes → STL/3MF for 3D printing
- Exported to OBJ/FBX for Roblox MeshPart / accessories (feature: `roblox`)

## Syntax Rules

1. Every construct is `name(args)` — function-call style
2. Arguments are comma-separated: numbers or nested expressions
3. Numbers are f32 literals: `1.0`, `0.5`, `-2.3`
4. Nesting is natural: `translate(0.0, 1.0, 0.0, sphere(0.5))`
5. Operations take 2+ children: `union(sphere(1.0), box3d(0.5, 0.5, 0.5))`
6. No trailing commas

## Complete Syntax Reference (79 constructs)

### Primitives (27)

| Syntax | Args | Description |
|--------|------|-------------|
| `sphere(r)` | radius | Sphere centered at origin |
| `box3d(hx, hy, hz)` | half-extents X, Y, Z | Axis-aligned box |
| `rounded_box(hx, hy, hz, r)` | half-extents + round radius | Box with rounded edges |
| `cylinder(r, h)` | radius, half-height | Cylinder along Y-axis |
| `torus(R, r)` | major radius, minor radius | Torus in XZ plane |
| `cone(r, h)` | radius, half-height | Cone along Y-axis |
| `capsule(r, h)` | radius, half-height | Tube with hemispherical caps |
| `ellipsoid(rx, ry, rz)` | radii X, Y, Z | Stretched sphere |
| `plane(nx, ny, nz, d)` | normal X, Y, Z, distance | Infinite half-space |
| `octahedron(s)` | size | Regular 8-faced solid |
| `rounded_cone(r1, r2, h)` | bottom radius, top radius, half-height | Cone with spherical ends |
| `pyramid(h)` | half-height | 4-sided pyramid |
| `hex_prism(r, h)` | hex radius, half-height | Hexagonal column |
| `link(l, r1, r2)` | half-length, major radius, tube radius | Chain link |
| `capped_cone(h, r1, r2)` | half-height, bottom radius, top radius | Frustum |
| `capped_torus(R, r, angle)` | major radius, minor radius, cap angle | Partial torus |
| `rounded_cylinder(r, rr, h)` | radius, round radius, half-height | Cylinder with rounded edges |
| `tube(r_out, t, h)` | outer radius, thickness, half-height | Hollow cylinder |
| `barrel(r, h, b)` | radius, half-height, bulge | Cylinder with parabolic bulge |
| `heart(s)` | size | 3D heart shape |
| `egg(ra, rb)` | base radius, top deformation | Egg shape |
| `helix(R, r, pitch, h)` | major radius, minor radius, pitch, half-height | Spiral tube |
| `tetrahedron(s)` | size | Regular tetrahedron |
| `box_frame(hx, hy, hz, e)` | half-extents + edge thickness | Wireframe box |
| `diamond(r, h)` | radius, half-height | Bipyramid |
| `star_polygon(r, n, m, h)` | radius, num points, inner ratio, half-height | Star prism |
| `cross_shape(l, t, r, h)` | length, thickness, round radius, half-height | Plus shape |

### CSG Operations (23)

| Syntax | Args | Description |
|--------|------|-------------|
| `union(a, b, ...)` | 2+ children | Combine shapes (logical OR) |
| `smooth_union(k, a, b, ...)` | blend radius, 2+ children | Smooth blend (organic forms) |
| `intersection(a, b, ...)` | 2+ children | Keep overlap only (logical AND) |
| `smooth_intersection(k, a, b, ...)` | blend radius, 2+ children | Smooth intersection |
| `subtract(a, b)` | base, cutter | Carve b out of a |
| `smooth_subtract(k, a, b)` | blend radius, base, cutter | Smooth carving with fillet |
| `chamfer_union(r, a, b, ...)` | chamfer size, 2+ children | Union with chamfered edge |
| `chamfer_intersection(r, a, b, ...)` | chamfer size, 2+ children | Intersection with chamfer |
| `chamfer_subtraction(r, a, b)` | chamfer size, base, cutter | Subtraction with chamfer |
| `stairs_union(r, n, a, b, ...)` | radius, steps, 2+ children | Staircase blend |
| `stairs_intersection(r, n, a, b, ...)` | radius, steps, 2+ children | Staircase intersection |
| `stairs_subtraction(r, n, a, b)` | radius, steps, base, cutter | Staircase subtraction |
| `columns_union(r, n, a, b, ...)` | radius, columns, 2+ children | Columnar blend |
| `columns_intersection(r, n, a, b, ...)` | radius, columns, 2+ children | Columnar intersection |
| `columns_subtraction(r, n, a, b)` | radius, columns, base, cutter | Columnar subtraction |
| `exp_smooth_union(k, a, b, ...)` | blend, 2+ children | Exponential smooth union |
| `exp_smooth_intersection(k, a, b, ...)` | blend, 2+ children | Exponential smooth intersection |
| `exp_smooth_subtraction(k, a, b)` | blend, base, cutter | Exponential smooth subtraction |
| `xor(a, b)` | two children | Exclusive OR |
| `pipe(r, a, b)` | radius, two children | Pipe at intersection edge |
| `engrave(r, a, b)` | depth, base, pattern | Engrave pattern into surface |
| `groove(ra, rb, a, b)` | two radii, two children | Groove at intersection |
| `tongue(ra, rb, a, b)` | two radii, two children | Tongue-and-groove joint |

### Transforms (4)

| Syntax | Args | Description |
|--------|------|-------------|
| `translate(x, y, z, child)` | offset X, Y, Z, child | Move shape |
| `rotate(rx, ry, rz, child)` | degrees X, Y, Z, child | Rotate (Euler angles in degrees) |
| `scale(s, child)` | uniform factor, child | Uniform scale |
| `scale_non_uniform(sx, sy, sz, child)` | factors X, Y, Z, child | Non-uniform scale |

### Modifiers (19)

| Syntax | Args | Description |
|--------|------|-------------|
| `round(r, child)` | fillet radius, child | Round all edges |
| `onion(t, child)` | thickness, child | Hollow into thin shell |
| `twist(k, child)` | strength, child | Twist around Y-axis |
| `bend(k, child)` | curvature, child | Bend the shape |
| `mirror(ax, ay, az, child)` | axis mask, child | Mirror across axes |
| `repeat(sx, sy, sz, child)` | spacing X, Y, Z, child | Infinite repetition |
| `elongate(hx, hy, hz, child)` | amount X, Y, Z, child | Stretch by inserting flat sections |
| `revolution(offset, child)` | distance from Y-axis, child | Revolve around Y-axis |
| `extrude(h, child)` | half-height, child | Extrude along Z |
| `taper(k, child)` | factor, child | Taper along Y |
| `displacement(s, child)` | strength, child | Sine wave displacement |
| `polar_repeat(n, child)` | count, child | Radial repetition around Y |
| `shear(xy, xz, yz, child)` | shear factors, child | Shear deformation |
| `noise(amp, freq, seed, child)` | amplitude, frequency, seed, child | Perlin noise displacement |
| `repeat_finite(cx, cy, cz, sx, sy, sz, child)` | count X,Y,Z, spacing X,Y,Z, child | Bounded repetition |
| `octant_mirror(child)` | child | Mirror in all 8 octants |
| `icosahedral_symmetry(child)` | child | 60-fold icosahedral symmetry |
| `with_material(id, child)` | material ID, child | Assign material |
| `surface_roughness(freq, amp, oct, child)` | frequency, amplitude, octaves, child | Surface roughness |

### 3D Print Structural Intent (3)

These are sugar syntax that expand to `Union(Onion(shell), Intersection(child, TPMS))`. Use them when generating objects for physical fabrication.

| Syntax | Args | TPMS Type | Best For |
|--------|------|-----------|----------|
| `lattice_infill(shell_t, scale, lattice_t, child)` | shell thickness, lattice scale, lattice thickness, child | Gyroid | General purpose — best strength-to-weight |
| `diamond_infill(shell_t, scale, lattice_t, child)` | shell thickness, lattice scale, lattice thickness, child | Diamond | High stiffness in all directions |
| `schwarz_infill(shell_t, scale, lattice_t, child)` | shell thickness, lattice scale, lattice thickness, child | Schwarz-P | Isotropic — equal strength in X/Y/Z |

### Time Controls (2)

| Syntax | Args | Description |
|--------|------|-------------|
| `animate(speed, amplitude, child)` | speed, amplitude, child | Animate over time |
| `morph(t, a, b)` | blend factor 0-1, shape A, shape B | Morph between two shapes |

## Examples for LLMs

### Simple: Snowman

```
union(
    sphere(1.0),
    translate(0.0, 1.3, 0.0, sphere(0.7)),
    translate(0.0, 2.2, 0.0, sphere(0.5))
)
```

### Organic: Mushroom

```
smooth_union(0.2,
    translate(0.0, 1.0, 0.0,
        scale_non_uniform(1.5, 0.4, 1.5, sphere(1.0))
    ),
    cylinder(0.3, 0.8)
)
```

### Mechanical: Gear

```
subtract(
    polar_repeat(12,
        translate(1.5, 0.0, 0.0, cylinder(0.15, 0.2))
    ),
    subtract(
        cylinder(1.8, 0.2),
        cylinder(0.5, 0.3)
    )
)
```

### Architecture: Arch

```
subtract(
    box3d(2.0, 2.0, 0.5),
    translate(0.0, 0.5, 0.0,
        cylinder(1.2, 0.6)
    )
)
```

### Organic: Twisted Vase

```
onion(0.05,
    twist(0.5,
        taper(0.3,
            cylinder(1.0, 2.0)
        )
    )
)
```

### Crystal Cluster

```
smooth_union(0.1,
    diamond(0.5, 1.5),
    translate(0.8, -0.5, 0.3,
        rotate(0.0, 0.0, 15.0, diamond(0.3, 1.0))
    ),
    translate(-0.6, -0.3, -0.5,
        rotate(10.0, 0.0, -10.0, diamond(0.4, 1.2))
    )
)
```

### Fractal-like: Symmetric Object

```
icosahedral_symmetry(
    translate(1.0, 0.0, 0.0, sphere(0.3))
)
```

### Scene with Ground

```
union(
    noise(0.1, 2.0, 42,
        plane(0.0, 1.0, 0.0, 0.0)
    ),
    translate(0.0, 1.0, 0.0,
        smooth_union(0.3,
            sphere(0.8),
            translate(0.0, 1.0, 0.0, sphere(0.5))
        )
    )
)
```

### 3D Printable: ALICE Coaster (10cm round, gyroid openwork)

```
// SDF geometric coaster (10cm round, 5mm thick)
// NOTE: Nest subtract sequentially — never union cutters together
subtract(
    subtract(
        subtract(
            subtract(
                cylinder(2.5, 0.125),
                // outer ring: 12 round holes
                polar_repeat(12, translate(1.8, 0.0, 0.0, cylinder(0.3, 0.2)))
            ),
            // middle ring: 12 smaller holes (15° offset)
            rotate(0.0, 15.0, 0.0,
                polar_repeat(12, translate(1.2, 0.0, 0.0, cylinder(0.2, 0.2)))
            )
        ),
        // inner ring: 6 hexagonal holes
        polar_repeat(6, translate(0.6, 0.0, 0.0, hex_prism(0.15, 0.2)))
    ),
    // center: hexagonal recess (not through)
    translate(0.0, 0.04, 0.0, hex_prism(0.25, 0.1))
)
```

## Structural Intent for 3D Printing

When generating objects for 3D printing, consider their physical purpose. LOL provides high-level modifiers that define internal structure at the math level — no slicer infill needed.

> For detailed manufacturing method guidance (FDM/SLA/SLS/CNC material presets, parameter tables, anti-patterns), see `LLM_PRINT_PROMPT.md`.

### Design Rules

| Purpose | Strategy | LOL Syntax |
|---------|----------|-----------|
| **Decorative** (vase, mask, figurine) | Hollow shell — save material | `onion(thickness, child)` |
| **Structural + Lightweight** (bracket, drone part, jig) | Shell + TPMS lattice infill | `lattice_infill(shell_t, scale, lattice_t, child)` |
| **Maximum Rigidity** (paperweight, base plate) | Leave solid — no modification | Use the primitive as-is |

### Parameter Guide

- `shell_t`: Outer wall thickness (0.02–0.1 typical, thicker = stronger walls)
- `scale`: TPMS spatial frequency (3.0–8.0 typical, higher = denser lattice = stronger but heavier)
- `lattice_t`: TPMS wall thickness (0.01–0.05 typical, thicker = stronger lattice struts)

### STL/3MF Output Pipeline

LOL text can be directly converted to printable mesh files:

```
LOL text → parse_lol() → SdfNode → sdf_to_mesh() → export_stl() / export_3mf() → Bambu Studio → Printer
```

One-liner API:
```rust
alice_lol::print_export::lol_to_stl("lattice_infill(0.05, 5.0, 0.02, sphere(1.0))", "output.stl", &PrintConfig::default());
```

### Examples for 3D Printing

**Decorative vase** (hollow, minimal material):
```
onion(0.02,
    twist(0.5,
        taper(0.3, cylinder(1.0, 2.0))
    )
)
```

**Structural bracket** (gyroid infill for strength + weight savings):
```
lattice_infill(0.05, 5.0, 0.02,
    subtract(
        box3d(2.0, 1.0, 0.5),
        translate(1.0, 0.0, 0.0, cylinder(0.3, 0.6))
    )
)
```

**Drone arm** (diamond infill for maximum stiffness):
```
diamond_infill(0.04, 6.0, 0.03,
    smooth_union(0.1,
        capsule(0.3, 2.0),
        translate(0.0, 2.0, 0.0, sphere(0.5))
    )
)
```

**Isotropic test specimen** (Schwarz-P for equal strength in all axes):
```
schwarz_infill(0.05, 4.0, 0.02, box3d(1.0, 1.0, 1.0))
```

## LOL vs JSON Comparison

### JSON (verbose, 15 lines):
```json
{"Union": {"a": {"Sphere": {"radius": 1.0}}, "b": {"Union": {"a": {"Translate": {"child": {"Sphere": {"radius": 0.7}}, "offset": [0.0, 1.3, 0.0]}}, "b": {"Translate": {"child": {"Sphere": {"radius": 0.5}}, "offset": [0.0, 2.2, 0.0]}}}}}}
```

### LOL (concise, 5 lines):
```
union(
    sphere(1.0),
    translate(0.0, 1.3, 0.0, sphere(0.7)),
    translate(0.0, 2.2, 0.0, sphere(0.5))
)
```

LOL is 3-5x shorter in tokens, easier for LLMs to generate correctly, and less prone to syntax errors (no bracket matching, no key quoting).

## Constraints for Scene Generation

- Keep total node count under 20 for real-time evaluation
- Maximum nesting depth: 6-8 levels
- All geometry should fit within bounds [-5, 5] on all axes
- Use `smooth_union` (k=0.1-0.3) for organic forms
- Use `subtract` / `smooth_subtract` for mechanical cuts
- Use `polar_repeat` / `repeat_finite` / `mirror` instead of duplicating nodes
- Ground: `plane(0.0, 1.0, 0.0, 0.0)` or `noise(0.1, 2.0, 42, plane(0.0, 1.0, 0.0, 0.0))`

### 3D Print Constraints

- **Nest `subtract` sequentially** when cutting multiple patterns. Do NOT `union` cutters together — it creates non-manifold edges. Instead of `subtract(base, union(cutter_a, cutter_b))`, write `subtract(subtract(base, cutter_a), cutter_b)`.
- **Avoid `intersection` with TPMS** (gyroid, schwarz_p, diamond_surface) for STL output — marching cubes produces non-manifold edges at TPMS surface boundaries. Use `subtract` with simple geometric patterns instead.
- **`lattice_infill` / `diamond_infill` / `schwarz_infill` are safe** — they expand to internal Union+Onion+Intersection, which meshes cleanly because the TPMS is fully enclosed.
- The `print_export` module automatically runs `MeshRepair::repair_all()` (degenerate triangle removal + vertex merge + normal fix) on every export.
- Keep mesh resolution ≤ 192 to stay under 1M triangles for slicer compatibility.
- Set slicer infill to **0%** when using `lattice_infill` / `diamond_infill` / `schwarz_infill` — the LOL code already defines internal structure.

## Parameter Convention

"Half" parameters mean HALF the total dimension:
- A box 4 units wide → `box3d(2.0, ...)` (half = 2.0)
- A cylinder 3 units tall → `cylinder(r, 1.5)` (half_height = 1.5)
- `rounded_box(hx, hy, hz, r)` — **all are half-extents**
- `cylinder(r, half_h)` — second arg is **half-height** (LOL runtime parser)

## Coordinate System

- Y-up, right-handed
- Origin (0,0,0) = center of scene
- Ground level = Y=0
- Scene bounds: [-5, 5] on all axes
- Default print scale: 1.0 LOL unit = 20mm (configurable via `PrintConfig::scale_mm`)

## SDF Optimization Rules for 3D Printing

### repeat_finite for Pattern Generation (MANDATORY)

Repeating patterns (holes, slots, cutouts) MUST use `repeat_finite`. Never generate individual `translate` × N nodes.

```
// NG: 200 nodes, O(n) eval per point
union(translate(a, hole), translate(b, hole), ...)

// OK: 1 node, O(1) eval per point
translate(cx, cy, 0, repeat_finite(count_x, count_y, 0, pitch_x, pitch_y, 0, hole))
```

**Effect**: 93% DSL size reduction, 58% mesh reduction, dramatically faster generation.

### Staggered Grid (e.g. SKADIS)

Two `repeat_finite` with `translate` offset:
```
union(
  translate(g1_cx, g1_cy, 0, repeat_finite(n, n, 0, pitch, pitch, 0, shape)),
  translate(g2_cx, g2_cy, 0, repeat_finite(m, m, 0, pitch, pitch, 0, shape))
)
```

### Through-Holes

Hole height must exceed plate thickness to guarantee penetration:
```
hole_half_height = (plate_thickness + 2.0) * 0.5
```

### Build Volume Validation

Always check output fits target printer:

| Printer | Max Size (mm, with 5mm margin) |
|---------|-------------------------------|
| Bambu H2D (single) | 315 × 310 × 315 |
| Bambu H2D (dual) | 290 × 310 × 315 |
| Bambu P1S / X1C | 246 × 246 × 246 |
| Bambu A1 mini | 170 × 170 × 170 |

### Multi-Part Assembly Validation (MANDATORY)

When generating multi-part designs (panel + connectors, case + lid, etc.),
**ALL parts must pass assembly validation before export.**

Rules:
1. **Hole diameters must match** across all parts using the same screw
2. **Part thicknesses must match** for flush assembly
3. **Hole spacings must be integer multiples** — connector hole pitch must equal or be a multiple of panel hole pitch
4. **Test alignment**: simulate placing connector on panel edge and verify holes align
5. **Edge-to-Edge Seam Rule**: When joining parts edge-to-edge, the distance between holes across the seam is exactly `margin_A + margin_B`. The connector's hole spacing MUST equal this sum
6. **Corner holes exist**: If a 4-way connector is designed, verify that the panel actually has holes at its corners — don't assume holes exist everywhere
7. **Grid Continuity**: For grid systems (SKADIS, Gridfinity), verify grid pitch continues uninterrupted across seams
8. **Slot clearance**: Clearance rules apply to ALL openings — rectangular slots (e.g. peg slots) need `+0.2 to +0.4mm` on BOTH width AND height, not just circular holes

```
// FATAL BUG example (actually happened):
// Panel holes: 40mm spacing
// Connector holes: 16mm spacing → NOT a multiple of 40 → connector won't fit!
//
// Fix: connector holes must be 40mm spacing (or 20mm, 80mm — integer multiples)
```

Anti-patterns:
- Designing connector hole spacing independently from panel — ALWAYS derive from panel pitch
- Assuming "close enough" spacing will work — FDM has 0.1-0.2mm accuracy, but 16mm vs 40mm is 24mm off
- Forgetting to check thickness match — 5mm panel + 4mm connector = misaligned screw depth
- Hard-coding interface dimensions inside functions — use SSOT global constants
- Skipping `buffer(0.01).buffer(-0.01)` after booleans — causes non-manifold mesh
- Exporting without `is_watertight` check — silent failure in slicer
- No build volume assertion — LLM generates objects larger than printer bed
- Cutouts overlapping functional holes (mount, connector) — the cutout eats the surrounding material and the hole vanishes into void. **Always add keepout zones around functional holes before generating cutouts**
- 4-way connector holes on axes (0,d) instead of diagonals (d,d) — screws hit the seam gap between panels. **Corner connector holes must be at (±d, ±d) diagonal positions**
- Mount holes placed inside peg slot Y-range — peg slots (height 15.3mm, center Y=10) extend from Y=2.35 to Y=17.65, swallowing any mount hole placed in that zone. **EDGE_MARGIN must be ≥ peg_height/2 + mount_radius + 5mm**
- Mount holes at same X as connector holes — larger mount hole visually merges with smaller connector hole. **Use different X positions for mount vs connector holes**
- Model Y-axis orientation in slicer — after `translate(-W/2, -H/2, 0)`, the panel's "top" edge (mount holes) appears at the bottom in most slicers. **Flip Y or clearly document orientation**
- Connector body too thin for screw head — `(body_edge - hole_center) < screw_head_radius` means screw head protrudes. **Minimum: `edge_margin = screw_head_R + 3mm` = 5.5mm for M2.5**
- Connector arm too narrow — wall between hole and arm edge < 2mm cracks under torque. **`arm_width ≥ 2 × (hole_offset + head_R + 3mm)`**
- Keyhole (daruma) slot direction — must extend AWAY from peg slots (toward panel edge). Verify: `slot_extremity < frame_width < peg_bottom_edge`
- Connector holes asymmetric (`arange(pitch, W, pitch)` starts at `pitch` but ends at `W-remainder`) — seam gap ≠ connector pitch. **Start at `EDGE_MARGIN`, verify `first_hole == W - last_hole`**
- Keyhole slot in wrong direction — slot must point toward panel edge (outward), not inward. Gravity pulls panel down → screw slides into slot toward edge → locks. **If slot points inward, panel falls off wall**
- Mount holes not symmetric around panel center — causes tilting when hung on 2 screws. **Verify: `X_left + X_right == PANEL_W` for each symmetric pair**
- Keyhole slots in different directions for top/bottom — gravity pulls panel DOWN uniformly, so ALL keyholes must have slots pointing the SAME direction (upward/toward top edge). Mixed directions make simultaneous locking physically impossible
- Connector holes not 180°-rotation-symmetric — modular panels must be interchangeable when rotated. **Build pattern from center outward using mirror: `conn_x_full = sorted(set(left_half + [W - x for x in left_half]))`**
- Panel outer shape with sharp 90° corners — causes warping on FDM bed, stress cracks at corner screw holes, and painful edges. **Always use `rounded_rect` (R ≥ 4mm) for panel outer shape, not `box`**
- Hard-coded derived values in connector functions — e.g. `[-20, 20]` instead of `[-GRID_PITCH/2, GRID_PITCH/2]`. **ALL derived values must trace back to SSOT constants**
- Same hole diameter on panel AND connector — both are clearance holes, screw falls through. **Panel = clearance hole (screw_dia + 0.2mm), Connector = tapping hole (screw_dia × 0.85 + 2×A)**
- Connector body extends into peg slot zone — oversized connector blocks peg insertion from behind. **Verify: `connector_half_height < peg_bottom_edge` with ≥1mm clearance**
- Sharp 90° corners from boolean intersection (e.g. cutout clipped by frame) — stress concentration causes cracks. **Use `buffer(0.5).buffer(-0.5)` after booleans to auto-fillet all sub-R0.5 corners**

### Single Source of Truth (SSOT) Rule

**CRITICAL for multi-part designs**: ALL interface dimensions must be defined as global
constants at the top of the script. Part generators MUST NOT compute their own values.

```
# GOOD: single definition, referenced everywhere
CONN_INSET = 4.0
CONN_PITCH = 40.0

def make_panel():
    hole_y = CONN_INSET  # ← references global
def make_connector():
    hole_y = CONN_INSET  # ← same global, guaranteed match

# BAD: each function computes independently
def make_panel():
    hole_y = OUTER_FRAME_W / 2.0  # = 4.0
def make_connector():
    hole_y = conn_inset + 1.0     # = 5.0 ← MISMATCH!
```

### Geometry Healing (Mandatory)

After every boolean operation, apply epsilon buffer cleanup:
```
result = shape.difference(cutout)
result = result.buffer(0.01).buffer(-0.01)  # removes micro-fragments
result = make_valid(result)                  # fixes self-intersections
```

Before every 3MF/STL export, validate and repair:
```
if not mesh.is_watertight:
    trimesh.repair.fill_holes(mesh)
    trimesh.repair.fix_normals(mesh)
assert mesh.is_watertight, "Mesh must be watertight for 3D printing"
```

### Mesh Resolution Guide

| Use | Resolution | Triangles | File Size |
|-----|-----------|-----------|-----------|
| Preview | 128 | ~500K | ~40MB |
| Print | 192 | ~1M | ~80MB |
| High quality | 256 | ~2M | ~170MB |

## Parametric Design Formulas (ALICE-Bamboo formulas.rs)

All dimensions derive from nozzle diameter (N), layer height (L), and material.

### Core Formulas

```
E = N × 1.125                           // extrusion width
min_wall = 3 × E                        // minimum wall thickness
clearance_snap = A + N × 0.5            // snap-fit clearance (per side)
clearance_slide = A + N × 0.75          // slide-fit clearance
pip_gap = L × 2                         // print-in-place gap
tap_hole = screw_dia × 0.85 + 2 × A    // tapping hole diameter
hole_model = target_dia + 2 × A         // FDM hole correction (circular)
slot_model_w = target_w + 2 × A        // FDM slot correction (rectangular — BOTH axes)
slot_model_h = target_h + 2 × A        // e.g. SKADIS peg: 5.0+0.3=5.3mm, 15.0+0.3=15.3mm
fillet = max(2 × N, 1.0)               // stress relief fillet
mount_hole_meat = max(hole_r + 5.0, frame_w) // load-bearing hole: ≥5mm meat to edge

// A = printer accuracy: Bambu=0.1mm, Prusa=0.15mm, Ender=0.2mm
```

### Standard Values (Bambu H2D, 0.4mm nozzle, 0.2mm layer, PLA)

| Parameter | Formula | Value |
|-----------|---------|-------|
| Extrusion width | N×1.125 | 0.45mm |
| Min wall | 3×E | 1.35mm |
| Snap clearance | A+N×0.5 | 0.3mm |
| Slide clearance | A+N×0.75 | 0.4mm |
| PiP gap | L×2 | 0.4mm |
| M3 tap hole | 3×0.85+0.2 | 2.75mm |

## Organizer Reference Dimensions

### Gridfinity Standard

```
grid_unit = 42.0mm
bin_size = 41.5mm (0.25mm clearance/side)
height_unit = 7.0mm
bin_ext_height = U × 7 + 4 (mm)
bin_int_depth = U × 7 - 3 (mm)
lip_height = 4.4mm
magnet = 6.0mm dia × 2.0mm
corner_fillet = 4.0mm
```

### IKEA SKADIS

```
peg_slot = 5 × 15mm (R2.5)
grid_pitch = 40mm
stagger_offset = 20mm
thickness = 5mm
edge_margin = peg_h/2 + min_wall = 8.85mm
```

### Standard Clearances (FDM, per side)

| Fit | Formula | 0.4mm nozzle |
|-----|---------|-------------|
| Press | A×0.5 | 0.05mm |
| Snug | A+N×0.25 | 0.2mm |
| Snap | A+N×0.5 | 0.3mm |
| Slide | A+N×0.75 | 0.4mm |
| Loose | A+N | 0.5mm |

### Common Object Dimensions

| Object | Key Dimension |
|--------|-------------|
| AA battery | 14.5mm dia × 50.5mm |
| 18650 cell | 18.4mm dia × 65.2mm |
| SD card | 24 × 32 × 2.1mm |
| M3 screw boss OD | 6.6mm (3×2.2) |
| 608 bearing | 22mm OD × 8mm ID × 7mm |
| K-Cup | 51mm top dia × 46mm H |
| Spice jar (standard) | 43-48mm dia |
| Toilet paper roll | 40mm bore × 120mm OD × 100mm W |

## Roblox Export (feature: `roblox`)

LOL DSL → OBJ/FBX for Roblox MeshPart / accessories.

### Quick Start

```rust
use alice_lol::roblox_export::{RobloxConfig, lol_to_obj_roblox};

lol_to_obj_roblox("smooth_union(0.3, sphere(1.0), box3d(0.5, 0.5, 0.5))", "hat.obj", &RobloxConfig::accessory())?;
```

### Roblox Constraints

| Constraint | Accessory | MeshPart |
|-----------|-----------|----------|
| Max triangles | 4,000 | 10,000 |
| Max size (studs) | 10×10×10 | 10×10×10 |
| Format | OBJ / FBX | OBJ / FBX |
| Coordinate | Y-up right-hand | Y-up right-hand |

### Presets

| Preset | `resolution` | `max_triangles` | Use case |
|--------|-------------|-----------------|----------|
| `RobloxConfig::accessory()` | 128 | 4,000 | UGC hats, weapons, decorations |
| `RobloxConfig::meshpart()` | 192 | 10,000 | General MeshParts |
| `RobloxConfig::preview()` | 64 | 4,000 | Fast preview |

### Design Tips for Roblox Accessories

- Keep shapes simple: `smooth_union` > many `subtract` (fewer triangles)
- Scale: default `scale_studs = 2.0` → 1.0 SDF unit = 2 studs
- Avoid thin features: Roblox rendering may Z-fight on <0.05 stud thickness
- No TPMS infill (`lattice_infill` etc.) — internal geometry wastes triangle budget
- Use `round()` modifier for smoother edges with fewer triangles
