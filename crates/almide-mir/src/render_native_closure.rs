//! #4006 family G: closures with heap params or a heap result.
//!
//! A scalar closure dispatches through the per-arity `__almd_ci_N` tables
//! (`render_native::push_closure_dispatch_tables`). Other signatures use a
//! table per declared parameter AND result kinds. A source function value
//! retains that complete signature through captures, fields and returns;
//! `CallIndirect` selects its table from that declaration. Hand-authored MIR
//! without a source function type retains the conservative legacy inference.
//! Heap args are borrowed, as at every MIR call; the result is owned.

use crate::lower::LowerError;
use crate::render_native::{var, wall, NTy, NativeSigKind, NativeSigs};
use crate::{CallArg, MirFunction, Op, Repr, ValueId};
use std::collections::BTreeMap;
use std::fmt::Write as _;

fn kind_code(k: NativeSigKind) -> String {
    match k {
        NativeSigKind::Never => "n".into(),
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
        NativeSigKind::Never => "std::convert::Infallible".into(),
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
        NativeSigKind::Never => NTy::Never,
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
        NTy::Never => NativeSigKind::Never,
        NTy::I64 | NTy::F32 => NativeSigKind::I64,
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

/// The dispatch key of a lambda's user params and result, or `None` for an
/// all-scalar signature (served by the per-arity tables).
pub(crate) fn sig_key(ps: &[NativeSigKind], ret: Option<NativeSigKind>) -> Option<String> {
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
pub(crate) fn lambda_sig(sigs: &NativeSigs, name: &str) -> Option<(Vec<NativeSigKind>, Option<NativeSigKind>)> {
    let (ps, r) = sigs.get(name)?;
    Some((ps.get(1..)?.to_vec(), *r))
}

/// The typed dispatch tables by signature key: the param kinds, the result,
/// and the (table index, name) of every lambda of that signature.
type DispatchTables<'a> = BTreeMap<String, (Vec<NativeSigKind>, Option<NativeSigKind>, Vec<(usize, &'a str)>)>;

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
        let Some(key) = sig_key(&ps, ret) else { continue };
        tables.entry(key).or_insert_with(|| (ps, ret, Vec::new())).2.push((idx, name));
    }
    for (key, (ps, ret, fns)) in tables {
        let env_ty = dispatch_env(sigs, fns[0].1);
        let params: String = ps.iter().enumerate().map(|(i, k)| format!(", a{i}: {}", param_spelling(*k))).collect();
        let args: String = (0..ps.len()).map(|i| format!(", a{i}")).collect();
        let ret_s = ret.map(|r| format!(" -> {}", crate::render_native_rec::owned_ty(owned_nty(r)))).unwrap_or_default();
        let _ = write!(out, "fn __almd_ci_{key}(idx: i64, env: {env_ty}{params}){ret_s} {{\n    match idx {{\n");
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
        NTy::Rec(i) if crate::render_native_rec::closure_sig(i).is_some() => Ok(format!("&{}", var(*env))),
        NTy::RecRef(i) if crate::render_native_rec::closure_sig(i).is_some() => Ok(var(*env)),
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
    let (mut kinds, mut codes) = indirect_args(rest, tys)?;
    let declared = match tys.get(env) {
        Some(NTy::Rec(i) | NTy::RecRef(i)) => crate::render_native_rec::closure_sig(*i),
        _ => None,
    };
    let declared_ret = if let Some((params, ret)) = &declared {
        if params.len() != kinds.len() { return Err(wall("native: indirect call arity disagrees with its checked function type")); }
        for ((kind, code), want) in kinds.iter_mut().zip(&mut codes).zip(params) {
            let got = owned_nty(*kind);
            if got != *want && !crate::native_value_repr::same_owned(got, *want) {
                *code = crate::native_value_repr::scalar_recode(code, got, *want)
                    .ok_or_else(|| wall("native: indirect argument disagrees with its checked function type"))?;
            }
            *kind = arg_kind(*want);
        }
        Some(ret.map(arg_kind))
    } else { None };
    let scalar_result = matches!(result, Some(Repr::Scalar { .. }));
    if declared.is_none() && kinds.iter().all(|k| *k == NativeSigKind::I64) && (scalar_result || dst.is_none()) {
        return Ok(false);
    }
    let ret = match declared_ret { Some(r) => r, None => indirect_ret(sigs, &kinds, dst)? };
    let key = sig_key(&kinds, ret).unwrap_or_else(|| kinds.len().to_string());
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

/// Hand-authored MIR can retain the scalar slice environment; checked source
/// closures carry a declared function type and use the typed environment.
pub(crate) fn dispatch_env(sigs: &NativeSigs, name: &str) -> &'static str {
    if sigs.get(name).and_then(|(ps, _)| ps.first())
        .is_some_and(|k| crate::render_native_rec::is_closure(owned_nty(*k))) {
        "&__AlmdClosure"
    } else { "&[i64]" }
}
