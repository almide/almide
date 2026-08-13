//! ADR-0001 S6 matrix gates for the time-unit surface.
//!
//! Every case here is GENERATED from `almide_lang::time_units` (the single
//! source) — adding a unit or a clock extends the gate in the same change,
//! so the surface cannot drift point-wise (S6-1, S6-4). The consuming-surface
//! table is pinned against the S4 clock column (S6-6).

use almide::canonicalize;
use almide::check::Checker;
use almide::diagnostic::Level;
use almide::lexer::Lexer;
use almide::parser::Parser;
use almide_lang::time_units::{
    unit_set_hint, TIME_CONSUMING_SURFACES, TIME_MODULES, TIME_UNITS,
};

fn check(input: &str) -> Vec<(Level, String, String)> {
    let tokens = Lexer::tokenize(input);
    let mut parser = Parser::new(tokens);
    let mut prog = parser.parse().expect("parse failed");
    let canon = canonicalize::canonicalize_program(&prog, std::iter::empty());
    let mut checker = Checker::from_env(canon.env);
    checker.diagnostics = canon.diagnostics;
    let diags = checker.infer_program(&mut prog);
    diags.into_iter().map(|d| (d.level, d.message, d.hint)).collect()
}

fn errors(input: &str) -> Vec<String> {
    check(input)
        .into_iter()
        .filter(|(l, _, _)| *l == Level::Error)
        .map(|(_, m, _)| m)
        .collect()
}

fn bounded_with(budget: &str) -> String {
    format!(
        "fn work() -> Int = 1\n\
         effect fn main() -> Unit = {{\n\
           let r = fan.bounded({budget}) {{ work() }} ?? -1\n\
           println(int.to_string(r))\n\
         }}\n"
    )
}

/// S6-1: all 12 constructor cells (2 clocks × 6 units) exist and carry their
/// nominal type. Cell detection is by CONSUMPTION: a `compute.*` budget
/// type-checks clean; a `duration.*` budget produces exactly the clock-mixing
/// error (which proves the cell resolved AND returned Duration — an unknown
/// unit would give the unknown-unit diagnostic instead).
#[test]
fn s6_1_constructor_matrix_all_cells() {
    for (unit, _) in TIME_UNITS {
        let errs = errors(&bounded_with(&format!("compute.{unit}(5)")));
        assert!(errs.is_empty(), "compute.{unit}: expected clean, got {errs:?}");

        let errs = errors(&bounded_with(&format!("duration.{unit}(5)")));
        assert_eq!(
            errs,
            vec!["expected Compute, found Duration".to_string()],
            "duration.{unit}: expected exactly the clock-mixing error"
        );
    }
}

/// S6-4: the unit-name set is closed and single-sourced — an unknown unit is
/// rejected on BOTH modules, and the diagnostic's closed-set hint is the
/// generated one (so the hint can never disagree with the actual set).
#[test]
fn s6_4_unknown_units_rejected_with_generated_hint() {
    for (module, _) in TIME_MODULES {
        for bad in ["msec", "sec", "m", "d", "millis"] {
            assert!(
                unit_set_hint(module).contains("ns / us / ms / s / min / h"),
                "hint must enumerate the closed set"
            );
            let diags = check(&bounded_with(&format!("{module}.{bad}(5)")));
            let hit = diags.iter().any(|(l, m, h)| {
                *l == Level::Error
                    && m == &format!("unknown unit '{module}.{bad}'")
                    && h == &unit_set_hint(module)
            });
            assert!(hit, "{module}.{bad}: expected unknown-unit error with the generated hint, got {diags:?}");
        }
    }
}

/// Bare Int is a named type error (the S2 firewall): a budget without a clock
/// never type-checks.
#[test]
fn s6_bare_int_budget_is_a_type_error() {
    let errs = errors(&bounded_with("5000"));
    assert_eq!(errs, vec!["expected Compute, found Int".to_string()]);
}

/// S6-3 (UFCS half): `n.ms()` — a unit name alone cannot pick a clock, so the
/// diagnostic must name BOTH constructor candidates instead of a nearest-match
/// guess.
#[test]
fn s6_3_ufcs_unit_is_ambiguous_naming_both_clocks() {
    for unit in ["ms", "us"] {
        let src = format!(
            "fn work() -> Int = 1\n\
             effect fn main() -> Unit = {{\n\
               let n = 100\n\
               let r = fan.bounded(n.{unit}()) {{ work() }} ?? -1\n\
               println(int.to_string(r))\n\
             }}\n"
        );
        let diags = check(&src);
        let hit = diags.iter().any(|(l, m, h)| {
            *l == Level::Error
                && m == &format!("ambiguous time unit '.{unit}()': the unit does not name a clock")
                && h.contains(&format!("compute.{unit}(n)"))
                && h.contains(&format!("duration.{unit}(n)"))
        });
        assert!(hit, ".{unit}(): expected the both-candidates diagnostic, got {diags:?}");
    }
}

/// S3 (T2-5): the operator matrix — accepted cells type-check clean, every
/// rejected cell carries its NAMED error. Generated over both clocks where
/// the rule is clock-symmetric.
#[test]
fn s3_operator_matrix() {
    let wrap = |expr: &str| {
        format!(
            "fn work() -> Int = 1\n\
             effect fn main() -> Unit = {{\n\
               let c = compute.ms(2)\n\
               let d = compute.ms(3)\n\
               let w = duration.ms(2)\n\
               let x = {expr}\n\
               let r = fan.bounded(compute.ms(1)) {{ work() }} ?? -1\n\
               println(int.to_string(r))\n\
             }}\n"
        )
    };
    // Accepted cells (the algebra): T+T, T-T, T*Int, Int*T, T<T, T==T.
    for good in ["c + d", "c - d", "c * 4", "4 * c", "w + w", "if c < d then 1 else 0", "if c == d then 1 else 0"] {
        let errs = errors(&wrap(good));
        assert!(errs.is_empty(), "S3 accepted cell `{good}` must be clean, got {errs:?}");
    }
    // Rejected cells, each with its named diagnostic.
    for (bad, msg) in [
        ("c * d", "cannot multiply two time quantities"),
        ("c * w", "cannot multiply two time quantities"),
        ("c + w", "cannot add Compute and Duration"),
        ("c - w", "cannot subtract Compute and Duration"),
        ("c + 5", "operator '+' needs two Compute values"),
        ("5 - c", "operator '-' needs two Compute values"),
        ("c / 2", "operator '/' is not defined on time types"),
        ("c % 2", "operator '%' is not defined on time types"),
        ("if c < w then 1 else 0", "cannot compare Compute and Duration"),
        ("if c < 5 then 1 else 0", "cannot compare Compute with Int"),
    ] {
        let errs = errors(&wrap(bad));
        assert!(
            errs.iter().any(|m| m.contains(msg)),
            "S3 rejected cell `{bad}`: expected `{msg}`, got {errs:?}"
        );
    }
}

/// T3-8 adjudication pin: a `fan { }` PARALLEL block can never appear inside
/// a metered region, because the region body is checked PURE and `fan {}`
/// demands an effect context (E007). This is what makes the native
/// thread-local fuel counters safe: no metered region ever spans threads.
/// (The other direction — a metered region INSIDE a fan arm — is defined and
/// deterministic per arm: each region's enter/exit brackets its own
/// execution context on every leg.)
#[test]
fn t3_8_fan_parallel_inside_metered_region_is_rejected() {
    let src = "fn work(n: Int) -> Int = n * 10\n\
         fn helper() -> Int = {\n\
           let (a, b) = fan { work(1), work(2) }\n\
           a + b\n\
         }\n\
         effect fn main() -> Unit = {\n\
           let r = fan.bounded(compute.ms(1)) { helper() } ?? -1\n\
           println(int.to_string(r))\n\
         }\n";
    let errs = errors(src);
    assert!(
        errs.iter().any(|m| m.contains("fan block can only be used inside an effect fn")),
        "fan {{}} inside a metered region must be an E007-class error, got {errs:?}"
    );
}

/// S6-6: the clock-declaration face — the declared surface set is exactly the
/// S4 clock column. A new time-consuming surface must extend
/// `TIME_CONSUMING_SURFACES` (the checker's budget typing reads it and panics
/// on an undeclared surface), and then THIS pin, in the same change.
#[test]
fn s6_6_clock_declaration_matrix_matches_s4() {
    let mut declared: Vec<(&str, &str)> = TIME_CONSUMING_SURFACES.to_vec();
    declared.sort();
    assert_eq!(
        declared,
        vec![
            ("fan.bounded", "Compute"),
            ("fan.race", "Compute"),
            ("fan.timeout", "Duration"),
        ],
        "S4 clock column drifted — update ADR-0001 S4, the table, and this pin together"
    );
}
