//! Call dispatch: the hub where `Call` nodes are routed.
//!
//! Taxonomy (from the IR `CallTarget` plus empirical IR inspection):
//!   - `Named`:   builtins (println/print/assert_eq/…), variant constructors,
//!                or user/stdlib free functions.
//!   - `Module`:  `(module, func)` — three outcomes:
//!                  (i)   an in-interp HOF (closure-taking combinator),
//!                  (ii)  a scalar/string native bridge fn, or
//!                  (iii) an almide-bodied stdlib fn lowered into the program.
//!   - `Method`:  residual UFCS — evaluate object, dispatch as `(module,func)`.
//!   - `Computed`: evaluate callee to a `Closure`, apply.

use std::rc::Rc;

use almide_base::intern::Sym;
use almide_ir::{CallTarget, IrExpr, IrExprKind};
use almide_lang::types::Ty;

use crate::env::Scope;
use crate::value::{Closure, Value, VariantPayload};
use crate::{Flow, Interpreter};

macro_rules! val {
    ($flow:expr) => {
        match $flow {
            Flow::Value(v) => v,
            other => return other,
        }
    };
}

/// Like `val!`, but for helpers that return `Option<Flow>` (e.g. a
/// name-router group fn) instead of a bare `Flow` — wraps the early-exit in
/// `Some` so the caller's `if let Some(flow) = helper(..) { return flow; }`
/// still sees the original non-Value `Flow` unchanged.
macro_rules! val_opt {
    ($flow:expr) => {
        match $flow {
            Flow::Value(v) => v,
            other => return Some(other),
        }
    };
}

impl<'a> Interpreter<'a> {
    pub(crate) fn eval_call(
        &mut self,
        target: &CallTarget,
        args: &[IrExpr],
        scope: &Scope,
    ) -> Flow {
        match target {
            CallTarget::Named { name } => self.eval_named_call(*name, args, scope),
            CallTarget::Module { module, func, .. } => {
                self.eval_module_call(*module, *func, args, scope)
            }
            CallTarget::Method { object, method } => {
                // Residual UFCS: evaluate the receiver, prepend as first arg,
                // and dispatch as a module call inferred from the receiver
                // kind. Post-lower this is rare; treat the method name as the
                // func and the receiver's kind as the module.
                let recv = val!(self.eval_expr(object, scope));
                let module = infer_module_for(&recv);
                let mut evaled = vec![recv];
                for a in args {
                    evaled.push(val!(self.eval_expr(a, scope)));
                }
                self.dispatch_module_resolved(module, *method, evaled)
            }
            CallTarget::Computed { callee } => {
                let f = val!(self.eval_expr(callee, scope));
                let mut evaled = Vec::with_capacity(args.len());
                for a in args {
                    evaled.push(val!(self.eval_expr(a, scope)));
                }
                match f {
                    Value::Closure(clo) => self.apply_closure(&clo, evaled),
                    other => Flow::Abort(format!(
                        "internal: call of non-closure {}",
                        other.type_name()
                    )),
                }
            }
        }
    }

    // ── Named calls ─────────────────────────────────────────────

    fn eval_named_call(&mut self, name: Sym, args: &[IrExpr], scope: &Scope) -> Flow {
        let n = name.as_str();

        // 1. Builtins.
        if let Some(flow) = self.eval_builtin_call(n, args, scope) {
            return flow;
        }

        // 2. Constructors and the executing module's own sibling — the pure
        //    lookups between the builtins and the flat fn table.
        if let Some(flow) = self.eval_named_ctor_or_sibling(name, args, scope) {
            return flow;
        }

        // 3. A user / stdlib free function lowered into the program. A stdlib
        //    IMPL name (`string_slice` — how a lowered MODULE body spells
        //    `string.slice`) first tries the SAME native bridge a
        //    module-spelled call takes, so both spellings share one resolution
        //    order; the lowered body stays the fallback. Mut-param impls skip
        //    the shortcut (the bridge has no write-back path, #1022).
        if let Some(func) = self.fns.get(&name).copied() {
            if let Some((m, f)) = crate::stdlib_pool::module_of_impl(name) {
                // The in-interp HOFs take closure ARGUMENTS and must see the
                // arg EXPRS — same tier order as `eval_module_call`.
                if is_hof(m.as_str(), f.as_str()) {
                    return self.eval_hof(m, f, args, scope);
                }
                if !func.params.iter().any(|p| p.is_mut) {
                    let mut evaled = Vec::with_capacity(args.len());
                    for a in args {
                        evaled.push(val!(self.eval_expr(a, scope)));
                    }
                    if let Some(result) = self.eval_container_op(m.as_str(), f.as_str(), &evaled)
                    {
                        return result;
                    }
                    if let Some(result) = crate::bridge::dispatch(m.as_str(), f.as_str(), &evaled)
                    {
                        return result;
                    }
                    let root = self.root_scope();
                    let flow = self.call_pool_tier(func, evaled, &root);
                    // #1226 return sync at the NAMED spelling too — the same
                    // body reachable both ways must read back the same way.
                    // Pool bodies only: a program fn that happens to share an
                    // impl name keeps the fixture tier's raw address model.
                    return if self.pool_fns.contains(&func.name) {
                        self.sync_at_pool_boundary(func, flow)
                    } else {
                        flow
                    };
                }
            }
            return self.eval_lowered_fn_call(func, args, scope);
        }

        // 4. A bare Named target inside a LOWERED MODULE body calling a module
        //    sibling (args' `option_or` → `option`). Runs only on a flat-table
        //    MISS (program fns stay authoritative) and only when exactly ONE
        //    loaded module defines the name — an ambiguous name abstains
        //    honestly rather than resolving from the wrong source (#1087).
        //    A convention method on a MODULE type (`Pt.repr`, #1836) is
        //    registered under the module-qualified spelling (`m.Pt.repr`)
        //    while the call site carries only `Type.method` (the subject
        //    type has no module) — the same unique-suffix resolution the
        //    wasm dispatcher applies to a cross-module Codec method.
        {
            let qualified_suffix = |f: &str| {
                n.contains('.') && f.strip_suffix(n).is_some_and(|p| p.ends_with('.'))
            };
            let mut hits = self
                .module_fns
                .iter()
                .filter(|((_, f), _)| *f == name || qualified_suffix(f.as_str()))
                .map(|(_, func)| *func);
            if let (Some(func), None) = (hits.next(), hits.next()) {
                return self.eval_lowered_fn_call(func, args, scope);
            }
        }

        Flow::Unsupported(format!("named call `{}`", n))
    }

    /// Step 2 of [`Self::eval_named_call`]: the pure lookups between the
    /// builtins and the flat fn table. `None` = none of them claims `name`.
    fn eval_named_ctor_or_sibling(&mut self, name: Sym, args: &[IrExpr], scope: &Scope) -> Option<Flow> {
        let n = name.as_str();
        // 2a. Variant constructor (Unit / Tuple). Record-variant ctors arrive
        //     as `Record` nodes, handled in eval. Look up in the registry.
        if let Some((ty_name, kind)) = self.variant_ctor(name) {
            return Some(self.eval_variant_ctor_call(ty_name, name, kind, args, scope));
        }
        // 2b. The stdlib's only BUNDLED variant type (bytes.Endian): its decl
        //     lives in the bundled module, never in the program, so the ctor
        //     registry above misses it. The checker already typed the ctor —
        //     build the variant value directly (the same value the inplace
        //     tier's `endian_is_big` and the bytes read bridge dispatch on).
        if args.is_empty() && matches!(n, "LittleEndian" | "BigEndian") {
            return Some(Flow::val(Value::Variant {
                ty: None,
                ctor: name,
                payload: VariantPayload::Unit,
            }));
        }
        // 2c. An opaque NEWTYPE's constructor (`SafeHtml(s)`, `Value(s)` under
        //     its `self.Value` identity, #1835): both backends erase the
        //     wrapper — the value IS its payload — so the call is an identity
        //     on its one argument. Registered by name from the program's
        //     Alias decls; an arity other than one is not a newtype call.
        if args.len() == 1 && self.newtype_ctors.contains(&name) {
            return Some(self.eval_expr(&args[0], scope));
        }
        // 2d. Inside a LOWERED MODULE body, a bare sibling call resolves
        //     against the executing module FIRST (#1844) — the scope the
        //     checker bound it in — before the flat table and before the
        //     unique-definer rule of step 4: `to_string(a)` inside
        //     `html.concat` is html's own, however many loaded modules (or
        //     the program) spell a `to_string`. The program root (space 0)
        //     has no owner and keeps the flat-table order below.
        let func = self.own_module_sibling(name)?;
        Some(self.eval_lowered_fn_call(func, args, scope))
    }

    /// Step 2d of [`Self::eval_named_ctor_or_sibling`]: the executing
    /// module's own definition of `name`, when a module body is executing
    /// (`cur_space`) and its module defines the name.
    fn own_module_sibling(&self, name: Sym) -> Option<&'a almide_ir::IrFunction> {
        let space = self.cur_space.get();
        let owner = self.program.modules.get(space.checked_sub(1)? as usize)?.name;
        self.module_fns.get(&(owner, name)).copied()
    }

    /// Step 2a of [`Self::eval_named_ctor_or_sibling`]: build the variant
    /// value a Unit- or Tuple-payload constructor names.
    fn eval_variant_ctor_call(
        &mut self,
        ty_name: Sym,
        ctor: Sym,
        kind: CtorKind,
        args: &[IrExpr],
        scope: &Scope,
    ) -> Flow {
        let payload = match kind {
            CtorKind::Unit => VariantPayload::Unit,
            CtorKind::Tuple => {
                let mut evaled = Vec::with_capacity(args.len());
                for a in args {
                    evaled.push(val!(self.eval_expr(a, scope)));
                }
                VariantPayload::Tuple(evaled)
            }
            CtorKind::Record => {
                // Should not arrive as a Named call, but handle defensively.
                return Flow::Unsupported(format!("record-variant ctor call {}", ctor));
            }
        };
        Flow::val(Value::Variant { ty: Some(ty_name), ctor, payload })
    }

    /// Step 3 of [`Self::eval_named_call`]: call a lowered Almide function.
    ///
    /// #1022: mut-parameter copy-in/copy-out. The backends' lowering returns
    /// each `mut` param's final buffer and writes it back at EVERY call
    /// position (C-132) — the interp mirrors that by keeping the callee frame
    /// alive and assigning each recorded caller lvalue from the param's final
    /// value. Recorded BEFORE evaluation, while the argument is still an
    /// expression with a binding identity.
    fn eval_lowered_fn_call(
        &mut self,
        func: &'a almide_ir::IrFunction,
        args: &[IrExpr],
        scope: &Scope,
    ) -> Flow {
        let writebacks = match self.mut_param_lvalues(func, args) {
            Ok(wb) => wb,
            Err(flow) => return flow,
        };
        let mut evaled = Vec::with_capacity(args.len());
        for a in args {
            evaled.push(val!(self.eval_expr(a, scope)));
        }
        let root = self.root_scope();
        let is_pool = self.pool_fns.contains(&func.name);
        // Depth is owned by `run_callable`'s per-hop bump — see
        // `call_pool_tier`.
        let (flow, frame) = self.call_function_keeping_frame(func, evaled, &root);
        // Copy-out only on a normal return — an abort/abstain never
        // half-writes state the backends would not have written either.
        if !matches!(flow, Flow::Value(_)) {
            return flow;
        }
        for (idx, lv) in writebacks {
            let Some(final_v) = frame.get(func.params[idx].var) else { continue };
            if let Err(e) = self.write_mut_lvalue(lv, final_v, scope) {
                return e;
            }
        }
        // #1226 return sync when a POOL body is called by its NAMED spelling
        // from fixture-tier code. A fixture's OWN fn never syncs — `import
        // prim` fixtures are written against the raw address model — and a
        // pool-internal call never syncs (the tier is address-uniform; see
        // `pool_fns`).
        if is_pool {
            self.sync_at_pool_boundary(func, flow)
        } else {
            flow
        }
    }

    /// The caller-side lvalues of a call's `mut`-parameter arguments — the
    /// slots the copy-out writes after the call (#1022).
    ///
    /// A plain `Var` and the one-level record field (`push9(b.items, 7)`) are
    /// the two shapes the backends' fixtures pin. Any OTHER lvalue shape (an
    /// index, a nested field) abstains by name: the backends write those back,
    /// and silently dropping the effect would be a wrong third vote. A
    /// non-lvalue argument cannot reach a checked program (E032 rejects a
    /// temporary to a `mut` param); the fall-through skip is defensive only.
    fn mut_param_lvalues(
        &self,
        func: &almide_ir::IrFunction,
        args: &[IrExpr],
    ) -> Result<Vec<(usize, MutLvalue)>, Flow> {
        let mut out = Vec::new();
        for (i, (param, arg)) in func.params.iter().zip(args.iter()).enumerate() {
            // TWO ways a parameter copies out, and the second is why #1436
            // existed. BY DECLARATION: a `mut` param, the C-132 lowering.
            // BY TYPE: a `Bytes` param. The byte-writer family
            // (`set_*`/`append_*`/`write_*`, and `fill`/`copy_from`/
            // `copy_within`) is `@intrinsic`s whose mutation lives in the
            // native `&mut Vec<u8>` signature — invisible to the `.almd`
            // declaration, so a user fn taking a PLAIN `Bytes` param and
            // writing into it mutates the CALLER's buffer on both backends
            // while carrying no `mut` marker for this gate to see. The interp
            // wrote back into the callee's own frame, the effect died at the
            // frame boundary, and the third judge voted the UNMODIFIED buffer
            // — a wrong vote where the module doc demands a skip. Copying a
            // Bytes param out unconditionally is sound in the other direction
            // too: a callee that never writes hands back the value it was
            // given, so the copy-out is a no-op.
            let by_decl = param.is_mut;
            let by_type = matches!(param.ty, almide_lang::types::Ty::Bytes);
            if !by_decl && !by_type {
                continue;
            }
            let kind = if by_decl { "mut-parameter" } else { "bytes-parameter" };
            match &arg.kind {
                almide_ir::IrExprKind::Var { id } => out.push((i, MutLvalue::Var(*id))),
                almide_ir::IrExprKind::Member { object, field } => match &object.kind {
                    almide_ir::IrExprKind::Var { id } => {
                        out.push((i, MutLvalue::Field(*id, *field)))
                    }
                    _ => {
                        return Err(Flow::Unsupported(format!(
                            "{kind} argument through a nested lvalue \
                             (`{}` param {i}) — only a Var or one-level record \
                             field copies out (#1022)",
                            func.name.as_str()
                        )))
                    }
                },
                almide_ir::IrExprKind::IndexAccess { .. }
                | almide_ir::IrExprKind::MapAccess { .. }
                | almide_ir::IrExprKind::TupleIndex { .. } => {
                    return Err(Flow::Unsupported(format!(
                        "{kind} argument through an index lvalue \
                         (`{}` param {i}) — not yet copied out (#1022)",
                        func.name.as_str()
                    )))
                }
                // A TEMPORARY argument. For a `mut` param this is unreachable
                // (E032 forbids it); for a by-type `Bytes` param it is legal
                // (`fill(bytes.new(4))`) and there is simply no caller slot to
                // copy back into — nothing downstream can observe the buffer,
                // so skipping is exact rather than a guess.
                _ => {}
            }
        }
        Ok(out)
    }

    /// Assign a copy-out value into a caller lvalue. The field arm is the same
    /// clone-record-and-set shape `exec_stmt_field_assign` uses, so the two
    /// cannot diverge on COW semantics.
    fn write_mut_lvalue(&mut self, lv: MutLvalue, v: Value, scope: &Scope) -> Result<(), Flow> {
        match lv {
            MutLvalue::Var(id) => {
                if scope.assign(id, v) {
                    Ok(())
                } else {
                    Err(Flow::Abort("internal: mut-param copy-out to an unbound var".into()))
                }
            }
            MutLvalue::Field(id, field) => {
                let cur = scope.get(id).ok_or_else(|| {
                    Flow::Abort("internal: mut-param copy-out to an unbound record".into())
                })?;
                match cur {
                    Value::Record { name, fields } => {
                        let mut new = (*fields).clone();
                        if let Some(slot) = new.iter_mut().find(|(k, _)| *k == field) {
                            slot.1 = v;
                        } else {
                            new.push((field, v));
                        }
                        scope.assign(id, Value::Record { name, fields: std::rc::Rc::new(new) });
                        Ok(())
                    }
                    _ => Err(Flow::Abort("internal: mut-param copy-out on non-Record".into())),
                }
            }
        }
    }

    /// `eval_named_call`'s builtins group (println/print/eprintln/eprint/
    /// assert/assert_eq/assert_ne/panic). `None` means `n` is not a builtin —
    /// the caller falls through to variant-ctor / user-fn dispatch.
    pub(crate) fn eval_builtin_call(&mut self, n: &str, args: &[IrExpr], scope: &Scope) -> Option<Flow> {
        match n {
            "println" | "print" | "eprintln" | "eprint" => self.eval_builtin_print(n, args, scope),
            _ => self.eval_builtin_assert(n, args, scope),
        }
    }

    fn eval_builtin_print(&mut self, n: &str, args: &[IrExpr], scope: &Scope) -> Option<Flow> {
        match n {
            "println" | "print" => {
                let mut evaled = Vec::with_capacity(args.len());
                for a in args {
                    evaled.push(val_opt!(self.eval_expr(a, scope)));
                }
                let line = match evaled.first() {
                    Some(v) => v.display_bare(),
                    None => String::new(),
                };
                self.stdout.push_str(&line);
                if n == "println" {
                    self.stdout.push('\n');
                }
                Some(Flow::val(Value::Unit))
            }
            "eprintln" | "eprint" => {
                let mut evaled = Vec::with_capacity(args.len());
                for a in args {
                    evaled.push(val_opt!(self.eval_expr(a, scope)));
                }
                let line = evaled.first().map(|v| v.display_bare()).unwrap_or_default();
                self.stderr.push_str(&line);
                if n == "eprintln" {
                    self.stderr.push('\n');
                }
                Some(Flow::val(Value::Unit))
            }
            _ => None,
        }
    }

    fn eval_builtin_assert(&mut self, n: &str, args: &[IrExpr], scope: &Scope) -> Option<Flow> {
        match n {
            "assert" => {
                let v = val_opt!(self.eval_expr(&args[0], scope));
                Some(match v {
                    Value::Bool(true) => Flow::val(Value::Unit),
                    Value::Bool(false) => Flow::Abort("assertion failed".into()),
                    other => Flow::Abort(format!(
                        "internal: assert on {}",
                        other.type_name()
                    )),
                })
            }
            "assert_eq" | "assert_ne" => {
                let a = val_opt!(self.eval_expr(&args[0], scope));
                let b = val_opt!(self.eval_expr(&args[1], scope));
                let eq = a == b;
                let ok = if n == "assert_eq" { eq } else { !eq };
                Some(if ok {
                    Flow::val(Value::Unit)
                } else {
                    // Mirror the native assert macro's panic message shape.
                    Flow::Abort(format!(
                        "assertion failed: {} {} {}",
                        a.almide_repr(),
                        if n == "assert_eq" { "==" } else { "!=" },
                        b.almide_repr()
                    ))
                })
            }
            "panic" => {
                let msg = match args.first() {
                    Some(a) => val_opt!(self.eval_expr(a, scope)).display_bare(),
                    None => "explicit panic".to_string(),
                };
                Some(Flow::Abort(msg))
            }
            _ => None,
        }
    }

    // ── Module calls ────────────────────────────────────────────

    fn eval_module_call(
        &mut self,
        module: Sym,
        func: Sym,
        args: &[IrExpr],
        scope: &Scope,
    ) -> Flow {
        // First: is this an in-interp HOF? Those take closure ARGUMENTS and
        // must be evaluated specially (an interp closure cannot become the
        // `Rc<dyn Fn>` a generic runtime HOF demands).
        if crate::dispatch::is_hof(module.as_str(), func.as_str()) {
            return self.eval_hof(module, func, args, scope);
        }

        // Second: is this an in-place `mut`-receiver mutator? Those must also
        // be intercepted BEFORE the eager arg evaluation below — once the
        // receiver is a value, the binding it came from is unrecoverable and
        // the mutation has nowhere to land. See `inplace.rs`.
        if crate::hofs::is_inplace_mutating_op(module.as_str(), func.as_str()) {
            return self.eval_inplace_mutation(module, func, args, scope);
        }

        // Third: `fan.any`, evaluated at the DETERMINISTIC contract's spec
        // point rather than with threads: first-Ok in LIST ORDER with
        // side-effect order pinned sequential (C-185, C-004's own comment:
        // "any (sequential in list order)"), so it must NOT ride the eager
        // loop below — an arm after the winner is never evaluated. All-fail
        // is the defined Err (C-005). `fan.settle`'s block form never gets
        // here: it lowers to `IrExprKind::Fan`, which `eval_fan` already
        // evaluates (tuple of per-arm Results, the no-1-tuple rule).
        if module.as_str() == "fan" && func.as_str() == "any" {
            return self.eval_fan_any(args, scope);
        }

        // Otherwise evaluate all args eagerly, then dispatch.
        let mut evaled = Vec::with_capacity(args.len());
        for a in args {
            evaled.push(val!(self.eval_expr(a, scope)));
        }
        // `prim.handle`'s STATIC argument type — the only honest way to tell
        // a Bytes value from a List[Int] value at materialization (see
        // `handle_arg_ty`). Stashed, not threaded: the hint is consumed by
        // `heap_prim_handle` before anything can re-enter dispatch.
        if module.as_str() == "prim" && func.as_str() == "handle" {
            self.handle_arg_ty = args.first().map(|a| a.ty.clone());
        }
        self.dispatch_module_resolved(module, func, evaled)
    }

    /// `fan.any`, first-Ok short-circuit — see the dispatch comment above for
    /// the contract pins. The block form arrives as ONE argument: the literal
    /// LIST of 0-ary thunk closures the parser synthesized. Each thunk is
    /// CALLED in list order; a thunk returns its raw Result when the arm is an
    /// effect call (the interp's effect convention) and a bare value when pure
    /// — the pure case takes the Ok adapter here, mirroring what FanLowering
    /// bakes into both backends (#514).
    fn eval_fan_any(&mut self, args: &[IrExpr], scope: &Scope) -> Flow {
        let [thunks_expr] = args else {
            // The mapper form (list + fn) has its own runtime name upstream
            // (`any_map`); anything else reaching here is not the block ABI.
            return Flow::Unsupported(format!(
                "fan.any with {} args (thunk-list ABI takes 1)",
                args.len()
            ));
        };
        let thunks_val = val!(self.eval_expr(thunks_expr, scope));
        let Value::List(thunks) = thunks_val else {
            return Flow::Unsupported("fan.any over a non-list thunk carrier".to_string());
        };
        for t in thunks.iter() {
            let Value::Closure(clo) = t else {
                return Flow::Unsupported("fan.any arm that is not a thunk closure".to_string());
            };
            let v = val!(self.apply_closure(clo, Vec::new()));
            match v {
                // First Ok wins; later arms are never evaluated — the pinned
                // sequential side-effect order (C-004/C-185).
                ok @ Value::Result(Ok(_)) => return Flow::val(ok),
                Value::Result(Err(_)) => {}
                // A pure arm cannot fail: its value IS the winner (#514's
                // Ok adapter), and evaluation stops here too.
                pure => return Flow::val(Value::Result(Ok(Box::new(pure)))),
            }
        }
        Flow::val(Value::Result(Err(Box::new(Value::str(
            "fan.any: all candidates failed".to_string(),
        )))))
    }

    /// An in-place `mut`-receiver mutator, evaluated as a read → transform →
    /// write-back on the receiver's binding. Faithful only when the receiver
    /// names a binding this frame can actually assign to; every other shape
    /// abstains under its own name rather than dropping the effect silently.
    fn eval_inplace_mutation(
        &mut self,
        module: Sym,
        func: Sym,
        args: &[IrExpr],
        scope: &Scope,
    ) -> Flow {
        let (m, f) = (module.as_str(), func.as_str());
        if !crate::inplace::writes_back(m, f) {
            return Flow::Unsupported(format!(
                "in-place byte-level buffer write `{m}.{f}` (writes a scalar into a \
                 buffer at an offset; each carries its own bounds rule and byte \
                 order — issue #1021)"
            ));
        }
        let Some(recv) = args.first().and_then(crate::inplace::receiver_var) else {
            return self.eval_inplace_on_temporary(m, f, args, scope);
        };
        // A `mut`-PARAMETER receiver is fine now: the write lands on the
        // callee frame's binding, and `eval_named_call`'s mut-param copy-out
        // (#1022) carries it back to the caller's slot — the same two-step the
        // backends' MutParamLoweringPass performs (C-132).
        let mut rest = Vec::with_capacity(args.len().saturating_sub(1));
        for a in &args[1..] {
            rest.push(val!(self.eval_expr(a, scope)));
        }
        // The mutation runs against the binding's own storage — `with_slot`
        // rather than get/assign, so `Rc::make_mut` inside can keep a push loop
        // linear. Every argument is already a value by here, so nothing under
        // this closure re-enters the scope.
        match scope.with_slot(recv, |slot| crate::inplace::apply(m, f, slot, rest)) {
            Some(Some(out)) => Flow::val(out),
            Some(None) => Flow::Abort(format!("internal: malformed `{m}.{f}` receiver or args")),
            None => Flow::Abort(format!("internal: `{m}.{f}` on an unbound receiver")),
        }
    }

    /// [`Self::eval_inplace_mutation`] for a receiver that is not a variable.
    /// A TEMPORARY — a call result (`bytes.append_u8(bytes.new(2), 7)`,
    /// #1849) — has no binding to write back to, and none is needed: both
    /// backends evaluate the arguments in order, mutate the temporary and
    /// drop it, so the statement observes nothing but its own argument
    /// evaluation. The mutation runs on the evaluated value here (through
    /// the same `inplace::apply`, so a temporary that ALIASES a live
    /// binding's `Rc` takes the COW copy and the binding is untouched —
    /// the fixture's `same(x)` probe) and the value is dropped with the
    /// call. A record field or an index is a different shape: the backends
    /// write THROUGH it, and this frame has no single slot for it — that
    /// shape still abstains under its own name.
    fn eval_inplace_on_temporary(
        &mut self,
        m: &str,
        f: &str,
        args: &[IrExpr],
        scope: &Scope,
    ) -> Flow {
        let Some(recv) = args.first() else {
            return Flow::Abort(format!("internal: `{m}.{f}` with no receiver"));
        };
        let shape = match &recv.kind {
            IrExprKind::Call { .. } => None,
            IrExprKind::Member { .. } => Some("a record field"),
            IrExprKind::IndexAccess { .. } | IrExprKind::TupleIndex { .. } => Some("an index"),
            _ => Some("a non-call expression"),
        };
        if let Some(shape) = shape {
            return Flow::Unsupported(format!(
                "in-place container mutation `{m}.{f}` through {shape} receiver \
                 (the backends write through it; this frame has no single binding \
                 to write back to)"
            ));
        }
        let mut temp = val!(self.eval_expr(recv, scope));
        let mut rest = Vec::with_capacity(args.len().saturating_sub(1));
        for a in &args[1..] {
            rest.push(val!(self.eval_expr(a, scope)));
        }
        match crate::inplace::apply(m, f, &mut temp, rest) {
            Some(out) => Flow::val(out),
            None => Flow::Abort(format!("internal: malformed `{m}.{f}` receiver or args")),
        }
    }

    /// Dispatch a `(module, func)` whose args are already evaluated. Tiers:
    /// The budget prim quartet over the interpreter's deterministic meter —
    /// byte-for-byte the arithmetic of the wasm BudgetEnter/Exit renders and
    /// the native BUDGET_SHIM (see `render_wasm_p2_b.rs` / `render_native.rs`).
    /// Reached from the eval's `RuntimeCall` arm: the fan.bounded/race
    /// frontend lowering emits these symbols directly, pre-codegen.
    pub(crate) fn budget_prim_rt(&mut self, symbol: &str, args: &[Value]) -> Flow {
        let int0 = || match args.first() {
            Some(Value::Int(n)) => *n,
            _ => 0,
        };
        match symbol {
            "almide_rt_prim_budget_enter" => {
                let units = (int0() / almide_lang::time_units::CM1_NS_PER_CHARGE).max(0);
                self.det_entry.set(units);
                let saved = self.det_fuel.get();
                self.det_saved.set(saved);
                if units < saved {
                    self.det_fuel.set(units);
                }
                self.det_region_depth.set(self.det_region_depth.get() + 1);
                Flow::val(Value::Int(saved))
            }
            "almide_rt_prim_budget_exhausted" => {
                // C-320 reap: a det_cut returned from the region fn without
                // reaching the exit prim, leaving det_entry armed (>= 0 while
                // a region is open — the enter clamp's invariant). Perform
                // the missed exit bookkeeping first — same arithmetic, saved
                // taken from det_saved — exactly as the backend renders do.
                if self.det_entry.get() >= 0 {
                    self.det_verdict.set(i64::from(self.det_fuel.get() < 0));
                    let consumed = self.det_entry.get() - self.det_fuel.get();
                    self.det_spend.set(consumed);
                    self.det_fuel.set(self.det_saved.get() - consumed);
                    self.det_entry.set(-1);
                    self.det_region_depth.set(self.det_region_depth.get().saturating_sub(1));
                }
                Flow::val(Value::Int(self.det_verdict.get()))
            }
            "almide_rt_prim_budget_exit" => {
                self.det_verdict.set(i64::from(self.det_fuel.get() < 0));
                let consumed = self.det_entry.get() - self.det_fuel.get();
                self.det_spend.set(consumed);
                self.det_fuel.set(int0() - consumed);
                self.det_entry.set(-1);
                self.det_region_depth.set(self.det_region_depth.get().saturating_sub(1));
                Flow::val(Value::Int(0))
            }
            "almide_rt_prim_budget_spend" => Flow::val(Value::Int(self.det_spend.get())),
            "almide_rt_prim_timeout_enter" => {
                let saved = self.t_deadline.get();
                let now = if Self::omega_replay() >= 0 { 0 } else { self.wall_now_ns() };
                let dl = now.saturating_add(int0());
                if dl < saved {
                    self.t_deadline.set(dl);
                }
                self.det_region_depth.set(self.det_region_depth.get() + 1);
                Flow::val(Value::Int(saved))
            }
            "almide_rt_prim_timeout_exit" => {
                let hit = self.t_hit.get();
                self.t_verdict.set(hit as i64);
                if hit && std::env::var("ALMIDE_OMEGA_RECORD").is_ok_and(|v| v == "1") {
                    self.stderr.push_str(&format!("__ALMD_OMEGA {}\n", self.t_ord.get()));
                }
                self.t_hit.set(false);
                self.t_deadline.set(int0());
                self.det_region_depth.set(self.det_region_depth.get().saturating_sub(1));
                Flow::val(Value::Int(0))
            }
            "almide_rt_prim_timeout_hit" => Flow::val(Value::Int(self.t_verdict.get())),
            other => Flow::Unsupported(format!("budget prim `{other}`")),
        }
    }

    /// interp-native container ops → scalar/string bridge → almide-bodied
    /// stdlib fn → unsupported.
    pub(crate) fn dispatch_module_resolved(
        &mut self,
        module: Sym,
        func: Sym,
        args: Vec<Value>,
    ) -> Flow {
        // `process.exit(n)` — terminate with code n, printing nothing extra
        // (the ALS-T18 assert desugar eprintlns its own line first).
        if module.as_str() == "process" && func.as_str() == "exit" {
            let code = match args.first() {
                Some(Value::Int(n)) => *n,
                _ => 1,
            };
            return Flow::Exit(code);
        }
        // Interp-native container ops (non-HOF: structural transforms).
        if let Some(result) = self.eval_container_op(module.as_str(), func.as_str(), &args) {
            return result;
        }

        // The argv floor (Stage 2 BRIDGEABLE burn-down): value-clean prims the
        // STATELESS bridge cannot serve — they read the run's argv, which is
        // interpreter state (`with_args`, empty by default = exactly how the
        // oracle harness runs every fixture on all three legs).
        // `args_get_list` answers argv[1..]; `args_get_list_full` prepends an
        // argv[0] whose only cross-target observable is NONEMPTINESS (the
        // fixtures assert it, never print it — C-181's argv0 normalization).
        if module.as_str() == "prim" {
            // The BLOCK HEAP floor (#1226, heap.rs). Same tier as argv /
            // env / fs and for the same reason: these read and MUTATE per-run
            // interpreter state, which the stateless `bridge::prim_fn` cannot
            // hold. Slice 1 served the flat String/Bytes family; slice 2 adds
            // the slot-block container family (`alloc_list*` / `alloc_set*` /
            // `alloc_map*` / `alloc_value`, `store_str` / `load_str` /
            // `load_handle`, `rc_inc` / `rc_dec`). What a block cannot
            // faithfully spell still falls through to the honest abstain, so
            // this stays a CLOSED family the voting gate can arbitrate.
            if let Some(flow) = self.heap_prim(func.as_str(), &args) {
                return flow;
            }
            match func.as_str() {
                // `prim.die(prim.handle(msg))` — the guarded-abort floor
                // (Stage 2 BRIDGEABLE burn-down: int_pow_negative_exponent,
                // list_chunk_zero, …). The argument is by construction a
                // handle to the full "Error: <reason>\n" line both backends
                // eprint VERBATIM before exit(1); the interp's Abort contract
                // prints `Error: {msg}\n`, so the bridged message is that line
                // with the frame stripped. A message outside the frame
                // abstains — this floor must never invent a stderr the
                // backends would not produce.
                "die" => {
                    let Some(addr) = heap_addr(args.first()) else {
                        return Flow::Unsupported("prim.die with a non-address".into());
                    };
                    let Some((bytes, _)) = self.heap.block_bytes(addr) else {
                        return Flow::Unsupported(
                            "prim.die outside this heap's arena".into(),
                        );
                    };
                    let msg = String::from_utf8_lossy(&bytes).into_owned();
                    let Some(reason) = msg
                        .strip_prefix("Error: ")
                        .and_then(|m| m.strip_suffix('\n'))
                    else {
                        return Flow::Unsupported(
                            "prim.die with a message outside the Error:-line contract"
                                .into(),
                        );
                    };
                    return Flow::Abort(reason.to_string());
                }
                "args_get_list" => {
                    let items: Vec<Value> =
                        self.args.iter().map(|s| Value::str(s.clone())).collect();
                    return Flow::val(Value::list(items));
                }
                "args_get_list_full" => {
                    let mut items = vec![Value::str("interp")];
                    items.extend(self.args.iter().map(|s| Value::str(s.clone())));
                    return Flow::val(Value::list(items));
                }
                // The env floor (same tier as argv, C-133): `prim.env_get`
                // reads the LIVE process environment — exactly what the other
                // two legs observe (native getenv; wasm WASI environ with
                // inherit-env), so all three answer the same bytes. Without
                // this arm the resolve below finds the lowered `env_get`
                // stdlib fn BY BARE NAME — the very fn whose body is this
                // prim — and the interp spun in that cycle until fuel ran
                // out (the module-identity bug class, #1087–#1094, one tier
                // down: a prim resolved from the wrong source).
                "env_get" => {
                    let Some(Value::Str(name)) = args.first() else {
                        return Flow::Abort("internal: prim.env_get expects a String".into());
                    };
                    return Flow::val(match std::env::var(name.as_str()) {
                        Ok(v) => Value::Option(Some(Box::new(Value::str(v)))),
                        Err(_) => Value::Option(None),
                    });
                }
                // The sandboxed fs floor (#1218, vfs.rs): writes land in the
                // per-interpreter overlay, reads fall back to the real fs
                // read-only. Same tier as the argv/env floors — these prims
                // read INTERPRETER state, which the stateless bridge cannot.
                "read_text_file" => {
                    let Some(Value::Str(path)) = args.first() else {
                        return Flow::Abort("internal: prim.read_text_file expects a String".into());
                    };
                    return Flow::val(match crate::vfs::read_text(&self.vfs, path) {
                        Ok(s) => Value::Result(Ok(Box::new(Value::str(s)))),
                        Err(e) => Value::Result(Err(Box::new(Value::str(e)))),
                    });
                }
                "write_text_file" => {
                    let (Some(Value::Str(path)), Some(Value::Str(content))) =
                        (args.first(), args.get(1))
                    else {
                        return Flow::Abort(
                            "internal: prim.write_text_file expects (String, String)".into(),
                        );
                    };
                    let (path, content) = (path.to_string(), content.to_string());
                    return Flow::val(match crate::vfs::write_text(&mut self.vfs, &path, &content) {
                        Ok(()) => Value::Result(Ok(Box::new(Value::Unit))),
                        Err(e) => Value::Result(Err(Box::new(Value::str(e)))),
                    });
                }
                "make_dir" => {
                    let Some(Value::Str(path)) = args.first() else {
                        return Flow::Abort("internal: prim.make_dir expects a String".into());
                    };
                    let path = path.to_string();
                    return Flow::val(match crate::vfs::make_dir(&mut self.vfs, &path) {
                        Ok(()) => Value::Result(Ok(Box::new(Value::Unit))),
                        Err(e) => Value::Result(Err(Box::new(Value::str(e)))),
                    });
                }
                "path_exists" => {
                    let Some(Value::Str(path)) = args.first() else {
                        return Flow::Abort("internal: prim.path_exists expects a String".into());
                    };
                    return Flow::val(Value::Bool(crate::vfs::exists(&self.vfs, path)));
                }
                "remove_all" => {
                    let Some(Value::Str(path)) = args.first() else {
                        return Flow::Abort("internal: prim.remove_all expects a String".into());
                    };
                    let path = path.to_string();
                    return match crate::vfs::remove_all(&mut self.vfs, &path) {
                        crate::vfs::RemoveOutcome::Removed => {
                            Flow::val(Value::Result(Ok(Box::new(Value::Unit))))
                        }
                        // A host path the overlay never wrote: refusing to
                        // delete real files is the sandbox's point, and
                        // pretending to would be a wrong vote — abstain.
                        crate::vfs::RemoveOutcome::HostOnly => Flow::Unsupported(
                            "prim.remove_all on a host path (the overlay is read-only toward the real fs)".into(),
                        ),
                        crate::vfs::RemoveOutcome::Missing => {
                            Flow::val(Value::Result(Err(Box::new(Value::str(
                                "No such file or directory (os error 2)".to_string(),
                            )))))
                        }
                    };
                }
                _ => {}
            }
        }

        // Scalar / string / math native bridge (intrinsic-symbol surface).
        if let Some(result) = crate::bridge::dispatch(module.as_str(), func.as_str(), &args) {
            return result;
        }

        let Some((func_def, gate_mut)) = self.resolve_lowered_body(module, func) else {
            return Flow::Unsupported(format!("{}.{}", module, func));
        };
        // These sites receive EAGERLY-evaluated args, so a `mut` parameter's
        // caller lvalue is already gone — the Named path's copy-out (#1022)
        // cannot run here. A mut-param callee must abstain rather than
        // silently drop the write-back (a wrong third vote).
        if gate_mut && func_def.params.iter().any(|p| p.is_mut) {
            return Flow::Unsupported(format!(
                "module call `{}.{}` with a `mut` parameter through the \
                 eager dispatch path (no caller lvalue to copy out — #1022)",
                module.as_str(),
                func.as_str()
            ));
        }
        let root = self.root_scope();
        let flow = self.call_pool_tier(func_def, args, &root);
        // #1226 RETURN SYNC. A self-hosted body that allocated a block returns
        // the block's ADDRESS (`prim.alloc_str` is an Int), so the value has to
        // be read back out of the arena before it leaves the pool tier —
        // `base64_encode` is exactly `alloc → handle → store → return out`.
        //
        // Driven by the DECLARED return type, never by guessing whether an Int
        // looks like an address: a fn returning a plain Int (`string.len`) must
        // not have its result reinterpreted as a handle. That guess is the
        // wrong-vote trap this whole slice is built to avoid.
        //
        // Applied ONLY at the boundary back OUT of the pool tier
        // (`pool_depth == 0` at the call): inside the tier a heap value IS its
        // address, and an eager rebuild there snapshots blocks the caller is
        // still writing through (see `pool_fns`).
        self.sync_at_pool_boundary(func_def, flow)
    }

    /// Run a resolved body, tracking the pool-tier boundary: while a POOL
    /// body (see `pool_fns`) is on the stack, heap values stay addresses and
    /// no return sync fires.
    fn call_pool_tier(
        &mut self,
        func: &'a almide_ir::IrFunction,
        args: Vec<Value>,
        root: &Scope,
    ) -> Flow {
        // Depth is owned by `run_callable`'s per-hop bump (a tail TRANSFER
        // into a pool fn must carry the tier with it — a second bump here
        // would leave the spine-exit sync reading a stale depth and skipping
        // the read-back, the codec `list.len on non-list` leak).
        self.call_function(func, args, root)
    }

    /// The return sync, gated to the pool-tier EXIT boundary. `named` marks
    /// the named-call spelling: there, only a POOL callee syncs — a fixture's
    /// own fn keeps the raw address model its `import prim` code was written
    /// against (pre-slice-2 behavior, which the prim-family fixtures pin).
    fn sync_at_pool_boundary(
        &self,
        func_def: &'a almide_ir::IrFunction,
        flow: Flow,
    ) -> Flow {
        if self.pool_depth > 0 {
            return flow;
        }
        self.sync_block_return(func_def, flow)
    }

    /// Read a returned block address back into the value its declared return
    /// type names. Anything else passes through untouched; an address whose
    /// block CANNOT faithfully spell the declared type abstains — passing the
    /// raw address onward would hand the fixture an Int where its type says
    /// container, and whatever that Int does next is a wrong vote.
    pub(crate) fn sync_block_return(&self, func_def: &'a almide_ir::IrFunction, flow: Flow) -> Flow {
        let Flow::Value(v) = &flow else { return flow };
        match self.sync_value(v, &func_def.ret_ty) {
            Ok(Some(rebuilt)) => Flow::Value(rebuilt),
            Ok(None) => flow,
            Err(why) => Flow::Unsupported(why),
        }
    }

    /// [`Self::sync_block_return`] driven by a bare `Ty` — the closure-boundary
    /// entry (`apply_closure` has a `ret_ty`, not an `IrFunction`).
    pub(crate) fn sync_flow_typed(&self, flow: Flow, ty: &Ty) -> Flow {
        let Flow::Value(v) = &flow else { return flow };
        match self.sync_value(v, ty) {
            Ok(Some(rebuilt)) => Flow::Value(rebuilt),
            Ok(None) => flow,
            Err(why) => Flow::Unsupported(why),
        }
    }

    /// The typed read-back behind `sync_block_return`, recursive through the
    /// carrier shells a body may have already built (`Option` / `Result` /
    /// tuples). `Ok(None)` = leave the value untouched.
    fn sync_value(&self, v: &Value, ty: &Ty) -> Result<Option<Value>, String> {
        use almide_lang::types::constructor::TypeConstructorId as C;
        match (v, ty) {
            (Value::Option(Some(x)), Ty::Applied(C::Option, ts)) if ts.len() == 1 => Ok(self
                .sync_value(x, &ts[0])?
                .map(|r| Value::Option(Some(Box::new(r))))),
            (Value::Result(Ok(x)), Ty::Applied(C::Result, ts)) if ts.len() == 2 => {
                Ok(self.sync_value(x, &ts[0])?.map(|r| Value::Result(Ok(Box::new(r)))))
            }
            (Value::Result(Err(x)), Ty::Applied(C::Result, ts)) if ts.len() == 2 => {
                Ok(self.sync_value(x, &ts[1])?.map(|r| Value::Result(Err(Box::new(r)))))
            }
            (Value::Tuple(xs), Ty::Tuple(ts)) if xs.len() == ts.len() => {
                let mut rebuilt = Vec::with_capacity(xs.len());
                let mut any = false;
                for (x, t) in xs.iter().zip(ts) {
                    match self.sync_value(x, t)? {
                        Some(r) => {
                            any = true;
                            rebuilt.push(r);
                        }
                        None => rebuilt.push(x.clone()),
                    }
                }
                Ok(any.then(|| Value::tuple(rebuilt)))
            }
            // A NATIVE container built inside the pool tier can hold
            // address elements (`__rx_split_go` accumulates `__rx_sub`
            // pieces with the native list concat): recurse per element by
            // the declared element type.
            (Value::List(xs), Ty::Applied(C::List, ts)) if ts.len() == 1 => {
                Ok(self.sync_elems(xs, &ts[0])?.map(Value::list))
            }
            (Value::Set(xs), Ty::Applied(C::Set, ts)) if ts.len() == 1 => {
                Ok(self.sync_elems(xs, &ts[0])?.map(|v| Value::Set(Rc::new(v))))
            }
            (Value::Map(es), Ty::Applied(C::Map, ts)) if ts.len() == 2 => {
                let mut rebuilt = Vec::with_capacity(es.len());
                let mut any = false;
                for (k, v) in es.iter() {
                    let nk = self.sync_value(k, &ts[0])?;
                    let nv = self.sync_value(v, &ts[1])?;
                    any |= nk.is_some() || nv.is_some();
                    rebuilt.push((nk.unwrap_or_else(|| k.clone()), nv.unwrap_or_else(|| v.clone())));
                }
                Ok(any.then(|| Value::Map(Rc::new(rebuilt))))
            }
            (Value::Int(i), _) => {
                let Some(addr) = u32::try_from(*i).ok().filter(|a| self.heap.kind(*a).is_some())
                else {
                    // An ordinary integer (or a non-base address): not ours.
                    return Ok(None);
                };
                if heap_modeled_ty(ty) {
                    return match self.rebuild_addr(addr, ty) {
                        Some(rebuilt) => Ok(Some(rebuilt)),
                        None => Err(format!(
                            "return sync: a block has no faithful read-back \
                             under the declared return type ({})",
                            ty_short(ty)
                        )),
                    };
                }
                use almide_lang::types::constructor::TypeConstructorId as C;
                if matches!(ty, Ty::Applied(C::List | C::Set | C::Map | C::Option | C::Result, _))
                {
                    // A container-typed return we cannot spell (a generic
                    // element erased to a type variable, an unmodeled Option
                    // block): the fixture expects a container, so the raw
                    // address must not leak into native ops — abstain.
                    return Err(format!(
                        "return sync: a block under an unspellable container \
                         return type ({})",
                        ty_short(ty)
                    ));
                }
                if matches!(ty, Ty::Named(n, args) if args.is_empty() && n.as_str() == "Value") {
                    // The dynamic `Value` leaves the pool tier as a CARRIER:
                    // the block address (so `prim.handle` re-enters the SAME
                    // block) plus the structural snapshot display and `==`
                    // read. A bare Int here used to leak to native ops as an
                    // integer (value_repr printed the address, value_eq
                    // pointer-compared to false).
                    return match self.dyn_value_of(addr) {
                        Some(d) => Ok(Some(d)),
                        None => Err(format!(
                            "return sync: a Value-typed block at {addr} cannot \
                             be walked (unknown tag or unreadable child)"
                        )),
                    };
                }
                // An opaque return type (a bare type variable, Int): the
                // address IS the value in the i64-uniform tier.
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    /// Per-element sync for a native list/set — `Ok(None)` when nothing
    /// needed rebuilding.
    fn sync_elems(&self, xs: &[Value], elem: &Ty) -> Result<Option<Vec<Value>>, String> {
        let mut rebuilt = Vec::with_capacity(xs.len());
        let mut any = false;
        for x in xs {
            match self.sync_value(x, elem)? {
                Some(r) => {
                    any = true;
                    rebuilt.push(r);
                }
                None => rebuilt.push(x.clone()),
            }
        }
        Ok(any.then_some(rebuilt))
    }

    /// The structural snapshot of a dynamic-`Value` block — value_core's tag
    /// walk (0 null, 1 bool, 2 int, 3 float, 4 str payload = child String
    /// handle, 5 array of n Value handles with n at @8, 6 object of n
    /// (String, Value) pairs with the SLOT count 2n at @8). `None` (an
    /// abstain upstream) for a non-block address, an unknown tag, an
    /// unreadable child, or a depth past the cap — never a guess.
    fn dyn_node_of(&self, addr: u32, depth: u32) -> Option<crate::value::DynNode> {
        use crate::value::DynNode;
        if depth > 512 || self.heap.kind(addr)? != crate::heap::BlockKind::Slots {
            return None;
        }
        let tag = self.heap.block_len(addr)?;
        Some(match tag {
            0 => DynNode::Null,
            1 => DynNode::Bool(self.heap.slot(addr, 0)? != 0),
            2 => DynNode::Int(self.heap.slot(addr, 0)?),
            3 => DynNode::Float(f64::from_bits(self.heap.slot(addr, 0)? as u64)),
            4 => {
                let child = u32::try_from(self.heap.slot(addr, 0)?).ok()?;
                let (bytes, kind) = self.heap.block_bytes(child)?;
                if kind != crate::heap::BlockKind::Str {
                    return None;
                }
                DynNode::Str(String::from_utf8(bytes).ok()?)
            }
            5 => {
                let n = self.heap.cap_field(addr)?;
                let mut xs = Vec::with_capacity(n as usize);
                for i in 0..n {
                    let child = u32::try_from(self.heap.slot(addr, i)?).ok()?;
                    xs.push(self.dyn_node_of(child, depth + 1)?);
                }
                DynNode::Arr(xs)
            }
            6 => {
                let pairs = self.heap.cap_field(addr)? / 2;
                let mut out = Vec::with_capacity(pairs as usize);
                for i in 0..pairs {
                    let kaddr = u32::try_from(self.heap.slot(addr, 2 * i)?).ok()?;
                    let (kb, kk) = self.heap.block_bytes(kaddr)?;
                    if kk != crate::heap::BlockKind::Str {
                        return None;
                    }
                    let vaddr = u32::try_from(self.heap.slot(addr, 2 * i + 1)?).ok()?;
                    out.push((
                        String::from_utf8(kb).ok()?,
                        self.dyn_node_of(vaddr, depth + 1)?,
                    ));
                }
                DynNode::Obj(out)
            }
            _ => return None,
        })
    }

    /// A dynamic-`Value` carrier for a block address, or `None` when the
    /// block cannot honestly be walked.
    fn dyn_value_of(&self, addr: u32) -> Option<Value> {
        Some(Value::Dyn {
            addr: addr as i64,
            node: Rc::new(self.dyn_node_of(addr, 0)?),
        })
    }

    /// A block address as the `Value` the declared type `ty` spells — `None`
    /// when the block's kind or shape cannot honestly spell it.
    fn rebuild_addr(&self, addr: u32, ty: &Ty) -> Option<Value> {
        use almide_lang::types::constructor::TypeConstructorId as C;
        use crate::heap::BlockKind as K;
        match ty {
            // The slice-1 read-back: the kind decides Str vs byte list.
            Ty::String | Ty::Bytes => {
                let (bytes, kind) = self.heap.block_bytes(addr)?;
                Some(match kind {
                    K::Str => Value::str(String::from_utf8(bytes).ok()?),
                    K::Bytes => {
                        Value::list(bytes.into_iter().map(|b| Value::Int(b as i64)).collect())
                    }
                    K::Slots => return None,
                })
            }
            Ty::Applied(C::List, ts) if ts.len() == 1 => {
                Some(Value::list(self.rebuild_seq(addr, &ts[0])?))
            }
            Ty::Applied(C::Set, ts) if ts.len() == 1 => {
                Some(Value::Set(Rc::new(self.rebuild_seq(addr, &ts[0])?)))
            }
            Ty::Applied(C::Map, ts) if ts.len() == 2 => self.rebuild_map(addr, &ts[0], &ts[1]),
            // A tuple block: one slot per element, `len` = the element count.
            Ty::Tuple(ts) => {
                if self.heap.kind(addr)? != crate::heap::BlockKind::Slots
                    || self.heap.block_len(addr)? as usize != ts.len()
                {
                    return None;
                }
                let elems: Option<Vec<Value>> = ts
                    .iter()
                    .enumerate()
                    .map(|(i, t)| self.rebuild_slot(self.heap.slot(addr, i as u32)?, t))
                    .collect();
                Some(Value::tuple(elems?))
            }
            _ => None,
        }
    }

    /// A List/Set block's elements. A `Bytes` block under a byte-element list
    /// type is the slice-1 family; otherwise the block must be a slot block
    /// with `len` = element count.
    fn rebuild_seq(&self, addr: u32, elem: &Ty) -> Option<Vec<Value>> {
        match self.heap.kind(addr)? {
            crate::heap::BlockKind::Bytes if matches!(elem, Ty::Int | Ty::Int64) => {
                let (bytes, _) = self.heap.block_bytes(addr)?;
                Some(bytes.into_iter().map(|b| Value::Int(b as i64)).collect())
            }
            crate::heap::BlockKind::Slots => {
                let n = self.heap.block_len(addr)?;
                (0..n)
                    .map(|i| self.rebuild_slot(self.heap.slot(addr, i)?, elem))
                    .collect()
            }
            _ => None,
        }
    }

    /// One raw slot as the `Value` its element type spells: scalars by value,
    /// heap elements by recursive block read, opaque types (`Value`, type
    /// variables) as the ADDRESS itself.
    fn rebuild_slot(&self, s: i64, elem: &Ty) -> Option<Value> {
        use almide_lang::types::constructor::TypeConstructorId as C;
        match elem {
            Ty::Int | Ty::Int64 | Ty::Int8 | Ty::Int16 | Ty::Int32 => Some(Value::Int(s)),
            Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64 => Some(Value::Int(s)),
            Ty::Float | Ty::Float64 => Some(Value::Float(f64::from_bits(s as u64))),
            Ty::Bool => Some(Value::Bool(s != 0)),
            Ty::String
            | Ty::Bytes
            | Ty::Tuple(_)
            | Ty::Applied(C::List | C::Set | C::Map | C::Option | C::Result, _) => {
                self.rebuild_addr(u32::try_from(s).ok()?, elem)
            }
            Ty::Named(n, args) if args.is_empty() && n.as_str() == "Value" => {
                self.dyn_value_of(u32::try_from(s).ok()?)
            }
            _ => Some(Value::Int(s)),
        }
    }

    /// A Map block's entries, by the three physical layouts the alloc family
    /// documents (see `heap_materialize`). Which layout applies is decided by
    /// the heapness of the DECLARED key/value types, same as the builders.
    fn rebuild_map(&self, addr: u32, kty: &Ty, vty: &Ty) -> Option<Value> {
        if self.heap.kind(addr)? != crate::heap::BlockKind::Slots {
            return None;
        }
        let len = self.heap.block_len(addr)?;
        let k_heap = heap_slot_is_child(kty);
        let v_heap = heap_slot_is_child(vty);
        let entries = if k_heap && v_heap { len / 2 } else { len };
        let mut pairs = Vec::with_capacity(entries as usize);
        for i in 0..entries {
            let (kslot, vslot) = if k_heap && v_heap {
                (self.heap.slot(addr, 2 * i)?, self.heap.slot(addr, 2 * i + 1)?)
            } else if k_heap {
                // alloc_map_skv: `entries` key slots then `entries` value
                // slots — the value region starts at slot `len`, NOT `cap/2`
                // (`map_set` may leave geometric slack above 2*entries).
                (self.heap.slot(addr, i)?, self.heap.slot(addr, len + i)?)
            } else {
                (self.heap.slot(addr, 2 * i)?, self.heap.slot(addr, 2 * i + 1)?)
            };
            pairs.push((self.rebuild_slot(kslot, kty)?, self.rebuild_slot(vslot, vty)?));
        }
        Some(Value::Map(Rc::new(pairs)))
    }

    /// The lowered Almide body `module.func` resolves to, paired with whether
    /// the eager-dispatch `mut`-parameter gate applies to it.
    ///
    /// Three sources in order: the module's own fn table; a top-level fn named
    /// exactly `func` (some stdlib helpers flatten); and LAST the self-hosted
    /// stdlib body from the shared registry (stdlib_pool) — the SAME source the
    /// wasm leg links for this call name, lowered once and layered into
    /// `self.fns` at construction. Consulted last so the interp-native surfaces
    /// above keep their vote provenance; what a pool body itself cannot
    /// evaluate (a heap/effect prim outside the scalar floor) abstains from
    /// inside with that prim named — a skip, never a guess — so the mut gate
    /// does not apply to it.
    ///
    /// A `Hole` body is an intrinsic stub, not an interpretable definition:
    /// each source skips it and falls through to the next.
    fn resolve_lowered_body(&self, module: Sym, func: Sym) -> Option<(&'a almide_ir::IrFunction, bool)> {
        fn bodied(d: &&almide_ir::IrFunction) -> bool {
            !matches!(d.body.kind, almide_ir::IrExprKind::Hole)
        }
        if let Some(d) = self.module_fns.get(&(module, func)).copied().filter(bodied) {
            return Some((d, true));
        }
        if let Some(d) = self.fns.get(&func).copied().filter(bodied) {
            return Some((d, true));
        }
        let impl_name = crate::stdlib_pool::impl_fn(module, func)?;
        self.fns.get(&impl_name).copied().filter(bodied).map(|d| (d, false))
    }

    /// The block-heap prim floor (#1226 slice 1). `None` = "not mine", which
    /// falls through to the argv/env/fs arms and ultimately to the honest
    /// abstain, so an unmodelled prim keeps its `Flow::Unsupported` rather than
    /// getting a guessed value.
    ///
    /// Split one fn per family, the way `bridge.rs` splits its scalar floor
    /// into `prim_bitwise_fn` / `prim_repr_fn` / …: a miss in one falls through
    /// to the next and ends as the same `None` an unmatched name would give, so
    /// the chain is equivalent to one flat table with the arms in this order.
    fn heap_prim(&mut self, func: &str, args: &[Value]) -> Option<Flow> {
        let out = self
            .heap_prim_handle(func, args)
            .or_else(|| self.heap_prim_alloc(func, args))
            .or_else(|| self.heap_prim_load(func, args))
            .or_else(|| self.heap_prim_store(func, args))
            .or_else(|| self.heap_prim_slot_io(func, args));
        if std::env::var("ALMIDE_HEAP_TRACE").is_ok_and(|v| v == "1") {
            if let Some(f) = &out {
                let shown = match f {
                    Flow::Value(v) => format!("{v:?}"),
                    Flow::Unsupported(m) => format!("UNSUPPORTED: {m}"),
                    _ => "<flow>".to_string(),
                };
                eprintln!("[heap] prim.{func}({args:?}) -> {shown}");
            }
        }
        out
    }

    /// `prim.handle(v)` — the base address of v's block. A value that has not
    /// been in the arena is materialized ONCE and bound, so the `+ 4` (len) and
    /// `+ 12` (payload) reads in one body agree on the same block.
    fn heap_prim_handle(&mut self, func: &str, args: &[Value]) -> Option<Flow> {
        if func != "handle" {
            return None;
        }
        let hint = self.handle_arg_ty.take();
        let Some(v) = args.first() else {
            return Some(Flow::Unsupported("prim.handle with no argument".into()));
        };
        Some(match self.heap_materialize_hinted(v, hint.as_ref()) {
            Ok(a) => Flow::val(Value::Int(a)),
            Err(why) => Flow::Unsupported(why),
        })
    }

    /// A value as its arena address — the read direction, generalized in
    /// slice 2 to the SCALAR container family. `Err` carries the abstain
    /// reason.
    ///
    /// Scalar-only is a correctness line, not laziness: the pool tier resolves
    /// a PUBLIC container fn (`map.get_or`, `list.…`) by NAME to its scalar
    /// core impl — the type-directed rewrite that picks `_skv`/`_str`/`_hval`
    /// variants is a MIR-lowering pass the interp never runs. A heap-keyed Map
    /// materialized in ANY layout would therefore be walked by a body that
    /// compares key slots as raw i64s, and same-content strings in different
    /// blocks would miss — a wrong vote (`tm_map_int_lit_print` printed -8 for
    /// 12 in exactly this shape). Scalar containers are the ones the core
    /// impls are CORRECT for; everything else abstains with its shape named.
    /// [`Self::heap_materialize`] with the call site's STATIC argument type.
    ///
    /// The hint does two jobs. It settles representation questions the value
    /// alone cannot — `Bytes` spells 1 payload byte per element where
    /// `List[Int]` spells an 8-byte slot, and a CHILD list's byte-vs-slot
    /// choice needs the element type, not inspection. And it is the
    /// IMPL-CORRECTNESS guard for heap elements: the hint IS the resolved
    /// body's own declared parameter type, so when the untyped pool resolves
    /// a public name to its scalar core impl and a heap-element container
    /// arrives, the declaration says `List[Int]`, the value says Strings,
    /// and the mismatch abstains — while a body genuinely declared over
    /// `List[(String, Value)]` (value_object) materializes exactly what it
    /// claims. Without a concrete hint (a generic `handle[A]`), inspection
    /// decides and stays scalar-only, as before.
    fn heap_materialize_hinted(&mut self, v: &Value, hint: Option<&Ty>) -> Result<i64, String> {
        use crate::heap::rc_key;
        use almide_lang::types::constructor::TypeConstructorId as C;
        match (v, hint) {
            // A declared Bytes is STRICT: a non-byte element under it has no
            // faithful byte, and falling back to slots would hand a
            // byte-reading body 8x-strided memory.
            (Value::List(rc), Some(Ty::Bytes)) => {
                let bytes: Result<Vec<u8>, String> = rc
                    .iter()
                    .map(|v| match v {
                        Value::Int(i) if (0..=255).contains(i) => Ok(*i as u8),
                        other => Err(format!(
                            "prim.handle of a Bytes-typed list holding a non-byte {}",
                            other.type_name()
                        )),
                    })
                    .collect();
                let a = self.heap.bind(rc_key(rc), &bytes?, crate::heap::BlockKind::Bytes);
                self.heap.keep(Rc::clone(rc));
                Ok(a as i64)
            }
            (Value::List(rc), Some(Ty::Applied(C::List | C::Set, ts))) if ts.len() == 1 => {
                let rc = Rc::clone(rc);
                let slots: Result<Vec<i64>, String> =
                    rc.iter().map(|e| self.heap_slot_hinted(e, &ts[0])).collect();
                let a = self.heap.bind_slots(rc_key(&rc), &slots?, rc.len() as u32);
                self.heap.keep(rc);
                Ok(a as i64)
            }
            (Value::Set(rc), Some(Ty::Applied(C::Set | C::List, ts))) if ts.len() == 1 => {
                let rc = Rc::clone(rc);
                let slots: Result<Vec<i64>, String> =
                    rc.iter().map(|e| self.heap_slot_hinted(e, &ts[0])).collect();
                let a = self.heap.bind_slots(rc_key(&rc), &slots?, rc.len() as u32);
                self.heap.keep(rc);
                Ok(a as i64)
            }
            // The paired scalar-map layout under its declared key/value types
            // — same strictness as the sequences. SCALAR declarations only:
            // a heap-keyed map spells the skv/interleaved layouts, not this
            // one, and those stay out of this slice.
            (Value::Map(rc), Some(Ty::Applied(C::Map, ts)))
                if ts.len() == 2
                    && !heap_slot_is_child(&ts[0])
                    && !heap_slot_is_child(&ts[1]) =>
            {
                let rc = Rc::clone(rc);
                let entries = rc.len() as u32;
                let mut slots = Vec::with_capacity(2 * rc.len());
                for (k, v) in rc.iter() {
                    slots.push(self.heap_slot_hinted(k, &ts[0])?);
                    slots.push(self.heap_slot_hinted(v, &ts[1])?);
                }
                let a = self.heap.bind_slots(rc_key(&rc), &slots, entries);
                self.heap.keep(rc);
                Ok(a as i64)
            }
            // A tuple is one more slot block: one slot per element, `len` =
            // the element count (value_object reads `(String, Value)` pairs
            // as `load64(tup+12)` / `load64(tup+20)`). Unlocked by the
            // `Value::Dyn` carrier (increment 4): the decode chains this
            // opens now hand fixture-level `==` and repr typed carriers,
            // not bare addresses.
            (Value::Tuple(rc), Some(Ty::Tuple(ts))) if ts.len() == rc.len() => {
                let rc = Rc::clone(rc);
                let slots: Result<Vec<i64>, String> = rc
                    .iter()
                    .zip(ts)
                    .map(|(e, t)| self.heap_slot_hinted(e, t))
                    .collect();
                let a = self.heap.bind_slots(rc_key(&rc), &slots?, rc.len() as u32);
                self.heap.keep(rc);
                Ok(a as i64)
            }
            _ => self.heap_materialize(v),
        }
    }

    /// One container element as its slot i64, driven by the DECLARED element
    /// type: scalars inline (NaN still abstains — #1403), heap elements as
    /// recursively-materialized children under their own hint.
    ///
    /// Scalars are STRICT against the declaration, and that strictness is the
    /// wrong-impl detector: a Float VALUE under a declared-Int element means
    /// the untyped pool resolved the scalar core impl where the backends'
    /// type-directed rewrite picks the `_f64` twin — the body would run, but
    /// its declared return type then mislabels the result and the f64 BITS
    /// leak out as integers (nightly fuzz 2026-08-19, seed 515402596033/74:
    /// `list.dedup([2.718…])` printed 4613303445314885481). A mismatch
    /// abstains; matching declarations pass. An Int under an OPAQUE declared
    /// type (`Value`, a type variable) is the address-identity and stays.
    fn heap_slot_hinted(&mut self, e: &Value, ty: &Ty) -> Result<i64, String> {
        let int_decl = matches!(
            ty,
            Ty::Int
                | Ty::Int8
                | Ty::Int16
                | Ty::Int32
                | Ty::Int64
                | Ty::UInt8
                | Ty::UInt16
                | Ty::UInt32
                | Ty::UInt64
        );
        let opaque_decl = matches!(ty, Ty::TypeVar(_) | Ty::Unknown)
            || matches!(ty, Ty::Named(n, args) if args.is_empty() && n.as_str() == "Value");
        match e {
            Value::Int(i) if int_decl || opaque_decl => Ok(*i),
            Value::Dyn { addr, .. } if opaque_decl => Ok(*addr),
            // An ADDRESS from the i64-uniform tier flowing back under a HEAP
            // declaration (a `load_handle`/`load64` borrow riding through
            // fixture-tier plumbing): accept iff it is a live block whose
            // KIND can spell the declared type — identity, no copy, aliasing
            // kept. A dead address or a kind mismatch stays the abstain.
            Value::Int(i) if heap_slot_is_child(ty) => {
                use crate::heap::BlockKind as K;
                let kind = u32::try_from(*i).ok().and_then(|a| self.heap.kind(a));
                let spellable = match (kind, ty) {
                    (Some(K::Str), Ty::String) => true,
                    (Some(K::Bytes), Ty::Bytes) => true,
                    (Some(K::Slots), Ty::Applied(..) | Ty::Tuple(_)) => true,
                    (Some(K::Slots), Ty::Named(n, args))
                        if args.is_empty() && n.as_str() == "Value" =>
                    {
                        true
                    }
                    _ => false,
                };
                if spellable {
                    Ok(*i)
                } else {
                    Err(format!(
                        "prim.handle of a container holding a non-block Int \
                         under the declared element type {}",
                        ty_short(ty)
                    ))
                }
            }
            Value::Float(_) if matches!(ty, Ty::Float | Ty::Float64) || opaque_decl => {
                heap_scalar_slot(e, "container")
            }
            Value::Bool(b) if matches!(ty, Ty::Bool) || opaque_decl => Ok(*b as i64),
            Value::Str(_) | Value::List(_) | Value::Set(_) | Value::Map(_) | Value::Tuple(_)
                if heap_slot_is_child(ty) && !opaque_decl =>
            {
                self.heap_materialize_hinted(e, Some(ty))
            }
            other => Err(format!(
                "prim.handle of a container holding a {} element under the \
                 declared element type {} (no faithful slot repr)",
                other.type_name(),
                ty_short(ty)
            )),
        }
    }

    fn heap_materialize(&mut self, v: &Value) -> Result<i64, String> {
        use crate::heap::{rc_key, BlockKind};
        match v {
            // The MIR is i64-uniform and the backends' `handle` is a bitwise
            // reinterpret, so on a value that is ALREADY an address — an
            // opaque `Value` flowing back into the pool tier, a `load_handle`
            // result — identity is the faithful model. It is identity for a
            // genuine scalar Int too, exactly as it is there.
            Value::Int(i) => Ok(*i),
            // The dynamic-Value carrier re-enters as ITS OWN block.
            Value::Dyn { addr, .. } => Ok(*addr),
            Value::Str(rc) => {
                let a = self.heap.bind(rc_key(rc), rc.as_bytes(), BlockKind::Str);
                self.heap.keep(Rc::clone(rc));
                Ok(a as i64)
            }
            // The interp models Bytes as List[Int]; an all-byte list stays the
            // byte block it was in slice 1 (the bytes.* domain). Any other
            // scalar list is a slot block.
            Value::List(rc) => {
                let bytes: Option<Vec<u8>> = rc
                    .iter()
                    .map(|v| match v {
                        Value::Int(i) if (0..=255).contains(i) => Some(*i as u8),
                        _ => None,
                    })
                    .collect();
                if let Some(b) = bytes {
                    let a = self.heap.bind(rc_key(rc), &b, BlockKind::Bytes);
                    self.heap.keep(Rc::clone(rc));
                    return Ok(a as i64);
                }
                let rc = Rc::clone(rc);
                let slots = heap_scalar_slots(&rc, "List")?;
                let a = self.heap.bind_slots(rc_key(&rc), &slots, rc.len() as u32);
                self.heap.keep(rc);
                Ok(a as i64)
            }
            Value::Set(rc) => {
                let rc = Rc::clone(rc);
                let slots = heap_scalar_slots(&rc, "Set")?;
                let a = self.heap.bind_slots(rc_key(&rc), &slots, rc.len() as u32);
                self.heap.keep(rc);
                Ok(a as i64)
            }
            // The `alloc_map` paired layout `[k0,v0,…]`, `len` = entry count.
            // Un-hinted, so Int/Bool only — see `heap_scalar_slots` for why a
            // Float's bits must not enter without a declared type to leave by.
            Value::Map(rc) => {
                let rc = Rc::clone(rc);
                let entries = rc.len() as u32;
                let mut slots = Vec::with_capacity(2 * rc.len());
                for (k, v) in rc.iter() {
                    for x in [k, v] {
                        slots.push(match x {
                            Value::Int(i) => *i,
                            Value::Bool(b) => *b as i64,
                            other => {
                                return Err(format!(
                                    "prim.handle of a Map holding a {} with no \
                                     declared type (its slot could not be typed back)",
                                    other.type_name()
                                ))
                            }
                        });
                    }
                }
                let a = self.heap.bind_slots(rc_key(&rc), &slots, entries);
                self.heap.keep(rc);
                Ok(a as i64)
            }
            other => Err(format!(
                "prim.handle of a {} (outside the slice-2 heap family)",
                other.type_name()
            )),
        }
    }

    /// `prim.alloc_str(n)` / `alloc_bytes(n)` — a zeroed byte block — and the
    /// slice-2 slot family (`alloc_list*` / `alloc_set*` / `alloc_map*` /
    /// `alloc_value`) — a zeroed block of n i64 slots. Returned as the
    /// ADDRESS; the body writes through it and returns the value;
    /// `sync_block_return` is the read-back.
    ///
    /// The size ceiling protects the INTERP process, not the program: the
    /// backends fail a huge allocation inside their own memory model, and this
    /// arena must abstain there rather than take the whole oracle down with a
    /// host OOM — an abstain is recorded, a dead process votes nothing.
    fn heap_prim_alloc(&mut self, func: &str, args: &[Value]) -> Option<Flow> {
        use crate::heap::BlockKind;
        let byte_kind = match func {
            "alloc_str" => Some(BlockKind::Str),
            "alloc_bytes" => Some(BlockKind::Bytes),
            _ => None,
        };
        let slot_family = matches!(
            func,
            "alloc_list"
                | "alloc_list_f64"
                | "alloc_set"
                | "alloc_map"
                | "alloc_list_str"
                | "alloc_set_str"
                | "alloc_map_str"
                | "alloc_map_skv"
                | "alloc_value"
        );
        if byte_kind.is_none() && !slot_family {
            return None;
        }
        let Some(Value::Int(n)) = args.first().filter(|v| matches!(v, Value::Int(i) if *i >= 0))
        else {
            return Some(Flow::Unsupported(format!("prim.{func} with a non-Int size")));
        };
        let bytes_wanted = if slot_family { n.checked_mul(8) } else { Some(*n) };
        if bytes_wanted.is_none_or(|b| b > 1 << 30) {
            return Some(Flow::Unsupported(format!(
                "prim.{func}({n}) beyond the interp arena ceiling"
            )));
        }
        Some(Flow::val(Value::Int(match byte_kind {
            Some(kind) => self.heap.alloc(*n as u32, kind) as i64,
            None => self.heap.alloc_slots(*n as u32) as i64,
        })))
    }

    /// `prim.load8` / `load32` / `load64`. An out-of-range address ABSTAINS:
    /// the two backends read real memory there, so a guessed 0 would be a wrong
    /// third vote on a program whose whole point is the byte it reads.
    fn heap_prim_load(&mut self, func: &str, args: &[Value]) -> Option<Flow> {
        let w = match func {
            "load8" => 1,
            "load32" => 4,
            "load64" => 8,
            _ => return None,
        };
        let Some(a) = heap_addr(args.first()) else {
            return Some(Flow::Unsupported(format!("prim.{func} with a non-address")));
        };
        Some(match self.heap.load(a, w) {
            Some(v) => Flow::val(Value::Int(v)),
            None => Flow::Unsupported(format!(
                "prim.{func} outside this heap's arena — the backends read real \
                 memory here, so a guessed value would be a wrong vote"
            )),
        })
    }

    /// `prim.store8` / `store32` / `store64`, little-endian like both backends.
    fn heap_prim_store(&mut self, func: &str, args: &[Value]) -> Option<Flow> {
        let w = match func {
            "store8" => 1,
            "store32" => 4,
            "store64" => 8,
            _ => return None,
        };
        let (Some(a), Some(Value::Int(v))) = (heap_addr(args.first()), args.get(1)) else {
            return Some(Flow::Unsupported(format!("prim.{func} with a non-address")));
        };
        Some(match self.heap.store(a, w, *v) {
            Some(()) => Flow::val(Value::Unit),
            None => Flow::Unsupported(format!("prim.{func} outside this heap's arena")),
        })
    }

    /// The slot-block element prims (#1226 slice 2): `store_str` (move a heap
    /// piece's handle into a slot), `load_str` / `load_handle` (borrow a slot's
    /// child back out), and the raw refcount pair `rc_inc` / `rc_dec`.
    fn heap_prim_slot_io(&mut self, func: &str, args: &[Value]) -> Option<Flow> {
        match func {
            "store_str" => {
                let Some(a) = heap_addr(args.first()) else {
                    return Some(Flow::Unsupported("prim.store_str with a non-address".into()));
                };
                let Some(piece) = args.get(1) else {
                    return Some(Flow::Unsupported("prim.store_str with no piece".into()));
                };
                Some(match self.heap_materialize(piece) {
                    Ok(h) => match self.heap.store(a, 8, h) {
                        Some(()) => Flow::val(Value::Unit),
                        None => {
                            Flow::Unsupported("prim.store_str outside this heap's arena".into())
                        }
                    },
                    Err(why) => Flow::Unsupported(why),
                })
            }
            "load_str" | "load_handle" => {
                let Some(a) = heap_addr(args.first()) else {
                    return Some(Flow::Unsupported(format!("prim.{func} with a non-address")));
                };
                let Some(h) = self.heap.load(a, 8) else {
                    return Some(Flow::Unsupported(format!(
                        "prim.{func} outside this heap's arena"
                    )));
                };
                Some(self.child_block_value(h, func))
            }
            // The WASI entropy exit, served with REAL per-process randomness:
            // the backends read OS entropy, and every fixture that passes the
            // 2-way byte-compare can only be asserting SHAPE (uniqueness
            // suffixes, draws-differ properties) — so any honest entropy
            // source is a faithful third vote, and a deterministic stand-in
            // would be the lie. Bytes land in the arena like any store.
            "random_get" => {
                let (Some(a), Some(Value::Int(len))) = (heap_addr(args.first()), args.get(1))
                else {
                    return Some(Flow::Unsupported("prim.random_get with a non-address".into()));
                };
                if *len < 0 {
                    return Some(Flow::Unsupported("prim.random_get with a negative len".into()));
                }
                use std::hash::{BuildHasher, Hasher};
                let mut word = 0u64;
                for i in 0..*len as u32 {
                    if i % 8 == 0 {
                        let mut h =
                            std::collections::hash_map::RandomState::new().build_hasher();
                        h.write_u32(i);
                        word = h.finish();
                    }
                    if self.heap.store(a + i, 1, (word & 0xff) as i64).is_none() {
                        return Some(Flow::Unsupported(
                            "prim.random_get outside this heap's arena".into(),
                        ));
                    }
                    word >>= 8;
                }
                Some(Flow::val(Value::Int(0)))
            }
            // Raw refcount adjust on a block base. The arena never FREES —
            // `keepalive` is its whole liveness model and an address must stay
            // valid for the run — so `rc_dec` to zero leaks by design: a leak
            // is invisible to the vote, a recycled address is not.
            "rc_inc" | "rc_dec" => {
                let Some(a) = heap_addr(args.first()) else {
                    return Some(Flow::Unsupported(format!("prim.{func} with a non-address")));
                };
                if self.heap.kind(a).is_none() {
                    return Some(Flow::Unsupported(format!(
                        "prim.{func} on an address that is not a block base"
                    )));
                }
                let rc = self.heap.load(a, 4).unwrap_or(0);
                let next = if func == "rc_inc" { rc + 1 } else { rc - 1 };
                self.heap.store(a, 4, next);
                Some(Flow::val(Value::Unit))
            }
            _ => None,
        }
    }

    /// A `Str`-block address as its `Value::Str` (adopted, so `prim.handle`
    /// answers the same block) — anything else unchanged. The `ConcatStr`
    /// coercion (see `apply_binop_concat`).
    pub(crate) fn coerce_block_str(&mut self, v: Value) -> Value {
        let Value::Int(i) = v else { return v };
        let Some(addr) = u32::try_from(i).ok() else { return v };
        if self.heap.kind(addr) == Some(crate::heap::BlockKind::Str) {
            if let Flow::Value(s) = self.child_block_value(i, "coerce") {
                return s;
            }
        }
        v
    }

    /// A list-block address as a native list (adopted) — `Bytes` as byte
    /// Ints, `Slots` as raw slot i64s (child addresses stay addresses; the
    /// exit sync types them). The `ConcatList` coercion.
    pub(crate) fn coerce_block_list(&mut self, v: Value) -> Value {
        use crate::heap::{rc_key, BlockKind};
        let Value::Int(i) = v else { return v };
        let Some(addr) = u32::try_from(i).ok() else { return v };
        match self.heap.kind(addr) {
            Some(BlockKind::Bytes) => match self.child_block_value(i, "coerce") {
                Flow::Value(l) => l,
                _ => v,
            },
            Some(BlockKind::Slots) => {
                let Some(n) = self.heap.block_len(addr) else { return v };
                let Some(slots) = (0..n)
                    .map(|k| self.heap.slot(addr, k).map(Value::Int))
                    .collect::<Option<Vec<_>>>()
                else {
                    return v;
                };
                let rc = Rc::new(slots);
                self.heap.adopt(rc_key(&rc), addr);
                self.heap.keep(Rc::clone(&rc));
                Value::List(rc)
            }
            _ => v,
        }
    }

    /// A slot's child, rebuilt as the `Value` its block kind spells — and
    /// RE-BOUND to that same address, so `prim.handle` on the borrow answers
    /// the child's own block rather than materializing a copy (aliasing).
    /// A `Slots` child stays an ADDRESS: the pool tier is i64-uniform and only
    /// a typed return boundary may rebuild a container.
    fn child_block_value(&mut self, h: i64, func: &str) -> Flow {
        use crate::heap::{rc_key, BlockKind};
        let Ok(addr) = u32::try_from(h) else {
            return Flow::Unsupported(format!("prim.{func} of a slot holding a negative value"));
        };
        match self.heap.kind(addr) {
            Some(BlockKind::Str) => {
                let Some((bytes, _)) = self.heap.block_bytes(addr) else {
                    return Flow::Unsupported(format!("prim.{func} of an unreadable Str block"));
                };
                let Ok(s) = String::from_utf8(bytes) else {
                    return Flow::Unsupported(format!(
                        "prim.{func} of a Str block holding non-UTF-8 bytes"
                    ));
                };
                let rc = Rc::new(s);
                self.heap.adopt(rc_key(&rc), addr);
                self.heap.keep(Rc::clone(&rc));
                Flow::val(Value::Str(rc))
            }
            Some(BlockKind::Bytes) => {
                let Some((bytes, _)) = self.heap.block_bytes(addr) else {
                    return Flow::Unsupported(format!("prim.{func} of an unreadable Bytes block"));
                };
                let rc: Rc<Vec<Value>> =
                    Rc::new(bytes.into_iter().map(|b| Value::Int(b as i64)).collect());
                self.heap.adopt(rc_key(&rc), addr);
                self.heap.keep(Rc::clone(&rc));
                Flow::val(Value::List(rc))
            }
            Some(BlockKind::Slots) => Flow::val(Value::Int(h)),
            None => Flow::Unsupported(format!(
                "prim.{func} of a slot that does not hold a block address"
            )),
        }
    }

    // ── FnRef ───────────────────────────────────────────────────

    /// A named function used as a value (`list.map(xs, double)`). We synthesize
    /// a closure value: there is no IR lambda, so we model it by a thin wrapper
    /// closure whose application re-dispatches to the named fn. Because the
    /// HOFs apply closures via `apply_closure`, we instead store the resolved
    /// IrFunction and special-case it — but `Closure` holds an IR body, so the
    /// simplest faithful model is to look up the fn and build a forwarding
    /// closure is not possible without an IR body. We therefore resolve a
    /// top-level fn into a closure over its own body + params.
    pub(crate) fn fn_ref_value(&mut self, name: Sym, _scope: &Scope) -> Flow {
        if let Some(func) = self.fns.get(&name).copied() {
            let params = func.params.iter().map(|p| p.var).collect();
            let clo = Closure {
                params,
                body: Rc::new(func.body.clone()),
                // A named fn closes only over top-level lets — the frame of
                // the SPACE its body's VarIds index (#1602: a module `__`
                // helper in the flat table captures its module's frame, not
                // the root's).
                captured: self.space_scope(self.fn_space_of(func)).clone(),
                // The fn's DECLARED return type rides along so the closure
                // boundary can run the same #1226 read-back a direct call
                // gets (value.as_array as an FnRef leaked raw addresses).
                ret_ty: Some(func.ret_ty.clone()),
            };
            return Flow::val(Value::Closure(Rc::new(clo)));
        }
        Flow::Unsupported(format!("fn-ref `{}`", name))
    }

    /// The shared global scope (top-level lets), the base every top-level fn
    /// call parents off. Seeded once by `run_main` / `ensure_globals`, so a
    /// global referenced from a nested call resolves correctly. Cheap to clone
    /// (Rc-shared).
    pub(crate) fn root_scope(&self) -> Scope {
        self.globals.clone()
    }
}

// ── Constructor registry ────────────────────────────────────────

/// A caller-side slot a `mut`-parameter argument names — where the copy-out
/// lands after the call (#1022).
#[derive(Clone, Copy)]
pub(crate) enum MutLvalue {
    Var(almide_ir::VarId),
    /// One-level record field (`push9(b.items, 7)`).
    Field(almide_ir::VarId, Sym),
}

#[derive(Clone, Copy)]
pub(crate) enum CtorKind {
    Unit,
    Tuple,
    Record,
}

impl<'a> Interpreter<'a> {
    /// Look up a variant constructor by name. Returns `(type_name,
    /// ctor_kind)`. Backed by the `variant_ctors` registry built once in
    /// `Interpreter::new` — this is on the hot path of every Named call,
    /// where it used to linearly rescan all type decls.
    pub(crate) fn variant_ctor(&self, name: Sym) -> Option<(Sym, CtorKind)> {
        self.variant_ctors.get(&name).copied()
    }
}

/// A heap prim's address argument: a non-negative Int, else `None` so the
/// caller abstains instead of reading somewhere arbitrary.
fn heap_addr(v: Option<&Value>) -> Option<u32> {
    match v {
        Some(Value::Int(i)) if *i >= 0 => u32::try_from(*i).ok(),
        _ => None,
    }
}

/// Container elements as raw slot i64s with NO declared element type in
/// sight: Int and Bool only. A Float would be stored as BITS and the resolved
/// body's (possibly mislabeling) declared return type is the only thing that
/// could type it back — exactly the leak the hinted path's strictness closes,
/// so the un-hinted path must not open it from the other side.
fn heap_scalar_slots(items: &[Value], shape: &str) -> Result<Vec<i64>, String> {
    items
        .iter()
        .map(|e| match e {
            Value::Int(i) => Ok(*i),
            Value::Bool(b) => Ok(*b as i64),
            other => Err(format!(
                "prim.handle of a {shape} holding a {} element with no \
                 declared element type (its slot could not be typed back)",
                other.type_name()
            )),
        })
        .collect()
}

fn heap_scalar_slot(e: &Value, shape: &str) -> Result<i64, String> {
    match e {
        Value::Int(i) => Ok(*i),
        // A NaN's BIT PATTERN is arch- and backend-conditional (#1403: x86
        // sign-set vs aarch64 canonical), and the slot impls compare raw
        // bits — the interp cannot know which pattern the backends hold.
        Value::Float(f) if f.is_nan() => Err(format!(
            "prim.handle of a {shape} holding a NaN float (NaN bits are \
             arch-conditional — #1403; a bit-compare vote would be a guess)"
        )),
        Value::Float(f) => Ok(f.to_bits() as i64),
        Value::Bool(b) => Ok(*b as i64),
        other => Err(format!(
            "prim.handle of a {shape} holding a {} element (the untyped pool \
             tier runs the scalar core impls, so only scalar slots are \
             faithful — #1226 slice 2)",
            other.type_name()
        )),
    }
}

/// Whether the DECLARED type of a returned block is one `rebuild_addr` can
/// spell IN FULL. A type outside this family that still received a block
/// address must abstain at the sync point: passing the raw address onward
/// hands native ops an Int where a container is expected (a wrong vote), and
/// rebuilding by guesswork is the same thing with extra steps.
fn heap_modeled_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId as C;
    match ty {
        Ty::String | Ty::Bytes => true,
        Ty::Applied(C::List | C::Set, ts) if ts.len() == 1 => heap_slot_ty(&ts[0]),
        Ty::Applied(C::Map, ts) if ts.len() == 2 => heap_slot_ty(&ts[0]) && heap_slot_ty(&ts[1]),
        Ty::Tuple(ts) => ts.iter().all(heap_slot_ty),
        _ => false,
    }
}

/// Whether a slot holding this DECLARED element/key/value type can be read
/// back faithfully: an inline scalar, a child block of a modeled type, or the
/// opaque dynamic `Value` (whose slots deliberately STAY addresses — the
/// i64-uniform tier). A type variable is none of these: the instantiation is
/// erased by the time the pool body returns, and Int-vs-String cannot be told
/// apart from the raw slot.
fn heap_slot_ty(ty: &Ty) -> bool {
    !heap_slot_is_child(ty) || heap_modeled_ty(ty) || matches!(ty, Ty::Named(n, args) if args.is_empty() && n.as_str() == "Value")
}

/// Whether a slot for this DECLARED element/key/value type holds a child
/// block address (heap) rather than an inline scalar — the same split the
/// `alloc_map` / `alloc_map_str` / `alloc_map_skv` builders make.
fn heap_slot_is_child(ty: &Ty) -> bool {
    !matches!(
        ty,
        Ty::Int
            | Ty::Int8
            | Ty::Int16
            | Ty::Int32
            | Ty::Int64
            | Ty::UInt8
            | Ty::UInt16
            | Ty::UInt32
            | Ty::UInt64
            | Ty::Float
            | Ty::Float32
            | Ty::Float64
            | Ty::Bool
    )
}

/// A compact spelling of a type for an abstain reason — the ledger keys on
/// these strings, so they must be stable and short, not `Debug`-shaped.
fn ty_short(ty: &Ty) -> String {
    use almide_lang::types::constructor::TypeConstructorId as C;
    match ty {
        Ty::String => "String".into(),
        Ty::Bytes => "Bytes".into(),
        Ty::Applied(C::List, ts) if ts.len() == 1 => format!("List[{}]", ty_short(&ts[0])),
        Ty::Applied(C::Set, ts) if ts.len() == 1 => format!("Set[{}]", ty_short(&ts[0])),
        Ty::Applied(C::Map, ts) if ts.len() == 2 => {
            format!("Map[{}, {}]", ty_short(&ts[0]), ty_short(&ts[1]))
        }
        Ty::Applied(C::Option, ts) if ts.len() == 1 => format!("{}?", ty_short(&ts[0])),
        Ty::Tuple(ts) => format!(
            "({})",
            ts.iter().map(ty_short).collect::<Vec<_>>().join(", ")
        ),
        Ty::Int => "Int".into(),
        Ty::Float => "Float".into(),
        Ty::Bool => "Bool".into(),
        other => format!("{other:?}").split('(').next().unwrap_or("?").to_string(),
    }
}

/// Infer the dispatch module for a residual UFCS `Method` receiver.
fn infer_module_for(v: &Value) -> Sym {
    let m = match v {
        Value::Str(_) => "string",
        Value::Int(_) => "int",
        Value::Float(_) => "float",
        Value::List(_) | Value::Range { .. } => "list",
        Value::Map(_) => "map",
        Value::Set(_) => "set",
        Value::Option(_) => "option",
        Value::Result(_) => "result",
        _ => "value",
    };
    almide_base::intern::sym(m)
}

// ── HOF registry ────────────────────────────────────────────────

/// Is `(module, func)` an in-interp higher-order function (takes a closure
/// argument)? Mirrors the runtime `Rc<dyn Fn>`-taking surface. The list is the
/// design's verified ~45 HOFs.
pub(crate) fn is_hof(module: &str, func: &str) -> bool {
    matches!(
        (module, func),
        ("list", "map")
            | ("list", "filter")
            | ("list", "find")
            | ("list", "any")
            | ("list", "all")
            | ("list", "count")
            | ("list", "flat_map")
            | ("list", "filter_map")
            | ("list", "fold")
            | ("list", "reduce")
            | ("list", "sort_by")
            | ("list", "take_while")
            | ("list", "drop_while")
            | ("list", "partition")
            | ("list", "group_by")
            | ("list", "find_index")
            | ("list", "update")
            | ("list", "scan")
            | ("list", "zip_with")
            | ("list", "unique_by")
            | ("list", "each")
            // The `__fallible_*` carriers (ADR-0006): what the checker instantiates
            // in place of the plain name above when the callback propagates
            // with `!`. They take a closure exactly like their siblings, so
            // they belong on this allowlist — omitting them made the whole
            // family fall through to `Unsupported`, which silently removed the
            // third oracle from the DEFAULT way to write a fallible traversal.
            // Bodies: `hofs.rs::eval_hof_list_try`.
            | ("list", "__fallible_map")
            | ("list", "__fallible_filter")
            | ("list", "__fallible_filter_map")
            | ("list", "__fallible_flat_map")
            | ("list", "__fallible_find")
            | ("list", "__fallible_fold")
            | ("list", "__fallible_each")
            | ("map", "map")
            | ("map", "filter")
            | ("map", "fold")
            | ("map", "any")
            | ("map", "all")
            | ("map", "count")
            | ("map", "find")
            | ("map", "update")
            | ("map", "upsert")
            | ("set", "filter")
            | ("set", "map")
            | ("set", "fold")
            | ("set", "any")
            | ("set", "all")
            | ("option", "map")
            | ("option", "flat_map")
            | ("option", "unwrap_or_else")
            | ("option", "filter")
            | ("option", "or_else")
            | ("result", "map")
            | ("result", "map_err")
            | ("result", "flat_map")
            | ("result", "unwrap_or_else")
            | ("result", "or_else")
            | ("result", "filter")
            | ("bytes", "map_each")
    )
}
