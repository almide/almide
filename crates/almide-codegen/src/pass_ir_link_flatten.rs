// ── IR Link Flatten Pass ─────────────────────────────────────────────
//
// Final nanopass: merges program.modules into root functions/types/top_lets.
// MUST run after UnifyVarTablesPass (VarIds already unified).
// After this, program.modules is empty. Walker renders flat program.

use almide_ir::*;
use almide_base::intern::{sym, Sym};
use almide_lang::types::Ty;
use super::pass::{NanoPass, PassResult, Target};
use std::collections::{HashSet, HashMap};

#[derive(Debug)]
pub struct IrLinkFlattenPass;

impl NanoPass for IrLinkFlattenPass {
    fn name(&self) -> &str { "IrLinkFlatten" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Rust]) }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        if program.modules.is_empty() {
            return PassResult { program, changed: false };
        }

        let modules = std::mem::take(&mut program.modules);

        let mut emitted_types: HashSet<String> = program.type_decls.iter()
            .map(|td| td.name.as_str().to_string())
            .collect();

        for module in modules {
            let mod_ident = module.versioned_name
                .map(|v| v.to_string().replace('.', "_"))
                .unwrap_or_else(|| module.name.to_string().replace('.', "_"));

            // Merge type declarations (deduplicate by name).
            // If both an alias and a non-alias exist for the same name,
            // keep the alias (so type_aliases expansion works).
            for td in module.type_decls {
                let name = td.name.as_str().to_string();
                if !emitted_types.contains(&name) {
                    emitted_types.insert(name.clone());
                    program.type_decls.push(td);
                } else if matches!(&td.kind, IrTypeDeclKind::Alias { .. }) {
                    // Replace non-alias with alias
                    if let Some(pos) = program.type_decls.iter().position(|t| t.name.as_str() == name) {
                        if !matches!(&program.type_decls[pos].kind, IrTypeDeclKind::Alias { .. }) {
                            program.type_decls[pos] = td;
                        }
                    }
                }
            }

            // Merge functions with module_origin (no renaming in IR)
            for mut func in module.functions {
                func.module_origin = Some(mod_ident.clone());
                program.functions.push(func);
            }

            // Merge top_lets (already prefixed by lower_module — must happen
            // there because cross-module Var references share the VarId)
            for tl in module.top_lets {
                program.top_lets.push(tl);
            }
        }

        // #433: user-module types arrived under their qualified canonical name
        // `mod.Type` (lowering pinned them so two packages' same-name types stay
        // distinct). A `.` is not a valid Rust/WASM identifier, so flatten each to
        // `almide_rt_mod_Type` and rewrite every reference — the type-side
        // analogue of how functions carry a `module_origin` prefix.
        mangle_qualified_type_names(&mut program);

        PassResult { program, changed: true }
    }
}

fn mangle_qualified_type_names(program: &mut IrProgram) {
    let mut map: HashMap<String, Sym> = HashMap::new();
    for td in &program.type_decls {
        let n = td.name.as_str();
        if n.contains('.') {
            map.insert(n.to_string(), sym(&format!("almide_rt_{}", n.replace('.', "_"))));
        }
    }
    if map.is_empty() {
        return;
    }

    for td in &mut program.type_decls {
        if let Some(nn) = map.get(td.name.as_str()) {
            td.name = *nn;
        }
        rename_type_decl_kind(&mut td.kind, &map);
    }
    for f in &mut program.functions {
        for p in &mut f.params {
            p.ty = rename_ty(&p.ty, &map);
        }
        f.ret_ty = rename_ty(&f.ret_ty, &map);
        let body = std::mem::replace(&mut f.body, IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None });
        f.body = rename_expr(body, &map);
    }
    for tl in &mut program.top_lets {
        tl.ty = rename_ty(&tl.ty, &map);
        let v = std::mem::replace(&mut tl.value, IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None });
        tl.value = rename_expr(v, &map);
    }
    for v in &mut program.var_table.entries {
        v.ty = rename_ty(&v.ty, &map);
    }
    for d in &mut program.def_table.entries {
        d.ty = rename_ty(&d.ty, &map);
    }
}

fn rename_type_decl_kind(kind: &mut IrTypeDeclKind, map: &HashMap<String, Sym>) {
    match kind {
        IrTypeDeclKind::Record { fields } => {
            for f in fields {
                f.ty = rename_ty(&f.ty, map);
            }
        }
        IrTypeDeclKind::Alias { target } => {
            *target = rename_ty(target, map);
        }
        IrTypeDeclKind::Variant { cases, .. } => {
            for c in cases {
                match &mut c.kind {
                    IrVariantKind::Unit => {}
                    IrVariantKind::Tuple { fields } => {
                        for t in fields {
                            *t = rename_ty(t, map);
                        }
                    }
                    IrVariantKind::Record { fields } => {
                        for f in fields {
                            f.ty = rename_ty(&f.ty, map);
                        }
                    }
                }
            }
        }
    }
}

/// Recursively rename `Ty::Named` / `Ty::Variant` syms via `map`.
fn rename_ty(ty: &Ty, map: &HashMap<String, Sym>) -> Ty {
    let t = ty.map_children(&|c| rename_ty(c, map));
    match t {
        Ty::Named(n, args) => match map.get(n.as_str()) {
            Some(nn) => Ty::Named(*nn, args),
            None => Ty::Named(n, args),
        },
        Ty::Variant { name, cases } => match map.get(name.as_str()) {
            Some(nn) => Ty::Variant { name: *nn, cases },
            None => Ty::Variant { name, cases },
        },
        other => other,
    }
}

/// Recursively rewrite every `expr.ty` (and child exprs) through `rename_ty`,
/// plus the type-bearing fields `map_children` does NOT reach: a `Bind`
/// statement's declared type, and a struct `Record { … }` literal's ctor name
/// (re-pinned from the expr's now-mangled struct type).
fn rename_expr(e: IrExpr, map: &HashMap<String, Sym>) -> IrExpr {
    let mut e = e.map_children(&mut |c| rename_expr(c, map));
    e.ty = rename_ty(&e.ty, map);
    match &mut e.kind {
        IrExprKind::Block { stmts, .. } => {
            for s in stmts.iter_mut() {
                if let IrStmtKind::Bind { ty, .. } = &mut s.kind {
                    *ty = rename_ty(ty, map);
                }
            }
        }
        IrExprKind::Record { name: Some(n), .. } => {
            // A struct literal carries its (now-qualified) type name as the ctor
            // (`mod.Type`, pinned by lowering); mangle it to the flat struct name.
            if let Some(nn) = map.get(n.as_str()) {
                *n = *nn;
            }
        }
        // Lambda params, closure captures, call type-args, and boxing casts
        // carry Tys outside expr.ty — the walker renders them verbatim into
        // Rust closure signatures and `as Rc<dyn Fn>` casts, so a missed
        // rename here surfaces as E0425 on the unmangled name.
        IrExprKind::Lambda { params, .. } => {
            for (_, ty) in params.iter_mut() {
                *ty = rename_ty(ty, map);
            }
        }
        IrExprKind::ClosureCreate { captures, .. } => {
            for (_, ty) in captures.iter_mut() {
                *ty = rename_ty(ty, map);
            }
        }
        IrExprKind::Call { type_args, .. } => {
            for ty in type_args.iter_mut() {
                *ty = rename_ty(ty, map);
            }
        }
        IrExprKind::RcWrap { cast_ty: Some(ty), .. } => {
            **ty = rename_ty(ty, map);
        }
        _ => {}
    }
    e
}
