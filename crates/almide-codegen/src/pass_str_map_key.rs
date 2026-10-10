//! StrMapKeyPass: a String-keyed insert-or-update whose key is an owned copy
//! of a borrowed string (`k.to_string()`) takes the key borrowed, and copies
//! it only when the key is new.
//!
//! Target: Rust only. Runs last, on the final IR.
//!
//! `map.set(m, k, v)` and `map.upsert(m, k, init, f)` consume their key, so
//! a key read from a `&str` — a borrow-inferred param, a `split_once` slice
//! (`SliceBinders`) — reaches them as `k.to_string()`: one allocation per
//! call, freed again whenever the key is already present, which is every call
//! but the first in a counting or aggregation loop. The `_str` twins take
//! `&str` and copy it on insertion only. Same entries, same order, same
//! values: only the number of copies changes.

use almide_base::intern::sym;
use almide_ir::*;
use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
use almide_lang::types::Ty;
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct StrMapKeyPass;

impl NanoPass for StrMapKeyPass {
    fn name(&self) -> &str { "StrMapKey" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Rust]) }
    /// Reads the `.to_string()` BorrowLowering and SliceBinders spell.
    fn depends_on(&self) -> Vec<&'static str> { vec!["BorrowLowering", "SliceBinders"] }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut v = Rewriter { changed: false };
        for func in &mut program.functions {
            v.visit_expr_mut(&mut func.body);
        }
        for tl in &mut program.top_lets {
            v.visit_expr_mut(&mut tl.value);
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                v.visit_expr_mut(&mut func.body);
            }
            for tl in &mut module.top_lets {
                v.visit_expr_mut(&mut tl.value);
            }
        }
        PassResult { program, changed: v.changed }
    }
}

/// The borrowed-key twin of a consuming insert-or-update (key = arg 1).
fn str_key_twin(symbol: &str) -> Option<&'static str> {
    match symbol {
        "almide_rt_map_set" => Some("almide_rt_map_set_str"),
        "almide_rt_map_upsert_fn" => Some("almide_rt_map_upsert_str_fn"),
        _ => None,
    }
}

/// `v.to_string()` of a String-typed variable: the variable itself.
fn owned_copy_of_var(e: &IrExpr) -> Option<&IrExpr> {
    let IrExprKind::Call { target: CallTarget::Method { object, method }, args, .. } = &e.kind else { return None };
    let is_copy = method.as_str() == "to_string" && args.is_empty();
    (is_copy && matches!(object.kind, IrExprKind::Var { .. }) && matches!(object.ty, Ty::String)).then_some(&**object)
}

struct Rewriter {
    changed: bool,
}

impl IrMutVisitor for Rewriter {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        walk_expr_mut(self, e);
        let IrExprKind::RuntimeCall { symbol, args } = &mut e.kind else { return };
        let Some(twin) = str_key_twin(symbol.as_str()) else { return };
        let Some(key) = args.get_mut(1) else { return };
        let Some(var) = owned_copy_of_var(key).cloned() else { return };
        let span = key.span;
        *key = IrExpr {
            kind: IrExprKind::Borrow { expr: Box::new(var), as_str: true, mutable: false },
            ty: Ty::String,
            span,
            def_id: None,
        };
        *symbol = sym(twin);
        self.changed = true;
    }
}
