
/// Where a rendered op writes, and the running state it updates.
///
/// `tys` is the value→type map the whole function accumulates, `out` the source
/// being built, and `used_shims` the set of runtime helpers the render has
/// pulled in. All three are per-function state that every op handler both reads
/// and extends, so they travel as one value instead of three `&mut`s that a
/// caller could pair with a different function's buffer.
pub(crate) struct OpSink<'a> {
    pub tys: &'a mut BTreeMap<ValueId, NTy>,
    pub out: &'a mut String,
    pub indent: usize,
    pub used_shims: &'a mut Vec<&'static str>,
}

/// `Op::ListGetScalar` — bounds-checked element load via the `almide_idx_get` shim.
fn render_list_get_scalar(
    dst: &ValueId,
    list: &ValueId,
    idx: &ValueId,
    sink: OpSink<'_>,
) -> Result<(), LowerError> {
    let OpSink { tys, out, indent, used_shims } = sink;
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
    // The shim checks the index, then reads (render_native_bounds).
    let slot = crate::render_native_bounds::Slot { base: *list, idx: Some(*idx), k: 0 };
    crate::render_native_bounds::check(slot);
    crate::render_native_bounds::access(slot);
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
    let lt = *tys.get(list).ok_or_else(|| wall("native: ListSet of untyped list"))?;
    // A borrowed param cannot be mutated in place (the MIR COW discipline
    // guarantees a MakeUnique'd OWNED vec here; a VecRef reaching this op
    // is a call-mode violation, walled like the Drop-of-param case).
    if lt != NTy::Vec {
        return Err(wall("native: ListSet on a non-owned list"));
    }
    used_shims.push(IDX_SET_SHIM);
    // The shim checks the index, then stores (render_native_bounds).
    let slot = crate::render_native_bounds::Slot { base: *list, idx: Some(*idx), k: 0 };
    crate::render_native_bounds::check(slot);
    crate::render_native_bounds::store(slot);
    line!("almide_idx_set(&mut {}, {}, {});", var(*list), var(*idx), scalar_bits(*val, tys)?);
    Ok(())
}

/// `Op::SetLocal` — assign (or first-declare) a loop-carried local.
pub(crate) fn render_set_local(
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
    let rhs = end_if_copy_rhs(*src, t);
    let store_t = crate::render_native::owned_nty(t);
    if let Some(prev) = tys.get(local) {
        if let Some(recode) = crate::native_value_repr::scalar_recode(&var(*src), t, *prev) {
            line!("{} = {};", var(*local), recode);
            return Ok(());
        }
        if *prev != store_t && !(crate::render_native::owned_nty(*prev) == *prev
            && crate::native_value_repr::same_owned(*prev, store_t)) {
            return Err(wall(format!("native: SetLocal changes a value's type ({prev:?} <- {store_t:?})")));
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
/// The call being rendered.
#[derive(Copy, Clone)]
pub(crate) struct NativeCall<'a> {
    pub dst: &'a Option<ValueId>,
    pub name: &'a String,
    pub args: &'a Vec<CallArg>,
    pub result: &'a Option<Repr>,
}

/// Where a native render writes, and what it may look up while writing.
///
/// The output buffer, the running value-type map and the shim list are all
/// accumulated across the whole function, so they travel together rather than
/// as three separate `&mut`s a caller could pair with the wrong `out`.
pub(crate) struct NativeSink<'a, 'f> {
    pub user_fns: &'a BTreeMap<&'a str, &'a MirFunction>,
    pub sigs: &'a NativeSigs,
    pub tys: &'a mut BTreeMap<ValueId, NTy>,
    pub out: &'f mut String,
    pub indent: usize,
    pub used_shims: &'a mut Vec<&'static str>,
}

fn render_call_fn(call: NativeCall<'_>, sink: NativeSink<'_, '_>) -> Result<(), LowerError> {
    let NativeSink { user_fns, sigs, tys, out, indent, used_shims } = sink;
    let name = call.name;
    if let Some(callee) = user_fns.get(name.as_str()) {
        // A closure argument of another signature than the linked body
        // declares is an argument kind it refuses (#4588): the call it
        // rendered is taken back.
        let marks = (out.len(), used_shims.len());
        let rendered = render_user_fn_call(call, callee, sigs, OpSink { tys: &mut *tys, out: &mut *out, indent, used_shims: &mut *used_shims })
            .and_then(|()| crate::native_closure_decl::check(call, user_fns, (&mut *out, &mut *used_shims), marks));
        return match rendered {
            // A linked self-hosted body that refuses the call's argument
            // kinds: the runtime body its declaration names serves it, if any.
            Err(e) => crate::native_hof::serve_or(e, call, user_fns, sigs, OpSink { tys, out, indent, used_shims }),
            ok => ok,
        };
    }
    if crate::native_hof::pruned(name) {
        return crate::native_hof::render_pruned(call, user_fns, sigs, OpSink { tys, out, indent, used_shims });
    }
    if name == "__str_concat" && render_concat_in_place(call, tys, out, indent) {
        Ok(())
    } else if let Some(shim_entry) = shim(name) {
        render_closed_shim_call(call, shim_entry, OpSink { tys, out, indent, used_shims })
    } else if let Some(floor) = crate::native_floor::declared(name) {
        render_declared_floor_call(call, floor, OpSink { tys, out, indent, used_shims })
    } else if crate::native_list_ops::is_list_concat(name) {
        render_list_concat(call, OpSink { tys, out, indent, used_shims })
    } else if let Some(floor) = protocol_instance_floor(&call, tys) {
        render_declared_floor_call(call, floor, OpSink { tys, out, indent, used_shims })
    } else {
        Err(wall(format!(
            "native: call to `{name}` — not a lowered user fn and not in the \
             native runtime floor"
        )))
    }
}

/// A user fn call (by declared sig table): each arg rendered in the callee's
/// declared param mode, then the result bound from the declared return.
/// Extracted verbatim from `render_call_fn` (codopsy r2, #852).
fn render_user_fn_call(
    call: NativeCall<'_>,
    callee: &MirFunction,
    sigs: &NativeSigs,
    s: OpSink<'_>,
) -> Result<(), LowerError> {
    let name = call.name;
    if call.args.len() != callee.params.len() {
        return Err(wall(format!("native: call to `{name}` arity mismatch")));
    }
    let callee_sig = sigs.get(name.as_str());
    let mut rendered_args = Vec::new();
    for (i, (a, p)) in call.args.iter().zip(&callee.params).enumerate() {
        let want = declared_param_want(callee_sig, i, p)?;
        let (code, got) = call_arg(a, s.tys)?;
        let arg = if callee.owned_params.contains(&p.value) {
            moved_arg(name, crate::render_native::owned_nty(want), code, got)?
        } else if matches!(want, NTy::Res | NTy::ResStr)
            && got == want
            && matches!(a, CallArg::Handle(v) if crate::render_native_move::moving_arg(*v))
        {
            // A by-value Result param given the caller's dying Result: moved,
            // where the copy would leave the caller one it never reads.
            crate::render_native_move::take_expr(&code)
        } else {
            user_arg_in_declared_mode(name, want, code, got)?
        };
        // A narrow param crosses in its real Rust type (#4006).
        rendered_args.push(match callee_sig.and_then(|(ps, _)| ps.get(i)).and_then(|k| k.narrow_rust()) {
            Some(t) => format!("({arg} as {t})"),
            None => arg,
        });
    }
    let head = crate::render_native_keyed::call_head(name, call.args, callee).unwrap_or_else(|| mangle(name));
    let mut rendered = format!("{head}({})", rendered_args.join(", "));
    if callee_sig.and_then(|(_, r)| *r).is_some_and(|k| k.narrow_rust().is_some()) {
        rendered = format!("({rendered} as i64)");
    }
    bind_user_fn_result(call, callee_sig, &rendered, s)
}

/// Decides the NTy a user-fn param WANTS its arg rendered as.
/// The DECLARED kind (sig table) disambiguates a heap param:
/// `&str` vs `&[i64]`; absent (a synthesized helper) the repr
/// fallback keeps the string convention.
/// Extracted verbatim from `render_call_fn` (codopsy r2, #852).
fn declared_param_want(
    callee_sig: Option<&(Vec<NativeSigKind>, Option<NativeSigKind>)>,
    i: usize,
    p: &crate::MirParam,
) -> Result<NTy, LowerError> {
    Ok(match callee_sig.and_then(|(ps, _)| ps.get(i)) {
        Some(NativeSigKind::I64 | NativeSigKind::Narrow { .. }) => NTy::I64,
        Some(NativeSigKind::Str) => NTy::StrRef,
        Some(NativeSigKind::ListI64) => NTy::VecRef,
        Some(NativeSigKind::ListStr) => NTy::VecStrRef,
        Some(NativeSigKind::Rec(i)) => NTy::RecRef(*i),
        Some(NativeSigKind::F64) => NTy::F64,
        Some(NativeSigKind::Opt) => NTy::Opt,
        Some(NativeSigKind::ResStr) => NTy::ResStr,
        Some(NativeSigKind::Res) => NTy::Res,
        None => repr_nty(&p.repr, true)?,
    })
}

/// Renders ONE user-fn call arg in the param's wanted mode: f64 through the
/// bits boundary, scalars must already be i64, an owned vec is borrowed for a
/// list param, and everything else follows the string convention — walling on
/// any kind mismatch. Extracted verbatim from `render_call_fn`
/// (codopsy r2, #852).
/// An argument handed to a MOVE param (#4006 class P): the caller's own value,
/// moved (its `Consume` follows the call — `MirFunction::call_moves`).
fn moved_arg(name: &str, want: NTy, code: String, got: NTy) -> Result<String, LowerError> {
    let same = crate::native_value_repr::same_owned(want, got);
    if !same || matches!(got, NTy::StrRef | NTy::VecRef | NTy::VecStrRef | NTy::RecRef(_)) {
        return Err(wall(format!("native: a borrowed or other-typed arg to a move param of `{name}`")));
    }
    Ok(code)
}

fn user_arg_in_declared_mode(
    name: &str,
    want: NTy,
    code: String,
    got: NTy,
) -> Result<String, LowerError> {
    match want {
        NTy::F64 => as_f64_arg(&code, got),
        NTy::I64 => {
            if got != NTy::I64 {
                return Err(wall(format!(
                    "native: heap arg to scalar param of `{name}`"
                )));
            }
            Ok(code)
        }
        NTy::VecStrRef | NTy::VecStr => match got {
            NTy::VecStr => Ok(format!("&{code}")),
            NTy::VecStrRef => Ok(code),
            _ => Err(wall(format!("native: non-string-list arg ({got:?}) to List[String] param of `{name}`"))),
        },
        NTy::RecRef(_) | NTy::Rec(_) => match got {
            g if crate::native_value_repr::same_owned(want, g) => {
                Ok(if matches!(g, NTy::RecRef(_) | NTy::VecRef | NTy::VecStrRef | NTy::StrRef) {
                    code
                } else { format!("&{code}") })
            }
            _ => crate::native_value_repr::tuple_copy(&code, got, want)
                .map(|value| format!("&({value})"))
                .ok_or_else(|| wall(format!("native: arg of another type ({got:?}) to a record param ({want:?}) of `{name}`"))),
        },
        NTy::ResStr | NTy::Res | NTy::Opt => carrier_arg_in_declared_mode(name, want, code, got),
        NTy::VecRef | NTy::Vec => {
            if !got.is_veccy() {
                return Err(wall(format!(
                    "native: non-list arg to list param of `{name}`"
                )));
            }
            Ok(match got {
                NTy::Vec => format!("&{code}"),
                _ => code,
            })
        }
        _ => {
            if !got.is_stringy() {
                return Err(wall(format!(
                    "native: scalar arg to heap param of `{name}`"
                )));
            }
            Ok(as_str_arg(&code, got))
        }
    }
}

/// The carrier arms of [`user_arg_in_declared_mode`] (`want` is `ResStr`,
/// `Res` or `Opt`): the arg must carry the same carrier.
fn carrier_arg_in_declared_mode(
    name: &str,
    want: NTy,
    code: String,
    got: NTy,
) -> Result<String, LowerError> {
    match want {
        NTy::ResStr => {
            if got != NTy::ResStr {
                return Err(wall(format!(
                    "native: non-Result arg to Result param of `{name}`"
                )));
            }
            Ok(format!("{code}.clone()"))
        }
        NTy::Res => {
            if got != NTy::Res {
                return Err(wall(format!(
                    "native: non-Result arg to Result param of `{name}`"
                )));
            }
            Ok(format!("{code}.clone()"))
        }
        // `NTy::Opt`, the one carrier left.
        _ => {
            if got != NTy::Opt {
                return Err(wall(format!(
                    "native: non-Option arg to Option param of `{name}`"
                )));
            }
            Ok(code)
        }
    }
}

/// Binds a user-fn call's result: the dst's NTy comes from the callee's
/// declared return (repr fallback otherwise), a dst-less call is a bare
/// statement, and an absent result repr defaults to scalar. Extracted verbatim
/// from `render_call_fn` (codopsy r2, #852).
fn bind_user_fn_result(
    call: NativeCall<'_>,
    callee_sig: Option<&(Vec<NativeSigKind>, Option<NativeSigKind>)>,
    rendered: &str,
    s: OpSink<'_>,
) -> Result<(), LowerError> {
    let NativeCall { dst, result, .. } = call;
    let OpSink { tys, out, indent, used_shims: _ } = s;
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    match (dst, result) {
        (Some(d), Some(r)) => {
            // A heap result is FRESH OWNED (the callee moved it out).
            // Its KIND comes from the callee's declared return.
            let t = match callee_sig.and_then(|(_, r)| *r) {
                Some(NativeSigKind::ListI64) => NTy::Vec,
                Some(NativeSigKind::ListStr) => NTy::VecStr,
                Some(NativeSigKind::Rec(i)) => NTy::Rec(i),
                Some(NativeSigKind::Str) => NTy::Str,
                Some(NativeSigKind::I64 | NativeSigKind::Narrow { .. }) => NTy::I64,
                Some(NativeSigKind::F64) => NTy::F64,
                Some(NativeSigKind::Res) => NTy::Res,
                Some(NativeSigKind::Opt) => NTy::Opt,
                Some(NativeSigKind::ResStr) => NTy::ResStr,
                None => repr_nty(r, false)?,
            };
            tys.insert(*d, t);
            let ty_name = crate::render_native_rec::owned_ty(t);
            line!("let mut {}: {} = {};", var(*d), ty_name, rendered);
        }
        (None, _) => line!("{rendered};"),
        (Some(d), None) => {
            // Result repr unknown: scalar by convention.
            tys.insert(*d, NTy::I64);
            line!("let mut {}: i64 = {};", var(*d), rendered);
        }
    }
    Ok(())
}

/// A CLOSED runtime shim call (the native runtime floor): the shim's fixed
/// param list gates each arg, its source is pulled into `used_shims`, and the
/// result binds by the shim's declared return. Extracted verbatim from
/// `render_call_fn` (codopsy r2, #852).
/// The list `+` operator: a fresh list of `a`'s elements then `b`'s.
/// An OWNED list operand of `+`: a `Vec` the render may grow or move from.
fn owned_list(t: NTy) -> bool {
    match t {
        NTy::Vec | NTy::VecStr => true,
        NTy::Rec(i) => crate::render_native_rec::list_elem(i).is_some(),
        _ => false,
    }
}

fn render_list_concat(call: NativeCall<'_>, s: OpSink<'_>) -> Result<(), LowerError> {
    let NativeCall { dst, name, args, .. } = call;
    let OpSink { tys, out, indent, .. } = s;
    let [a, b] = args.as_slice() else {
        return Err(wall(format!("native: `{name}` arity mismatch")));
    };
    let (a, at) = call_arg(a, tys)?;
    let (b, bt) = call_arg(b, tys)?;
    let mismatch = || wall(format!("native: `{name}` over operands outside the native lists ({at:?}, {bt:?})"));
    let (sa, ty) = crate::native_list_ops::as_slice(&a, at).ok_or_else(mismatch)?;
    let (sb, _) = crate::native_list_ops::as_slice(&b, bt).ok_or_else(mismatch)?;
    if !crate::native_value_repr::same_owned(at, bt) {
        return Err(mismatch());
    }
    let Some(d) = dst else {
        return Err(wall(format!("native: `{name}` without a result")));
    };
    tys.insert(*d, crate::native_list_ops::owned(at));
    for _ in 0..indent {
        out.push_str("    ");
    }
    // The left list's last use (#4006 class P): grow its own buffer. The
    // right side must be another value that cannot view the left one's
    // buffer: an owned list of its own (`xs + xs` reads the left list as the
    // right one). The right list's last use, when its elements own heap
    // (a `List[String]`, a list of lists — #4503's prepend `[x] + xss`):
    // move its elements over instead of cloning each one.
    // Or the left list is a borrowed field or payload read whose carrier
    // dies at the concatenation (`sc = { ...sc, funs: sc.funs + [..] }`,
    // `set_moving_args`' taken args) and the right one an owned list of its
    // own: grow that field's list itself, moved out of the carrier.
    if let [CallArg::Handle(l), CallArg::Handle(r)] = args.as_slice() {
        let taken = crate::render_native_move::taken_arg(*l);
        if l != r && (owned_list(at) || taken.is_some()) && owned_list(bt) {
            let left = crate::render_native_move::moving_arg(*l);
            let right = crate::render_native_move::moving_arg(*r) && bt != NTy::Vec;
            if left || right {
                let head = match (left, taken) {
                    (true, _) => format!("let mut r: {ty} = {}", crate::render_native_move::take_expr(&a)),
                    (false, Some(take)) => format!("let mut r: {ty} = {take}"),
                    (false, None) => format!("let mut r: {ty} = Vec::with_capacity({sa}.len() + {b}.len()); r.extend_from_slice({sa})"),
                };
                let tail = if right {
                    format!("r.extend({})", crate::render_native_move::take_expr(&b))
                } else if bt == NTy::Vec && crate::render_native_move::is_stack_single(*r) {
                    // `xs + [x]` on a dying `xs`: a push, not a slice copy.
                    format!("r.push({b}[0])")
                } else {
                    format!("r.extend_from_slice({sb})")
                };
                writeln!(out, "let mut {}: {ty} = {{ {head}; {tail}; r }};", var(*d))
                    .expect("writing to a String cannot fail");
                return Ok(());
            }
        }
    }
    writeln!(
        out,
        "let mut {}: {ty} = {{ let mut r: {ty} = Vec::with_capacity({sa}.len() + {sb}.len()); r.extend_from_slice({sa}); r.extend_from_slice({sb}); r }};",
        var(*d)
    )
    .expect("writing to a String cannot fail");
    Ok(())
}

/// A stdlib call its declaration puts on the native floor (`@intrinsic`, a
/// per-type signature, a runtime body the header can carry): the runtime
/// body is rendered beside the program and called as v0 calls it.
/// The declared floor entry of the conformer a protocol-generic stdlib call
/// answers with, given the kinds its args carry (`list.join` over strings).
fn protocol_instance_floor(call: &NativeCall<'_>, tys: &BTreeMap<ValueId, NTy>) -> Option<&'static crate::native_floor::FloorFn> {
    let kinds: Vec<NTy> = call.args.iter().map(|a| call_arg(a, tys).map(|(_, k)| k)).collect::<Result<_, _>>().ok()?;
    crate::native_floor::declared(&crate::native_floor::protocol_instance(call.name, &kinds)?)
}

fn render_declared_floor_call(
    call: NativeCall<'_>,
    floor: &'static crate::native_floor::FloorFn,
    s: OpSink<'_>,
) -> Result<(), LowerError> {
    let NativeCall { dst, name, args, .. } = call;
    let OpSink { tys, out, indent, used_shims } = s;
    // The body only reads the Values it takes by value: lend them instead.
    let floor = floor.by_ref.as_deref().unwrap_or(floor);
    let mut rendered: Vec<(String, NTy)> = args.iter().map(|a| call_arg(a, tys)).collect::<Result<_, _>>()?;
    let mut dying: Vec<bool> =
        args.iter().map(|a| matches!(a, CallArg::Handle(v) if crate::render_native_move::moving_arg(*v))).collect();
    // A borrowed read whose carrier dies at the call: the call takes the
    // payload, moved out, where its param (or its `@reuse` twin's) is by value.
    take_dying_payloads(args, floor, &mut rendered, &mut dying);
    // A `mut b: Bytes` fn called for its result (the move-mode rewrite's
    // `c = bytes.append_u16_le(c, x)`): it edits the arg itself, which dies
    // here, and that buffer is the result.
    let edited = match (dst, floor.ret) {
        (Some(_), crate::native_floor::FloorRet::Unit) => {
            floor.params.iter().position(|p| *p == crate::native_floor::FloorParam::BytesMut).and_then(|k| {
                let (code, got) = rendered.get(k)?.clone();
                // Only the arg itself, when it dies here: a borrowed or still-live
                // buffer would be a copy v0 never makes, so that call walls.
                let own = NTy::Rec(got.rec()?);
                (got == own && dying[k]).then_some(())?;
                let src = crate::render_native_move::take_expr(&code);
                rendered[k] = ("__edited".to_string(), own);
                dying[k] = false;
                Some((src, own))
            })
        }
        _ => None,
    };
    let dst_map = dst.and_then(crate::render_native_rec::declared_map);
    let (expr, ret) = crate::native_floor::render_call(floor, name, &rendered, &dying, dst_map)?;
    // A result read only through borrows, from a Value the function borrows:
    // the runtime's borrowing twin, no copy of the field (v0's decode path).
    let by_ref = match (dst, &floor.borrow_twin) {
        (Some(d), Some((twin, items))) if crate::render_native_borrow_read::borrowed_result(*d) && edited.is_none() => {
            Some((twin.as_str(), items))
        }
        _ => None,
    };
    let expr = match by_ref {
        Some((twin, _)) => expr.replacen(&format!("{}(", floor.symbol), &format!("{twin}("), 1),
        None => expr,
    };
    let (expr, ret) = match edited {
        Some((src, own)) => (
            format!("{{ let mut __edited: {t} = {src}; {expr}; __edited }}", t = crate::render_native_rec::owned_ty(own)),
            Some(own),
        ),
        None => (expr, ret),
    };
    used_shims.push(crate::native_floor::FLOOR_HEADER);
    match (&floor.twin, by_ref) {
        (_, Some((_, items))) => used_shims.extend(items.iter().copied()),
        (Some((_, twin, items)), None) if expr.contains(&format!("{twin}(")) => used_shims.extend(items.iter().copied()),
        _ => used_shims.extend(floor.items.iter().copied()),
    }
    for _ in 0..indent {
        out.push_str("    ");
    }
    match (dst, ret) {
        (Some(d), Some(t)) => {
            tys.insert(*d, t);
            let ty_name = match by_ref {
                Some(_) => format!(
                    "Result<&{}, AlmideString>",
                    crate::render_native_rec::owned_ty(NTy::Rec(crate::render_native_rec::value_index()))
                ),
                None => crate::render_native_rec::owned_ty(t),
            };
            writeln!(out, "let mut {}: {} = {};", var(*d), ty_name, expr).expect("writing to a String cannot fail");
        }
        (None, _) => writeln!(out, "{expr};").expect("writing to a String cannot fail"),
        (Some(_), None) => return Err(wall(format!("native: floor call `{name}` has no result"))),
    }
    Ok(())
}

/// `a + b` over strings where one side is an owned string that dies at the
/// concatenation: the result reuses its buffer — `b.prepend(a)` /
/// `a.concat(b)`, AlmideStr's in-place edits when the buffer is unique — the
/// expression v0's `AlmideConcat` renders, where the shim allocates a third
/// string. `false` leaves the call to the shim.
fn render_concat_in_place(call: NativeCall<'_>, tys: &mut BTreeMap<ValueId, NTy>, out: &mut String, indent: usize) -> bool {
    let (Some(d), [CallArg::Handle(a), CallArg::Handle(b)]) = (call.dst, call.args.as_slice()) else { return false };
    if a == b {
        return false;
    }
    let (Some(ta), Some(tb)) = (tys.get(a).copied(), tys.get(b).copied()) else { return false };
    let dying = |v: &ValueId, t: NTy| t == NTy::Str && crate::render_native_move::moving_arg(*v);
    // Edit the side whose buffer can be edited in place: an owned, dying
    // string that is not a literal (a literal is an immortal static block,
    // copied before any edit), the left one first as v0's `AlmideConcat`
    // appends into an owned left and prepends only onto a borrowed one.
    let editable = |v: &ValueId, t: NTy| dying(v, t) && !crate::render_native_move::is_static_str(*v);
    let operand = |v: &ValueId, t: NTy| crate::render_native_move::static_str_operand(*v).unwrap_or_else(|| as_str_arg(&var(*v), t));
    let left = |a: &ValueId, b: &ValueId, tb: NTy| format!("{}.concat({})", crate::render_native_move::take_expr(&var(*a)), operand(b, tb));
    let right = |a: &ValueId, b: &ValueId, ta: NTy| format!("{}.prepend({})", crate::render_native_move::take_expr(&var(*b)), operand(a, ta));
    let expr = if editable(a, ta) && tb.is_stringy() {
        left(a, b, tb)
    } else if editable(b, tb) && ta.is_stringy() {
        right(a, b, ta)
    } else if dying(a, ta) && tb.is_stringy() {
        left(a, b, tb)
    } else if dying(b, tb) && ta.is_stringy() {
        right(a, b, ta)
    } else {
        return false;
    };
    for _ in 0..indent {
        out.push_str("    ");
    }
    writeln!(out, "let mut {}: AlmideString = {expr};", var(*d)).expect("writing to a String cannot fail");
    tys.insert(*d, NTy::Str);
    true
}

fn render_closed_shim_call(
    call: NativeCall<'_>,
    shim_entry: (&'static [NTy], Option<NTy>, &'static str),
    s: OpSink<'_>,
) -> Result<(), LowerError> {
    let NativeCall { dst, name, args, .. } = call;
    let (param_tys, ret_ty, shim_src) = shim_entry;
    let OpSink { tys, out, indent, used_shims } = s;
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    if args.len() != param_tys.len() {
        return Err(wall(format!("native: shim `{name}` arity mismatch")));
    }
    let mut rendered_args = Vec::new();
    for (a, want) in args.iter().zip(param_tys) {
        let (code, got) = call_arg(a, tys)?;
        rendered_args.push(shim_arg_in_declared_mode(name, *want, code, got)?);
    }
    used_shims.push(shim_src);
    let call = format!("{}({})", shim_rust_name(name), rendered_args.join(", "));
    match (dst, ret_ty) {
        (Some(d), Some(t)) => {
            tys.insert(*d, t);
            let ty_name = match t {
                NTy::Str => "AlmideString",
                NTy::Vec => "Vec<i64>",
                NTy::VecStr => "Vec<AlmideString>",
                _ => "i64",
            };
            line!("let mut {}: {} = {};", var(*d), ty_name, call);
        }
        (None, _) => line!("{call};"),
        (Some(_), None) => {
            return Err(wall(format!("native: shim `{name}` has no result")))
        }
    }
    Ok(())
}

/// Renders ONE shim call arg against the shim's declared NTy: f64 through the
/// bits boundary, i64 must match exactly, heap args by the string convention —
/// walling on any mismatch. Extracted verbatim from `render_call_fn`
/// (codopsy r2, #852).
fn shim_arg_in_declared_mode(
    name: &str,
    want: NTy,
    code: String,
    got: NTy,
) -> Result<String, LowerError> {
    match want {
        NTy::F64 => as_f64_arg(&code, got),
        NTy::I64 => {
            if got != NTy::I64 {
                return Err(wall(format!("native: shim `{name}` arg type mismatch")));
            }
            Ok(code)
        }
        // A scalar-list param (#1869): borrowed at the MIR call boundary —
        // an owned `Vec<i64>` local by reference, a `&[i64]` param as is.
        NTy::VecRef | NTy::Vec => match got {
            NTy::Vec => Ok(format!("&{code}")),
            NTy::VecRef => Ok(code),
            _ => Err(wall(format!("native: shim `{name}` arg type mismatch"))),
        },
        NTy::VecStrRef | NTy::VecStr => match got {
            NTy::VecStr => Ok(format!("&{code}")),
            NTy::VecStrRef => Ok(code),
            _ => Err(wall(format!("native: shim `{name}` arg type mismatch"))),
        },
        _ => {
            if !got.is_stringy() {
                return Err(wall(format!("native: shim `{name}` arg type mismatch")));
            }
            // Heap args are BORROWED at the MIR level — by reference.
            Ok(as_str_arg(&code, got))
        }
    }
}
