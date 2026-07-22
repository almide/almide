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

/// Like `val!`, but for the `Option<Flow>` return type of `eval_builtin_call`
/// (a name-router phase that returns `None` to fall through to the next
/// phase, so a non-Value `Flow` must be wrapped in `Some` to short-circuit).
macro_rules! opt_val {
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

        // 3. A user / stdlib free function lowered into the program.
        if let Some(func) = self.fns.get(&name).copied() {
            let mut evaled = Vec::with_capacity(args.len());
            for a in args {
                evaled.push(val!(self.eval_expr(a, scope)));
            }
            let root = self.root_scope();
            return self.call_function(func, evaled, &root);
        }

        Flow::Unsupported(format!("named call `{}`", n))
    }

    /// The fixed-name builtins (`println`/`assert`/`panic`/…). Returns `None`
    /// when `n` isn't one of them, so `eval_named_call` falls through to the
    /// variant-ctor / user-fn phases.
    fn eval_builtin_call(&mut self, n: &str, args: &[IrExpr], scope: &Scope) -> Option<Flow> {
        match n {
            "println" | "print" => {
                let mut evaled = Vec::with_capacity(args.len());
                for a in args {
                    evaled.push(opt_val!(self.eval_expr(a, scope)));
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
                    evaled.push(opt_val!(self.eval_expr(a, scope)));
                }
                let line = evaled.first().map(|v| v.display_bare()).unwrap_or_default();
                self.stderr.push_str(&line);
                if n == "eprintln" {
                    self.stderr.push('\n');
                }
                Some(Flow::val(Value::Unit))
            }
            "assert" => {
                let v = opt_val!(self.eval_expr(&args[0], scope));
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
                let a = opt_val!(self.eval_expr(&args[0], scope));
                let b = opt_val!(self.eval_expr(&args[1], scope));
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
                    Some(a) => opt_val!(self.eval_expr(a, scope)).display_bare(),
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

        // Otherwise evaluate all args eagerly, then dispatch.
        let mut evaled = Vec::with_capacity(args.len());
        for a in args {
            evaled.push(val!(self.eval_expr(a, scope)));
        }
        self.dispatch_module_resolved(module, func, evaled)
    }

    /// Dispatch a `(module, func)` whose args are already evaluated. Tiers:
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
        if let Some(func_def) = self.module_fns.get(&(module, func)).copied() {
            // Only interpret if it has a real (non-Hole) body.
            if !matches!(func_def.body.kind, almide_ir::IrExprKind::Hole) {
                let root = self.root_scope();
                return self.call_function(func_def, args, &root);
            }
        }
        // A top-level fn named exactly `func` (some stdlib helpers flatten).
        if let Some(func_def) = self.fns.get(&func).copied() {
            if !matches!(func_def.body.kind, almide_ir::IrExprKind::Hole) {
                let root = self.root_scope();
                return self.call_function(func_def, args, &root);
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
