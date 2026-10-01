//! A call into another module that leaves out defaulted parameters gets those
//! defaults written into the call, as named arguments, before the call is
//! checked — so they are type-checked where they are filled, in the caller.
//!
//! A default is written in the callee's module. Filled only at lowering, its
//! expression was a clone of the callee's parse: its node ids index the
//! callee's program, and every type lowering read for it was whatever node of
//! THIS program shared the id. `retype_filled_default` could correct the
//! default's outermost type from the declared parameter, never what is inside
//! it: `fallback: Wrapper = Wrap(7)` lowered its `7` as a `String`. The native
//! leg rendered it regardless; the MIR lowering, which trusts the types, could
//! not, and walled the calling function.
//!
//! Filled here, the default is the expression the caller would have written —
//! the callee's own names qualified by the caller's import alias
//! (`surface.Top`), an earlier parameter's reference replaced by that
//! parameter's argument (#664) — with fresh ids from the checker's own range,
//! so the checker types every node of it in the caller's type map. Lowering
//! then finds every parameter given and fills nothing.
//!
//! Calls into the bundled stdlib keep the lowering fill: its defaults are
//! checked with the stdlib, and its calls are everywhere.
//!
//! A record literal is the same shape (#3165): `shapes.Box { n: 1 }` over
//! `type Box = { n: Int, e: Int = K }` had its omitted `e` spliced in at
//! codegen as the IR the DECLARING module lowered, whose `K` is a `VarId` of
//! that module's var table — in the caller's table the same id is some
//! unrelated local (native: `e: b`, rustc E0425 or a silently different
//! value; wasm: a type-mismatch wall). Written in here, qualified, the
//! default is lowered by the caller like any field it wrote, and no backend
//! splices another module's IR.

use std::collections::HashMap;

use almide_base::intern::{Sym, sym};
use almide_lang::ast::{self, ExprKind};

use super::Checker;

impl Checker {
    pub(crate) fn fill_cross_module_defaults(
        &mut self,
        callee: &ast::Expr,
        args: &[ast::Expr],
        named_args: &mut Vec<(Sym, ast::Expr)>,
    ) {
        let ExprKind::Member { object, field, .. } = &callee.kind else { return };
        let ExprKind::Ident { name: alias, .. } = &object.kind else { return };
        // `record.method(...)` on a value is a field call, not a module's fn.
        if self.env.lookup_var(alias).is_some() {
            return;
        }
        let Some(module) = self.env.import_table.resolve(alias.as_str()) else { return };
        if almide_lang::stdlib_info::is_bundled_module(module.as_str()) {
            return;
        }
        let key = sym(&format!("{}.{}", module, field));
        let Some(defaults) = self.env.fn_defaults.get(&key).cloned() else { return };
        let Some(sig) = self.env.functions.get(&key).cloned() else { return };
        let params: Vec<Sym> = sig.params.iter().map(|(n, _)| sym(&n.to_string())).collect();
        if args.len() > params.len() {
            return;
        }
        // What each parameter is given at this call, for a default that names
        // an earlier one (`fn rect(w: Int, h: Int = w)`).
        let mut given: HashMap<Sym, ast::Expr> = args.iter().zip(&params).map(|(a, p)| (*p, a.clone())).collect();
        given.extend(named_args.iter().map(|(n, e)| (*n, e.clone())));
        for (j, param) in params.iter().enumerate().skip(args.len()) {
            if named_args.iter().any(|(n, _)| n == param) {
                continue;
            }
            // No default: the missing argument is the arity check's to report.
            let Some(Some(default)) = defaults.get(j) else { continue };
            let mut filled = default.clone();
            crate::lower::qualify_callee_module_idents(&mut filled, module, &self.env);
            crate::lower::substitute_call_params(&mut filled, &given);
            self.renumber_synthesized(&mut filled);
            given.insert(*param, filled.clone());
            named_args.push((*param, filled));
        }
        // In parameter order, the order lowering places and evaluates them in:
        // the named-argument check lines the appended slots up with the
        // parameters they name, and a filled default must not land between a
        // written argument and its own slot.
        named_args.sort_by_key(|(n, _)| params.iter().position(|p| p == n).unwrap_or(usize::MAX));
    }

    /// Write the defaults of a record literal's omitted fields into the
    /// literal when its type is declared in ANOTHER user module (#3165).
    pub(crate) fn fill_cross_module_field_defaults(&mut self, n: &Sym, fields: &mut Vec<ast::FieldInit>) {
        let Some(key) = self.record_literal_defaults_key(n) else { return };
        let Some((module, defaults)) = self.env.field_default_exprs.get(&key).cloned() else { return };
        // The declaring module's own literal lowers its default in its own
        // scope; only a crossing needs it re-qualified.
        if self.current_module_prefix.as_deref() == Some(module.as_str()) {
            return;
        }
        for (field, default) in defaults {
            if fields.iter().any(|f| f.name == field) {
                continue;
            }
            let mut filled = default;
            crate::lower::qualify_callee_module_idents(&mut filled, module, &self.env);
            self.renumber_synthesized(&mut filled);
            fields.push(ast::FieldInit { name: field, value: filled });
        }
    }

    /// The `field_default_exprs` key of the type a named record literal
    /// builds — resolved the way `infer_expr_record_named` resolves it: a
    /// record-payload case first (`mod.Type.Case`), else a named record. A
    /// qualified head names a case of that module alone (#3176).
    fn record_literal_defaults_key(&self, n: &Sym) -> Option<Sym> {
        let ctor_sym = n.rsplit_once('.').map(|(_, b)| sym(b)).unwrap_or(*n);
        if let Some((type_name, _)) = self.env.lookup_ctor_written(n.as_str(), self.current_module_prefix.as_deref()) {
            let ty = self.record_case_type_name(n, type_name);
            return Some(sym(&format!("{}.{}", ty, ctor_sym)));
        }
        Some(self.record_type_canon(n))
    }

    /// Give every node of `expr` a fresh id from the checker's own range.
    fn renumber_synthesized(&mut self, expr: &mut ast::Expr) {
        let mut next = self.next_synth_expr_id;
        ast::visit_expr_mut(expr, &mut |e| {
            e.id = ast::ExprId(next);
            next += 1;
        });
        self.next_synth_expr_id = next;
    }
}
