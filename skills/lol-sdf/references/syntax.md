# ALICE-LOL DSL Syntax Reference (Runtime Parser, 124 Constructs)

**Source of truth**: `~/ALICE-LOL/LLM_REFERENCE.md` (parent crate). This file mirrors the essentials for skill self-containment; if you need the full LLM-oriented guide (semantic anti-patterns, worked examples, coordinate-frame gotchas), read the parent doc.

## Syntax rules

1. Every construct is `name(args)` — function-call style
2. Arguments are comma-separated: numbers (`f32`) or nested expressions
3. Numbers: `1.0`, `0.5`, `-2.3` (bare integer `1` also accepted)
4. Nesting: `translate(0.0, 1.0, 0.0, sphere(0.5))`
5. Operations take 2+ children: `union(sphere(1.0), box3d(0.5, 0.5, 0.5))`
6. No trailing commas
7. `//` line comments and whitespace allowed between tokens
8. **Variable capture** (`{rust_expr}` and bare identifiers) is `proc_macro`-only — the runtime parser rejects these. LLM output should emit numeric constants only.

## Primitives (71)

### Basic solids
- `sphere(r)` — radius
- `box3d(hx, hy, hz)` — half-extents
- `rounded_box(hx, hy, hz, r)` — half-extents + round radius
- `cylinder(r, h)` — radius, half-height (Y-axis)
- `torus(R, r)` — major, minor (XZ plane)
- `cone(r, h)` — radius, half-height
- `capsule(r, h)` — radius, half-height (tube with hemispherical caps)
- `ellipsoid(rx, ry, rz)` — radii per axis
- `plane(nx, ny, nz, d)` — normal + distance (infinite half-space)
- `octahedron(s)` / `tetrahedron(s)` / `dodecahedron(s)` / `icosahedron(s)` — Platonic solids

### Cylinders / cones variants
- `rounded_cone(r1, r2, h)` — cone with spherical ends
- `capped_cone(h, r1, r2)` — frustum
- `capped_torus(R, r, angle)` — partial torus
- `rounded_cylinder(r, rr, h)` — cylinder with rounded edges
- `tube(r_out, t, h)` — hollow cylinder (outer radius, wall thickness, half-height)
- `barrel(r, h, b)` — cylinder with parabolic bulge
- `infinite_cylinder(...)` / `infinite_cone(...)` — for repetition modifiers
- `hex_prism(r, h)` — hexagonal column

### Special shapes
- `pyramid(h)` — 4-sided
- `link(l, r1, r2)` — chain link
- `helix(R, r, pitch, h)` — spiral tube
- `heart(s)` / `egg(ra, rb)` / `diamond(r, h)` / `star_polygon(r, n, m, h)` / `cross_shape(l, t, r, h)` / `box_frame(hx, hy, hz, e)` (wireframe box)
- `triangle(...)`, `bezier(...)`, `triangular_prism(...)`
- `cut_sphere(...)`, `cut_hollow_sphere(...)`, `death_star(...)`, `solid_angle(...)`
- `rhombus(...)`, `horseshoe(...)`, `vesica(...)`
- `superellipsoid(...)`, `rounded_x(...)`, `pie(...)`, `trapezoid(...)`, `parallelogram(...)`
- `tunnel(...)`, `uneven_capsule(...)`, `arc_shape(...)`, `moon(...)`, `blobby_cross(...)`, `parabola_segment(...)`, `regular_polygon(...)`
- `stairs_prim(...)`, `chamfered_cube(...)`, `truncated_octahedron(s)`, `truncated_icosahedron(s)`

### TPMS (Triply Periodic Minimal Surfaces)
- `gyroid`, `schwarz_p`, `diamond_surface`, `neovius`, `lidinoid`, `iwp`, `frd`, `fischer_koch_s`, `pmy`

For lightweight lattice infill, **prefer `lattice_infill` / `diamond_infill` / `schwarz_infill` modifiers** (see Modifiers section) over raw TPMS + `intersection` — the modifiers guarantee manifold mesh.

### 2D primitives (for `extrude` / `revolution`)
- `circle_2d`, `rect_2d`, `segment_2d`, `rounded_rect_2d`, `annular_2d`

## CSG Operations (23)

### Sharp
- `union(a, b, ...)` — combine (OR)
- `intersection(a, b, ...)` — keep overlap (AND)
- `subtract(a, b)` — a minus b (asymmetric)
- `xor(a, b)` — symmetric difference
- `pipe(r, a, b)` — pipe along intersection edge
- `engrave(r, a, b)` — carve pattern into surface
- `groove(ra, rb, a, b)` / `tongue(ra, rb, a, b)` — tongue-and-groove joint

### Smooth (organic blend, first arg is blend radius `k`)
- `smooth_union(k, a, b, ...)`, `smooth_intersection(k, a, b, ...)`, `smooth_subtract(k, a, b)`
- `exp_smooth_union(k, a, b, ...)`, `exp_smooth_intersection`, `exp_smooth_subtraction`

### Chamfered (hard bevel)
- `chamfer_union(r, a, b, ...)`, `chamfer_intersection`, `chamfer_subtraction(r, a, b)`

### Stepped (staircase)
- `stairs_union(r, n, a, b, ...)`, `stairs_intersection`, `stairs_subtraction`

### Columnar
- `columns_union(r, n, a, b, ...)`, `columns_intersection`, `columns_subtraction`

## Transforms (4)

- `translate(x, y, z, child)`
- `rotate(rx, ry, rz, child)` — Euler degrees
- `scale(s, child)` — uniform
- `scale_non_uniform(sx, sy, sz, child)`

## Modifiers (20)

### Surface
- `round(r, child)` — global corner rounding
- `onion(t, child)` — hollow shell of thickness `t`
- `shell(t, child)` — alias for onion in many contexts
- `surface_roughness(...)`, `displacement(...)`, `noise(...)`

### Deformation
- `twist(angle_deg, child)`, `bend(angle_deg, child)`, `taper(...)`, `elongate(...)`

### Repetition
- `repeat(...)`, `repeat_finite(...)`, `polar_repeat(n, child)`, `mirror(axis, child)`
- `octant_mirror(child)`, `icosahedral_symmetry(child)` — full symmetry groups

### Generation from 2D
- `revolution(child_2d)` — revolve around Y
- `extrude(h, child_2d)` — extrude along Z
- `sweep_bezier(...)`

### 3D Print structural intent (safe for mesh export)
- `lattice_infill(...)`, `diamond_infill(...)`, `schwarz_infill(...)`

### Material
- `with_material(mat_id, child)` — tag for downstream PBR pipeline

## Time control (2)

- `animate(track, child)` — keyframe animation
- `morph(t, a, b)` — linear morph between two SDF trees at parameter `t`

## Laws (3) — proc_macro only

The runtime parser does **not** accept Law declarations. Apply laws in Rust:

- `NonOverlap { a, b, priority }` — a and b must not intersect
- `Containment { inner, outer, priority }` — inner must be fully inside outer
- `MinThickness { child, min_mm, priority }` — every wall of `child` at least `min_mm` thick

`priority` is `Hard` (violation → error) or `Soft` (violation → warning with residual).

## Composition rules (critical for print safety)

1. **`subtract` must nest** for multiple cutters:
   - ❌ `subtract(base, union(hole1, hole2, hole3))` — non-manifold risk
   - ✅ `subtract(subtract(subtract(base, hole1), hole2), hole3)`
2. **TPMS infill**: use the `*_infill` modifier, not raw `intersection(shell, gyroid)`.
3. **Resolution**: mesh generation ≤ 192 to stay under 1M triangles.
4. **Coordinate frame**: Y-up, right-hand.

## Example

```
smooth_union(0.2,
    sphere(1.0),
    translate(2.0, 0.0, 0.0, box3d(0.5, 0.5, 0.5))
)
```

Compiles to a smoothly-blended sphere + offset box. Evaluable on CPU / GPU, transpilable to GLSL / WGSL / HLSL, exportable to STL / 3MF / OBJ / GLB.

## See also

- `print-guide.md` — 3D print-specific system prompt (material presets, anti-patterns)
- `lol.gbnf` — GBNF grammar for constrained decoding
- Parent `~/ALICE-LOL/LLM_REFERENCE.md` for full worked examples and semantic guidance
