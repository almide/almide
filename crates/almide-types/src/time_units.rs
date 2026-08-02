//! ADR-0001 time-unit surface, single-sourced (S2 / S4 / S6).
//!
//! Every consumer of the closed unit set — the checker's constructor
//! resolution, the lowering's ns erasure, diagnostics hints, and the matrix
//! gates — reads THESE tables. Adding a unit or a clock here extends every
//! consumer and every gate at once; adding it anywhere else is a bug the
//! matrix gate (`tests/time_units_matrix_test.rs`) exists to catch.

/// CM-1 v0.3: nanoseconds per charge unit — the calibration constant between
/// the declared clock and the abstract machine's charge units. RECALIBRATED
/// 2026-08-02 by the standing D5 gate: heavy(100M) = 100,000,003 units ran
/// 0.25s native release AND 0.28s wasmtime (Apple M-class, min of 3) →
/// ~2.5ns/unit on BOTH targets; pinned at 3 to center the ADR-0001 D5
/// declared 5x band. RATIO-ONLY contract. Consumed by the wasm BudgetEnter
/// render, the native BUDGET_SHIM template, the interp's budget prims, and
/// `--time-report` — via this ONE definition (re-exported as
/// `almide_mir::charge_probe::CM1_NS_PER_CHARGE`).
pub const CM1_NS_PER_CHARGE: i64 = 3;

/// The closed unit set (S2): unit name → nanoseconds per unit. Order is the
/// canonical display order used in diagnostics.
pub const TIME_UNITS: &[(&str, i64)] = &[
    ("ns", 1),
    ("us", 1_000),
    ("ms", 1_000_000),
    ("s", 1_000_000_000),
    ("min", 60_000_000_000),
    ("h", 3_600_000_000_000),
];

/// The two nominal clocks (S1): constructor module → checker type name.
/// The types exist only in the checker (the clock firewall); lowering erases
/// both to a plain Int of nanoseconds.
pub const TIME_MODULES: &[(&str, &str)] = &[("compute", "Compute"), ("duration", "Duration")];

/// S4 clock column: every surface that CONSUMES a time quantity, with the
/// clock it reads. The checker resolves a budget parameter's expected clock
/// through [`surface_clock`] — a time-consuming surface missing from this
/// table fails loudly on its first type-check (the S6-6 face check).
pub const TIME_CONSUMING_SURFACES: &[(&str, &str)] =
    &[("fan.bounded", "Compute"), ("fan.race", "Compute"), ("fan.timeout", "Duration")];

/// Nanoseconds per unit, `None` for a name outside the closed set.
pub fn unit_factor(unit: &str) -> Option<i64> {
    TIME_UNITS.iter().find(|(n, _)| *n == unit).map(|(_, f)| *f)
}

/// Checker type name for a constructor module (`compute` → `Compute`).
pub fn clock_type_of_module(module: &str) -> Option<&'static str> {
    TIME_MODULES.iter().find(|(m, _)| *m == module).map(|(_, t)| *t)
}

/// The declared clock a surface reads (`fan.bounded` → `Compute`).
pub fn surface_clock(surface: &str) -> Option<&'static str> {
    TIME_CONSUMING_SURFACES.iter().find(|(s, _)| *s == surface).map(|(_, c)| *c)
}

/// The closed-set hint shown on an unknown unit — the matrix answer beats a
/// nearest-match guess (LLMs invent `msec` / `5m`).
pub fn unit_set_hint(module: &str) -> String {
    let names: Vec<&str> = TIME_UNITS.iter().map(|(n, _)| *n).collect();
    format!("The unit set is closed: {module}.{}", names.join(" / "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factors_are_strictly_increasing_and_exact() {
        for w in TIME_UNITS.windows(2) {
            assert!(w[0].1 < w[1].1, "unit factors must be strictly increasing");
        }
        assert_eq!(unit_factor("ns"), Some(1));
        assert_eq!(unit_factor("h"), Some(3_600_000_000_000));
        assert_eq!(unit_factor("msec"), None);
    }

    #[test]
    fn clock_tables_pin_the_adr_shape() {
        // S1: exactly two clocks. S4: every declared surface reads a clock
        // that exists. A new clock or surface extends these tables — and this
        // pin — in the same change.
        assert_eq!(TIME_MODULES.len(), 2);
        for (surface, clock) in TIME_CONSUMING_SURFACES {
            assert!(
                TIME_MODULES.iter().any(|(_, t)| t == clock),
                "surface {surface} declares unknown clock {clock}"
            );
        }
        assert_eq!(surface_clock("fan.bounded"), Some("Compute"));
        assert_eq!(surface_clock("fan.race"), Some("Compute"));
        assert_eq!(surface_clock("fan.timeout"), Some("Duration"));
    }
}
