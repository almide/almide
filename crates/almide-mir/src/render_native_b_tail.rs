// ── tail of render_native_b.rs, include!-spliced back at module level ──
//
// A pure code move: this file continues its parent verbatim. The split exists
// only so the parent stays under the 800-line ceiling the codopsy gate holds
// this crate to; there is no boundary of meaning here, and `include!` at module
// level is the one splice Rust allows (an impl-item position rejects it).

/// `Op::Prim { kind: FloatBin(op), .. }` — a binary float op.
pub(crate) fn render_float_bin(
    op: &crate::FBinOp,
    d: &ValueId,
    args: &[ValueId],
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
pub(crate) fn render_float_un(
    op: &crate::FUnOp,
    d: &ValueId,
    args: &[ValueId],
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
        // TIES TO EVEN — the exact semantics of wasm's `f64.nearest`, so the
        // two legs agree bit for bit (#1197's canonical fast-exp range
        // reduction). NOT `.round()`, which is half-away-from-zero.
        FUnOp::Nearest => format!("({a}).round_ties_even()"),
    };
    tys.insert(*d, NTy::F64);
    line!("let mut {}: f64 = {expr};", var(*d));
    Ok(())
}

/// `Op::Prim { kind: FloatCmp(op), .. }` — a float comparison (result is i64 0/1).
pub(crate) fn render_float_cmp(
    op: &crate::FCmpOp,
    d: &ValueId,
    args: &[ValueId],
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
    line!("{}", crate::render_native_cond::bind(*d, &format!("{a} {sym} {b}")));
    Ok(())
}

/// `Op::Call` — a witness-level runtime call (`println` lowers through these).
fn render_call_witness(
    dst: &Option<ValueId>,
    func: &crate::RtFn,
    args: &Vec<CallArg>,
    sink: OpSink<'_>,
) -> Result<(), LowerError> {
    let OpSink { tys, out, indent, used_shims } = sink;
    let tys: &BTreeMap<ValueId, NTy> = tys;
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
    tys: &BTreeMap<ValueId, NTy>,
    out: &mut String,
    indent: &mut usize,
    if_stack: &mut Vec<Option<IfJoin>>,
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
        if_stack.push(Some((marker, *d, tys.keys().copied().collect())));
    } else {
        if_stack.push(None);
    }
    line!("if {} {{", crate::render_native_cond::truthy(*cond));
    *indent += 1;
}

/// `Op::Else` — close the `then` arm, patch the if-as-value join decl in (its
/// type is known now), open the `else` arm.
fn render_else(
    val: &Option<ValueId>,
    borrows: &std::collections::BTreeSet<ValueId>,
    tys: &mut BTreeMap<ValueId, NTy>,
    out: &mut String,
    indent: &mut usize,
    if_stack: &mut [Option<IfJoin>],
) -> Result<(), LowerError> {
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..*indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    if let Some(Some((marker, d, known))) = if_stack.last_mut() {
        let v = val.ok_or_else(|| wall("native: if-value arm without a yield"))?;
        let t = *tys.get(&v).ok_or_else(|| wall("native: if-value yield untyped"))?;
        let arm_own = !known.contains(&v);
        // Every heap join is declared unset (#4006, no-perf-loss): both arms
        // assign it, so a default built and dropped per evaluation buys nothing.
        let (decl, join_t, rhs) = match t {
            // #3532: a borrow join holds the arm's shared reference.
            NTy::VecRef | NTy::StrRef if borrows.contains(d) => (
                format!("let {}: {};", var(*d), if t == NTy::VecRef { "&[i64]" } else { "&str" }),
                t,
                var(v),
            ),
            NTy::RecRef(i) if borrows.contains(d) => (
                format!("let {}: &{};", var(*d), crate::render_native_rec::struct_name(i)),
                t,
                var(v),
            ),
            NTy::I64 => (
                format!("let mut {}: i64 = 0;", var(*d)),
                NTy::I64,
                var(v),
            ),
            NTy::Str => (
                format!("let mut {}: AlmideString;", var(*d)),
                NTy::Str,
                format!("{}.clone()", var(v)),
            ),
            NTy::StrRef => (
                format!("let mut {}: AlmideString;", var(*d)),
                NTy::Str,
                format!("AlmideString::from({})", var(v)),
            ),
            NTy::Vec => (
                format!("let mut {}: Vec<i64>;", var(*d)),
                NTy::Vec,
                format!("{}.clone()", var(v)),
            ),
            NTy::VecRef => (
                format!("let mut {}: Vec<i64>;", var(*d)),
                NTy::Vec,
                format!("{}.to_vec()", var(v)),
            ),
            NTy::F64 => (
                format!("let mut {}: f64 = 0.0;", var(*d)),
                NTy::F64,
                var(v),
            ),
            NTy::Res => (
                format!("let mut {}: Result<i64, AlmideString> = Ok(0);", var(*d)),
                NTy::Res,
                format!("{}.clone()", var(v)),
            ),
            NTy::VecStr | NTy::VecStrRef => (
                format!("let mut {}: Vec<AlmideString>;", var(*d)),
                NTy::VecStr,
                if t == NTy::VecStr { format!("{}.clone()", var(v)) } else { format!("{}.to_vec()", var(v)) },
            ),
            // Declared unset: both arms assign it (a placeholder would be
            // built and dropped on every pass through the join).
            NTy::Rec(i) | NTy::RecRef(i) => (
                format!("let mut {}: {};", var(*d), crate::render_native_rec::struct_name(i)),
                NTy::Rec(i),
                format!("{}.clone()", var(v)),
            ),
            NTy::Opt => (
                format!("let mut {}: Option<i64> = None;", var(*d)),
                NTy::Opt,
                var(v),
            ),
            NTy::ResStr => (
                format!("let mut {}: Result<AlmideString, AlmideString>;", var(*d)),
                NTy::ResStr,
                format!("{}.clone()", var(v)),
            ),
        };
        *out = out.replacen(marker.as_str(), &decl, 1);
        tys.insert(*d, join_t);
        line!("{} = {};", var(*d), join_rhs(v, t, arm_own, rhs));
        // the else arm's own values are the ones defined after this point
        *known = tys.keys().copied().collect();
    }
    *indent -= 1;
    line!("}} else {{");
    *indent += 1;
    Ok(())
}

/// `Op::EndIf` — close the `else` arm, converging the if-as-value join (if any).
fn render_end_if(
    val: &Option<ValueId>,
    borrows: &std::collections::BTreeSet<ValueId>,
    tys: &BTreeMap<ValueId, NTy>,
    out: &mut String,
    indent: &mut usize,
    if_stack: &mut Vec<Option<IfJoin>>,
) -> Result<(), LowerError> {
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..*indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    let top = if_stack.pop().ok_or_else(|| wall("native: EndIf without IfThen"))?;
    if let Some((_, d, known)) = top {
        let v = val.ok_or_else(|| wall("native: if-value arm without a yield"))?;
        let t = *tys.get(&v).ok_or_else(|| wall("native: if-value yield untyped"))?;
        let join_t = *tys.get(&d).ok_or_else(|| wall("native: if-value join untyped"))?;
        if borrows.contains(&d) && matches!(join_t, NTy::VecRef | NTy::StrRef | NTy::RecRef(_)) {
            if t != join_t {
                return Err(wall("native: borrow-join arms disagree on type"));
            }
            line!("{} = {};", var(d), var(v));
            *indent -= 1;
            line!("}}");
            return Ok(());
        }
        let rhs = end_if_copy_rhs(v, t);
        if let Some(recode) = crate::native_value_repr::scalar_recode(&var(v), t, join_t) {
            line!("{} = {};", var(d), recode);
            *indent -= 1;
            line!("}}");
            return Ok(());
        }
        let arm_t = crate::render_native::owned_nty(t);
        if arm_t != join_t && !(crate::render_native::owned_nty(join_t) == join_t
            && crate::native_value_repr::same_owned(arm_t, join_t)) {
            return Err(wall(format!("native: if-value arms disagree on type ({join_t:?} vs {arm_t:?})")));
        }
        line!("{} = {};", var(d), join_rhs(v, t, !known.contains(&v), rhs));
    }
    *indent -= 1;
    line!("}}");
    Ok(())
}

/// The owned copy of `v`, by its NTy: an if-value arm's yield copied into the
/// join (the copy [`join_rhs`] keeps when the value outlives the arm), or a
/// `SetLocal`'s source.
fn end_if_copy_rhs(v: ValueId, t: NTy) -> String {
    match t {
        NTy::I64 | NTy::F64 => var(v),
        NTy::Str => format!("{}.clone()", var(v)),
        NTy::StrRef => format!("AlmideString::from({})", var(v)),
        NTy::Vec => format!("{}.clone()", var(v)),
        NTy::VecRef => format!("{}.to_vec()", var(v)),
        NTy::Res => format!("{}.clone()", var(v)),
        NTy::VecStr => format!("{}.clone()", var(v)),
        NTy::VecStrRef => format!("{}.to_vec()", var(v)),
        NTy::Rec(_) | NTy::RecRef(_) => format!("{}.clone()", var(v)),
        NTy::Opt => var(v),
        NTy::ResStr => format!("{}.clone()", var(v)),
    }
}

/// An if-as-value join: the decl marker, the join value, and the values that
/// existed when the current arm opened.
pub(crate) type IfJoin = (String, ValueId, std::collections::BTreeSet<ValueId>);

/// The join's right-hand side: an owned heap value the arm itself defined
/// dies with the arm, so it moves (its `.clone()` copied it a second time);
/// a value that outlives the arm keeps `copy`.
fn join_rhs(v: ValueId, t: NTy, arm_own: bool, copy: String) -> String {
    // A value the arm itself defined dies with the arm's block: the join takes
    // it by move (a list of strings or a generated record included — copying
    // one deep-cloned every element, #4006 no-perf-loss audit).
    match t {
        NTy::Str | NTy::Vec | NTy::Res | NTy::ResStr | NTy::VecStr | NTy::Rec(_) if arm_own => var(v),
        _ => copy,
    }
}

/// A rendered call argument with the NTy it carries (`Imm` is always i64).
pub(crate) fn call_arg(a: &CallArg, tys: &BTreeMap<ValueId, NTy>) -> Result<(String, NTy), LowerError> {
    match a {
        CallArg::Handle(v) | CallArg::Scalar(v) => {
            let t = *tys.get(v).ok_or_else(|| wall("native: call arg untyped"))?;
            Ok((var(*v), t))
        }
        CallArg::Imm(n) => Ok((format!("{n}i64"), NTy::I64)),
        other => Err(wall(format!("native: call arg {other:?} — outside the rung subset"))),
    }
}

/// MIR scalar slots and integer operations carry IEEE bits for Float values.
/// Numeric float arithmetic uses FloatBin; this conversion never casts a value.
pub(crate) fn scalar_bits(v: ValueId, tys: &BTreeMap<ValueId, NTy>) -> Result<String, LowerError> {
    match tys.get(&v) {
        Some(NTy::I64) => Ok(var(v)),
        Some(NTy::F64) => Ok(format!("({}.to_bits() as i64)", var(v))),
        other => Err(wall(format!("native: scalar bits of {other:?}"))),
    }
}

pub(crate) fn render_int_binop(
    op: &IntOp,
    a: ValueId,
    b: ValueId,
    tys: &BTreeMap<ValueId, NTy>,
    used_shims: &mut Vec<&'static str>,
) -> Result<String, LowerError> {
    let (l, r) = (scalar_bits(a, tys)?, scalar_bits(b, tys)?);
    Ok(match op {
        IntOp::Add => format!("{l}.wrapping_add({r})"),
        IntOp::Sub => format!("{l}.wrapping_sub({r})"),
        IntOp::Mul => format!("{l}.wrapping_mul({r})"),
        IntOp::Div | IntOp::Mod | IntOp::DivU | IntOp::ModU => checked_int_binop(op, b, &l, &r, used_shims),
        // The bitwise floor: wasm's i64.and/or/xor and its shifts, whose count
        // is taken mod 64 — `wrapping_sh*` masks the same six bits.
        IntOp::And => format!("({l} & {r})"),
        IntOp::Or => format!("({l} | {r})"),
        IntOp::Xor => format!("({l} ^ {r})"),
        IntOp::Shl => format!("{l}.wrapping_shl({r} as u32)"),
        IntOp::Shr => format!("{l}.wrapping_shr({r} as u32)"),
        IntOp::ShrU => format!("((({l} as u64).wrapping_shr({r} as u32)) as i64)"),
        IntOp::LtU => format!("(({l} as u64) < ({r} as u64)) as i64"),
        IntOp::LeU => format!("(({l} as u64) <= ({r} as u64)) as i64"),
        IntOp::GtU => format!("(({l} as u64) > ({r} as u64)) as i64"),
        IntOp::GeU => format!("(({l} as u64) >= ({r} as u64)) as i64"),
        // The signed comparisons render through `cmp_int_expr` (as a `bool`
        // when only a branch reads them — render_native_cond), never here.
        _ => return Err(wall("native: a signed comparison reached the arithmetic render")),
    })
}

/// The dividing arms of [`render_int_binop`] (`op` is `Div`, `Mod`, `DivU`
/// or `ModU`).
fn checked_int_binop(op: &IntOp, b: ValueId, l: &str, r: &str, used_shims: &mut Vec<&'static str>) -> String {
    match op {
        // Div/Mod carry the C-001/C-002 abort discipline — route through the
        // same checked shims the CallFn path uses (one definition of the abort).
        IntOp::Div => {
            used_shims.push(shim("__chk_div").expect("\"__chk_div\" is a literal shim() match arm, always Some").2);
            format!("rt_chk_div({l}, {r})")
        }
        // A literal divisor takes the standard route's constant remainder
        // (render_native_rem.rs); it can never be 0 or -1, so no abort is lost.
        IntOp::Mod if literal_rem_divisor(b).is_some() => {
            used_shims.push(rem_const_shim());
            format!("almide_rem_const::<{}>({l})", literal_rem_divisor(b).unwrap_or_default())
        }
        IntOp::Mod => {
            used_shims.push(shim("__chk_mod").expect("\"__chk_mod\" is a literal shim() match arm, always Some").2);
            format!("rt_chk_mod({l}, {r})")
        }
        // The unsigned 64-bit lane (#872): the i64 local carries the u64 bit
        // pattern — cast both sides for div/rem/ordering. Divide-by-zero
        // aborts through the same checked shim family as the signed pair
        // (identical stderr bytes); there is no MIN÷-1 case unsigned.
        IntOp::DivU => {
            used_shims.push(shim("__chk_div_u").expect("\"__chk_div_u\" is a literal shim() match arm, always Some").2);
            format!("rt_chk_div_u({l}, {r})")
        }
        // `IntOp::ModU`, the one dividing op left.
        _ => {
            used_shims.push(shim("__chk_mod_u").expect("\"__chk_mod_u\" is a literal shim() match arm, always Some").2);
            format!("rt_chk_mod_u({l}, {r})")
        }
    }
}

/// A signed integer comparison as a Rust `bool` expression (`None` for any
/// other op; the unsigned lane's stay in [`render_int_binop`]).
pub(crate) fn cmp_int_expr(op: &IntOp, l: &str, r: &str) -> Option<String> {
    Some(match op {
        IntOp::Eq => format!("{l} == {r}"),
        IntOp::Ne => format!("{l} != {r}"),
        IntOp::Lt => format!("{l} < {r}"),
        IntOp::Le => format!("{l} <= {r}"),
        IntOp::Gt => format!("{l} > {r}"),
        IntOp::Ge => format!("{l} >= {r}"),
        _ => return None,
    })
}

/// A user fn name that is a valid Rust identifier (dots from module paths).
pub(crate) fn mangle(name: &str) -> String {
    format!("almd_{}", name.replace(['.', '$'], "_"))
}

fn op_name(op: &Op) -> String {
    // The variant name is the first token of the Debug form (`Alloc { dst:
    // … }` → "Alloc") — total over every Op, present and future, where the
    // old hand table answered "unknown" for anything it lagged behind.
    let d = format!("{op:?}");
    d.split([' ', '{', '(']).next().unwrap_or("Op").to_string()
}

/// [`render_declared_floor_call`]'s moved payloads: a borrowed read whose
/// carrier dies at the call is taken, moved out, where the floor fn's param
/// (or its `@reuse` twin's) is by value and no other arg reads that carrier.
fn take_dying_payloads(
    args: &[CallArg],
    floor: &crate::native_floor::FloorFn,
    rendered: &mut [(String, NTy)],
    dying: &mut [bool],
) {
    for (k, a) in args.iter().enumerate() {
        let CallArg::Handle(v) = a else { continue };
        let Some(take) = crate::render_native_move::taken_arg(*v) else { continue };
        let by_value = matches!(
            floor.params.get(k),
            Some(
                crate::native_floor::FloorParam::Map(false)
                    | crate::native_floor::FloorParam::Set(false)
                    | crate::native_floor::FloorParam::Value(false)
                    | crate::native_floor::FloorParam::Matrix(false)
            )
        ) || floor.twin.as_ref().is_some_and(|t| t.0 == k);
        // No other arg may read the same carrier while it is edited.
        let base_of = |x: &ValueId| {
            crate::render_native_addr::payload_take_of(*x)
                .map(|p| p.0)
                .or_else(|| crate::render_native_addr::field_of(*x).map(|p| p.0))
        };
        let base = base_of(v);
        let shared = args.iter().enumerate().any(|(j, b)| {
            j != k && matches!(b, CallArg::Handle(x) if Some(*x) == base || (base.is_some() && base_of(x) == base))
        });
        // Only a borrowed read has a payload to take.
        let owned = crate::render_native::owned_nty(rendered[k].1);
        if !by_value || shared || owned == rendered[k].1 {
            continue;
        }
        rendered[k] = (format!("{{ {take} }}"), owned);
        dying[k] = true;
    }
}
