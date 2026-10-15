//! 単位の宣言 (`units(<unit>, <purpose>, <program>)`) の試験
//!
//! 1. [`Unit`] / [`Purpose`] は言語の定義 (`alice-lol/language/constructs.toml` の `unit` 表と
//!    `purpose`) と名前・順序・倍率まで一致する (文法も同じ表から生成する)
//! 2. `parse_declared` は宣言を読み、`parse_lol` / `parse_program` は宣言を黙って捨てずに止まる
//! 3. 宣言した program の書き出しは、倍率が単位の mm、範囲が形の外接箱で、寸法は閉形式と一致する
//!    (oracle: 球の直径 = 2r、箱の辺 = 2h、単位の換算は定義の mm) 宣言が無ければ今までどおりで
//!    `LOL-U001` を出す

use alice_lol::print_export::{declared_config, node_to_mesh, ExportError, PrintConfig};
use alice_lol::runtime_parser::{parse_declared, parse_lol, parse_program};
use alice_lol::units::{Purpose, Unit, UnitsDecl};
use glam::Vec3;

const DEFINITION: &str = include_str!("../language/constructs.toml");

/// `[[unit]]` の行 (name, mm) を定義の順に
fn definition_units() -> Vec<(String, Option<f64>)> {
    let mut out: Vec<(String, Option<f64>)> = Vec::new();
    let mut in_unit = false;
    for line in DEFINITION.lines().map(str::trim) {
        if line.starts_with('[') {
            in_unit = line == "[[unit]]";
            if in_unit {
                out.push((String::new(), None));
            }
            continue;
        }
        if !in_unit {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        let last = out.last_mut().expect("a key before any [[unit]]");
        match key.trim() {
            "name" => value.trim_matches('"').clone_into(&mut last.0),
            "mm" => last.1 = Some(value.parse().expect("mm is a number")),
            _ => {}
        }
    }
    out
}

fn definition_list(key: &str) -> Vec<String> {
    let line = DEFINITION
        .lines()
        .find(|l| l.starts_with(&format!("{key} = [")))
        .unwrap_or_else(|| panic!("no `{key} = [` line"));
    line.split('[')
        .nth(1)
        .unwrap()
        .trim_end_matches(']')
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .collect()
}

#[test]
fn units_are_the_definition_table_in_order_with_its_scale() {
    let rows = definition_units();
    assert_eq!(rows.len(), Unit::ALL.len(), "{rows:?}");
    for (unit, (name, mm)) in Unit::ALL.iter().zip(&rows) {
        assert_eq!(unit.name(), name);
        // the f32 of the code against the f64 of the definition: within one f32 ulp of the value
        match (unit.mm_per_unit(), mm) {
            (Some(code), Some(def)) => {
                assert!(
                    (f64::from(code) - def).abs() <= def * f64::from(f32::EPSILON),
                    "{name}"
                );
            }
            (code, def) => assert_eq!(code.is_some(), def.is_some(), "{name}"),
        }
        assert_eq!(Unit::from_name(name), Some(*unit));
    }
}

#[test]
fn purposes_are_the_definition_list_in_order() {
    let names = definition_list("purpose");
    assert_eq!(
        names,
        Purpose::ALL.iter().map(|p| p.name()).collect::<Vec<_>>()
    );
}

#[test]
fn a_unit_without_length_is_for_render_only() {
    let mut compared = 0;
    for unit in Unit::ALL {
        for purpose in Purpose::ALL {
            let ok = UnitsDecl::new(unit, purpose).is_some();
            assert_eq!(
                ok,
                unit.mm_per_unit().is_some() || purpose == Purpose::Render,
                "{unit:?} {purpose:?}"
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 10);
}

#[test]
fn parse_declared_reads_the_declaration_and_the_body() {
    let d =
        parse_declared("units(in, render, program(sphere(1.0), entities(box3d(0.1, 0.1, 0.1))))")
            .unwrap();
    let u = d.units.unwrap();
    assert_eq!((u.unit, u.purpose), (Unit::In, Purpose::Render));
    assert_eq!(d.program.sdf_registry.len(), 1);
    let d = parse_declared(" units( cm ,print, sphere(2.0) ) ").unwrap();
    assert_eq!(d.units.unwrap().unit, Unit::Cm);
    assert!(parse_declared("sphere(1.0)").unwrap().units.is_none());
}

#[test]
fn parse_declared_refuses_bad_declarations() {
    for (text, why) in [
        ("units(km, print, sphere(1.0))", "unknown unit"),
        ("units(mm, show, sphere(1.0))", "unknown purpose"),
        ("units(world, print, sphere(1.0))", "render only"),
        (
            "units(mm, print, units(mm, print, sphere(1.0)))",
            "only at the root",
        ),
        ("units(mm, print, sphere(1.0)", "expected"),
        ("units(mm, print, sphere(1.0)) x", "trailing"),
    ] {
        let e = parse_declared(text).expect_err(text);
        assert!(e.message.contains(why), "{text}: {}", e.message);
    }
}

#[test]
fn parse_lol_and_parse_program_do_not_drop_a_declaration() {
    for text in [
        "units(mm, print, sphere(1.0))",
        " units(world, render, sphere(1.0))",
    ] {
        assert!(
            parse_lol(text)
                .unwrap_err()
                .message
                .contains("parse_declared"),
            "{text}"
        );
        assert!(
            parse_program(text)
                .unwrap_err()
                .message
                .contains("parse_declared"),
            "{text}"
        );
    }
}

fn mesh_extent_mm(text: &str, base: &PrintConfig) -> Vec3 {
    let d = parse_declared(text).unwrap();
    let (config, warnings) = declared_config(&d, base).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let mesh = node_to_mesh(&d.program.sdf, &config);
    let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    for v in &mesh.vertices {
        lo = lo.min(v.position);
        hi = hi.max(v.position);
    }
    hi - lo
}

/// 球の直径 = 2r (mm に換算)、箱の辺 = 2h、許容は格子の cell 幅 (範囲 / 解像度 × 倍率) の 2 倍
#[test]
fn a_declared_program_exports_at_its_unit_and_is_not_clipped() {
    let base = PrintConfig::default();
    let mut compared = 0;
    for (text, want_mm) in [
        ("units(mm, print, sphere(25.0))", Vec3::splat(50.0)),
        ("units(cm, print, sphere(2.5))", Vec3::splat(50.0)),
        ("units(m, print, sphere(0.025))", Vec3::splat(50.0)),
        (
            "units(in, print, box3d(1.0, 0.5, 2.0))",
            Vec3::new(50.8, 25.4, 101.6),
        ),
        (
            "units(mm, print, translate(300.0, 0.0, 0.0, sphere(40.0)))",
            Vec3::splat(80.0),
        ),
    ] {
        let got = mesh_extent_mm(text, &base);
        let cell = want_mm.max_element() * 1.1 / f32::from(u16::try_from(base.resolution).unwrap());
        assert!(
            (got - want_mm).abs().max_element() <= 2.0 * cell,
            "{text}: {got} vs {want_mm} (cell {cell})"
        );
        compared += 1;
    }
    assert_eq!(compared, 5);
}

#[test]
fn an_undeclared_program_keeps_the_config_and_warns() {
    let base = PrintConfig::default();
    let d = parse_declared("sphere(1.0)").unwrap();
    let (config, warnings) = declared_config(&d, &base).unwrap();
    assert_eq!(
        (config.scale_mm, config.bounds_min, config.bounds_max),
        (base.scale_mm, base.bounds_min, base.bounds_max)
    );
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].code, "LOL-U001");
    assert!(
        warnings[0].message.contains("scale_mm=10"),
        "{}",
        warnings[0].message
    );
    assert!(warnings[0].to_string().starts_with("warning[LOL-U001]: "));
}

#[test]
fn a_world_scene_or_a_shape_past_the_search_box_is_not_exported() {
    let base = PrintConfig::default();
    for (text, why) in [
        ("units(world, render, sphere(1.0))", "no physical size"),
        ("units(mm, print, sphere(1500.0))", "search box"),
        ("units(m, print, box3d(0.2, 0.2, 2.0))", "search box"),
    ] {
        let d = parse_declared(text).unwrap();
        match declared_config(&d, &base) {
            Err(ExportError::Units(m)) => assert!(m.contains(why), "{text}: {m}"),
            other => panic!("{text}: {other:?}"),
        }
    }
}

#[test]
fn lol_to_stl_uses_the_declaration_and_reports_the_warning() {
    let dir = std::env::temp_dir().join(format!("alice-lol-units-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let base = PrintConfig::preview();
    let declared = alice_lol::print_export::lol_to_stl(
        "units(mm, print, sphere(25.0))",
        dir.join("a.stl"),
        &base,
    )
    .unwrap();
    assert!(declared.warnings.is_empty());
    let plain =
        alice_lol::print_export::lol_to_stl("sphere(1.0)", dir.join("b.stl"), &base).unwrap();
    assert_eq!(
        plain.warnings.iter().map(|w| w.code).collect::<Vec<_>>(),
        ["LOL-U001"]
    );
    let e = alice_lol::print_export::lol_to_3mf(
        "units(world, render, sphere(1.0))",
        dir.join("c.3mf"),
        &base,
    )
    .unwrap_err();
    assert!(matches!(e, ExportError::Units(_)), "{e}");
    std::fs::remove_dir_all(&dir).ok();
}

/// oracle: a cube of half 10 mm turned 45° about x spans 20 mm in x and 20·√2 in y and z; the
/// bounds are that box plus the 2-cell margin
#[test]
fn the_bounds_of_a_rotated_box_are_its_closed_form_box() {
    let base = PrintConfig::default();
    let d = parse_declared("units(mm, print, rotate(45.0, 0.0, 0.0, box3d(10.0, 10.0, 10.0)))")
        .unwrap();
    let (config, _) = declared_config(&d, &base).unwrap();
    let want = Vec3::new(20.0, 20.0 * 2.0_f32.sqrt(), 20.0 * 2.0_f32.sqrt());
    let pad = 2.0 * want.max_element() / f32::from(u16::try_from(base.resolution).unwrap());
    let got = config.bounds_max - config.bounds_min - Vec3::splat(2.0 * pad);
    assert!((got - want).abs().max_element() < 0.01, "{got} vs {want}");
}
