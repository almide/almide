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

include!("dispatch_module.rs");
include!("dispatch_sync.rs");
include!("dispatch_heap.rs");

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
