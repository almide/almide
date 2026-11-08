//! #4006 family G: closures with heap params or a heap result.
//!
//! A scalar closure dispatches through the per-arity `__almd_ci_N` tables
//! (`render_native::push_closure_dispatch_tables`). Other signatures use a
//! table per declared parameter AND result kinds. A source function value
//! retains that complete signature through captures, fields and returns;
//! `CallIndirect` selects its table from that declaration; an environment
//! without one walls.
//! Heap args are borrowed, as at every MIR call, unless the call hands an
//! arg over to a lambda that takes it over (a move param,
//! `native_move_params`): that call picks the table whose key marks the
//! moved positions. Every lambda also sits in the lending table of its
//! kinds, whose arm copies a lent arg the lambda takes over. The result is
//! owned.

use crate::lower::LowerError;
use crate::render_native::{var, wall, NTy, NativeSigKind, NativeSigs};
use crate::{CallArg, MirFunction, Op, ValueId};
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

/// The dispatch key of a lambda's user params (`moved[i]`: the call hands
/// param `i` over) and result, or `None` for an all-scalar signature (served
/// by the per-arity tables).
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

/// The owned copy of the lent (borrowed-spelled) arg `a` of kind `k`, for a
/// lambda that takes that param over.
fn owned_copy(k: NativeSigKind, a: &str) -> String {
    match k {
        NativeSigKind::Str => format!("AlmideString::from({a})"),
        _ => format!("ToOwned::to_owned({a})"),
    }
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
        // The lending table of its kinds, and the handing-over one of its modes.
        for m in [vec![false; ps.len()], moved.clone()] {
            let Some(key) = sig_key(&ps, &m, ret) else { continue };
            let fns = &mut tables.entry(key).or_insert_with(|| (ps.clone(), m, ret, Vec::new())).3;
            if !fns.contains(&(idx, name)) {
                fns.push((idx, name));
            }
        }
    }
    for (key, (ps, handed, ret, fns)) in tables {
        let env_ty = dispatch_env(sigs, fns[0].1);
        let params: String = ps
            .iter()
            .zip(&handed)
            .enumerate()
            .map(|(i, (k, h))| match h {
                true => format!(", a{i}: {}", crate::render_native_rec::owned_ty(owned_nty(*k))),
                false => format!(", a{i}: {}", param_spelling(*k)),
            })
            .collect();
        let ret_s = ret.map(|r| format!(" -> {}", crate::render_native_rec::owned_ty(owned_nty(r)))).unwrap_or_default();
        let _ = write!(out, "fn __almd_ci_{key}(idx: i64, env: {env_ty}{params}){ret_s} {{\n    match idx {{\n");
        for (idx, name) in fns {
            let takes = lambda_moves(user_fns[name]);
            let args: String = ps
                .iter()
                .enumerate()
                .map(|(i, k)| match (handed[i], takes.get(i).copied().unwrap_or(false)) {
                    // A param passed by value is already the lambda's own.
                    (false, true) if param_spelling(*k).starts_with('&') => format!(", {}", owned_copy(*k, &format!("a{i}"))),
                    _ => format!(", a{i}"),
                })
                .collect();
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

/// The closure env as the table fn takes it (a borrowed block).
fn indirect_env(env: &ValueId, tys: &BTreeMap<ValueId, NTy>) -> Result<String, LowerError> {
    let et = *tys.get(env).ok_or_else(|| wall("native: CallIndirect env untyped"))?;
    match et {
        NTy::Rec(i) if crate::render_native_rec::closure_sig(i).is_some() => Ok(format!("&{}", var(*env))),
        NTy::RecRef(i) if crate::render_native_rec::closure_sig(i).is_some() => Ok(var(*env)),
        _ => Err(wall("native: CallIndirect env is not a closure block")),
    }
}

/// A `CallIndirect`, through the table of its declared function type (an
/// all-scalar one: the per-arity table). `Ok(false)`: `op` is no
/// `CallIndirect`. `handed`: the values a call-site move right after `op`
/// hands over.
pub(crate) fn render_typed_call_indirect(
    op: &Op,
    handed: &[ValueId],
    tys: &mut BTreeMap<ValueId, NTy>,
    out: &mut String,
    indent: usize,
) -> Result<bool, LowerError> {
    let Op::CallIndirect { dst, table_idx, args, .. } = op else { return Ok(false) };
    let Some((CallArg::Handle(env), rest)) = args.split_first() else {
        return Err(wall("native: CallIndirect without a leading env arg"));
    };
    let (mut kinds, moved, mut codes) = indirect_args(rest, handed, tys)?;
    let Some(NTy::Rec(i) | NTy::RecRef(i)) = tys.get(env).copied() else {
        return Err(wall("native: CallIndirect env is not a closure block"));
    };
    let Some((params, ret)) = crate::render_native_rec::closure_sig(i) else {
        return Err(wall("native: an indirect call through an environment with no checked function type"));
    };
    if params.len() != kinds.len() { return Err(wall("native: indirect call arity disagrees with its checked function type")); }
    for ((kind, code), want) in kinds.iter_mut().zip(&mut codes).zip(&params) {
        let got = owned_nty(*kind);
        if got != *want && !crate::native_value_repr::same_owned(got, *want) {
            *code = crate::native_value_repr::scalar_recode(code, got, *want)
                .ok_or_else(|| wall("native: indirect argument disagrees with its checked function type"))?;
        }
        *kind = arg_kind(*want);
    }
    let ret = ret.map(arg_kind);
    let key = sig_key(&kinds, &moved, ret).unwrap_or_else(|| kinds.len().to_string());
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
