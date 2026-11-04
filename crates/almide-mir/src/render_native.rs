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
    /// An OWNED `Vec<AlmideString>` local — a `List[String]` slot block (#4006
    /// List[String] slab), built and read through `render_native_addr`.
    VecStr,
    /// A BORROWED `&[AlmideString]` — a `List[String]` fn param.
    VecStrRef,
    /// An OWNED generated record struct (`render_native_rec`, by table index).
    Rec(u16),
    /// A BORROWED `&AlmdRecN` — a heap-record fn param.
    RecRef(u16),
    /// An `Option<i64>` local — the native scalar-Option carrier (#4006
    /// family B; produced by the `native_option_rewrite` prims, an
    /// Opt-returning call, or an Option param). `Copy`: passed by value.
    Opt,
    /// An OWNED `Result<AlmideString, AlmideString>` — the String-Ok Result
    /// carrier (#4006 family C; `native_result_str_rewrite`).
    ResStr,
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
    /// `List[String]` — `&[AlmideString]` in, `Vec<AlmideString>` out.
    ListStr,
    /// A heap record — `&AlmdRecN` in, `AlmdRecN` out (`render_native_rec`).
    Rec(u16),
    /// `Option<i64>` — an Option of a scalar (Int, Bool, Float as bits).
    Opt,
    /// `Result<AlmideString, AlmideString>` — a `Result[String, String]`.
    ResStr,
    /// A narrow integer (`Int8`..`UInt32`) crossing a call boundary as its
    /// real Rust type (`i32`, `u8`, …): the value is the lane's i64 inside
    /// the body, converted with `as` at the boundary (#4006). The MIR keeps
    /// every narrow value re-wrapped, so both conversions are exact.
    Narrow { bits: u8, signed: bool },
}

impl NativeSigKind {
    /// The kind on the i64 lane: a narrow integer travels as `i64` where an
    /// ABI is shared by dispatch (a closure table, a function value).
    pub(crate) fn lane(self) -> Self {
        match self {
            NativeSigKind::Narrow { .. } => NativeSigKind::I64,
            k => k,
        }
    }

    /// The Rust spelling of a narrow integer kind (`None` for any other kind).
    pub(crate) fn narrow_rust(self) -> Option<&'static str> {
        let NativeSigKind::Narrow { bits, signed } = self else { return None };
        Some(match (bits, signed) {
            (8, true) => "i8",
            (16, true) => "i16",
            (32, true) => "i32",
            (8, false) => "u8",
            (16, false) => "u16",
            _ => "u32",
        })
    }
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
/// C-196: call-stack exhaustion is the defined abort `Error: stack overflow` +
/// exit 1, as the codegen leg's runtime prelude makes it. The guard module is
/// shared with that leg (`almide_base::native_stack_guard`); this render's
/// stdout is std's own line-buffered handle, so the handler has no buffer of
/// ours to write out first. Kept out of line, as v0's install sits in its
/// runtime rlib: its cost reads the process's memory map, which differs from
/// binary to binary, and the small-program perf check
/// (scripts/check-native-v1-perf-small.sh) leaves it out of a program's work
/// by toggling collection on this function.
const STACK_GUARD_GLUE_SHIM: &str = "#[inline(never)]\nfn almide_stack_guard_install() {\n    #[cfg(all(any(target_os = \"linux\", target_os = \"macos\"), not(miri)))]\n    almide_stack_guard::install();\n}\n#[cfg(any(target_os = \"linux\", target_os = \"macos\"))]\nfn almide_stack_guard_stdout() {}";
const IDX_GET_SHIM: &str = "fn almide_idx_get(v: &[i64], i: i64) -> i64 {\n        if i < 0 || i as usize >= v.len() { eprintln!(\"Error: index out of bounds\"); std::process::exit(1); }\n        v[i as usize]\n}";
const IDX_SET_SHIM: &str = "fn almide_idx_set(v: &mut Vec<i64>, i: i64, x: i64) {\n        if i < 0 || i as usize >= v.len() { eprintln!(\"Error: index out of bounds\"); std::process::exit(1); }\n        v[i as usize] = x;\n}";

/// Borrow a stringy value as `&str` for a call argument.
pub(crate) fn as_str_arg(code: &str, t: NTy) -> String {
    // A string literal's binding passes the literal itself (#4006,
    // no-perf-loss): the same bytes, and a comparison of two literals folds.
    let literal = code
        .strip_prefix('v')
        .and_then(|n| n.parse::<u32>().ok())
        .and_then(|n| crate::render_native_move::static_str_operand(ValueId(n)));
    match t {
        NTy::Str if literal.is_some() => literal.unwrap_or_default(),
        NTy::Str => format!("&{code}"),
        NTy::StrRef => code.to_string(),
        NTy::Vec | NTy::VecRef => unreachable!("as_str_arg on vec"),
        NTy::I64 | NTy::F64 => unreachable!("as_str_arg on scalar"),
        NTy::Res => unreachable!("as_str_arg on a Result carrier"),
        NTy::VecStr | NTy::VecStrRef => unreachable!("as_str_arg on a string list"),
        NTy::Rec(_) | NTy::RecRef(_) => unreachable!("as_str_arg on a record"),
        NTy::Opt => unreachable!("as_str_arg on an Option carrier"),
        NTy::ResStr => unreachable!("as_str_arg on a Result carrier"),
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
    if almide_base::env::flag("ALMIDE_DUMP_VERIFY") {
        use std::io::Write as _;
        let mut err = std::io::stderr();
        let _ = writeln!(err, "== verify-stage fn {} ==", func.name);
        for (i, op) in func.ops.iter().enumerate() {
            let _ = writeln!(err, "  [{i}] {op:?}");
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
/// `user_fns` BTreeMap, so def and call site agree by construction). An
/// i64-returning lambda gets an arm, and so does a Unit one (its arm runs it
/// and yields 0, which the only call that reaches it — a CallIndirect with no
/// destination — discards); a heap-returning one is reachable only through a
/// CallIndirect with a heap result, which walls in `render_fn`, so its missing
/// arm can never be hit; the `_` arm is the §13 controlled halt.
fn push_closure_dispatch_tables(
    out: &mut String,
    user_fns: &BTreeMap<&str, &MirFunction>,
    fn_rets: &BTreeMap<String, Option<NTy>>,
    sigs: &NativeSigs,
) {
    let mut arities: BTreeMap<usize, Vec<(usize, &str)>> = BTreeMap::new();
    let lambda_names = user_fns
        .keys()
        .copied()
        .filter(|n| n.starts_with("__lambda_"));
    for (idx, name) in lambda_names.enumerate() {
        if !matches!(fn_rets.get(name), Some(Some(NTy::I64)) | Some(None)) {
            continue;
        }
        // A heap-param lambda dispatches through its signature's typed table.
        if sigs.get(name).is_some_and(|(ps, _)| ps.iter().skip(1).any(|k| *k != NativeSigKind::I64)) {
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
            if fn_rets.get(name) == Some(&None) {
                out.push_str(&format!("        {idx} => {{ {}(env{args}); 0 }}\n", mangle(name)));
            } else {
                out.push_str(&format!("        {idx} => {}(env{args}),\n", mangle(name)));
            }
        }
        out.push_str(
            "        _ => { eprintln!(\"Error: closure index out of range\"); \
             std::process::exit(1) }\n    }\n}\n\n",
        );
    }
}

/// The runtime part of a rendered program: a module named [`RT_MOD`]
/// between the two marker lines, its body a valid crate root of its own
/// (the build splits it out, `src/cli/native_rt_crate.rs`).
pub const RT_MOD: &str = "almide_v1_rt";
pub const RT_MOD_BEGIN: &str = "// __ALMIDE_V1_RT_BEGIN__\n";
pub const RT_MOD_END: &str = "// __ALMIDE_V1_RT_END__\n";
const RT_MOD_ATTRS: &str = "#![allow(dead_code, unused_imports, unused_variables, unused_mut, unused_parens, non_snake_case, non_camel_case_types, uncommon_codepoints, confusable_idents, mixed_script_confusables)]";

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
    render_items(prog, sigs, true)
}

/// [`try_render_native_program`] without the `main` requirement: the shims,
/// every fn and the closure dispatch tables. The metered-region splice
/// (#3669) renders its region fns through here, with no `main` of its own.
pub fn render_native_items(prog: &MirProgram, sigs: &NativeSigs) -> Result<String, LowerError> {
    // The splice lives inside the host program, whose runtime it shares: no
    // runtime module of its own.
    render_items(prog, sigs, false)
}

fn render_items(prog: &MirProgram, sigs: &NativeSigs, runtime_module: bool) -> Result<String, LowerError> {
    let user_fns: BTreeMap<&str, &MirFunction> = prog
        .functions
        .iter()
        .map(|f| (f.name.as_str(), f))
        .collect();
    let mut used_shims: Vec<&'static str> = Vec::new();
    let mut bodies = String::new();
    fan_rendered_reset();
    crate::render_native_bounds::reset();
    let mut fn_rets: BTreeMap<String, Option<NTy>> = BTreeMap::new();
    for func in &prog.functions {
        verify_ownership_or_wall(func)?;
        let (rendered, ret_nty) = render_fn(func, &user_fns, sigs, &mut used_shims)
            .map_err(|e| e.with_fn_context(&func.name))?;
        let rendered = narrow_abi_wrapper(func, sigs, rendered)?;
        fn_rets.insert(func.name.clone(), ret_nty);
        bodies.push_str(&rendered);
        bodies.push('\n');
    }

    let mut out = String::from(
        "// Generated by the Almide v1 trust spine (native leg).\n\
         #![allow(unused_variables, unused_mut, unreachable_code, dead_code, non_snake_case, uncommon_codepoints, confusable_idents, mixed_script_confusables)]\n\n",
    );
    used_shims.sort();
    used_shims.dedup();
    let (runtime, glue): (Vec<&str>, Vec<&str>) = used_shims.into_iter().partition(|s| crate::native_floor::is_runtime_text(s));
    if fan_rendered() || !runtime_module {
        // The fan runtime's output macros and exit routing reach every item
        // after them, the runtime's included: one crate, as before.
        out.push_str(&crate::render_native_str::almide_string_alias());
        for s in runtime {
            out.push_str(s);
            out.push_str("\n\n");
        }
    } else {
        // The runtime part — the shared string buffer and the runtime items
        // the floor takes along — in one module the program reaches through
        // a glob. The build compiles it as a crate of its own under the
        // runtime's profile, as v0 links its runtime rlib: no debug
        // assertions, overflow checks pinned off (owner ruling 2026-11-02);
        // the program's own code keeps the program profile.
        out.push_str(RT_MOD_BEGIN);
        out.push_str(&format!("pub mod {RT_MOD} {{\n{RT_MOD_ATTRS}\n"));
        out.push_str(&crate::render_native_str::almide_string_alias());
        for s in runtime {
            out.push_str(&crate::native_floor::pub_items(s));
            out.push_str("\n\n");
        }
        out.push_str(&format!("}}\n{RT_MOD_END}pub use {RT_MOD}::*;\n\n"));
    }
    out.push_str(&crate::render_native_rec::struct_defs());
    for s in glue {
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
    push_closure_dispatch_tables(&mut out, &user_fns, &fn_rets, sigs);
    crate::render_native_closure::push_typed_dispatch_tables(&mut out, &user_fns, sigs);
    if fan_rendered() {
        // #4006 family H: the fan runtime ahead of every other item (its
        // output macros shadow std's for all that follows), and every exit
        // through the trap wait.
        let at = out.find("\n\n").map_or(0, |i| i + 2);
        out.insert_str(at, &fan_glue());
        out = route_exits(&out);
    }
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
            Some(NativeSigKind::ListStr) => NTy::VecStrRef,
            Some(NativeSigKind::Rec(i)) => NTy::RecRef(*i),
            Some(NativeSigKind::Str) => NTy::StrRef,
            Some(NativeSigKind::I64 | NativeSigKind::Narrow { .. }) => NTy::I64,
            Some(NativeSigKind::F64) => NTy::F64,
            Some(NativeSigKind::Opt) => NTy::Opt,
            Some(NativeSigKind::ResStr) => NTy::ResStr,
            // A carrier Result param (#4006 family C) arrives by value.
            Some(NativeSigKind::Res) => NTy::Res,
            None => repr_nty(&p.repr, true)?,
        };
        // A MOVE param (#4006 class P) arrives as the caller's own value.
        let nty = if func.owned_params.contains(&p.value) { owned_nty(nty) } else { nty };
        tys.insert(p.value, nty);
    }
    Ok(tys)
}

/// A string literal's binding that nothing reads (#4006, no-perf-loss): every
/// read of it rendered the literal itself as a `&str` operand
/// (`render_native_move::static_str_operand`), so its static block view is
/// built and dropped per evaluation for nothing. The binding goes; a static
/// block's drop is a no-op, so nothing else changes.
fn drop_unread_static_strs(out: String) -> String {
    let lines: Vec<&str> = out.lines().collect();
    let name_of = |l: &str| -> Option<String> {
        let t = l.trim_start();
        let rest = t.strip_prefix("let mut ")?;
        let (name, tail) = rest.split_once(": AlmideString = { static S: crate::almide_str_block::AlmideStrStatic<")?;
        tail.ends_with("AlmideString::from_static(&S) };").then(|| name.to_string())
    };
    let used = |name: &str| {
        lines.iter().filter(|l| {
            l.match_indices(name).any(|(at, _)| {
                let before = l[..at].chars().next_back().is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
                let after = l[at + name.len()..].chars().next().is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
                before && after
            })
        })
        .count()
    };
    let mut kept = String::with_capacity(out.len());
    for l in &lines {
        if name_of(l).is_some_and(|n| used(&n) == 1) {
            continue;
        }
        kept.push_str(l);
        kept.push('\n');
    }
    kept
}

/// The owned counterpart of a borrowed param type: what a MOVE param (#4006
/// class P) holds.
pub(crate) fn owned_nty(t: NTy) -> NTy {
    match t {
        NTy::VecRef => NTy::Vec,
        NTy::VecStrRef => NTy::VecStr,
        NTy::RecRef(i) => NTy::Rec(i),
        NTy::StrRef => NTy::Str,
        other => other,
    }
}

include!("render_native_loop.rs");
include!("render_native_tail.rs");
include!("render_native_b.rs");
include!("render_native_b_tail.rs");
include!("render_native_fan.rs");
include!("render_native_rem.rs");
