//! #4006 family G: closures with heap params or a heap result.
//!
//! A scalar closure dispatches through the per-arity `__almd_ci_N` tables
//! (`render_native::push_closure_dispatch_tables`). A closure whose declared
//! signature (`crate::lambda_sigs`, recorded at the lift) has a heap param or
//! result dispatches through a table per SIGNATURE: every lifted lambda with
//! the same parameter and result kinds shares one `__almd_ci_<key>` fn, and a
//! `CallIndirect` picks the table by its arguments' kinds and its result.
//! Heap args are borrowed, as at every MIR call; the result is owned.

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
fn owned_nty(k: NativeSigKind) -> NTy {
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
fn arg_kind(t: NTy) -> NativeSigKind {
    match t {
        NTy::I64 => NativeSigKind::I64,
        NTy::F64 => NativeSigKind::F64,
        NTy::Str | NTy::StrRef => NativeSigKind::Str,
        NTy::Vec | NTy::VecRef => NativeSigKind::ListI64,
        NTy::VecStr | NTy::VecStrRef => NativeSigKind::ListStr,
        NTy::Rec(i) | NTy::RecRef(i) | NTy::RecMut(i) => NativeSigKind::Rec(i),
        NTy::Opt => NativeSigKind::Opt,
        NTy::Res => NativeSigKind::Res,
        NTy::ResStr => NativeSigKind::ResStr,
    }
}

/// The dispatch key of a lambda's user params and result, or `None` for an
/// all-scalar signature (served by the per-arity tables).
fn sig_key(ps: &[NativeSigKind], ret: Option<NativeSigKind>) -> Option<String> {
    if ps.iter().all(|p| *p == NativeSigKind::I64) && ret == Some(NativeSigKind::I64) {
        return None;
    }
    let mut k = format!("{}", ps.len());
    for p in ps {
        k.push('_');
        k.push_str(&kind_code(*p));
    }
    k.push_str("__");
    k.push_str(&ret.map(kind_code).unwrap_or_else(|| "u".into()));
    Some(k)
}

/// A lifted lambda's user-param kinds and result (the env param dropped).
fn lambda_sig(sigs: &NativeSigs, name: &str) -> Option<(Vec<NativeSigKind>, Option<NativeSigKind>)> {
    let (ps, r) = sigs.get(name)?;
    Some((ps.get(1..)?.to_vec(), *r))
}

/// The typed dispatch tables by signature key: the param kinds, the result,
/// and the (table index, name) of every lambda of that signature.
type DispatchTables<'a> = BTreeMap<String, (Vec<NativeSigKind>, Option<NativeSigKind>, Vec<(usize, &'a str)>)>;

/// The typed dispatch tables: one per signature that some lambda has.
pub(crate) fn push_typed_dispatch_tables(
    out: &mut String,
    user_fns: &BTreeMap<&str, &MirFunction>,
    sigs: &NativeSigs,
) {
    let mut tables: DispatchTables<'_> = BTreeMap::new();
    let lambdas = user_fns.keys().copied().filter(|n| n.starts_with("__lambda_"));
    for (idx, name) in lambdas.enumerate() {
        let Some((ps, ret)) = lambda_sig(sigs, name) else { continue };
        let Some(key) = sig_key(&ps, ret) else { continue };
        tables.entry(key).or_insert_with(|| (ps, ret, Vec::new())).2.push((idx, name));
    }
    for (key, (ps, ret, fns)) in tables {
        let params: String = ps.iter().enumerate().map(|(i, k)| format!(", a{i}: {}", param_spelling(*k))).collect();
        let args: String = (0..ps.len()).map(|i| format!(", a{i}")).collect();
        let ret_s = ret.map(|r| format!(" -> {}", crate::render_native_rec::owned_ty(owned_nty(r)))).unwrap_or_default();
        let _ = write!(out, "fn __almd_ci_{key}(idx: i64, env: &[i64]{params}){ret_s} {{\n    match idx {{\n");
        for (idx, name) in fns {
            let _ = writeln!(out, "        {idx} => {}(env{args}),", crate::render_native::mangle(name));
        }
        out.push_str("        _ => { eprintln!(\"Error: closure index out of range\"); std::process::exit(1) }\n    }\n}\n\n");
    }
}

/// A `CallIndirect`'s non-env args: each one's sig kind and its rendered code.
fn indirect_args(rest: &[CallArg], tys: &BTreeMap<ValueId, NTy>) -> Result<(Vec<NativeSigKind>, Vec<String>), LowerError> {
    let mut kinds = Vec::new();
    let mut codes = Vec::new();
    for a in rest {
        let (code, t) = match a {
            CallArg::Handle(v) | CallArg::Scalar(v) => {
                (var(*v), *tys.get(v).ok_or_else(|| wall("native: CallIndirect arg untyped"))?)
            }
            CallArg::Imm(n) => (format!("{n}i64"), NTy::I64),
            other => return Err(wall(format!("native: CallIndirect arg {other:?}"))),
        };
        kinds.push(arg_kind(t));
        codes.push(match t {
            NTy::Str | NTy::Vec | NTy::VecStr | NTy::Rec(_) => format!("&{code}"),
            NTy::Res | NTy::ResStr => format!("{code}.clone()"),
            _ => code,
        });
    }
    Ok((kinds, codes))
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
/// is no `CallIndirect`.
pub(crate) fn render_typed_call_indirect(
    op: &Op,
    sigs: &NativeSigs,
    tys: &mut BTreeMap<ValueId, NTy>,
    out: &mut String,
    indent: usize,
) -> Result<bool, LowerError> {
    let Op::CallIndirect { dst, table_idx, args, result } = op else { return Ok(false) };
    let Some((CallArg::Handle(env), rest)) = args.split_first() else { return Ok(false) };
    let (kinds, codes) = indirect_args(rest, tys)?;
    let scalar_result = matches!(result, Some(Repr::Scalar { .. }));
    if kinds.iter().all(|k| *k == NativeSigKind::I64) && (scalar_result || dst.is_none()) {
        return Ok(false);
    }
    let ret = indirect_ret(sigs, &kinds, dst)?;
    let Some(key) = sig_key(&kinds, ret) else { return Ok(false) };
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
