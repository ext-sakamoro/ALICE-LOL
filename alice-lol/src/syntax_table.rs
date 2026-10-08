//! The names the runtime parser accepts, grouped as the README lists them
//!
//! [`crate::runtime_parser`] dispatches on a `match` over names, so there is no
//! list to export without changing how the parser works. This table is that
//! list, written once: the tests below keep it equal to the parser in both
//! directions (every name in the table is dispatched, every dispatched name is
//! in the table), and `scripts/readme_sync.py` keeps the syntax tables of
//! `README.md` / `README_JP.md` equal to it. The table exists only for those
//! checks, so it is compiled for tests only.

/// SDF construct names per README group (`parse_lol` / `parse_expr_inner`)
#[rustfmt::skip] // one line per group of names, not one line per name
const SDF_SYNTAX: &[(&str, &[&str])] = &[
    (
        "primitives",
        &[
            "sphere", "box3d", "rounded_box", "cylinder", "torus", "cone", "capsule", "capsule_ab",
            "ellipsoid", "plane", "octahedron", "rounded_cone", "pyramid", "hex_prism", "link",
            "capped_cone", "capped_torus", "rounded_cylinder", "tube", "barrel", "heart", "egg",
            "helix", "tetrahedron", "box_frame", "diamond", "star_polygon", "cross_shape",
            "triangle", "bezier", "triangular_prism", "cut_sphere", "cut_hollow_sphere",
            "death_star", "solid_angle", "rhombus", "horseshoe", "vesica", "infinite_cylinder",
            "infinite_cone", "gyroid", "chamfered_cube", "schwarz_p", "superellipsoid", "rounded_x",
            "pie", "trapezoid", "parallelogram", "tunnel", "uneven_capsule", "arc_shape", "moon",
            "blobby_cross", "parabola_segment", "regular_polygon", "stairs_prim", "dodecahedron",
            "icosahedron", "truncated_octahedron", "truncated_icosahedron", "diamond_surface",
            "neovius", "lidinoid", "iwp", "frd", "fischer_koch_s", "pmy", "circle_2d", "rect_2d",
            "segment_2d", "rounded_rect_2d", "annular_2d", "terrain",
        ],
    ),
    (
        "csg",
        &[
            "union", "smooth_union", "intersection", "smooth_intersection", "subtract",
            "smooth_subtract", "chamfer_union", "chamfer_intersection", "chamfer_subtraction",
            "stairs_union", "stairs_intersection", "stairs_subtraction", "xor", "pipe", "engrave",
            "groove", "tongue", "columns_union", "columns_intersection", "columns_subtraction",
            "exp_smooth_union", "exp_smooth_intersection", "exp_smooth_subtraction",
        ],
    ),
    (
        "transforms",
        &[
            "translate", "rotate", "scale", "scale_non_uniform",
        ],
    ),
    (
        "modifiers",
        &[
            "round", "onion", "twist", "bend", "mirror", "repeat", "elongate", "revolution",
            "extrude", "taper", "displacement", "polar_repeat", "shear", "noise", "repeat_finite",
            "octant_mirror", "icosahedral_symmetry", "with_material", "surface_roughness",
            "sweep_bezier",
        ],
    ),
    (
        "print",
        &[
            "lattice_infill", "diamond_infill", "schwarz_infill",
        ],
    ),
    (
        "time",
        &[
            "animate", "morph",
        ],
    ),
    (
        "stdlib",
        &[
            "shopping_cart_coin", "skadis_panel", "skadis_hook_l", "skadis_hook_j", "skadis_hook_s",
            "skadis_container", "skadis_clip", "skadis_shelf", "skadis_elastic_cord", "mug",
            "gridfinity_bin", "gridfinity_bin_ex", "wall_hook", "drawer_organizer", "shelf_divider",
            "sticky_note_holder", "business_card_holder", "pen_cup", "phone_stand",
            "headphone_holder", "under_desk_mount", "desk_shelf", "monitor_riser", "coaster",
            "tissue_box_cover", "storage_box", "cable_clip", "led_channel", "card_tray",
            "token_well", "wrench_holder", "socket_rail", "hex_bit_holder", "raspi_case",
            "esp32_enclosure", "battery_18650_holder", "toothbrush_holder", "drill_bit_holder",
            "pliers_rack", "spice_rack", "egg_tray", "utensil_caddy", "filament_spool_holder",
            "nozzle_holder", "build_plate_rack", "cutlery_tray", "pill_organizer", "magnetic_strip",
            "hairdryer_holder", "kcup_holder", "hex_key_holder", "wrap_holder", "sock_divider",
            "soap_tray", "razor_holder", "chopstick_holder", "swatch_holder", "tp_holder",
            "sd_card_holder", "driver_rack", "cotton_dispenser", "sink_caddy", "clamp_rack",
            "dry_box", "outdoor_enclosure", "jewelry_stand", "phone_dock", "cutting_board_rack",
            "tape_dispenser", "shower_caddy", "caliper_holder", "bag_clip_org", "can_rack",
            "led_hub_box", "makeup_organizer", "vesa_mount", "l_bracket", "t_slot_bracket_2020",
            "raspi_mount_plate", "heat_set_array", "flange_mount", "dovetail_pair",
            "profile_extrusion", "snap_fit_pair", "boss_array", "screw_hole", "tap_hole",
            "counterbore", "countersink", "heat_set_hole", "bolt", "bracket_l", "flange_circular",
            "t_slot_2020", "profile_2020", "profile_3030", "dovetail", "slot", "snap_fit_annular",
            "pin_hinge_knuckle", "boss", "rib", "bearing_seat", "rack_shelf", "cable_grommet",
            "curtain_rod_bracket", "dowel_hole", "wood_screw_pilot", "arduino_mount_plate",
            "pixhawk_mount", "servo_mount", "jst_ph_slot",
        ],
    ),
];

/// Intent verb names (`parse_program`, the third argument of `program(...)`)
#[rustfmt::skip]
const INTENT_SYNTAX: &[&str] = &[
    "grasp", "release", "catch", "walk", "gaze", "point", "throw", "push", "pull", "turn",
    "align", "follow", "avoid", "rest", "latent", "seq", "par", "music",
];

/// The clause keywords of an audit law (`parse_law`), one per line of the law
const LAW_SYNTAX: &[&str] = &["audit", "evidence", "expect", "range"];

/// SDF names the `lol!` macro does not accept (runtime parser only), in
/// addition to the whole `stdlib` group
const RUNTIME_ONLY: &[&str] = &["capsule_ab"];

#[cfg(test)]
mod tests {
    use super::{INTENT_SYNTAX, LAW_SYNTAX, RUNTIME_ONLY, SDF_SYNTAX};
    use crate::runtime_parser::{parse_law, parse_lol, parse_program};
    use std::collections::BTreeSet;

    const PARSER_SRC: &str = include_str!("runtime_parser.rs");

    /// The body of `fn <name>` in `runtime_parser.rs` (up to the next item at
    /// the same indentation)
    fn fn_body(name: &str) -> &'static str {
        let head = format!("fn {name}(");
        let start = PARSER_SRC
            .find(&head)
            .unwrap_or_else(|| panic!("runtime_parser.rs has no `{head}`"));
        let rest = &PARSER_SRC[start..];
        let end = rest[head.len()..]
            .find("\n    fn ")
            .map_or(rest.len(), |i| i + head.len());
        &rest[..end]
    }

    /// The names of the top-level `"name" =>` / `"a" | "b" =>` arms of the
    /// dispatch `match` in a function body (nested matches are indented deeper)
    fn dispatched(body: &str) -> Vec<String> {
        let mut out = Vec::new();
        for line in body.lines() {
            let Some(arm) = line.strip_prefix("            \"") else {
                continue;
            };
            let Some((names, _)) = arm.split_once("=>") else {
                continue;
            };
            for name in format!("\"{names}").split('"').skip(1).step_by(2) {
                out.push(name.to_string());
            }
        }
        out
    }

    fn table_sdf() -> Vec<&'static str> {
        SDF_SYNTAX
            .iter()
            .flat_map(|(_, names)| names.iter().copied())
            .collect()
    }

    fn assert_no_duplicates(names: &[&str], what: &str) {
        let mut seen = BTreeSet::new();
        let dup: Vec<_> = names.iter().filter(|n| !seen.insert(**n)).collect();
        assert!(dup.is_empty(), "{what}: listed twice: {dup:?}");
    }

    #[test]
    fn sdf_table_equals_the_parser_dispatch() {
        let table = table_sdf();
        assert_no_duplicates(&table, "SDF_SYNTAX");
        let parser = dispatched(fn_body("parse_expr_inner"));
        assert!(parser.len() > 200, "found only {} arms", parser.len());
        let parser: BTreeSet<&str> = parser.iter().map(String::as_str).collect();
        let table: BTreeSet<&str> = table.into_iter().collect();
        let missing: Vec<_> = parser.difference(&table).collect();
        let extra: Vec<_> = table.difference(&parser).collect();
        assert!(
            missing.is_empty() && extra.is_empty(),
            "parser names not in SDF_SYNTAX: {missing:?}; SDF_SYNTAX names the parser does not dispatch: {extra:?}"
        );
    }

    #[test]
    fn intent_table_equals_the_parser_dispatch() {
        assert_no_duplicates(INTENT_SYNTAX, "INTENT_SYNTAX");
        let parser = dispatched(fn_body("parse_intent"));
        let parser: BTreeSet<&str> = parser.iter().map(String::as_str).collect();
        let table: BTreeSet<&str> = INTENT_SYNTAX.iter().copied().collect();
        assert!(table.len() >= 10, "INTENT_SYNTAX has {} names", table.len());
        assert_eq!(parser, table, "parse_intent arms vs INTENT_SYNTAX");
    }

    #[test]
    fn law_table_equals_the_parser_dispatch() {
        assert_no_duplicates(LAW_SYNTAX, "LAW_SYNTAX");
        let parser = dispatched(fn_body("parse_law"));
        let parser: BTreeSet<&str> = parser.iter().map(String::as_str).collect();
        let table: BTreeSet<&str> = LAW_SYNTAX.iter().copied().collect();
        assert!(table.len() >= 4, "LAW_SYNTAX has {} names", table.len());
        assert_eq!(parser, table, "parse_law arms vs LAW_SYNTAX");
    }

    #[test]
    fn every_law_keyword_reaches_its_arm() {
        // 表に載っている keyword は、引数が足りない形でも「未知の語」ではなく
        // その腕の中で落ちる (= 表と parser が同じものを指している行動的な確認)
        let unknown = parse_law("audit a\nnot_a_law_keyword x\n")
            .unwrap_err()
            .message;
        assert!(unknown.contains("unknown audit law clause"), "{unknown}");
        for name in LAW_SYNTAX {
            let err = parse_law(&format!("audit a\n{name}\n"))
                .unwrap_err()
                .message;
            assert!(
                !err.contains("unknown audit law clause"),
                "`{name}` is in LAW_SYNTAX but the parser does not know it: {err}"
            );
        }
    }

    #[test]
    fn every_sdf_name_reaches_its_arm() {
        // Behavioural check of the same equality: an incomplete call of a
        // listed name fails inside its arm, never with the unknown-name error
        let unknown = parse_lol("not_a_lol_name(").unwrap_err().message;
        assert!(unknown.contains("unknown LOL expression"), "{unknown}");
        for name in table_sdf() {
            let err = parse_lol(&format!("{name}(")).unwrap_err().message;
            assert!(
                !err.contains("unknown LOL expression"),
                "`{name}` is in SDF_SYNTAX but the parser does not know it: {err}"
            );
        }
    }

    #[test]
    fn every_intent_verb_reaches_its_arm() {
        let call = |verb: &str| format!("program(sphere(1.0), entities(), {verb}(");
        let unknown = parse_program(&call("not_a_verb")).unwrap_err().message;
        assert!(unknown.contains("unknown intent verb"), "{unknown}");
        for verb in INTENT_SYNTAX {
            let err = parse_program(&call(verb)).unwrap_err().message;
            assert!(
                !err.contains("unknown intent verb"),
                "`{verb}` is in INTENT_SYNTAX but the parser does not know it: {err}"
            );
        }
    }

    #[test]
    fn runtime_only_names_are_sdf_names_outside_stdlib() {
        let table: BTreeSet<&str> = table_sdf().into_iter().collect();
        let stdlib: BTreeSet<&str> = SDF_SYNTAX
            .iter()
            .filter(|(g, _)| *g == "stdlib")
            .flat_map(|(_, n)| n.iter().copied())
            .collect();
        assert!(!stdlib.is_empty());
        for name in RUNTIME_ONLY {
            assert!(table.contains(name) && !stdlib.contains(name), "{name}");
        }
    }
}
