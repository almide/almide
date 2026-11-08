//! #4006 family G: closures with heap params or a heap result.
//!
//! A scalar closure dispatches through the per-arity `__almd_ci_N` tables
//! (`render_native::push_closure_dispatch_tables`). A closure whose declared
//! signature (`crate::lambda_sigs`, recorded at the lift) has a heap param or
//! result dispatches through a table per SIGNATURE: every lifted lambda with
//! the same parameter and result kinds shares one `__almd_ci_<key>` fn, and a
//! `CallIndirect` picks the table by its arguments' kinds and its result.
//! A heap arg is borrowed, unless the lambdas of the table take it over (a
//! move param, `native_move_params`): the site then hands it over and the
//! table passes it on. The modes are part of the key. The result is owned.

use crate::lower::LowerError;
use crate::render_native::{var, wall, NTy, NativeSigKind, NativeSigs};
use crate::{CallArg, MirFunction, Op, Repr, ValueId};
use std::collections::BTreeMap;
use std::fmt::Write as _;

fn kind_code(k: NativeSigKind) -> String {
    match k {
        NativeSigKind::I64 | NativeSigKind::Narrow { .. } => "i".into(),
        NativeSigKind::F64 => "f".into(),
        NativeSigKind::Str => "s".into(),
        NativeSigKind::ListI64 => "l".into(),
        NativeSigKind::ListStr => "ls".into(),
        NativeSigKind::Rec(i) => format!("r{i}"),
        NativeSigKind::Opt => "o".into(),
        NativeSigKind::Res => "e".into(),
        NativeSigKind::ResStr => "es".into(),
    }
}

/// The borrowed spelling of a param of kind `k`.
fn param_spelling(k: NativeSigKind) -> String {
    match k {
        NativeSigKind::I64 | NativeSigKind::Narrow { .. } => "i64".into(),
        NativeSigKind::F64 => "f64".into(),
        NativeSigKind::Str => "&str".into(),
        NativeSigKind::ListI64 => "&[i64]".into(),
        NativeSigKind::ListStr => "&[AlmideString]".into(),
        NativeSigKind::Rec(i) => format!("&{}", crate::render_native_rec::struct_name(i)),
        NativeSigKind::Opt => "Option<i64>".into(),
        NativeSigKind::Res => "Result<i64, AlmideString>".into(),
        NativeSigKind::ResStr => "Result<AlmideString, AlmideString>".into(),
    }
}

/// The owned value kind a result of kind `k` binds as.
pub(crate) fn owned_nty(k: NativeSigKind) -> NTy {
    match k {
        NativeSigKind::I64 | NativeSigKind::Narrow { .. } => NTy::I64,
        NativeSigKind::F64 => NTy::F64,
        NativeSigKind::Str => NTy::Str,
        NativeSigKind::ListI64 => NTy::Vec,
        NativeSigKind::ListStr => NTy::VecStr,
        NativeSigKind::Rec(i) => NTy::Rec(i),
        NativeSigKind::Opt => NTy::Opt,
        NativeSigKind::Res => NTy::Res,
        NativeSigKind::ResStr => NTy::ResStr,
    }
}

/// The kind a value of `t` passes as.
pub(crate) fn arg_kind(t: NTy) -> NativeSigKind {
    match t {
        NTy::I64 => NativeSigKind::I64,
        NTy::F64 => NativeSigKind::F64,
        NTy::Str | NTy::StrRef => NativeSigKind::Str,
        NTy::Vec | NTy::VecRef => NativeSigKind::ListI64,
        NTy::VecStr | NTy::VecStrRef => NativeSigKind::ListStr,
        NTy::Rec(i) | NTy::RecRef(i) => NativeSigKind::Rec(i),
        NTy::Opt => NativeSigKind::Opt,
        NTy::Res => NativeSigKind::Res,
        NTy::ResStr => NativeSigKind::ResStr,
    }
}

/// The dispatch key of a lambda's user params (`moved[i]`: param `i` is a
/// move param) and result, or `None` for an all-scalar signature (served by
/// the per-arity tables).
pub(crate) fn sig_key(ps: &[NativeSigKind], moved: &[bool], ret: Option<NativeSigKind>) -> Option<String> {
    if ps.iter().all(|p| *p == NativeSigKind::I64) && ret == Some(NativeSigKind::I64) {
        return None;
    }
    let mut k = format!("{}", ps.len());
    for (i, p) in ps.iter().enumerate() {
        k.push('_');
        k.push_str(&kind_code(*p));
        if moved.get(i).copied().unwrap_or(false) {
            k.push('m');
        }
    }
    k.push_str("__");
    k.push_str(&ret.map(kind_code).unwrap_or_else(|| "u".into()));
    Some(k)
}

/// Which of a lifted lambda's user params (the env param dropped) are move
/// params.
pub(crate) fn lambda_moves(f: &MirFunction) -> Vec<bool> {
    f.params.iter().skip(1).map(|p| f.owned_params.contains(&p.value)).collect()
}

/// A lifted lambda's user-param kinds and result (the env param dropped).
pub(crate) fn lambda_sig(sigs: &NativeSigs, name: &str) -> Option<(Vec<NativeSigKind>, Option<NativeSigKind>)> {
    let (ps, r) = sigs.get(name)?;
    Some((ps.get(1..)?.to_vec(), *r))
}

/// The typed dispatch tables by signature key: the param kinds, the result,
/// and the (table index, name) of every lambda of that signature.
type DispatchTables<'a> = BTreeMap<String, (Vec<NativeSigKind>, Vec<bool>, Option<NativeSigKind>, Vec<(usize, &'a str)>)>;

/// The typed dispatch tables: one per signature that some lambda has. A
/// lambda declared `-> Float` whose body returns the MIR's i64 bit lane (a
/// literal, a passed-through Float slot, #4606) already returns the f64 its
/// signature declares (`render_native_tail`'s declared-Float return), so each
/// arm is the plain call.
pub(crate) fn push_typed_dispatch_tables(out: &mut String, user_fns: &BTreeMap<&str, &MirFunction>, sigs: &NativeSigs) {
    let mut tables: DispatchTables<'_> = BTreeMap::new();
    let lambdas = user_fns.keys().copied().filter(|n| n.starts_with("__lambda_"));
    for (idx, name) in lambdas.enumerate() {
        let Some((ps, ret)) = lambda_sig(sigs, name) else { continue };
        let moved = lambda_moves(user_fns[name]);
        let Some(key) = sig_key(&ps, &moved, ret) else { continue };
        tables.entry(key).or_insert_with(|| (ps, moved, ret, Vec::new())).3.push((idx, name));
    }
    for (key, (ps, moved, ret, fns)) in tables {
        let params: String = ps
            .iter()
            .enumerate()
            .map(|(i, k)| match moved.get(i) {
                Some(true) => format!(", a{i}: {}", crate::render_native_rec::owned_ty(owned_nty(*k))),
                _ => format!(", a{i}: {}", param_spelling(*k)),
            })
            .collect();
        let args: String = (0..ps.len()).map(|i| format!(", a{i}")).collect();
        let ret_s = ret.map(|r| format!(" -> {}", crate::render_native_rec::owned_ty(owned_nty(r)))).unwrap_or_default();
        let _ = write!(out, "fn __almd_ci_{key}(idx: i64, env: &[i64]{params}){ret_s} {{\n    match idx {{\n");
        for (idx, name) in fns {
            let _ = writeln!(out, "        {idx} => {}(env{args}),", crate::render_native::mangle(name));
        }
        out.push_str("        _ => { eprintln!(\"Error: closure index out of range\"); std::process::exit(1) }\n    }\n}\n\n");
    }
}

/// A `CallIndirect`'s non-env args: each one's sig kind, whether the caller
/// hands it over (in `handed`: a call-site move follows the call), and its
/// rendered code — a handed-over value passes itself.
type IndirectArgs = (Vec<NativeSigKind>, Vec<bool>, Vec<String>);
fn indirect_args(rest: &[CallArg], handed: &[ValueId], tys: &BTreeMap<ValueId, NTy>) -> Result<IndirectArgs, LowerError> {
    let mut kinds = Vec::new();
    let mut moved = Vec::new();
    let mut codes = Vec::new();
    for a in rest {
        let (code, t) = match a {
            CallArg::Handle(v) | CallArg::Scalar(v) => {
                (var(*v), *tys.get(v).ok_or_else(|| wall("native: CallIndirect arg untyped"))?)
            }
            CallArg::Imm(n) => (format!("{n}i64"), NTy::I64),
            other => return Err(wall(format!("native: CallIndirect arg {other:?}"))),
        };
        // The call-site move hands over an owned value (the caller's own, or
        // the copy it made for the call); rustc refuses anything else.
        let handed_here = matches!(a, CallArg::Handle(v) if handed.contains(v));
        kinds.push(arg_kind(t));
        moved.push(handed_here);
        codes.push(match t {
            _ if handed_here => code,
            NTy::Str | NTy::Vec | NTy::VecStr | NTy::Rec(_) => format!("&{code}"),
            NTy::Res | NTy::ResStr => format!("{code}.clone()"),
            _ => code,
        });
    }
    Ok((kinds, moved, codes))
}

/// The result kind: the one every lambda of these param kinds returns.
fn indirect_ret(sigs: &NativeSigs, kinds: &[NativeSigKind], dst: &Option<ValueId>) -> Result<Option<NativeSigKind>, LowerError> {
    let mut rets: Vec<Option<NativeSigKind>> = Vec::new();
    for (name, (ps, r)) in sigs.iter() {
        if name.starts_with("__lambda_") && ps.get(1..) == Some(kinds) && !rets.contains(r) {
            rets.push(*r);
        }
    }
    match (dst, rets.as_slice()) {
        (_, [one]) => Ok(*one),
        (None, _) => Err(wall("native: a dropped closure result of several kinds")),
        (Some(_), _) => Err(wall("native: a closure call whose result kind no lambda signature fixes")),
    }
}

/// The closure env as the table fn takes it (a borrowed block).
fn indirect_env(env: &ValueId, tys: &BTreeMap<ValueId, NTy>) -> Result<String, LowerError> {
    let et = *tys.get(env).ok_or_else(|| wall("native: CallIndirect env untyped"))?;
    match et {
        NTy::Vec => Ok(format!("&{}", var(*env))),
        NTy::VecRef => Ok(var(*env)),
        _ => Err(wall("native: CallIndirect env is not a closure block")),
    }
}

/// A `CallIndirect` with a heap arg or result, through its signature's table.
/// `Ok(false)`: an all-scalar call (the per-arity tables serve it), or `op`
/// is no `CallIndirect`. `handed`: the values a call-site move right after
/// `op` hands over.
pub(crate) fn render_typed_call_indirect(
    op: &Op,
    handed: &[ValueId],
    sigs: &NativeSigs,
    tys: &mut BTreeMap<ValueId, NTy>,
    out: &mut String,
    indent: usize,
) -> Result<bool, LowerError> {
    let Op::CallIndirect { dst, table_idx, args, result } = op else { return Ok(false) };
    let Some((CallArg::Handle(env), rest)) = args.split_first() else { return Ok(false) };
    let (kinds, moved, codes) = indirect_args(rest, handed, tys)?;
    let scalar_result = matches!(result, Some(Repr::Scalar { .. }));
    if kinds.iter().all(|k| *k == NativeSigKind::I64) && (scalar_result || dst.is_none()) {
        return Ok(false);
    }
    let ret = indirect_ret(sigs, &kinds, dst)?;
    let Some(key) = sig_key(&kinds, &moved, ret) else { return Ok(false) };
    let env_code = indirect_env(env, tys)?;
    let call = format!(
        "__almd_ci_{key}({}, {env_code}{})",
        var(*table_idx),
        codes.iter().map(|c| format!(", {c}")).collect::<String>()
    );
    for _ in 0..indent {
        out.push_str("    ");
    }
    match (dst, ret) {
        (Some(d), Some(r)) => {
            let t = owned_nty(r);
            tys.insert(*d, t);
            let _ = writeln!(out, "let mut {}: {} = {call};", var(*d), crate::render_native_rec::owned_ty(t));
        }
        (None, _) => {
            let _ = writeln!(out, "{call};");
        }
        (Some(_), None) => return Err(wall("native: a Unit closure result bound to a value")),
    }
    Ok(true)
}

/// The values the call-site moves right after op `i` of `func` hand over
/// (`MirFunction::call_moves`).
pub(crate) fn handed_after(func: &MirFunction, i: usize) -> Vec<ValueId> {
    // Each call-site move is a `Consume` of the one value it hands over.
    let mut out = Vec::new();
    for j in (i + 1..func.ops.len()).take_while(|j| func.call_moves.contains(j)) {
        crate::mir_ops::op_values(&func.ops[j], &mut out);
    }
    out
}
