//! MIR → native Rust renderer — the NATIVE leg of the trust spine (#764).
//!
//! Renders the SAME Perceus-disciplined MIR the wasm leg consumes, mapping the
//! ownership ops onto Rust's own memory management instead of literal RC:
//!
//!   Dup        → `.clone()` / `.to_string()` (a new owned handle; the clone IS the +1)
//!   Drop       → erased — Rust's scope-end (or reassignment) drop realizes the free.
//!                `verify_ownership` certifies the balance on the SAME ops pre-render.
//!   CallFn     → a user fn call, or a CLOSED runtime-boundary shim (`print_str`,
//!                `int.to_string`, `__str_concat`, …) mapped to native Rust —
//!                mirroring v0's runtime/rs floor, never re-implemented inline
//!
//! Ownership modes across calls follow the MIR call-mode signature: a heap arg is
//! BORROWED (`&str` param), a heap result is FRESH OWNED (`String` return).
//!
//! HONEST WALL: anything outside the rung subset returns `Err(LowerError::
//! Unsupported)` — the CLI falls back to v0. A rendered program is never wrong;
//! an unrenderable one declines loudly. Same discipline as the wasm ladder.
//!
//! Rung-2 subset: i64 scalars; String values (literals, `int.to_string`,
//! `__str_concat`, `string.eq`, `string.len`); String params/returns on user fns;
//! full scalar-or-String control flow (if-as-value, loops).

use crate::lower::LowerError;
use crate::{CallArg, Init, IntOp, MirFunction, MirProgram, Op, Repr, ValueId};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// The native type a MIR value renders to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum NTy {
    I64,
    /// An OWNED `String` local (fresh from a literal alloc, a heap-returning
    /// call, or a clone).
    Str,
    /// A BORROWED `&str` — a heap fn param (the MIR call mode borrows heap args).
    StrRef,
    /// An OWNED `Vec<i64>` local (rung 4 — a scalar-list literal / clone / call result).
    Vec,
    /// A BORROWED `&[i64]` — a scalar-list fn param (the same borrow call mode).
    VecRef,
    /// A real `f64` local (rung 5 — no i64-bits convention on native). MIR
    /// carries Float as i64 BITS; the boundary into a float op converts via
    /// `f64::from_bits` (bit-exact), and every float-op result stays `f64`.
    F64,
    /// An OWNED `Result<i64, String>` local — the T1-3 native Result carrier
    /// (produced by the `native_result_rewrite` prims / a Res-returning call).
    Res,
}

impl NTy {
    fn is_stringy(self) -> bool {
        matches!(self, NTy::Str | NTy::StrRef)
    }
    fn is_veccy(self) -> bool {
        matches!(self, NTy::Vec | NTy::VecRef)
    }
}

/// The DECLARED signature kind of a param/return, computed by the pipeline from the
/// Almide-level `Ty` (a `MirParam` carries only reprs — `Repr::Ptr` is String OR
/// List, and only the declaration disambiguates). Rung 4: scalar lists join strings.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NativeSigKind {
    I64,
    Str,
    ListI64,
    F64,
    /// `Result<i64, String>` — the T1-3 native Result carrier (returns only).
    Res,
}

/// fn name → (param kinds, return kind; None = Unit). Built by the pipeline where
/// the declared `Ty` is visible; the render trusts it (the precision wall already
/// rejected anything outside these kinds).
pub type NativeSigs = std::collections::BTreeMap<String, (Vec<NativeSigKind>, Option<NativeSigKind>)>;

fn wall(msg: impl Into<String>) -> LowerError {
    LowerError::Unsupported(msg.into())
}

fn var(v: ValueId) -> String {
    format!("v{}", v.0)
}

/// The rung-4 bounds-checked element accessors — byte-identical abort text to the
/// wasm `$elem_addr_chk` ("Error: index out of bounds" + exit 1) and to v0 native.
const IDX_GET_SHIM: &str = "fn almide_idx_get(v: &[i64], i: i64) -> i64 {\n        if i < 0 || i as usize >= v.len() { eprintln!(\"Error: index out of bounds\"); std::process::exit(1); }\n        v[i as usize]\n}";
const IDX_SET_SHIM: &str = "fn almide_idx_set(v: &mut Vec<i64>, i: i64, x: i64) {\n        if i < 0 || i as usize >= v.len() { eprintln!(\"Error: index out of bounds\"); std::process::exit(1); }\n        v[i as usize] = x;\n}";

/// Borrow a stringy value as `&str` for a call argument.
fn as_str_arg(code: &str, t: NTy) -> String {
    match t {
        NTy::Str => format!("&{code}"),
        NTy::StrRef => code.to_string(),
        NTy::Vec | NTy::VecRef => unreachable!("as_str_arg on vec"),
        NTy::I64 | NTy::F64 => unreachable!("as_str_arg on scalar"),
        NTy::Res => unreachable!("as_str_arg on a Result carrier"),
    }
}

/// Read a MIR scalar as a real `f64`: an I64 local holds the f64 BITS (the MIR
/// Float convention — every float literal is a `ConstInt` of the bits), an F64
/// local IS the value. Bit-exact either way.
fn as_f64_arg(code: &str, t: NTy) -> Result<String, LowerError> {
    match t {
        NTy::F64 => Ok(code.to_string()),
        NTy::I64 => Ok(format!("f64::from_bits({code} as u64)")),
        _ => Err(wall("native: float op on a heap value")),
    }
}

/// The CLOSED runtime-boundary map: self-hosted runtime fn name → (arg NTys
/// [stringy args listed as `Str`], result, native Rust shim). Adding a name here
/// is adding to the trusted floor — keep it tiny; everything else walls. Every
/// addition needs a differential-corpus row in the same PR
/// (tests/native_v1_differential_test.rs).
fn shim(name: &str) -> Option<(&'static [NTy], Option<NTy>, &'static str)> {
    match name {
        "int.to_string" => Some((
            &[NTy::I64],
            Some(NTy::Str),
            "fn rt_int_to_string(n: i64) -> String { n.to_string() }",
        )),
        "print_str" => Some((
            &[NTy::Str],
            None,
            "fn rt_print_str(s: &str) { println!(\"{}\", s); }",
        )),
        // The §13 abort convention's message channel (assert desugar, time-ctor
        // negative trap): stderr line, exact v0 oracle behavior.
        "eprintln" => Some((
            &[NTy::Str],
            None,
            "fn rt_eprintln(s: &str) { eprintln!(\"{}\", s); }",
        )),
        "__str_concat" => Some((
            &[NTy::Str, NTy::Str],
            Some(NTy::Str),
            "fn rt_str_concat(a: &str, b: &str) -> String { [a, b].concat() }",
        )),
        "string.eq" => Some((
            &[NTy::Str, NTy::Str],
            Some(NTy::I64),
            "fn rt_string_eq(a: &str, b: &str) -> i64 { (a == b) as i64 }",
        )),
        "string.len" => Some((
            // Codepoint count, NOT byte length (C-016 discipline).
            &[NTy::Str],
            Some(NTy::I64),
            "fn rt_string_len(s: &str) -> i64 { s.chars().count() as i64 }",
        )),
        // String predicates/transforms: each shim is the EXACT v0 native oracle
        // expression (runtime/rs/src/string.rs delegates to Rust std the same way),
        // so the differential gate pins byte-equality, and C-016/C-019/C-020's
        // full-Unicode discipline carries over unchanged.
        "string.contains" => Some((
            &[NTy::Str, NTy::Str],
            Some(NTy::I64),
            "fn rt_string_contains(s: &str, sub: &str) -> i64 { s.contains(sub) as i64 }",
        )),
        "string.starts_with" => Some((
            &[NTy::Str, NTy::Str],
            Some(NTy::I64),
            "fn rt_string_starts_with(s: &str, p: &str) -> i64 { s.starts_with(p) as i64 }",
        )),
        "string.ends_with" => Some((
            &[NTy::Str, NTy::Str],
            Some(NTy::I64),
            "fn rt_string_ends_with(s: &str, p: &str) -> i64 { s.ends_with(p) as i64 }",
        )),
        "string.to_upper" => Some((
            &[NTy::Str],
            Some(NTy::Str),
            "fn rt_string_to_upper(s: &str) -> String { s.to_uppercase() }",
        )),
        "string.to_lower" => Some((
            &[NTy::Str],
            Some(NTy::Str),
            "fn rt_string_to_lower(s: &str) -> String { s.to_lowercase() }",
        )),
        "string.trim" => Some((
            &[NTy::Str],
            Some(NTy::Str),
            "fn rt_string_trim(s: &str) -> String { s.trim().to_string() }",
        )),
        "string.repeat" => Some((
            &[NTy::Str, NTy::I64],
            Some(NTy::Str),
            // The SAME clamp + ceiling as the v0 runtime and the wasm self-host
            // (stdlib/string_repeat.almd): a negative count is the empty string
            // (C-054), and a result past the shared 2^31 ceiling aborts in the
            // T6 form. The bare `s.repeat(n as usize)` turned `repeat(s, -1)`
            // into a capacity-overflow PANIC (exit 101) on this leg while the
            // wasm leg printed normally — a crash-form divergence the C-161
            // rule forbids (differential fuzz, seed 1785015406589852000 index
            // 1012). Keep in sync with ALMIDE_REPEAT_MAX_BYTES.
            "fn rt_string_repeat(s: &str, n: i64) -> String {\n    let n = n.max(0);\n    if (s.len() as i64).saturating_mul(n) > (1i64 << 31) {\n        eprintln!(\"Error: repeat result too large\");\n        std::process::exit(1);\n    }\n    s.repeat(n as usize)\n}",
        )),
        "string.cmp" => Some((
            // Byte-wise lexicographic, -1/0/1 (C-019: rt_string_extra cmp = native oracle).
            &[NTy::Str, NTy::Str],
            Some(NTy::I64),
            "fn rt_string_cmp(a: &str, b: &str) -> i64 {\n    match a.cmp(b) { std::cmp::Ordering::Less => -1, std::cmp::Ordering::Equal => 0, std::cmp::Ordering::Greater => 1 }\n}",
        )),
        "float.to_string" => Some((
            // The EXACT v0 native oracle (runtime/rs/src/float.rs::almide_rt_float_to_string):
            // shortest round-trip Display, integral values forced to a `.0` tail.
            &[NTy::F64],
            Some(NTy::Str),
            "fn rt_float_to_string(n: f64) -> String {\n    let s = format!(\"{}\", n);\n    if n.fract() == 0.0 && !s.contains('.') && !s.contains(\"inf\") && !s.contains(\"NaN\") {\n        format!(\"{}.0\", s)\n    } else {\n        s\n    }\n}",
        )),
        "__chk_div" => Some((
            &[NTy::I64, NTy::I64],
            Some(NTy::I64),
            "fn rt_chk_div(a: i64, b: i64) -> i64 {\n    if b == 0 { eprintln!(\"Error: division by zero\"); std::process::exit(1); }\n    if a == i64::MIN && b == -1 { eprintln!(\"Error: integer overflow\"); std::process::exit(1); }\n    a / b\n}",
        )),
        "__chk_mod" => Some((
            &[NTy::I64, NTy::I64],
            Some(NTy::I64),
            // v0's `almide_mod` macro prints "division by zero" for a zero rhs (mod and
            // div share the message — the C-002 oracle text); keep byte parity.
            "fn rt_chk_mod(a: i64, b: i64) -> i64 {\n    if b == 0 { eprintln!(\"Error: division by zero\"); std::process::exit(1); }\n    if a == i64::MIN && b == -1 { eprintln!(\"Error: integer overflow\"); std::process::exit(1); }\n    a % b\n}",
        )),
        // The unsigned 64-bit lane (#872): the i64 slot carries the u64 bit
        // pattern. Same divide-by-zero message bytes; no MIN÷-1 case unsigned.
        "__chk_div_u" => Some((
            &[NTy::I64, NTy::I64],
            Some(NTy::I64),
            "fn rt_chk_div_u(a: i64, b: i64) -> i64 {\n    if b == 0 { eprintln!(\"Error: division by zero\"); std::process::exit(1); }\n    ((a as u64) / (b as u64)) as i64\n}",
        )),
        "__chk_mod_u" => Some((
            &[NTy::I64, NTy::I64],
            Some(NTy::I64),
            "fn rt_chk_mod_u(a: i64, b: i64) -> i64 {\n    if b == 0 { eprintln!(\"Error: division by zero\"); std::process::exit(1); }\n    ((a as u64) % (b as u64)) as i64\n}",
        )),
        _ => None,
    }
}

fn shim_rust_name(name: &str) -> String {
    format!("rt_{}", name.trim_start_matches("__").replace('.', "_"))
}

/// Render a whole MIR program to a self-contained Rust source, or WALL.
pub fn try_render_native_program(prog: &MirProgram, sigs: &NativeSigs) -> Result<String, LowerError> {
    let user_fns: BTreeMap<&str, &MirFunction> =
        prog.functions.iter().map(|f| (f.name.as_str(), f)).collect();
    if !user_fns.contains_key("main") {
        return Err(wall("native: no main in the MIR program"));
    }

    let mut used_shims: Vec<&'static str> = Vec::new();
    let mut bodies = String::new();
    let mut fn_rets: BTreeMap<String, Option<NTy>> = BTreeMap::new();
    for func in &prog.functions {
        // The Perceus balance is machine-checked on the SAME ops this render
        // erases Drops from — the certificate that scope-end drop realizes it.
        if let Err(violations) = crate::verify_ownership(func) {
            return Err(wall(format!(
                "native: ownership verification failed for `{}`: {violations:?}",
                func.name
            )));
        }
        let (rendered, ret_nty) = render_fn(func, &user_fns, sigs, &mut used_shims)?;
        fn_rets.insert(func.name.clone(), ret_nty);
        bodies.push_str(&rendered);
        bodies.push('\n');
    }

    let mut out = String::from(
        "// Generated by the Almide v1 trust spine (native leg).\n\
         #![allow(unused_variables, unused_mut, unreachable_code, dead_code, non_snake_case)]\n\n",
    );
    used_shims.sort();
    used_shims.dedup();
    for s in used_shims {
        out.push_str(s);
        out.push_str("\n\n");
    }
    out.push_str(&bodies);
    // Rung-5 closures slab: the CallIndirect dispatch tables. One dispatcher per
    // ARITY (user args beyond the env block); the index space is the SAME
    // name-sorted lambda order `Op::FuncRef` renders (both derive from the
    // `user_fns` BTreeMap, so def and call site agree by construction). Only an
    // i64-returning lambda gets an arm — a heap-returning one is reachable only
    // through a CallIndirect with a heap result, which walls above, so its
    // missing arm can never be hit; the `_` arm is the §13 controlled halt.
    let lambda_names: Vec<&str> = user_fns
        .keys()
        .copied()
        .filter(|n| n.starts_with("__lambda_"))
        .collect();
    if !lambda_names.is_empty() {
        let mut arities: BTreeMap<usize, Vec<(usize, &str)>> = BTreeMap::new();
        for (idx, name) in lambda_names.iter().enumerate() {
            if fn_rets.get(*name) != Some(&Some(NTy::I64)) {
                continue;
            }
            let arity = user_fns[name].params.len().saturating_sub(1);
            arities.entry(arity).or_default().push((idx, name));
        }
        for (arity, fns) in arities {
            let params: String = (0..arity).map(|i| format!(", a{i}: i64")).collect::<String>();
            let args: String = (0..arity).map(|i| format!(", a{i}")).collect::<String>();
            out.push_str(&format!(
                "fn __almd_ci_{arity}(idx: i64, env: &[i64]{params}) -> i64 {{\n    match idx {{\n"
            ));
            for (idx, name) in fns {
                out.push_str(&format!("        {idx} => {}(env{args}),\n", mangle(name)));
            }
            out.push_str(
                "        _ => { eprintln!(\"Error: closure index out of range\"); \
                 std::process::exit(1) }\n    }\n}\n\n",
            );
        }
    }
    Ok(out)
}

/// The `Op::FuncRef` table index of a lifted lambda: its position in the
/// NAME-SORTED lambda list (the `user_fns` BTreeMap order — the same order the
/// dispatch tables above are generated from).
fn lambda_index(user_fns: &BTreeMap<&str, &MirFunction>, name: &str) -> Option<usize> {
    user_fns.keys().filter(|n| n.starts_with("__lambda_")).position(|n| *n == name)
}

/// Native param/result NTy for a repr: scalars are i64; a heap repr is a STRING
/// (the pipeline's precision wall on declared `Ty` guarantees this).
fn repr_nty(repr: &Repr, borrowed: bool) -> Result<NTy, LowerError> {
    match repr {
        Repr::Scalar { .. } => Ok(NTy::I64),
        Repr::Ptr { .. } | Repr::Boxed { .. } => Ok(if borrowed { NTy::StrRef } else { NTy::Str }),
    }
}

/// Seed the value-type map with the function's PARAM NTys. The MIR call mode
/// BORROWS heap args. The DECLARED kind (from the sig table) disambiguates a
/// heap `Repr::Ptr` param: `&str` vs `&[i64]`.
fn seed_param_ntys(
    func: &MirFunction,
    sigs: &NativeSigs,
) -> Result<BTreeMap<ValueId, NTy>, LowerError> {
    let own_sig = sigs.get(func.name.as_str());
    let mut tys: BTreeMap<ValueId, NTy> = BTreeMap::new();
    for (i, p) in func.params.iter().enumerate() {
        let nty = match own_sig.and_then(|(ps, _)| ps.get(i)) {
            Some(NativeSigKind::ListI64) => NTy::VecRef,
            Some(NativeSigKind::Str) => NTy::StrRef,
            Some(NativeSigKind::I64) => NTy::I64,
            Some(NativeSigKind::F64) => NTy::F64,
            // Result params are rejected by the sig gate; a Res kind can
            // only name a RETURN.
            Some(NativeSigKind::Res) => {
                return Err(wall("native: Result-typed param — outside the rung subset"))
            }
            None => repr_nty(&p.repr, true)?,
        };
        tys.insert(p.value, nty);
    }
    Ok(tys)
}

fn render_fn(
    func: &MirFunction,
    user_fns: &BTreeMap<&str, &MirFunction>,
    sigs: &NativeSigs,
    used_shims: &mut Vec<&'static str>,
) -> Result<(String, Option<NTy>), LowerError> {
    let mut tys = seed_param_ntys(func, sigs)?;
    let is_main = func.name == "main";
    let mut out = String::new();
    let mut indent = 1usize;
    // Each open if-as-value join: (marker, dst) — the decl is patched in once the
    // first arm yield reveals the join type.
    let mut if_stack: Vec<Option<(String, ValueId)>> = Vec::new();

    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    // Dead pure-Handle elision (rung-5 variants slab): the variant-match lower
    // still threads a `Prim{Handle}` for the heap-payload arms, but a
    // scalar-only match reads every slot through ListGetScalar and leaves the
    // handle dead. Handle is PURE (address materialization, no ownership, no
    // side effect), so skipping an unused one is sound — and it keeps the
    // subset honest: a USED Handle still walls below.
    let used = native_used_values(func);
    if is_main && crate::charge_probe::probe_enabled() {
        used_shims.push(COUNTER_SHIM);
        used_shims.push(CHARGE_SHIM);
        line!("let __almd_probe_guard = __AlmdProbeGuard;");
        line!("let _ = &__almd_probe_guard;");
    }
    for op in &func.ops {
        match op {
            Op::Prim { kind: crate::PrimKind::Handle, dst: Some(d), .. } if !used.contains(d) => {
                line!("// dead handle elided");
            }
            // Rung-5 closures slab: a FuncRef is the lambda's DISPATCH-TABLE index
            // (the name-sorted position shared with the `__almd_ci_*` tables).
            Op::FuncRef { dst, name } => {
                let idx = lambda_index(user_fns, name).ok_or_else(|| {
                    wall(format!("native: FuncRef to unknown lambda `{name}`"))
                })?;
                tys.insert(*dst, NTy::I64);
                line!("let mut {}: i64 = {idx}; // fn table: {name}", var(*dst));
            }
            Op::CallFn { dst, name, args, result } => {
                render_call_fn(
                    crate::render_native::NativeCall { dst, name, args, result },
                    crate::render_native::NativeSink {
                        user_fns, sigs, tys: &mut tys, out: &mut out, indent, used_shims,
                    },
                )?
            }
            Op::Charge { site, cost } => {
                used_shims.push(COUNTER_SHIM);
                used_shims.push(CHARGE_SHIM);
                let tr = crate::charge_probe::probe_enabled();
                used_shims.push(FUEL_LT0_SHIM);
                line!("__almd_charge({site}, {cost}, {tr});");
                // T1-1 strict cut (see the wasm arm): the dummy return value's
                // type is known only after the body typed `func.ret`, so a
                // marker is emitted here and patched at the end of render_fn.
                line!("if __almd_fuel_lt0() {{ {CUT_RET_MARKER} }}");
            }
            // The §13 termination convention's exit half (assert desugar tail,
            // time-ctor negative trap): a user exit code, no message of its own.
            Op::Prim { kind: crate::PrimKind::ProcExit, dst: None, args } => {
                line!("std::process::exit({} as i32);", var(args[0]));
            }
            // ── T1-3 native Result carrier (native_result_rewrite) ──
            Op::Prim { kind: crate::PrimKind::ResMakeOk, dst: Some(d), args } => {
                tys.insert(*d, NTy::Res);
                line!("let {}: Result<i64, String> = Ok({});", var(*d), var(args[0]));
            }
            Op::Prim { kind: crate::PrimKind::ResMakeErrStr, dst: Some(d), args } => {
                let src = match tys.get(&args[0]) {
                    Some(NTy::Str) => format!("{}.clone()", var(args[0])),
                    Some(NTy::StrRef) => format!("{}.to_string()", var(args[0])),
                    other => {
                        return Err(wall(format!(
                            "native: ResMakeErrStr over a non-string payload ({other:?})"
                        )))
                    }
                };
                tys.insert(*d, NTy::Res);
                line!("let {}: Result<i64, String> = Err({});", var(*d), src);
            }
            Op::Prim { kind: crate::PrimKind::ResTag, dst: Some(d), args } => {
                tys.insert(*d, NTy::I64);
                line!("let {}: i64 = {}.is_err() as i64;", var(*d), var(args[0]));
            }
            Op::Prim { kind: crate::PrimKind::ResOkScalar, dst: Some(d), args } => {
                tys.insert(*d, NTy::I64);
                line!(
                    "let {}: i64 = match &{} {{ Ok(x) => *x, Err(_) => 0 }};",
                    var(*d),
                    var(args[0])
                );
            }
            Op::Prim { kind: crate::PrimKind::ResErrStr, dst: Some(d), args } => {
                // A BORROW of the Err payload (the verifier aliases it to the
                // Result's object); "" on the Ok side (unreached — the tag
                // dispatch guards it).
                tys.insert(*d, NTy::StrRef);
                line!(
                    "let {}: &str = match &{} {{ Err(e) => e.as_str(), Ok(_) => \"\" }};",
                    var(*d),
                    var(args[0])
                );
            }
            Op::Prim { kind: crate::PrimKind::BudgetEnter, dst: Some(d), args } => {
                used_shims.push(COUNTER_SHIM);
                used_shims.push(BUDGET_SHIM.as_str());
                tys.insert(*d, NTy::I64);
                line!("let {} = __almd_budget_enter({});", var(*d), var(args[0]));
            }
            Op::Prim { kind: crate::PrimKind::BudgetExhausted, dst: Some(d), .. } => {
                used_shims.push(COUNTER_SHIM);
                used_shims.push(BUDGET_SHIM.as_str());
                tys.insert(*d, NTy::I64);
                line!("let {} = __almd_budget_exhausted();", var(*d));
            }
            Op::Prim { kind: crate::PrimKind::BudgetExit, dst: Some(d), args } => {
                used_shims.push(COUNTER_SHIM);
                used_shims.push(BUDGET_SHIM.as_str());
                tys.insert(*d, NTy::I64);
                line!("let {} = __almd_budget_exit({});", var(*d), var(args[0]));
            }
            Op::Prim { kind: crate::PrimKind::BudgetSpend, dst: Some(d), .. } => {
                used_shims.push(COUNTER_SHIM);
                used_shims.push(BUDGET_SHIM.as_str());
                tys.insert(*d, NTy::I64);
                line!("let {} = __almd_budget_spend();", var(*d));
            }
            other => {
                let handled = render_native_call_op(
                    other,
                    crate::render_native::OpSink { tys: &mut tys, out: &mut out, indent, used_shims },
                )? || render_native_scalar_op(
                    other,
                    crate::render_native::OpSink { tys: &mut tys, out: &mut out, indent, used_shims },
                )? || render_native_flow_op(other, &mut tys, &mut out, &mut indent, &mut if_stack)?;
                if !handled {
                    let detail = if let Op::Prim { kind, .. } = other {
                        format!("Prim {kind:?}")
                    } else {
                        op_name(other).to_string()
                    };
                    return Err(wall(format!(
                        "native: op {detail:?} in `{}` — outside the rung subset",
                        func.name
                    )));
                }
            }
        }
    }

    if !if_stack.is_empty() {
        return Err(wall("native: unbalanced IfThen/EndIf markers"));
    }

    // Signature: the return type is known only after the body typed `func.ret`.
    let mut sig = render_native_fn_sig(func, &tys, is_main)?;
    sig.push_str(" {\n");

    // The trailing return expression (moved out — fresh owned for heap).
    if let Some(v) = func.ret {
        out.push_str("    ");
        out.push_str(&native_ret_expr(v, tys[&v]));
        out.push('\n');
    }
    out.push_str("}\n");
    let ret_nty = func.ret.map(|v| tys[&v]);
    // T1-1: patch every strict-cut marker with the now-known typed default
    // (never observed — the region verdict is Err by the time a cut fires).
    if out.contains(CUT_RET_MARKER) {
        let cut_ret = match ret_nty {
            None => "return;",
            Some(NTy::I64) => "return 0;",
            Some(NTy::F64) => "return 0.0;",
            Some(NTy::Str | NTy::StrRef) => "return String::new();",
            Some(NTy::Vec | NTy::VecRef) => "return Vec::new();",
            Some(NTy::Res) => "return Ok(0);",
        };
        out = out.replace(CUT_RET_MARKER, cut_ret);
    }
    Ok((format!("{sig}{out}"), ret_nty))
}

/// The trailing return expression: a borrowed param is moved out as a fresh
/// owned value; everything else returns the local directly.
fn native_ret_expr(v: ValueId, t: NTy) -> String {
    match t {
        NTy::I64 | NTy::F64 | NTy::Str | NTy::Vec | NTy::Res => var(v),
        NTy::StrRef => format!("{}.to_string()", var(v)),
        NTy::VecRef => format!("{}.to_vec()", var(v)),
    }
}

/// The rendered `fn` signature. The return type is known only after the body
/// typed `func.ret`, so this reads the accumulated NTy map — a param's NTy was
/// seeded from the SIG table, so a list param renders `&[i64]` (repr alone
/// cannot tell).
fn render_native_fn_sig(
    func: &MirFunction,
    tys: &BTreeMap<ValueId, NTy>,
    is_main: bool,
) -> Result<String, LowerError> {
    if is_main {
        if func.ret.is_some() {
            return Err(wall("native: main with a return value"));
        }
        return Ok(String::from("fn main()"));
    }
    let params: Vec<String> = func
        .params
        .iter()
        .map(|p| {
            let t = tys.get(&p.value).copied().unwrap_or(NTy::I64);
            let spelled = match t {
                NTy::StrRef | NTy::Str => "&str",
                NTy::VecRef | NTy::Vec => "&[i64]",
                NTy::I64 => "i64",
                NTy::F64 => "f64",
                NTy::Res => "Result<i64, String>",
            };
            format!("{}: {}", var(p.value), spelled)
        })
        .collect();
    let ret = match func.ret {
        None => String::new(),
        Some(v) => match tys.get(&v) {
            Some(NTy::I64) => " -> i64".to_string(),
            Some(NTy::Str) => " -> String".to_string(),
            Some(NTy::StrRef) => " -> String".to_string(),
            Some(NTy::Vec) => " -> Vec<i64>".to_string(),
            Some(NTy::VecRef) => " -> Vec<i64>".to_string(),
            Some(NTy::F64) => " -> f64".to_string(),
            Some(NTy::Res) => " -> Result<i64, String>".to_string(),
            None => return Err(wall("native: return value untyped")),
        },
    };
    Ok(format!("fn {}({}){}", mangle(&func.name), params.join(", "), ret))
}

/// The values a function's ops (or its return) actually READ — the read-set the
/// dead pure-Handle elision in [`render_fn`] consults. A `Prim{Handle}` whose
/// dst never appears here is address materialization feeding nothing.
fn native_used_values(func: &MirFunction) -> std::collections::BTreeSet<ValueId> {
    let mut u = std::collections::BTreeSet::new();
    for op in &func.ops {
        u.extend(native_op_reads(op));
    }
    if let Some(r) = func.ret {
        u.insert(r);
    }
    u
}

/// The values ONE op reads, for [`native_used_values`]. Deliberately partial —
/// ops absent here (Drop*, Alloc, markers) keep their operands OUT of the
/// read-set, which is what lets a drop-only Handle count as dead.
fn native_op_reads(op: &Op) -> Vec<ValueId> {
    match op {
        Op::IntBinOp { a, b, .. } => vec![*a, *b],
        Op::Prim { args, .. } => args.clone(),
        Op::Call { args, .. } | Op::CallFn { args, .. } => args
            .iter()
            .filter_map(|a| match a {
                CallArg::Handle(v) | CallArg::Scalar(v) => Some(*v),
                _ => None,
            })
            .collect(),
        Op::ListGetScalar { list, idx, .. } => vec![*list, *idx],
        Op::ListSetScalar { list, idx, val } => vec![*list, *idx, *val],
        Op::SetLocal { src, .. } => vec![*src],
        Op::Dup { src, .. } => vec![*src],
        Op::IfThen { cond, .. } => vec![*cond],
        Op::Else { val } | Op::EndIf { val } => val.iter().copied().collect(),
        _ => vec![],
    }
}

/// The CALL-shaped ops that render through an [`OpSink`]: indirect dispatch,
/// the rung-4 bounds-checked element accessors, and witness-level runtime
/// calls (`println` lowers through these). A CallIndirect dispatches through
/// the arity's `__almd_ci_*` table: the leading Handle arg is the closure
/// block (BORROWED env — `&[i64]`), the rest are scalar user args, the result
/// an i64; heap args/results are outside this slab (the wasm leg keeps them;
/// native walls). The element accessors' shims abort with the byte-identical
/// "Error: index out of bounds" + exit 1 the wasm `$elem_addr_chk` and v0
/// native emit. `Ok(false)` = not this tier's op.
fn render_native_call_op(op: &Op, s: OpSink<'_>) -> Result<bool, LowerError> {
    match op {
        Op::CallIndirect { dst, table_idx, args, result } => {
            render_call_indirect(dst, table_idx, args, result, s)?
        }
        Op::ListGetScalar { dst, list, idx } => render_list_get_scalar(dst, list, idx, s)?,
        Op::ListSetScalar { list, idx, val } => render_list_set_scalar(list, idx, val, s)?,
        Op::Call { dst, func, args, .. } => render_call_witness(dst, func, args, s)?,
        _ => return Ok(false),
    }
    Ok(true)
}

/// One VALUE-PRODUCING op (const, alloc, dup, list literal, int/float
/// arithmetic, local rebind) rendered into the sink. `Ok(false)` = not this
/// tier's op — the caller tries the flow tier, then walls. Arm bodies are
/// verbatim from the former inline [`render_fn`] op match.
fn render_native_scalar_op(op: &Op, s: OpSink<'_>) -> Result<bool, LowerError> {
    let OpSink { tys, out, indent, used_shims } = s;
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    match op {
        Op::ConstInt { dst, value } => {
            tys.insert(*dst, NTy::I64);
            line!("let mut {}: i64 = {}i64;", var(*dst), value);
        }
        Op::Alloc { dst, init, .. } => match init {
            Init::Str(s) => {
                tys.insert(*dst, NTy::Str);
                line!("let mut {}: String = String::from({s:?});", var(*dst));
            }
            other => return Err(wall(format!("native: Alloc {other:?} — outside the rung subset"))),
        },
        Op::Dup { dst, src } => render_dup(dst, src, tys, out, indent)?,
        // Rung-4 scalar-list literal: the natural Vec spelling. Elements are raw
        // i64 slot values (the wasm leg stores the same bits).
        Op::ListLit { dst, elems } => {
            for e in elems {
                if tys.get(e) != Some(&NTy::I64) {
                    return Err(wall("native: ListLit with a non-scalar element"));
                }
            }
            tys.insert(*dst, NTy::Vec);
            let items = elems.iter().map(|e| var(*e)).collect::<Vec<_>>().join(", ");
            line!("let mut {}: Vec<i64> = vec![{items}];", var(*dst));
        }
        Op::SetLocal { local, src } => render_set_local(local, src, tys, out, indent)?,
        Op::IntBinOp { dst, op, a, b } => {
            tys.insert(*dst, NTy::I64);
            let rendered = render_int_binop(op, *a, *b, used_shims)?;
            line!("let mut {}: i64 = {};", var(*dst), rendered);
        }
        _ => return render_native_float_op(op, OpSink { tys, out, indent, used_shims }),
    }
    Ok(true)
}

/// Rung-5 float floor: MIR floats are i64 BITS; native computes in real
/// f64. Every op below is IEEE-754-exact on both targets (hardware ops,
/// identical bit results), so byte-identity holds through
/// `float.to_string`. Min/Max/CopySign are excluded: Rust's `f64::min`
/// NaN semantics differ from wasm `f64.min` (they only occur inside
/// self-host bodies, which never render natively).
fn render_native_float_op(op: &Op, s: OpSink<'_>) -> Result<bool, LowerError> {
    let OpSink { tys, out, indent, .. } = s;
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    match op {
        Op::Prim { kind: crate::PrimKind::FloatBin(op), dst: Some(d), args } if args.len() == 2 => {
            render_float_bin(op, d, args, tys, out, indent)?
        }
        // `float.from_int` — int (i64) to f64, carried per the float floor.
        Op::Prim { kind: crate::PrimKind::F64FromInt, dst: Some(d), args } if args.len() == 1 => {
            tys.insert(*d, NTy::F64);
            line!("let mut {}: f64 = ({} as f64);", var(*d), var(args[0]));
        }
        Op::Prim { kind: crate::PrimKind::FloatUn(op), dst: Some(d), args } if args.len() == 1 => {
            render_float_un(op, d, args, tys, out, indent)?
        }
        Op::Prim { kind: crate::PrimKind::FloatCmp(op), dst: Some(d), args } if args.len() == 2 => {
            render_float_cmp(op, d, args, tys, out, indent)?
        }
        _ => return Ok(false),
    }
    Ok(true)
}

/// One CONTROL-FLOW / ownership op (if-as-value markers, loops, drops, the
/// bookkeeping no-ops) rendered into the buffer. `Ok(false)` = not this tier's
/// op — the caller walls. Arm bodies are verbatim from the former inline
/// [`render_fn`] op match.
fn render_native_flow_op(
    op: &Op,
    tys: &mut BTreeMap<ValueId, NTy>,
    out: &mut String,
    indent: &mut usize,
    if_stack: &mut Vec<Option<(String, ValueId)>>,
) -> Result<bool, LowerError> {
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..*indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    match op {
        Op::DropVariant { .. } | Op::Drop { .. } | Op::DropListStr { .. } => {
            return render_native_drop_op(op, tys, out, *indent)
        }
        // Pure ownership bookkeeping — no native code.
        Op::Consume { .. } | Op::Borrow { .. } | Op::MakeUnique { .. } => {}
        Op::IfThen { cond, dst } => render_if_then(cond, dst, out, indent, if_stack),
        Op::Else { val } => render_else(val, tys, out, indent, if_stack)?,
        Op::EndIf { val } => render_end_if(val, tys, out, indent, if_stack)?,
        Op::LoopStart => {
            line!("loop {{");
            *indent += 1;
        }
        Op::LoopBreakUnless { cond } => {
            line!("if {} == 0 {{ break; }}", var(*cond));
        }
        Op::LoopEnd => {
            *indent -= 1;
            line!("}}");
        }
        _ => return Ok(false),
    }
    Ok(true)
}

/// The drop-family ops. All three erase to a scope-end comment when the value
/// shape is admitted; a shape violation is a wall. `Ok(false)` = a
/// NON-closure `DropVariant` — the caller walls with the generic
/// outside-the-rung-subset message, exactly as the former inline match did.
fn render_native_drop_op(
    op: &Op,
    tys: &BTreeMap<ValueId, NTy>,
    out: &mut String,
    indent: usize,
) -> Result<bool, LowerError> {
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    match op {
        // A scalar-capture closure block is a plain `Vec<i64>` — its recursive
        // `$__drop_closure` erases to scope-end (the drop header is 0: no heap,
        // no nested, no closure slots to free). A non-Vec value here would be a
        // heap-capturing block (prim-built) — its OWNING fn walls on the prims
        // long before this drop renders.
        Op::DropVariant { v, ty } if ty.as_str() == "closure" => {
            match tys.get(v) {
                Some(NTy::Vec) => line!("// drop(closure block): scope-end"),
                other => {
                    return Err(wall(format!(
                        "native: DropVariant(closure) of a non-Vec value ({other:?})"
                    )))
                }
            }
        }
        // Drop is ERASED: Rust frees at scope end (or at reassignment for a
        // loop-carried handle). `verify_ownership` above certified balance.
        Op::Drop { v } => {
            if matches!(tys.get(v), Some(NTy::StrRef | NTy::VecRef)) {
                return Err(wall("native: Drop of a borrowed param — MIR call-mode violation"));
            }
            line!("// drop: scope-end");
        }
        // A RECORD result's drop routes as the mask-driven `DropListStr` (the
        // record block IS a list block; the mask lists its heap slots). The
        // native rung-5 subset admits ALL-SCALAR records only — the mask is
        // empty, the free is the block itself → scope-end, same as `Drop`.
        // Anything non-Vec here would carry heap slots → wall.
        Op::DropListStr { v } => {
            match tys.get(v) {
                Some(NTy::Vec) => line!("// drop(record/list block): scope-end"),
                // The T1-3 Result carrier frees like any owned Rust value.
                Some(NTy::Res) => line!("// drop(result carrier): scope-end"),
                Some(NTy::VecRef) => {
                    return Err(wall("native: DropListStr of a borrowed param — MIR call-mode violation"))
                }
                other => {
                    return Err(wall(format!(
                        "native: DropListStr of a non-list value ({other:?}) — outside the rung subset"
                    )))
                }
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}

/// `Op::Dup` — mint a fresh handle from `src` per its NTy (verbatim arm body
/// extracted from [`render_fn`]; see that function for the op-loop context).
fn render_dup(
    dst: &ValueId,
    src: &ValueId,
    tys: &mut BTreeMap<ValueId, NTy>,
    out: &mut String,
    indent: usize,
) -> Result<(), LowerError> {
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    let t = *tys.get(src).ok_or_else(|| wall("native: Dup of untyped value"))?;
    match t {
        NTy::I64 => {
            tys.insert(*dst, NTy::I64);
            line!("let mut {} = {};", var(*dst), var(*src));
        }
        NTy::Res => {
            tys.insert(*dst, NTy::Res);
            line!("let mut {} = {}.clone();", var(*dst), var(*src));
        }
        NTy::Str => {
            tys.insert(*dst, NTy::Str);
            line!("let mut {} = {}.clone();", var(*dst), var(*src));
        }
        NTy::StrRef => {
            // Dup of a borrowed param mints a fresh owned handle.
            tys.insert(*dst, NTy::Str);
            line!("let mut {} = {}.to_string();", var(*dst), var(*src));
        }
        NTy::Vec => {
            tys.insert(*dst, NTy::Vec);
            line!("let mut {} = {}.clone();", var(*dst), var(*src));
        }
        NTy::VecRef => {
            // Dup of a borrowed list param mints a fresh owned Vec.
            tys.insert(*dst, NTy::Vec);
            line!("let mut {} = {}.to_vec();", var(*dst), var(*src));
        }
        NTy::F64 => {
            tys.insert(*dst, NTy::F64);
            line!("let mut {} = {};", var(*dst), var(*src));
        }
    }
    Ok(())
}

include!("render_native_b.rs");


/// T1-1: the strict-cut return marker — every Charge site emits
/// `if __almd_fuel_lt0() { <marker> }` and `render_fn` patches the marker
/// with a `return <default of the fn's ret type>;` once the ret NTy is known
/// (the same late-patch technique as the if-value JOIN markers).
const CUT_RET_MARKER: &str = "/*__CUT_RET__*/";

/// The exhaustion read the strict cut branches on.
const FUEL_LT0_SHIM: &str =
    "fn __almd_fuel_lt0() -> bool { __ALMD_FUEL.with(|f| f.get()) < 0 }";

/// Stage 1 probe shim: fuel/trace thread-locals + the charge fn + the guard
/// that prints the triple's (consumed, trace) legs on main exit. Same hash
/// arithmetic as the wasm leg (wrapping i64, trace*1000003+site).
const COUNTER_SHIM: &str = "thread_local! {
    static __ALMD_FUEL: std::cell::Cell<i64> = const { std::cell::Cell::new(i64::MAX) };
    static __ALMD_FUEL_ENTRY: std::cell::Cell<i64> = const { std::cell::Cell::new(0) };
    static __ALMD_B_VERDICT: std::cell::Cell<i64> = const { std::cell::Cell::new(0) };
    static __ALMD_B_SPEND: std::cell::Cell<i64> = const { std::cell::Cell::new(0) };
    static __ALMD_TRACE: std::cell::Cell<i64> = const { std::cell::Cell::new(0) };
}";

/// The probe charge fn: fuel counts DOWN (consumed = MAX - fuel); the trace is
/// the order-sensitive hash. Only probe builds call it with tracing on.
const CHARGE_SHIM: &str = "fn __almd_charge(site: i64, cost: i64, trace: bool) {
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
static BUDGET_SHIM: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    BUDGET_SHIM_TEMPLATE
        .replace("__ALMD_CM1_NS__", &crate::charge_probe::CM1_NS_PER_CHARGE.to_string())
});

const BUDGET_SHIM_TEMPLATE: &str = "fn __almd_budget_enter(budget_ns: i64) -> i64 {
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
