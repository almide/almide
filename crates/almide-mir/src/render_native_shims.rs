//! The native runtime shims — the tiny Rust functions the rung-subset
//! renderer links against (`rt_*`), each the EXACT v0 native oracle
//! expression so the differential gate pins byte-equality. One table row per
//! shim: (MIR call name, param types, return type, Rust source). Adding a
//! shim is adding a row; `shim` itself never changes.

use super::render_native::NTy;

const SHIMS: &[(&str, &[NTy], Option<NTy>, &str)] = &[
    ("int.to_string", &[NTy::I64],
            Some(NTy::Str),
            "fn rt_int_to_string(n: i64) -> String { n.to_string() }"),
    ("print_str", &[NTy::Str],
            None,
            "fn rt_print_str(s: &str) { println!(\"{}\", s); }"),
    // The §13 abort convention's message channel (assert desugar, time-ctor
    // negative trap): stderr line, exact v0 oracle behavior.
    ("eprintln", &[NTy::Str],
            None,
            "fn rt_eprintln(s: &str) { eprintln!(\"{}\", s); }"),
    ("__str_concat", &[NTy::Str, NTy::Str],
            Some(NTy::Str),
            "fn rt_str_concat(a: &str, b: &str) -> String { [a, b].concat() }"),
    ("string.eq", &[NTy::Str, NTy::Str],
            Some(NTy::I64),
            "fn rt_string_eq(a: &str, b: &str) -> i64 { (a == b) as i64 }"),
    ("string.len", // Codepoint count, NOT byte length (C-016 discipline).
            &[NTy::Str],
            Some(NTy::I64),
            "fn rt_string_len(s: &str) -> i64 { s.chars().count() as i64 }"),
    // String predicates/transforms: each shim is the EXACT v0 native oracle
    // expression (runtime/rs/src/string.rs delegates to Rust std the same way),
    // so the differential gate pins byte-equality, and C-016/C-019/C-020's
    // full-Unicode discipline carries over unchanged.
    ("string.contains", &[NTy::Str, NTy::Str],
            Some(NTy::I64),
            "fn rt_string_contains(s: &str, sub: &str) -> i64 { s.contains(sub) as i64 }"),
    ("string.starts_with", &[NTy::Str, NTy::Str],
            Some(NTy::I64),
            "fn rt_string_starts_with(s: &str, p: &str) -> i64 { s.starts_with(p) as i64 }"),
    ("string.ends_with", &[NTy::Str, NTy::Str],
            Some(NTy::I64),
            "fn rt_string_ends_with(s: &str, p: &str) -> i64 { s.ends_with(p) as i64 }"),
    ("string.to_upper", &[NTy::Str],
            Some(NTy::Str),
            "fn rt_string_to_upper(s: &str) -> String { s.to_uppercase() }"),
    ("string.to_lower", &[NTy::Str],
            Some(NTy::Str),
            "fn rt_string_to_lower(s: &str) -> String { s.to_lowercase() }"),
    ("string.trim", &[NTy::Str],
            Some(NTy::Str),
            "fn rt_string_trim(s: &str) -> String { s.trim().to_string() }"),
    ("string.repeat", &[NTy::Str, NTy::I64],
            Some(NTy::Str),
            // The SAME clamp + ceiling as the v0 runtime and the wasm self-host
            // (stdlib/string_repeat.almd): a negative count is the empty string
            // (C-054), and a result past the shared 2^31 ceiling aborts in the
            // T6 form. The bare `s.repeat(n as usize)` turned `repeat(s, -1)`
            // into a capacity-overflow PANIC (exit 101) on this leg while the
            // wasm leg printed normally — a crash-form divergence the C-161
            // rule forbids (differential fuzz, seed 1785015406589852000 index
            // 1012). Keep in sync with ALMIDE_REPEAT_MAX_BYTES.
            "fn rt_string_repeat(s: &str, n: i64) -> String {\n    let n = n.max(0);\n    if (s.len() as i64).saturating_mul(n) > (1i64 << 31) {\n        eprintln!(\"Error: repeat result too large\");\n        std::process::exit(1);\n    }\n    s.repeat(n as usize)\n}"),
    ("string.cmp", // Byte-wise lexicographic, -1/0/1 (C-019: rt_string_extra cmp = native oracle).
            &[NTy::Str, NTy::Str],
            Some(NTy::I64),
            "fn rt_string_cmp(a: &str, b: &str) -> i64 {\n    match a.cmp(b) { std::cmp::Ordering::Less => -1, std::cmp::Ordering::Equal => 0, std::cmp::Ordering::Greater => 1 }\n}"),
    ("float.to_string", // The EXACT v0 native oracle (runtime/rs/src/float.rs::almide_rt_float_to_string):
            // shortest round-trip Display, integral values forced to a `.0` tail.
            &[NTy::F64],
            Some(NTy::Str),
            "fn rt_float_to_string(n: f64) -> String {\n    let s = format!(\"{}\", n);\n    if n.fract() == 0.0 && !s.contains('.') && !s.contains(\"inf\") && !s.contains(\"NaN\") {\n        format!(\"{}.0\", s)\n    } else {\n        s\n    }\n}"),
    ("__chk_div", &[NTy::I64, NTy::I64],
            Some(NTy::I64),
            "fn rt_chk_div(a: i64, b: i64) -> i64 {\n    if b == 0 { eprintln!(\"Error: division by zero\"); std::process::exit(1); }\n    if a == i64::MIN && b == -1 { eprintln!(\"Error: integer overflow\"); std::process::exit(1); }\n    a / b\n}"),
    ("__chk_mod", &[NTy::I64, NTy::I64],
            Some(NTy::I64),
            // v0's `almide_mod` macro prints "division by zero" for a zero rhs (mod and
            // div share the message — the C-002 oracle text); keep byte parity.
            "fn rt_chk_mod(a: i64, b: i64) -> i64 {\n    if b == 0 { eprintln!(\"Error: division by zero\"); std::process::exit(1); }\n    if a == i64::MIN && b == -1 { eprintln!(\"Error: integer overflow\"); std::process::exit(1); }\n    a % b\n}"),
    // The unsigned 64-bit lane (#872): the i64 slot carries the u64 bit
    // pattern. Same divide-by-zero message bytes; no MIN÷-1 case unsigned.
    ("__chk_div_u", &[NTy::I64, NTy::I64],
            Some(NTy::I64),
            "fn rt_chk_div_u(a: i64, b: i64) -> i64 {\n    if b == 0 { eprintln!(\"Error: division by zero\"); std::process::exit(1); }\n    ((a as u64) / (b as u64)) as i64\n}"),
    ("__chk_mod_u", &[NTy::I64, NTy::I64],
            Some(NTy::I64),
            "fn rt_chk_mod_u(a: i64, b: i64) -> i64 {\n    if b == 0 { eprintln!(\"Error: division by zero\"); std::process::exit(1); }\n    ((a as u64) % (b as u64)) as i64\n}"),
];

pub(crate) fn shim(name: &str) -> Option<(&'static [NTy], Option<NTy>, &'static str)> {
    SHIMS.iter().find(|(n, ..)| *n == name).map(|(_, p, r, src)| (*p, *r, *src))
}

pub(crate) fn shim_rust_name(name: &str) -> String {
    format!("rt_{}", name.trim_start_matches("__").replace('.', "_"))
}

// ── The metering shims (fuel charge / strict cut / timeout / budget) ──

/// T1-1: the strict-cut return marker — every Charge site emits
/// `if __almd_fuel_lt0() { <marker> }` and `render_fn` patches the marker
/// with a `return <default of the fn's ret type>;` once the ret NTy is known
/// (the same late-patch technique as the if-value JOIN markers).
pub(crate) const CUT_RET_MARKER: &str = "/*__CUT_RET__*/";

/// The exhaustion read the strict cut branches on.
pub(crate) const FUEL_LT0_SHIM: &str =
    "fn __almd_fuel_lt0() -> bool { __ALMD_FUEL.with(|f| f.get()) < 0 }";

/// T3-5: the dynamic (size-proportional) charge — 1 + len/16, same trace
/// arithmetic as the static charge.
pub(crate) const CHARGE_DYN_SHIM: &str = "fn __almd_charge_dyn(site: i64, len: i64, trace: bool) {
    __ALMD_FUEL.with(|f| f.set(f.get().wrapping_sub(1 + (len >> 4))));
    if trace {
        __ALMD_TRACE.with(|t| t.set(t.get().wrapping_mul(1000003).wrapping_add(site)));
    }
}";

/// T5-1: the wall-deadline shims (fan.timeout). The clock is monotonic
/// (`Instant` since first use); in REPLAY mode (`ALMIDE_OMEGA` baked at
/// compile time) the clock is never read — the baked ordinal decides the
/// cut. RECORD mode (`ALMIDE_OMEGA_RECORD=1` baked) prints
/// `__ALMD_OMEGA <ord>` on stderr at each fired region exit.
pub(crate) static TIMEOUT_SHIM: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    TIMEOUT_SHIM_TEMPLATE
        .replace("__ALMD_OMEGA_V__", &crate::charge_probe::omega_replay().to_string())
        .replace("__ALMD_OMEGA_REC__", if crate::charge_probe::omega_record() { "true" } else { "false" })
});

pub(crate) const TIMEOUT_SHIM_TEMPLATE: &str = "const __ALMD_OMEGA: i64 = __ALMD_OMEGA_V__;
pub(crate) const __ALMD_OMEGA_RECORD: bool = __ALMD_OMEGA_REC__;
pub(crate) static __ALMD_T0: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new(); // wasm-safe: GENERATED native-only shim (fan.timeout reads the wall clock at program runtime — oracle-tier semantics, never the compile path)
fn __almd_now_ns() -> i64 {
    __ALMD_T0.get_or_init(std::time::Instant::now).elapsed().as_nanos() as i64 // wasm-safe: generated shim (see above)
}
thread_local! {
    static __ALMD_T_DEADLINE: std::cell::Cell<i64> = const { std::cell::Cell::new(i64::MAX) };
    static __ALMD_T_HIT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static __ALMD_T_VERDICT: std::cell::Cell<i64> = const { std::cell::Cell::new(0) };
    static __ALMD_T_ORD: std::cell::Cell<i64> = const { std::cell::Cell::new(0) };
}
fn __almd_timeout_enter(d_ns: i64) -> i64 {
    let saved = __ALMD_T_DEADLINE.with(|d| d.get());
    let now = if __ALMD_OMEGA >= 0 { 0 } else { __almd_now_ns() };
    let dl = now.saturating_add(d_ns);
    if dl < saved {
        __ALMD_T_DEADLINE.with(|d| d.set(dl));
    }
    saved
}
fn __almd_timeout_exit(saved: i64) -> i64 {
    let hit = __ALMD_T_HIT.with(|h| h.get());
    __ALMD_T_VERDICT.with(|v| v.set(hit as i64));
    if __ALMD_OMEGA_RECORD && hit {
        eprintln!(\"__ALMD_OMEGA {}\", __ALMD_T_ORD.with(|o| o.get()));
    }
    __ALMD_T_HIT.with(|h| h.set(false));
    __ALMD_T_DEADLINE.with(|d| d.set(saved));
    0
}
fn __almd_timeout_hit() -> i64 {
    __ALMD_T_VERDICT.with(|v| v.get())
}
fn __almd_wall_hit() -> bool {
    if __ALMD_T_DEADLINE.with(|d| d.get()) == i64::MAX {
        return false;
    }
    if __ALMD_T_HIT.with(|h| h.get()) {
        return true;
    }
    let ord = __ALMD_T_ORD.with(|o| { o.set(o.get() + 1); o.get() });
    if __ALMD_OMEGA >= 0 {
        if ord >= __ALMD_OMEGA {
            __ALMD_T_HIT.with(|h| h.set(true));
        }
        return __ALMD_T_HIT.with(|h| h.get());
    }
    if __almd_now_ns() >= __ALMD_T_DEADLINE.with(|d| d.get()) {
        __ALMD_T_HIT.with(|h| h.set(true));
    }
    __ALMD_T_HIT.with(|h| h.get())
}";

/// Stage 1 probe shim: fuel/trace thread-locals + the charge fn + the guard
/// that prints the triple's (consumed, trace) legs on main exit. Same hash
/// arithmetic as the wasm leg (wrapping i64, trace*1000003+site).
pub(crate) const COUNTER_SHIM: &str = "thread_local! {
    static __ALMD_FUEL: std::cell::Cell<i64> = const { std::cell::Cell::new(i64::MAX) };
    static __ALMD_FUEL_ENTRY: std::cell::Cell<i64> = const { std::cell::Cell::new(0) };
    static __ALMD_B_VERDICT: std::cell::Cell<i64> = const { std::cell::Cell::new(0) };
    static __ALMD_B_SPEND: std::cell::Cell<i64> = const { std::cell::Cell::new(0) };
    static __ALMD_TRACE: std::cell::Cell<i64> = const { std::cell::Cell::new(0) };
}";

/// The probe charge fn: fuel counts DOWN (consumed = MAX - fuel); the trace is
/// the order-sensitive hash. Only probe builds call it with tracing on.
pub(crate) const CHARGE_SHIM: &str = "fn __almd_charge(site: i64, cost: i64, trace: bool) {
    __ALMD_FUEL.with(|f| f.set(f.get().wrapping_sub(cost)));
    if trace {
        __ALMD_TRACE.with(|t| t.set(t.get().wrapping_mul(1000003).wrapping_add(site)));
    }
}
struct __AlmdProbeGuard;
impl Drop for __AlmdProbeGuard {
    fn drop(&mut self) {
        eprintln!(\"__ALMD_PROBE {} {}\",
            (i64::MAX.wrapping_sub(__ALMD_FUEL.with(|f| f.get()))) as u64,
            __ALMD_TRACE.with(|t| t.get()) as u64);
    }
}";

/// Stage 2 budget fns — the exact wasm-leg arithmetic (min-cap, lazy verdict,
/// streaming exit). The ns→unit divisor is injected from the single CM-1
/// definition ([`crate::charge_probe::CM1_NS_PER_CHARGE`]) so this shim cannot
/// drift from the wasm BudgetEnter render.
pub(crate) static BUDGET_SHIM: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    BUDGET_SHIM_TEMPLATE
        .replace("__ALMD_CM1_NS__", &crate::charge_probe::CM1_NS_PER_CHARGE.to_string())
});

pub(crate) const BUDGET_SHIM_TEMPLATE: &str = "fn __almd_budget_enter(budget_ns: i64) -> i64 {
    let units = budget_ns / __ALMD_CM1_NS__;
    __ALMD_FUEL_ENTRY.with(|e| e.set(units));
    let saved = __ALMD_FUEL.with(|f| f.get());
    if units < saved {
        __ALMD_FUEL.with(|f| f.set(units));
    }
    saved
}
fn __almd_budget_exhausted() -> i64 {
    __ALMD_B_VERDICT.with(|v| v.get())
}
fn __almd_budget_exit(saved: i64) -> i64 {
    __ALMD_B_VERDICT.with(|v| v.set(i64::from(__ALMD_FUEL.with(|f| f.get()) < 0)));
    let consumed = __ALMD_FUEL_ENTRY.with(|e| e.get()) - __ALMD_FUEL.with(|f| f.get());
    __ALMD_B_SPEND.with(|s| s.set(consumed));
    __ALMD_FUEL.with(|f| f.set(saved - consumed));
    0
}
fn __almd_budget_spend() -> i64 {
    __ALMD_B_SPEND.with(|s| s.get())
}";
