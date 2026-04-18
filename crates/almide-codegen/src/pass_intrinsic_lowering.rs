//! IntrinsicLoweringPass: rewrite `CallTarget::Module { m, f }` calls
//! targeting an `@intrinsic(symbol)`-annotated stdlib fn into
//! `IrExprKind::RuntimeCall { symbol, args }`.
//!
//! This is Phase 1e-2 of the dispatch unification arc
//! (`docs/roadmap/active/dispatch-unification-plan.md`). Starting here,
//! downstream emit (Rust walker, WASM emitter) can consume a single
//! target-neutral IR node for runtime fn calls; the per-target
//! `pass_stdlib_lowering` / `emit_<m>_call` paths remain for `@inline_rust`
//! and L2-L3 dispatchers that have not yet migrated.
//!
//! The pass reads `@intrinsic("almide_rt_...")` attributes from
//! `program.modules[*].functions[*].attrs` and builds a
//! `(module, func) → symbol` map. Its `IrMutVisitor` then rewrites every
//! matching call site across top-level fns, top-lets, and nested module fns.
//!
//! Ordering: runs on both Rust and WASM targets. Must execute before
//! `StdlibLoweringPass` so that Rust-target code sees the already-rewritten
//! `RuntimeCall` node and does NOT emit an `InlineRust` template for the
//! same call. Also before `ResolveCalls` to avoid the bundled → Named
//! rewrite competing with this rewrite.

use std::collections::{HashMap, HashSet};
use almide_base::intern::{Sym, sym};
use almide_ir::*;
use almide_ir::visit_mut::{IrMutVisitor, walk_expr_mut, walk_stmt_mut};
use almide_lang::types::{Ty, TypeConstructorId};
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct IntrinsicLoweringPass;

impl NanoPass for IntrinsicLoweringPass {
    fn name(&self) -> &str { "IntrinsicLowering" }

    fn targets(&self) -> Option<Vec<Target>> {
        // Both targets: the point of this arc is a single lowering site.
        None
    }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let map = collect_intrinsics(&program);
        if map.is_empty() {
            return PassResult { program, changed: false };
        }
        let symbols: HashSet<Sym> = map.values().copied().collect();

        struct Rewriter<'a> {
            map: &'a HashMap<(Sym, Sym), Sym>,
            symbols: &'a HashSet<Sym>,
        }
        impl<'a> IrMutVisitor for Rewriter<'a> {
            fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
                walk_expr_mut(self, expr);
                let IrExprKind::Call { target, args, .. } = &mut expr.kind else { return };
                match target {
                    CallTarget::Module { module, func } => {
                        let Some(&symbol) = self.map.get(&(*module, *func)) else { return };
                        let args = std::mem::take(args);
                        expr.kind = IrExprKind::RuntimeCall { symbol, args };
                    }
                    CallTarget::Named { name } => {
                        // Frontend may have pre-lowered `int.parse(s)` to
                        // `Named { "almide_rt_int_parse" }` before this pass
                        // runs. If the symbol matches one declared via
                        // `@intrinsic(...)`, take ownership and rewrite to
                        // RuntimeCall so downstream emit paths converge.
                        if !self.symbols.contains(name) { return; }
                        let symbol = *name;
                        let args = std::mem::take(args);
                        expr.kind = IrExprKind::RuntimeCall { symbol, args };
                    }
                    CallTarget::Method { object, method } => {
                        // UFCS: `obj.method(args)`. Method name may arrive
                        // either as a bare `"to_string"` (resolve module
                        // from `object.ty`) or as a prefixed `"int.to_string"`
                        // (module is explicit) — the frontend emits the
                        // prefixed form for bundled-stdlib UFCS. On hit,
                        // prepend `obj` to the arg list and rewrite to
                        // RuntimeCall.
                        let method_str = method.as_str();
                        let (module, func) = if let Some(dot) = method_str.find('.') {
                            (sym(&method_str[..dot]), sym(&method_str[dot + 1..]))
                        } else {
                            let Some(m) = module_for_ty(&object.ty) else { return };
                            (m, *method)
                        };
                        let Some(&symbol) = self.map.get(&(module, func)) else { return };
                        let obj = std::mem::replace(object.as_mut(), IrExpr::default());
                        let mut new_args = Vec::with_capacity(args.len() + 1);
                        new_args.push(obj);
                        new_args.extend(std::mem::take(args));
                        expr.kind = IrExprKind::RuntimeCall { symbol, args: new_args };
                    }
                    _ => {}
                }
            }
            fn visit_stmt_mut(&mut self, stmt: &mut IrStmt) {
                walk_stmt_mut(self, stmt);
            }
        }

        let mut rw = Rewriter { map: &map, symbols: &symbols };
        for func in &mut program.functions {
            rw.visit_expr_mut(&mut func.body);
        }
        for tl in &mut program.top_lets {
            rw.visit_expr_mut(&mut tl.value);
        }
        for mi in 0..program.modules.len() {
            for fi in 0..program.modules[mi].functions.len() {
                let mut body = std::mem::replace(
                    &mut program.modules[mi].functions[fi].body,
                    IrExpr::default(),
                );
                rw.visit_expr_mut(&mut body);
                program.modules[mi].functions[fi].body = body;
            }
            for ti in 0..program.modules[mi].top_lets.len() {
                let mut val = std::mem::replace(
                    &mut program.modules[mi].top_lets[ti].value,
                    IrExpr::default(),
                );
                rw.visit_expr_mut(&mut val);
                program.modules[mi].top_lets[ti].value = val;
            }
        }
        PassResult { program, changed: true }
    }
}

/// Map a UFCS receiver type to the stdlib module name that owns
/// methods for it. Used to resolve `Method` call targets (`42.to_string()`)
/// to their stdlib module (`int`) so the intrinsic map can be queried.
fn module_for_ty(ty: &Ty) -> Option<Sym> {
    let name = match ty {
        Ty::Int => "int",
        Ty::Int8 => "int8",
        Ty::Int16 => "int16",
        Ty::Int32 => "int32",
        Ty::UInt8 => "uint8",
        Ty::UInt16 => "uint16",
        Ty::UInt32 => "uint32",
        Ty::UInt64 => "uint64",
        Ty::Float => "float",
        Ty::Float32 => "float32",
        Ty::String => "string",
        Ty::Bool => "bool",
        Ty::Bytes => "bytes",
        Ty::Applied(tc, _) => match tc {
            TypeConstructorId::List => "list",
            TypeConstructorId::Map => "map",
            TypeConstructorId::Set => "set",
            TypeConstructorId::Option => "option",
            TypeConstructorId::Result => "result",
            _ => return None,
        },
        _ => return None,
    };
    Some(sym(name))
}

/// Collect every `(module, func) → runtime_symbol` declared via
/// `@intrinsic("symbol")` across bundled stdlib / user modules.
fn collect_intrinsics(program: &IrProgram) -> HashMap<(Sym, Sym), Sym> {
    use almide_lang::ast::AttrValue;
    use almide_base::intern::sym;

    let mut out = HashMap::new();
    for module in &program.modules {
        for func in &module.functions {
            let Some(attr) = func.attrs.iter().find(|a| a.name.as_str() == "intrinsic") else {
                continue;
            };
            let Some(first) = attr.args.first() else { continue };
            let AttrValue::String { value } = &first.value else { continue };
            out.insert((module.name, func.name), sym(value));
        }
    }
    out
}
