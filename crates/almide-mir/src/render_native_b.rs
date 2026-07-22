
/// `Op::CallIndirect` — dispatch through the arity's `__almd_ci_*` table.
fn render_call_indirect(
    dst: &Option<ValueId>,
    table_idx: &ValueId,
    args: &Vec<CallArg>,
    result: &Option<Repr>,
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
    let Some((CallArg::Handle(env), rest)) = args.split_first() else {
        return Err(wall("native: CallIndirect without a leading env arg"));
    };
    let et = *tys
        .get(env)
        .ok_or_else(|| wall("native: CallIndirect env untyped"))?;
    if !et.is_veccy() {
        return Err(wall("native: CallIndirect env is not a closure block"));
    }
    let env_code = if et == NTy::Vec { format!("&{}", var(*env)) } else { var(*env) };
    let mut rendered = vec![var(*table_idx), env_code];
    for a in rest {
        let (code, got) = call_arg(a, tys)?;
        if got != NTy::I64 {
            return Err(wall(
                "native: CallIndirect non-scalar user arg — outside the closures slab",
            ));
        }
        rendered.push(code);
    }
    let call = format!("__almd_ci_{}({})", rest.len(), rendered.join(", "));
    match (dst, result) {
        (Some(d), Some(Repr::Scalar { .. })) => {
            tys.insert(*d, NTy::I64);
            line!("let mut {}: i64 = {call};", var(*d));
        }
        (None, _) => line!("{call};"),
        _ => {
            return Err(wall(
                "native: CallIndirect heap result — outside the closures slab",
            ))
        }
    }
    Ok(())
}

/// `Op::ListGetScalar` — bounds-checked element load via the `almide_idx_get` shim.
fn render_list_get_scalar(
    dst: &ValueId,
    list: &ValueId,
    idx: &ValueId,
    tys: &mut BTreeMap<ValueId, NTy>,
    out: &mut String,
    indent: usize,
    used_shims: &mut Vec<&'static str>,
) -> Result<(), LowerError> {
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    let lt = *tys.get(list).ok_or_else(|| wall("native: ListGet of untyped list"))?;
    if !lt.is_veccy() {
        return Err(wall("native: ListGet on a non-list value"));
    }
    used_shims.push(IDX_GET_SHIM);
    tys.insert(*dst, NTy::I64);
    let borrow = if lt == NTy::Vec { "&" } else { "" };
    line!(
        "let mut {}: i64 = almide_idx_get({borrow}{}, {});",
        var(*dst),
        var(*list),
        var(*idx)
    );
    Ok(())
}

/// `Op::ListSetScalar` — bounds-checked element store via the `almide_idx_set` shim.
fn render_list_set_scalar(
    list: &ValueId,
    idx: &ValueId,
    val: &ValueId,
    tys: &BTreeMap<ValueId, NTy>,
    out: &mut String,
    indent: usize,
    used_shims: &mut Vec<&'static str>,
) -> Result<(), LowerError> {
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    let lt = *tys.get(list).ok_or_else(|| wall("native: ListSet of untyped list"))?;
    // A borrowed param cannot be mutated in place (the MIR COW discipline
    // guarantees a MakeUnique'd OWNED vec here; a VecRef reaching this op
    // is a call-mode violation, walled like the Drop-of-param case).
    if lt != NTy::Vec {
        return Err(wall("native: ListSet on a non-owned list"));
    }
    used_shims.push(IDX_SET_SHIM);
    line!("almide_idx_set(&mut {}, {}, {});", var(*list), var(*idx), var(*val));
    Ok(())
}

/// `Op::SetLocal` — assign (or first-declare) a loop-carried local.
fn render_set_local(
    local: &ValueId,
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
    let t = *tys.get(src).ok_or_else(|| wall("native: SetLocal of untyped value"))?;
    let rhs = match t {
        NTy::I64 | NTy::F64 => var(*src),
        NTy::Str => format!("{}.clone()", var(*src)),
        NTy::StrRef => format!("{}.to_string()", var(*src)),
        NTy::Vec => format!("{}.clone()", var(*src)),
        NTy::VecRef => format!("{}.to_vec()", var(*src)),
    };
    let store_t = match t {
        NTy::StrRef => NTy::Str,
        NTy::VecRef => NTy::Vec,
        other => other,
    };
    if let Some(prev) = tys.get(local) {
        if *prev != store_t {
            return Err(wall("native: SetLocal changes a value's type"));
        }
        line!("{} = {};", var(*local), rhs);
    } else {
        tys.insert(*local, store_t);
        line!("let mut {} = {};", var(*local), rhs);
    }
    Ok(())
}

/// `Op::CallFn` — a user fn call (by declared sig table) or a CLOSED runtime
/// shim call; the single largest op arm (native call-mode resolution).
#[allow(clippy::too_many_arguments)]
fn render_call_fn(
    dst: &Option<ValueId>,
    name: &String,
    args: &Vec<CallArg>,
    result: &Option<Repr>,
    user_fns: &BTreeMap<&str, &MirFunction>,
    sigs: &NativeSigs,
    tys: &mut BTreeMap<ValueId, NTy>,
    out: &mut String,
    indent: usize,
    used_shims: &mut Vec<&'static str>,
) -> Result<(), LowerError> {
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    if let Some(callee) = user_fns.get(name.as_str()) {
        if args.len() != callee.params.len() {
            return Err(wall(format!("native: call to `{name}` arity mismatch")));
        }
        let callee_sig = sigs.get(name.as_str());
        let mut rendered_args = Vec::new();
        for (i, (a, p)) in args.iter().zip(&callee.params).enumerate() {
            // The DECLARED kind (sig table) disambiguates a heap param:
            // `&str` vs `&[i64]`; absent (a synthesized helper) the repr
            // fallback keeps the string convention.
            let want = match callee_sig.and_then(|(ps, _)| ps.get(i)) {
                Some(NativeSigKind::I64) => NTy::I64,
                Some(NativeSigKind::Str) => NTy::StrRef,
                Some(NativeSigKind::ListI64) => NTy::VecRef,
                Some(NativeSigKind::F64) => NTy::F64,
                None => repr_nty(&p.repr, true)?,
            };
            let (code, got) = call_arg(a, tys)?;
            match want {
                NTy::F64 => {
                    rendered_args.push(as_f64_arg(&code, got)?);
                }
                NTy::I64 => {
                    if got != NTy::I64 {
                        return Err(wall(format!(
                            "native: heap arg to scalar param of `{name}`"
                        )));
                    }
                    rendered_args.push(code);
                }
                NTy::VecRef | NTy::Vec => {
                    if !got.is_veccy() {
                        return Err(wall(format!(
                            "native: non-list arg to list param of `{name}`"
                        )));
                    }
                    rendered_args.push(match got {
                        NTy::Vec => format!("&{code}"),
                        _ => code,
                    });
                }
                _ => {
                    if !got.is_stringy() {
                        return Err(wall(format!(
                            "native: scalar arg to heap param of `{name}`"
                        )));
                    }
                    rendered_args.push(as_str_arg(&code, got));
                }
            }
        }
        let call = format!("{}({})", mangle(name), rendered_args.join(", "));
        match (dst, result) {
            (Some(d), Some(r)) => {
                // A heap result is FRESH OWNED (the callee moved it out).
                // Its KIND comes from the callee's declared return.
                let t = match callee_sig.and_then(|(_, r)| *r) {
                    Some(NativeSigKind::ListI64) => NTy::Vec,
                    Some(NativeSigKind::Str) => NTy::Str,
                    Some(NativeSigKind::I64) => NTy::I64,
                    Some(NativeSigKind::F64) => NTy::F64,
                    None => repr_nty(r, false)?,
                };
                tys.insert(*d, t);
                let ty_name = match t {
                    NTy::Str => "String",
                    NTy::Vec => "Vec<i64>",
                    NTy::F64 => "f64",
                    _ => "i64",
                };
                line!("let mut {}: {} = {};", var(*d), ty_name, call);
            }
            (None, _) => line!("{call};"),
            (Some(d), None) => {
                // Result repr unknown: scalar by convention.
                tys.insert(*d, NTy::I64);
                line!("let mut {}: i64 = {};", var(*d), call);
            }
        }
    } else if let Some((param_tys, ret_ty, shim_src)) = shim(name) {
        if args.len() != param_tys.len() {
            return Err(wall(format!("native: shim `{name}` arity mismatch")));
        }
        let mut rendered_args = Vec::new();
        for (a, want) in args.iter().zip(param_tys) {
            let (code, got) = call_arg(a, tys)?;
            match want {
                NTy::F64 => {
                    rendered_args.push(as_f64_arg(&code, got)?);
                }
                NTy::I64 => {
                    if got != NTy::I64 {
                        return Err(wall(format!("native: shim `{name}` arg type mismatch")));
                    }
                    rendered_args.push(code);
                }
                _ => {
                    if !got.is_stringy() {
                        return Err(wall(format!("native: shim `{name}` arg type mismatch")));
                    }
                    // Heap args are BORROWED at the MIR level — by reference.
                    rendered_args.push(as_str_arg(&code, got));
                }
            }
        }
        used_shims.push(shim_src);
        let call = format!("{}({})", shim_rust_name(name), rendered_args.join(", "));
        match (dst, ret_ty) {
            (Some(d), Some(t)) => {
                tys.insert(*d, t);
                let ty_name = if t == NTy::Str { "String" } else { "i64" };
                line!("let mut {}: {} = {};", var(*d), ty_name, call);
            }
            (None, _) => line!("{call};"),
            (Some(_), None) => {
                return Err(wall(format!("native: shim `{name}` has no result")))
            }
        }
        let _ = result;
    } else {
        return Err(wall(format!(
            "native: call to `{name}` — not a lowered user fn and not in the \
             native runtime floor"
        )));
    }
    Ok(())
}

/// `Op::Prim { kind: FloatBin(op), .. }` — a binary float op.
fn render_float_bin(
    op: &crate::FBinOp,
    d: &ValueId,
    args: &Vec<ValueId>,
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
    use crate::FBinOp;
    let sym = match op {
        FBinOp::Add => "+",
        FBinOp::Sub => "-",
        FBinOp::Mul => "*",
        FBinOp::Div => "/",
        FBinOp::Min | FBinOp::Max | FBinOp::CopySign => {
            return Err(wall(format!(
                "native: float op {op:?} — outside the rung subset (NaN semantics)"
            )))
        }
    };
    let a = as_f64_arg(&var(args[0]), *tys.get(&args[0]).ok_or_else(|| wall("native: float arg untyped"))?)?;
    let b = as_f64_arg(&var(args[1]), *tys.get(&args[1]).ok_or_else(|| wall("native: float arg untyped"))?)?;
    tys.insert(*d, NTy::F64);
    line!("let mut {}: f64 = {a} {sym} {b};", var(*d));
    Ok(())
}

/// `Op::Prim { kind: FloatUn(op), .. }` — a unary float op.
fn render_float_un(
    op: &crate::FUnOp,
    d: &ValueId,
    args: &Vec<ValueId>,
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
    use crate::FUnOp;
    let a = as_f64_arg(&var(args[0]), *tys.get(&args[0]).ok_or_else(|| wall("native: float arg untyped"))?)?;
    let expr = match op {
        FUnOp::Neg => format!("-({a})"),
        FUnOp::Abs => format!("({a}).abs()"),
        FUnOp::Sqrt => format!("({a}).sqrt()"),
        FUnOp::Floor => format!("({a}).floor()"),
        FUnOp::Ceil => format!("({a}).ceil()"),
    };
    tys.insert(*d, NTy::F64);
    line!("let mut {}: f64 = {expr};", var(*d));
    Ok(())
}

/// `Op::Prim { kind: FloatCmp(op), .. }` — a float comparison (result is i64 0/1).
fn render_float_cmp(
    op: &crate::FCmpOp,
    d: &ValueId,
    args: &Vec<ValueId>,
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
    use crate::FCmpOp;
    let sym = match op {
        FCmpOp::Lt => "<",
        FCmpOp::Le => "<=",
        FCmpOp::Gt => ">",
        FCmpOp::Ge => ">=",
        FCmpOp::Eq => "==",
        FCmpOp::Ne => "!=",
    };
    let a = as_f64_arg(&var(args[0]), *tys.get(&args[0]).ok_or_else(|| wall("native: float arg untyped"))?)?;
    let b = as_f64_arg(&var(args[1]), *tys.get(&args[1]).ok_or_else(|| wall("native: float arg untyped"))?)?;
    tys.insert(*d, NTy::I64);
    line!("let mut {}: i64 = ({a} {sym} {b}) as i64;", var(*d));
    Ok(())
}

/// `Op::Call` — a witness-level runtime call (`println` lowers through these).
fn render_call_witness(
    dst: &Option<ValueId>,
    func: &crate::RtFn,
    args: &Vec<CallArg>,
    tys: &BTreeMap<ValueId, NTy>,
    out: &mut String,
    indent: usize,
    used_shims: &mut Vec<&'static str>,
) -> Result<(), LowerError> {
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    use crate::RtFn;
    match (func, args.as_slice()) {
        (RtFn::PrintStr, [a]) => {
            let (code, t) = call_arg(a, tys)?;
            if !t.is_stringy() {
                return Err(wall("native: print_str of a non-String"));
            }
            used_shims.push(shim("print_str").expect("\"print_str\" is a literal shim() match arm, always Some").2);
            line!("rt_print_str({});", as_str_arg(&code, t));
        }
        (RtFn::PrintInt, [a]) => {
            let (code, t) = call_arg(a, tys)?;
            if t != NTy::I64 {
                return Err(wall("native: print_int of a non-Int"));
            }
            line!("println!(\"{{}}\", {code});");
        }
        other => {
            return Err(wall(format!(
                "native: runtime call {other:?} — outside the rung subset"
            )))
        }
    }
    if dst.is_some() {
        return Err(wall("native: print with a result — outside the rung subset"));
    }
    Ok(())
}

/// `Op::IfThen` — open an `if`, pushing the if-as-value join marker (if any).
fn render_if_then(
    cond: &ValueId,
    dst: &Option<ValueId>,
    out: &mut String,
    indent: &mut usize,
    if_stack: &mut Vec<Option<(String, ValueId)>>,
) {
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..*indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    if let Some(d) = dst {
        // if-as-value: the join decl is patched in at the first arm
        // yield, when its type is known.
        let marker = format!("//__JOIN_{}__", var(*d));
        line!("{marker}");
        if_stack.push(Some((marker, *d)));
    } else {
        if_stack.push(None);
    }
    line!("if {} != 0 {{", var(*cond));
    *indent += 1;
}

/// `Op::Else` — close the `then` arm, patch the if-as-value join decl in (its
/// type is known now), open the `else` arm.
fn render_else(
    val: &Option<ValueId>,
    tys: &mut BTreeMap<ValueId, NTy>,
    out: &mut String,
    indent: &mut usize,
    if_stack: &Vec<Option<(String, ValueId)>>,
) -> Result<(), LowerError> {
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..*indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    if let Some(Some((marker, d))) = if_stack.last() {
        let v = val.ok_or_else(|| wall("native: if-value arm without a yield"))?;
        let t = *tys.get(&v).ok_or_else(|| wall("native: if-value yield untyped"))?;
        let (decl, join_t, rhs) = match t {
            NTy::I64 => (
                format!("let mut {}: i64 = 0;", var(*d)),
                NTy::I64,
                var(v),
            ),
            NTy::Str => (
                format!("let mut {}: String = String::new();", var(*d)),
                NTy::Str,
                format!("{}.clone()", var(v)),
            ),
            NTy::StrRef => (
                format!("let mut {}: String = String::new();", var(*d)),
                NTy::Str,
                format!("{}.to_string()", var(v)),
            ),
            NTy::Vec => (
                format!("let mut {}: Vec<i64> = Vec::new();", var(*d)),
                NTy::Vec,
                format!("{}.clone()", var(v)),
            ),
            NTy::VecRef => (
                format!("let mut {}: Vec<i64> = Vec::new();", var(*d)),
                NTy::Vec,
                format!("{}.to_vec()", var(v)),
            ),
            NTy::F64 => (
                format!("let mut {}: f64 = 0.0;", var(*d)),
                NTy::F64,
                var(v),
            ),
        };
        *out = out.replacen(marker, &decl, 1);
        tys.insert(*d, join_t);
        line!("{} = {};", var(*d), rhs);
    }
    *indent -= 1;
    line!("}} else {{");
    *indent += 1;
    Ok(())
}

/// `Op::EndIf` — close the `else` arm, converging the if-as-value join (if any).
fn render_end_if(
    val: &Option<ValueId>,
    tys: &BTreeMap<ValueId, NTy>,
    out: &mut String,
    indent: &mut usize,
    if_stack: &mut Vec<Option<(String, ValueId)>>,
) -> Result<(), LowerError> {
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..*indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    let top = if_stack.pop().ok_or_else(|| wall("native: EndIf without IfThen"))?;
    if let Some((_, d)) = top {
        let v = val.ok_or_else(|| wall("native: if-value arm without a yield"))?;
        let t = *tys.get(&v).ok_or_else(|| wall("native: if-value yield untyped"))?;
        let join_t = *tys.get(&d).ok_or_else(|| wall("native: if-value join untyped"))?;
        let rhs = match t {
            NTy::I64 | NTy::F64 => var(v),
            NTy::Str => format!("{}.clone()", var(v)),
            NTy::StrRef => format!("{}.to_string()", var(v)),
            NTy::Vec => format!("{}.clone()", var(v)),
            NTy::VecRef => format!("{}.to_vec()", var(v)),
        };
        let arm_t = match t {
            NTy::StrRef => NTy::Str,
            NTy::VecRef => NTy::Vec,
            other => other,
        };
        if arm_t != join_t {
            return Err(wall("native: if-value arms disagree on type"));
        }
        line!("{} = {};", var(d), rhs);
    }
    *indent -= 1;
    line!("}}");
    Ok(())
}

/// A rendered call argument with the NTy it carries (`Imm` is always i64).
fn call_arg(a: &CallArg, tys: &BTreeMap<ValueId, NTy>) -> Result<(String, NTy), LowerError> {
    match a {
        CallArg::Handle(v) | CallArg::Scalar(v) => {
            let t = *tys.get(v).ok_or_else(|| wall("native: call arg untyped"))?;
            Ok((var(*v), t))
        }
        CallArg::Imm(n) => Ok((format!("{n}i64"), NTy::I64)),
        other => Err(wall(format!("native: call arg {other:?} — outside the rung subset"))),
    }
}

fn render_int_binop(
    op: &IntOp,
    a: ValueId,
    b: ValueId,
    used_shims: &mut Vec<&'static str>,
) -> Result<String, LowerError> {
    let (l, r) = (var(a), var(b));
    Ok(match op {
        IntOp::Add => format!("{l}.wrapping_add({r})"),
        IntOp::Sub => format!("{l}.wrapping_sub({r})"),
        IntOp::Mul => format!("{l}.wrapping_mul({r})"),
        // Div/Mod carry the C-001/C-002 abort discipline — route through the
        // same checked shims the CallFn path uses (one definition of the abort).
        IntOp::Div => {
            used_shims.push(shim("__chk_div").expect("\"__chk_div\" is a literal shim() match arm, always Some").2);
            format!("rt_chk_div({l}, {r})")
        }
        IntOp::Mod => {
            used_shims.push(shim("__chk_mod").expect("\"__chk_mod\" is a literal shim() match arm, always Some").2);
            format!("rt_chk_mod({l}, {r})")
        }
        IntOp::Eq => format!("({l} == {r}) as i64"),
        IntOp::Ne => format!("({l} != {r}) as i64"),
        IntOp::Lt => format!("({l} < {r}) as i64"),
        IntOp::Le => format!("({l} <= {r}) as i64"),
        IntOp::Gt => format!("({l} > {r}) as i64"),
        IntOp::Ge => format!("({l} >= {r}) as i64"),
        other => return Err(wall(format!("native: int op {other:?} — outside the rung subset"))),
    })
}

/// A user fn name that is a valid Rust identifier (dots from module paths).
fn mangle(name: &str) -> String {
    format!("almd_{}", name.replace(['.', '$'], "_"))
}

fn op_name(op: &Op) -> &'static str {
    match op {
        Op::Alloc { .. } => "Alloc",
        Op::Const { .. } => "Const",
        Op::ConstInt { .. } => "ConstInt",
        Op::Dup { .. } => "Dup",
        Op::Drop { .. } => "Drop",
        Op::DropListStr { .. } => "DropListStr",
        Op::Consume { .. } => "Consume",
        Op::Borrow { .. } => "Borrow",
        Op::MakeUnique { .. } => "MakeUnique",
        Op::Pure { .. } => "Pure",
        Op::Call { .. } => "Call",
        Op::CallFn { .. } => "CallFn",
        Op::CallImport { .. } => "CallImport",
        Op::CallIndirect { .. } => "CallIndirect",
        Op::FuncRef { .. } => "FuncRef",
        Op::IntBinOp { .. } => "IntBinOp",
        Op::Prim { .. } => "Prim",
        Op::IfThen { .. } => "IfThen",
        Op::Else { .. } => "Else",
        Op::EndIf { .. } => "EndIf",
        Op::LoopStart => "LoopStart",
        Op::LoopBreakUnless { .. } => "LoopBreakUnless",
        Op::LoopEnd => "LoopEnd",
        Op::SetLocal { .. } => "SetLocal",
        _ => "unknown",
    }
}
