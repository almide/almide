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
use almide_ir::{CallTarget, IrExpr};

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

        // 2. Variant constructor (Unit / Tuple). Record-variant ctors arrive
        //    as `Record` nodes, handled in eval. Look up in the registry.
        if let Some((ty_name, kind)) = self.variant_ctor(name) {
            return match kind {
                CtorKind::Unit => Flow::val(Value::Variant {
                    ty: Some(ty_name),
                    ctor: name,
                    payload: VariantPayload::Unit,
                }),
                CtorKind::Tuple => {
                    let mut evaled = Vec::with_capacity(args.len());
                    for a in args {
                        evaled.push(val!(self.eval_expr(a, scope)));
                    }
                    Flow::val(Value::Variant {
                        ty: Some(ty_name),
                        ctor: name,
                        payload: VariantPayload::Tuple(evaled),
                    })
                }
                CtorKind::Record => {
                    // Should not arrive as a Named call, but handle defensively.
                    Flow::Unsupported(format!("record-variant ctor call {}", n))
                }
            };
        }

        // 2b. The stdlib's only BUNDLED variant type (bytes.Endian): its decl
        //     lives in the bundled module, never in the program, so the ctor
        //     registry above misses it. The checker already typed the ctor —
        //     build the variant value directly (the same value the inplace
        //     tier's `endian_is_big` and the bytes read bridge dispatch on).
        if args.is_empty() && matches!(n, "LittleEndian" | "BigEndian") {
            return Flow::val(Value::Variant {
                ty: None,
                ctor: name,
                payload: VariantPayload::Unit,
            });
        }

        // 3. A user / stdlib free function lowered into the program.
        if let Some(func) = self.fns.get(&name).copied() {
            // #1022: mut-parameter copy-in/copy-out. The backends' lowering
            // returns each `mut` param's final buffer and writes it back at
            // EVERY call position (C-132) — the interp mirrors that by keeping
            // the callee frame alive and assigning each recorded caller lvalue
            // from the param's final value. Recorded BEFORE evaluation, while
            // the argument is still an expression with a binding identity.
            let writebacks = match self.mut_param_lvalues(func, args) {
                Ok(wb) => wb,
                Err(flow) => return flow,
            };
            let mut evaled = Vec::with_capacity(args.len());
            for a in args {
                evaled.push(val!(self.eval_expr(a, scope)));
            }
            let root = self.root_scope();
            let (flow, frame) = self.call_function_keeping_frame(func, evaled, &root);
            // Copy-out only on a normal return — an abort/abstain never
            // half-writes state the backends would not have written either.
            if matches!(flow, Flow::Value(_)) {
                for (idx, lv) in writebacks {
                    let Some(final_v) = frame.get(func.params[idx].var) else { continue };
                    if let Err(e) = self.write_mut_lvalue(lv, final_v, scope) {
                        return e;
                    }
                }
            }
            return flow;
        }

        Flow::Unsupported(format!("named call `{}`", n))
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
            if !param.is_mut {
                continue;
            }
            match &arg.kind {
                almide_ir::IrExprKind::Var { id } => out.push((i, MutLvalue::Var(*id))),
                almide_ir::IrExprKind::Member { object, field } => match &object.kind {
                    almide_ir::IrExprKind::Var { id } => {
                        out.push((i, MutLvalue::Field(*id, *field)))
                    }
                    _ => {
                        return Err(Flow::Unsupported(format!(
                            "mut-parameter argument through a nested lvalue \
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
                        "mut-parameter argument through an index lvalue \
                         (`{}` param {i}) — not yet copied out (#1022)",
                        func.name.as_str()
                    )))
                }
                // Unreachable in a checked program (E032 forbids a temporary
                // to a `mut` param) — defensively skip rather than guess.
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
    fn eval_builtin_call(&mut self, n: &str, args: &[IrExpr], scope: &Scope) -> Option<Flow> {
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

        // Otherwise evaluate all args eagerly, then dispatch.
        let mut evaled = Vec::with_capacity(args.len());
        for a in args {
            evaled.push(val!(self.eval_expr(a, scope)));
        }
        self.dispatch_module_resolved(module, func, evaled)
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
            return Flow::Unsupported(format!(
                "in-place container mutation `{m}.{f}` through a non-variable receiver \
                 (a record field / index / temporary has no single binding to write back to)"
            ));
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
                let units = int0() / almide_lang::time_units::CM1_NS_PER_CHARGE;
                self.det_entry.set(units);
                let saved = self.det_fuel.get();
                if units < saved {
                    self.det_fuel.set(units);
                }
                self.det_region_depth.set(self.det_region_depth.get() + 1);
                Flow::val(Value::Int(saved))
            }
            "almide_rt_prim_budget_exhausted" => Flow::val(Value::Int(self.det_verdict.get())),
            "almide_rt_prim_budget_exit" => {
                self.det_verdict.set(i64::from(self.det_fuel.get() < 0));
                let consumed = self.det_entry.get() - self.det_fuel.get();
                self.det_spend.set(consumed);
                self.det_fuel.set(int0() - consumed);
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

        // Scalar / string / math native bridge (intrinsic-symbol surface).
        if let Some(result) = crate::bridge::dispatch(module.as_str(), func.as_str(), &args) {
            return result;
        }

        // An almide-bodied stdlib fn lowered into the program (pre-ir_link it
        // lives under program.modules; some helpers are top-level fns).
        //
        // These sites receive EAGERLY-evaluated args, so a `mut` parameter's
        // caller lvalue is already gone — the Named path's copy-out (#1022)
        // cannot run here. A mut-param callee must abstain rather than
        // silently drop the write-back (a wrong third vote).
        let mut_param_gate = |func_def: &almide_ir::IrFunction| -> Option<Flow> {
            func_def.params.iter().any(|p| p.is_mut).then(|| {
                Flow::Unsupported(format!(
                    "module call `{}.{}` with a `mut` parameter through the \
                     eager dispatch path (no caller lvalue to copy out — #1022)",
                    module.as_str(),
                    func.as_str()
                ))
            })
        };
        if let Some(func_def) = self.module_fns.get(&(module, func)).copied() {
            // Only interpret if it has a real (non-Hole) body.
            if !matches!(func_def.body.kind, almide_ir::IrExprKind::Hole) {
                if let Some(flow) = mut_param_gate(func_def) {
                    return flow;
                }
                let root = self.root_scope();
                return self.call_function(func_def, args, &root);
            }
        }
        // A top-level fn named exactly `func` (some stdlib helpers flatten).
        if let Some(func_def) = self.fns.get(&func).copied() {
            if !matches!(func_def.body.kind, almide_ir::IrExprKind::Hole) {
                if let Some(flow) = mut_param_gate(func_def) {
                    return flow;
                }
                let root = self.root_scope();
                return self.call_function(func_def, args, &root);
            }
        }

        // The self-hosted stdlib body from the shared registry (stdlib_pool):
        // the SAME source the wasm leg links for this call name, lowered once and
        // layered into `self.fns` at construction. Consulted LAST so the
        // interp-native surfaces above keep their vote provenance; what the body
        // itself cannot evaluate (a heap/effect prim outside the scalar floor)
        // abstains from inside with that prim named — a skip, never a guess.
        if let Some(impl_name) = crate::stdlib_pool::impl_fn(module, func) {
            if let Some(func_def) = self.fns.get(&impl_name).copied() {
                if !matches!(func_def.body.kind, almide_ir::IrExprKind::Hole) {
                    let root = self.root_scope();
                    return self.call_function(func_def, args, &root);
                }
            }
        }

        Flow::Unsupported(format!("{}.{}", module, func))
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
                // Top-level fn closes only over top-level lets, modeled by the
                // root scope.
                captured: self.root_scope(),
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
    /// Look up a variant constructor by name in the program's type decls.
    /// Returns `(type_name, ctor_kind)`.
    pub(crate) fn variant_ctor(&self, name: Sym) -> Option<(Sym, CtorKind)> {
        use almide_ir::{IrTypeDeclKind, IrVariantKind};
        for td in &self.program.type_decls {
            if let IrTypeDeclKind::Variant { cases, .. } = &td.kind {
                for case in cases {
                    if case.name == name {
                        let kind = match case.kind {
                            IrVariantKind::Unit => CtorKind::Unit,
                            IrVariantKind::Tuple { .. } => CtorKind::Tuple,
                            IrVariantKind::Record { .. } => CtorKind::Record,
                        };
                        return Some((td.name, kind));
                    }
                }
            }
        }
        None
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
            | ("map", "map")
            | ("map", "filter")
            | ("map", "fold")
            | ("map", "any")
            | ("map", "all")
            | ("map", "count")
            | ("map", "find")
            | ("map", "update")
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
            | ("bytes", "map_each")
    )
}
