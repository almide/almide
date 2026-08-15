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
use crate::{CallArg, IntOp, MirFunction, MirProgram, Op, Repr, ValueId};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// The native type a MIR value renders to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum NTy {
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
pub type NativeSigs =
    std::collections::BTreeMap<String, (Vec<NativeSigKind>, Option<NativeSigKind>)>;

pub(crate) fn wall(msg: impl Into<String>) -> LowerError {
    LowerError::Unsupported(msg.into())
}

pub(crate) fn var(v: ValueId) -> String {
    format!("v{}", v.0)
}

/// The rung-4 bounds-checked element accessors — byte-identical abort text to the
/// wasm `$elem_addr_chk` ("Error: index out of bounds" + exit 1) and to v0 native.
const IDX_GET_SHIM: &str = "fn almide_idx_get(v: &[i64], i: i64) -> i64 {\n        if i < 0 || i as usize >= v.len() { eprintln!(\"Error: index out of bounds\"); std::process::exit(1); }\n        v[i as usize]\n}";
const IDX_SET_SHIM: &str = "fn almide_idx_set(v: &mut Vec<i64>, i: i64, x: i64) {\n        if i < 0 || i as usize >= v.len() { eprintln!(\"Error: index out of bounds\"); std::process::exit(1); }\n        v[i as usize] = x;\n}";

/// Borrow a stringy value as `&str` for a call argument.
pub(crate) fn as_str_arg(code: &str, t: NTy) -> String {
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
pub(crate) fn as_f64_arg(code: &str, t: NTy) -> Result<String, LowerError> {
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
use crate::render_native_op_families::{
    render_native_meter_op, render_native_result_op,
    render_native_scalar_op, render_native_termination_op,
};
use crate::render_native_shims::{
    shim, shim_rust_name, CHARGE_SHIM, COUNTER_SHIM, CUT_RET_MARKER,
};

/// The Perceus balance is machine-checked on the SAME ops this render erases
/// Drops from — the certificate that scope-end drop realizes it. A violation is
/// a wall, with the whole op list dumped under `ALMIDE_DUMP_VERIFY`.
fn verify_ownership_or_wall(func: &MirFunction) -> Result<(), LowerError> {
    let Err(violations) = crate::verify_ownership(func) else {
        return Ok(());
    };
    if std::env::var_os("ALMIDE_DUMP_VERIFY").is_some() {
        eprintln!("== verify-stage fn {} ==", func.name);
        for (i, op) in func.ops.iter().enumerate() {
            eprintln!("  [{i}] {op:?}");
        }
    }
    Err(wall(format!(
        "native: ownership verification failed for `{}`: {violations:?}",
        func.name
    )))
}

/// Rung-5 closures slab: the CallIndirect dispatch tables. One dispatcher per
/// ARITY (user args beyond the env block); the index space is the SAME
/// name-sorted lambda order `Op::FuncRef` renders (both derive from the
/// `user_fns` BTreeMap, so def and call site agree by construction). Only an
/// i64-returning lambda gets an arm — a heap-returning one is reachable only
/// through a CallIndirect with a heap result, which walls in `render_fn`, so its
/// missing arm can never be hit; the `_` arm is the §13 controlled halt.
fn push_closure_dispatch_tables(
    out: &mut String,
    user_fns: &BTreeMap<&str, &MirFunction>,
    fn_rets: &BTreeMap<String, Option<NTy>>,
) {
    let mut arities: BTreeMap<usize, Vec<(usize, &str)>> = BTreeMap::new();
    let lambda_names = user_fns
        .keys()
        .copied()
        .filter(|n| n.starts_with("__lambda_"));
    for (idx, name) in lambda_names.enumerate() {
        if fn_rets.get(name) != Some(&Some(NTy::I64)) {
            continue;
        }
        let arity = user_fns[name].params.len().saturating_sub(1);
        arities.entry(arity).or_default().push((idx, name));
    }
    for (arity, fns) in arities {
        let params: String = (0..arity).map(|i| format!(", a{i}: i64")).collect();
        let args: String = (0..arity).map(|i| format!(", a{i}")).collect();
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

/// Render a whole MIR program to a self-contained Rust source, or WALL.
pub fn try_render_native_program(
    prog: &MirProgram,
    sigs: &NativeSigs,
) -> Result<String, LowerError> {
    let user_fns: BTreeMap<&str, &MirFunction> = prog
        .functions
        .iter()
        .map(|f| (f.name.as_str(), f))
        .collect();
    if !user_fns.contains_key("main") {
        return Err(wall("native: no main in the MIR program"));
    }

    let mut used_shims: Vec<&'static str> = Vec::new();
    let mut bodies = String::new();
    let mut fn_rets: BTreeMap<String, Option<NTy>> = BTreeMap::new();
    for func in &prog.functions {
        verify_ownership_or_wall(func)?;
        let (rendered, ret_nty) = render_fn(func, &user_fns, sigs, &mut used_shims)
            .map_err(|e| e.with_fn_context(&func.name))?;
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
    push_closure_dispatch_tables(&mut out, &user_fns, &fn_rets);
    Ok(out)
}

/// The `Op::FuncRef` table index of a lifted lambda: its position in the
/// NAME-SORTED lambda list (the `user_fns` BTreeMap order — the same order the
/// dispatch tables above are generated from).
fn lambda_index(user_fns: &BTreeMap<&str, &MirFunction>, name: &str) -> Option<usize> {
    user_fns
        .keys()
        .filter(|n| n.starts_with("__lambda_"))
        .position(|n| *n == name)
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
            Op::Prim {
                kind: crate::PrimKind::Handle,
                dst: Some(d),
                ..
            } if !used.contains(d) => {
                line!("// dead handle elided");
            }
            // Rung-5 closures slab: a FuncRef is the lambda's DISPATCH-TABLE index
            // (the name-sorted position shared with the `__almd_ci_*` tables).
            Op::FuncRef { dst, name } => {
                let idx = lambda_index(user_fns, name)
                    .ok_or_else(|| wall(format!("native: FuncRef to unknown lambda `{name}`")))?;
                tys.insert(*dst, NTy::I64);
                line!("let mut {}: i64 = {idx}; // fn table: {name}", var(*dst));
            }
            Op::CallFn {
                dst,
                name,
                args,
                result,
            } => render_call_fn(
                crate::render_native::NativeCall {
                    dst,
                    name,
                    args,
                    result,
                },
                crate::render_native::NativeSink {
                    user_fns,
                    sigs,
                    tys: &mut tys,
                    out: &mut out,
                    indent,
                    used_shims,
                },
            )?,
            other => {
                let handled = render_native_call_op(
                    other,
                    crate::render_native::OpSink {
                        tys: &mut tys,
                        out: &mut out,
                        indent,
                        used_shims,
                    },
                )? || render_native_meter_op(
                    other,
                    crate::render_native::OpSink {
                        tys: &mut tys,
                        out: &mut out,
                        indent,
                        used_shims,
                    },
                )? || render_native_result_op(
                    other,
                    crate::render_native::OpSink {
                        tys: &mut tys,
                        out: &mut out,
                        indent,
                        used_shims,
                    },
                )? || render_native_termination_op(
                    other,
                    crate::render_native::OpSink {
                        tys: &mut tys,
                        out: &mut out,
                        indent,
                        used_shims,
                    },
                )? || render_native_scalar_op(
                    other,
                    crate::render_native::OpSink {
                        tys: &mut tys,
                        out: &mut out,
                        indent,
                        used_shims,
                    },
                )? || render_native_flow_op(
                    other,
                    &mut tys,
                    &mut out,
                    &mut indent,
                    &mut if_stack,
                )?;
                if !handled {
                    let detail = if let Op::Prim { kind, .. } = other {
                        format!("Prim {kind:?}")
                    } else {
                        op_name(other)
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
    finish_native_fn(func, sigs, is_main, &mut tys, out)
}

/// The tail of [`render_fn`]: the lifted-effect `Ok(..)` wrap decision, the
/// signature (renderable only after the body typed `func.ret`), the trailing
/// return expression, and the T1-1 strict-cut marker patch (the typed
/// default is known only here).
fn finish_native_fn(
    func: &MirFunction,
    sigs: &NativeSigs,
    is_main: bool,
    tys: &mut BTreeMap<ValueId, NTy>,
    mut out: String,
) -> Result<(String, Option<NTy>), LowerError> {
    // A LIFTED effect fn (declared scalar ret, wrapped carrier ABI — the sigs
    // table widening in the pipeline): the body computes the raw scalar and
    // the `Ok(..)` wrap happens HERE, at the single return seam. The wrap is
    // carried as a RETURN-TYPE OVERRIDE, never by retyping the ret VALUE:
    // when the body is a bare parameter passthrough (`effect fn f(x) = x`),
    // ret and param share one ValueId, and retyping it turned the PARAM's
    // rendered type into `Result<i64, String>` while callers kept passing the
    // raw scalar — the almide#1429 signature/body split.
    let lifted_wrap = !is_main
        && matches!(
            sigs.get(func.name.as_str()),
            Some((_, Some(NativeSigKind::Res)))
        )
        && matches!(func.ret.and_then(|v| tys.get(&v)), Some(NTy::I64));
    let ret_override = if lifted_wrap { Some(NTy::Res) } else { None };
    // Signature: the return type is known only after the body typed `func.ret`.
    let mut sig = render_native_fn_sig(func, &tys, is_main, ret_override)?;
    sig.push_str(" {\n");

    // The trailing return expression (moved out — fresh owned for heap).
    if let Some(v) = func.ret {
        out.push_str("    ");
        if lifted_wrap {
            out.push_str(&format!("Ok({})", var(v)));
        } else {
            out.push_str(&native_ret_expr(v, tys[&v]));
        }
        out.push('\n');
    }
    out.push_str("}\n");
    let ret_nty = if lifted_wrap { Some(NTy::Res) } else { func.ret.map(|v| tys[&v]) };
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
    ret_override: Option<NTy>,
) -> Result<String, LowerError> {
    if is_main {
        if func.ret.is_some() {
            return Err(wall("native: main with a return value"));
        }
        return Ok(String::from("fn main()"));
    }
    // A param that any `Op::SetLocal` retargets (the MIR tail-recursion loop
    // reassigns its params each iteration) must render `mut` — without it the
    // loop body's `v0 = …;` is an E0384 on a plain scalar accumulator
    // recursion (`count(n - 1, acc + 1)`), a compile error rustc only surfaces
    // on the run path because every test harness rides the v0 fallback.
    let reassigned: std::collections::HashSet<ValueId> = func
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::SetLocal { local, .. } => Some(*local),
            _ => None,
        })
        .collect();
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
            let mut_prefix = if reassigned.contains(&p.value) {
                "mut "
            } else {
                ""
            };
            format!("{mut_prefix}{}: {}", var(p.value), spelled)
        })
        .collect();
    let ret = match func.ret {
        None => String::new(),
        Some(v) => match ret_override.as_ref().or_else(|| tys.get(&v)) {
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
    Ok(format!(
        "fn {}({}){}",
        mangle(&func.name),
        params.join(", "),
        ret
    ))
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
        Op::CallIndirect {
            dst,
            table_idx,
            args,
            result,
        } => render_call_indirect(dst, table_idx, args, result, s)?,
        Op::ListGetScalar { dst, list, idx } => render_list_get_scalar(dst, list, idx, s)?,
        Op::ListSetScalar { list, idx, val } => render_list_set_scalar(list, idx, val, s)?,
        Op::Call {
            dst, func, args, ..
        } => render_call_witness(dst, func, args, s)?,
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
        // Frame-targeted early exit: a Rust `return` at any nesting depth —
        // the same move-out expression the trailing return uses. (A LIFTED
        // effect fn's `Ok(..)` wrap happens at the single tail seam only; an
        // early `Return` inside one would mistype and rustc rejects the
        // build — an honest wall, revisit when a lowering emits that shape.)
        Op::Return { val } => match val {
            Some(v) => {
                let t = *tys
                    .get(v)
                    .ok_or_else(|| wall("native: Return of an untyped value"))?;
                line!("return {};", native_ret_expr(*v, t));
            }
            None => line!("return;"),
        },
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
        Op::DropVariant { v, ty } if ty.as_str() == "closure" => match tys.get(v) {
            Some(NTy::Vec) => line!("// drop(closure block): scope-end"),
            other => {
                return Err(wall(format!(
                    "native: DropVariant(closure) of a non-Vec value ({other:?})"
                )))
            }
        },
        // Drop is ERASED: Rust frees at scope end (or at reassignment for a
        // loop-carried handle). `verify_ownership` above certified balance.
        Op::Drop { v } => {
            if matches!(tys.get(v), Some(NTy::StrRef | NTy::VecRef)) {
                return Err(wall(
                    "native: Drop of a borrowed param — MIR call-mode violation",
                ));
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
                    return Err(wall(
                        "native: DropListStr of a borrowed param — MIR call-mode violation",
                    ))
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
pub(crate) fn render_dup(
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
    let t = *tys
        .get(src)
        .ok_or_else(|| wall("native: Dup of untyped value"))?;
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
