//! 研究 Law のデモ: 理想気体の式を SI と (kPa, L) の 2 通りで書き、
//! 別条件での再計算 / 同条件での比較 / oracle 照合 / 新しい証拠の判定を行う
//!
//! ```text
//! cargo run -p alice-lol --example research_law_demo
//! ```

use alice_lol::research_law::{
    compare, Bridge, IngestPolicy, LawExpr, Observation, Param, Provenance, ResearchLaw,
    ResearchLawError, ResearchOracle, ResearchVerdict, Unit, ValidRange, Var,
};

/// CODATA 2018 の気体定数 [J/(mol K)]
const R: f64 = 8.314_462_618;

fn ideal_gas(
    name: &str,
    p_unit: &str,
    v_unit: &str,
    v_range: ValidRange,
) -> Result<ResearchLaw, ResearchLawError> {
    ResearchLaw::new(
        name,
        "n*R*T/V",
        Var::new("P", p_unit),
        &[
            Var::new("n", "mol"),
            Var::new("T", "K"),
            Var::new("V", v_unit),
        ],
        &[Param::new("R", R, 0.0, "J/(mol*K)")],
        &[
            (
                "n",
                ValidRange {
                    lo: 0.01,
                    hi: 100.0,
                },
            ),
            (
                "T",
                ValidRange {
                    lo: 1.0,
                    hi: 2000.0,
                },
            ),
            ("V", v_range),
        ],
        Provenance::new("ideal gas equation of state", "closed form"),
    )
}

fn main() -> Result<(), ResearchLawError> {
    let si = ideal_gas(
        "ideal gas (SI)",
        "Pa",
        "m^3",
        ValidRange { lo: 1e-6, hi: 10.0 },
    )?
    .with_oracle(ResearchOracle::new(
        &[("n", 1.0), ("T", 273.15), ("V", 0.022_413_969_54)],
        101_325.0,
        1.0,
        "molar volume of an ideal gas at 273.15 K and 101.325 kPa",
    ));
    let kpa_l = ideal_gas(
        "ideal gas (kPa, L)",
        "kPa",
        "L",
        ValidRange { lo: 1e-3, hi: 1e4 },
    )?;

    println!("{}", si.describe());

    // 別条件での再計算 (測っていない条件)
    let p = si.evaluate(&[("n", 2.0), ("T", 350.0), ("V", 0.04)])?;
    println!("P(n=2 mol, T=350 K, V=0.04 m^3) = {p:.3} Pa");

    // 成立範囲の外は値を返さない
    match si.evaluate(&[("n", 1.0), ("T", 5000.0), ("V", 1.0)]) {
        Err(e) => println!("T = 5000 K: {e}"),
        Ok(v) => println!("T = 5000 K: unexpected value {v}"),
    }

    // 同条件での比較 (単位は bridge が換算する)
    let bridge = Bridge::new(&[("n", "n"), ("T", "T"), ("V", "V")]);
    let c = compare(
        &si,
        &kpa_l,
        &bridge,
        &[("n", 1.0), ("T", 300.0), ("V", 0.025)],
    )?;
    println!(
        "compare: {:.6} Pa vs {:.6} Pa (relative difference {:.2e})",
        c.a_value, c.b_value_in_a_unit, c.relative
    );

    for (i, outcome) in si.check_oracles().iter().enumerate() {
        println!(
            "oracle {i}: passed = {} (error {:?})",
            outcome.passed, outcome.error
        );
    }

    // 新しい証拠: R を 0.5% 大きく測った観測は、parameter の更新で説明できる
    let observations: Vec<Observation> =
        [(1.0, 300.0, 0.025), (2.0, 400.0, 0.05), (0.5, 250.0, 0.01)]
            .iter()
            .map(|&(n, t, v)| {
                Observation::new(&[("n", n), ("T", t), ("V", v)], n * R * 1.005 * t / v)
            })
            .collect();
    let policy = IngestPolicy {
        abs_tolerance: 1e-6,
        break_factor: 4.0,
    };
    match si.ingest(&observations, &policy) {
        ResearchVerdict::ParameterUpdate {
            previous_rms,
            updated,
        } => println!(
            "ingest: parameter update (rms before {previous_rms:.3} Pa), R = {:?}",
            updated.param("R")
        ),
        other => println!("ingest: {other:?}"),
    }

    inspect(&si, &bridge, &observations)
}

/// 法則が持つ構成要素 (式 / 単位 / 成立範囲 / 証拠 / 残差) を個別に読む
fn inspect(
    law: &ResearchLaw,
    bridge: &Bridge,
    observations: &[Observation],
) -> Result<(), ResearchLawError> {
    let gas_constant = Unit::parse("J/(mol*K)")?;
    println!(
        "unit J/(mol*K): dimension {} (exponents {:?}), scale to SI {}",
        gas_constant.dimension(),
        gas_constant.dimension().exponents(),
        gas_constant.scale()
    );
    let expr = LawExpr::parse("n*R*T/V")?;
    println!("identifiers of {}: {:?}", expr.text(), expr.identifiers());
    println!(
        "constant expression 2^10 = {}",
        LawExpr::parse("2^10")?.evaluate_constant()?
    );

    println!(
        "{}: {} [{}] = {}",
        law.name(),
        law.output().name,
        law.output().unit,
        law.expression().text()
    );
    for var in law.inputs() {
        println!(
            "  input {} [{}] range {:?}",
            var.name,
            var.unit,
            law.validity(&var.name)
        );
    }
    for p in law.params() {
        println!("  param {} = {} [{}]", p.name, p.value, p.unit);
    }
    println!(
        "  source: {}, oracle cases: {}, bridge pairs: {:?}",
        law.provenance().source,
        law.oracles().len(),
        bridge.pairs()
    );

    // 観測を証拠として保持すると、残差はその証拠から測った値になる
    let with_evidence = law.clone().with_evidence(observations)?;
    let r = with_evidence.residual();
    println!(
        "  evidence: {} observations, residual rms {:.3} Pa, max {:.3} Pa (before: {} observations)",
        with_evidence.evidence().len(),
        r.rms,
        r.max_abs,
        law.residual().n
    );
    Ok(())
}
