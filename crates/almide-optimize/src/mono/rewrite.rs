use std::collections::HashMap;
use std::collections::BTreeMap;
use almide_ir::*;
use almide_ir::visit_mut::{IrMutVisitor, walk_expr_mut, walk_stmt_mut};
use almide_lang::types::Ty;
use super::utils::{MonoKey, BoundedParam, mangle_suffix};
use super::discovery::{collect_mono_bindings, extract_typevar_binding};
use super::specialization::substitute_ty;

/// Rewrite call sites to point to specialized functions.
pub(super) fn rewrite_calls(
    program: &mut IrProgram,
    bound_fns: &HashMap<String, Vec<BoundedParam>>,
    instances: &BTreeMap<MonoKey, HashMap<String, Ty>>,
) {
    let fn_param_types: HashMap<String, Vec<Ty>> = program.functions.iter()
        .filter(|f| !f.is_test && bound_fns.contains_key::<str>(&f.name))
        .map(|f| (f.name.to_string(), f.params.iter().map(|p| p.ty.clone()).collect()))
        .collect();
    let fn_generics: HashMap<String, Vec<String>> = program.functions.iter()
        .filter(|f| !f.is_test && bound_fns.contains_key::<str>(&f.name))
        .filter_map(|f| f.generics.as_ref().map(|gs| (f.name.to_string(), gs.iter().map(|g| g.name.to_string()).collect())))
        .collect();
    let fn_ret_types: HashMap<String, Ty> = program.functions.iter()
        .filter(|f| !f.is_test && bound_fns.contains_key::<str>(&f.name))
        .map(|f| (f.name.to_string(), f.ret_ty.clone()))
        .collect();

    let mut rw = RewriteVisitor {
        bound_fns,
        instances,
        fn_param_types: &fn_param_types,
        fn_generics: &fn_generics,
        fn_ret_types: &fn_ret_types,
    };
    for func in &mut program.functions {
        rw.visit_expr_mut(&mut func.body);
    }
    for tl in &mut program.top_lets {
        rw.visit_expr_mut(&mut tl.value);
    }
}

// ── Visitor implementation ────────────────────────────────────────

struct RewriteVisitor<'a> {
    bound_fns: &'a HashMap<String, Vec<BoundedParam>>,
    instances: &'a BTreeMap<MonoKey, HashMap<String, Ty>>,
    fn_param_types: &'a HashMap<String, Vec<Ty>>,
    fn_generics: &'a HashMap<String, Vec<String>>,
    fn_ret_types: &'a HashMap<String, Ty>,
}

impl<'a> IrMutVisitor for RewriteVisitor<'a> {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        // Recurse into children first (bottom-up rewriting)
        walk_expr_mut(self, expr);
        // Then rewrite call targets at this node
        if let IrExprKind::Call { target, args, type_args } = &mut expr.kind {
            self.rewrite_call(expr.ty.clone(), target, args, type_args, &mut expr.ty);
        }
    }
    fn visit_stmt_mut(&mut self, stmt: &mut IrStmt) {
        walk_stmt_mut(self, stmt);
    }
}

impl<'a> RewriteVisitor<'a> {
    fn rewrite_call(
        &self,
        _orig_ty: Ty,
        target: &mut CallTarget,
        args: &mut Vec<IrExpr>,
        type_args: &[Ty],
        expr_ty: &mut Ty,
    ) {
        match target {
            CallTarget::Named { .. } => self.rewrite_named_call(target, args, type_args, expr_ty),
            CallTarget::Method { .. } => self.rewrite_method_call(target, args, expr_ty),
            _ => {}
        }
    }

    /// `CallTarget::Named`: bind type-vars from args + explicit type_args + return type,
    /// then redirect to the matching specialized instance if one was created.
    fn rewrite_named_call(&self, target: &mut CallTarget, args: &mut [IrExpr], type_args: &[Ty], expr_ty: &mut Ty) {
        let CallTarget::Named { name } = target else { unreachable!() };
        let Some(bounded_params) = self.bound_fns.get(name.as_str()) else { return };
        let pt = self.fn_param_types.get(name.as_str())
            .map(|pts| pts.as_slice()).unwrap_or(&[]);
        let mut bindings = collect_mono_bindings(bounded_params, args, pt);

        // Supplement from explicit type_args
        if !type_args.is_empty() {
            if let Some(gnames) = self.fn_generics.get(name.as_str()) {
                for (gname, ta) in gnames.iter().zip(type_args.iter()) {
                    if !bindings.contains_key(gname) || matches!(bindings.get(gname), Some(Ty::Unknown)) {
                        bindings.insert(gname.clone(), ta.clone());
                    }
                }
            }
        }

        // Infer from return type
        self.infer_from_return_type(name.as_str(), expr_ty, &mut bindings);

        if !bindings.is_empty() {
            let suffix = mangle_suffix(&bindings);
            if self.instances.contains_key(&(name.to_string(), suffix.clone())) {
                *name = format!("{}__{}", name, suffix).into();
                *expr_ty = substitute_ty(expr_ty, &bindings);
            }
        }
    }

    /// `CallTarget::Method`: bind type-vars from the UFCS-expanded args (receiver + args)
    /// + return type, then rewrite to a `Named` call on the specialized instance if one
    /// was created and every binding resolved to a concrete type.
    fn rewrite_method_call(&self, target: &mut CallTarget, args: &mut Vec<IrExpr>, expr_ty: &mut Ty) {
        let CallTarget::Method { object, method } = target else { unreachable!() };
        let Some(bounded_params) = self.bound_fns.get(method.as_str()) else { return };
        let pt = self.fn_param_types.get(method.as_str())
            .map(|pts| pts.as_slice()).unwrap_or(&[]);
        let mut ufcs_args: Vec<IrExpr> = vec![(**object).clone()];
        ufcs_args.extend(args.iter().cloned());
        let mut bindings = collect_mono_bindings(bounded_params, &ufcs_args, pt);

        // Infer from return type
        self.infer_from_return_type(method.as_str(), expr_ty, &mut bindings);

        let all_concrete = !bindings.is_empty() && bindings.values().all(|ty|
            !matches!(ty, Ty::Unknown) && !ty.contains_unknown()
            && !matches!(ty, Ty::TypeVar(_)) && !ty.contains_typevar()
        );
        if all_concrete {
            let suffix = mangle_suffix(&bindings);
            if self.instances.contains_key(&(method.to_string(), suffix.clone())) {
                let mono_name = format!("{}__{}", method, suffix);
                let obj_expr = (**object).clone();
                let mut new_args: Vec<IrExpr> = vec![obj_expr];
                new_args.extend(args.drain(..));
                *args = new_args;
                *target = CallTarget::Named { name: mono_name.into() };
                *expr_ty = substitute_ty(expr_ty, &bindings);
            }
        }
    }

    fn infer_from_return_type(&self, fn_name: &str, expr_ty: &Ty, bindings: &mut HashMap<String, Ty>) {
        if bindings.is_empty() || bindings.values().any(|v| matches!(v, Ty::Unknown)) {
            if let Some(gnames) = self.fn_generics.get(fn_name) {
                if let Some(ret_ty) = self.fn_ret_types.get(fn_name) {
                    for gname in gnames {
                        if !bindings.contains_key(gname) || matches!(bindings.get(gname), Some(Ty::Unknown)) {
                            let extracted = extract_typevar_binding(ret_ty, expr_ty, gname);
                            if !matches!(extracted, Ty::Unknown) {
                                bindings.insert(gname.clone(), extracted);
                            }
                        }
                    }
                }
            }
        }
    }
}
