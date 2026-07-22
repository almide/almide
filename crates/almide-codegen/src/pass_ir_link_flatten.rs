// ── IR Link Flatten Pass ─────────────────────────────────────────────
//
// Final nanopass: merges program.modules into root functions/types/top_lets.
// MUST run after UnifyVarTablesPass (VarIds already unified).
// After this, program.modules is empty. Walker renders flat program.

use almide_ir::*;
use almide_ir::annotations::CodegenAnnotations;
use almide_base::intern::{sym, Sym};
use almide_lang::types::Ty;
use super::pass::{NanoPass, PassResult, Target};
use std::collections::{HashSet, HashMap};

#[derive(Debug)]
pub struct IrLinkFlattenPass;

impl NanoPass for IrLinkFlattenPass {
    fn name(&self) -> &str { "IrLinkFlatten" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Rust]) }
    // #559: requires VarIds already unified by UnifyVarTables.
    fn depends_on(&self) -> Vec<&'static str> { vec!["UnifyVarTables"] }

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
                merge_module_type_decl(td, &mut emitted_types, &mut program.type_decls);
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

/// Per-`td` body of `IrLinkFlattenPass::run`'s type-decl merge loop,
/// extracted verbatim (cog>30 decomposition). Deduplicates by name,
/// preferring an `Alias` over a non-alias when both exist for the same
/// name (so `type_aliases` expansion works).
fn merge_module_type_decl(td: IrTypeDecl, emitted_types: &mut HashSet<String>, type_decls: &mut Vec<IrTypeDecl>) {
    let name = td.name.as_str().to_string();
    if !emitted_types.contains(&name) {
        emitted_types.insert(name.clone());
        type_decls.push(td);
    } else if matches!(&td.kind, IrTypeDeclKind::Alias { .. }) {
        // Replace non-alias with alias
        if let Some(pos) = type_decls.iter().position(|t| t.name.as_str() == name) {
            if !matches!(&type_decls[pos].kind, IrTypeDeclKind::Alias { .. }) {
                type_decls[pos] = td;
            }
        }
    }
}

fn mangle_qualified_type_names(program: &mut IrProgram) {
    let map = build_type_rename_map(&program.type_decls);
    if map.is_empty() {
        return;
    }

    for td in &mut program.type_decls {
        if let Some(nn) = map.get(td.name.as_str()) {
            td.name = *nn;
        }
        rename_type_decl_kind(&mut td.kind, &map);
    }
    // Twin decls now share one canonical name — keep the first, drop the rest
    // (identical shapes; a second `pub struct Msg` would be E0428).
    {
        let mut seen: std::collections::HashSet<Sym> = std::collections::HashSet::new();
        program.type_decls.retain(|td| seen.insert(td.name));
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
    remap_codegen_annotations(&mut program.codegen_annotations, &map);
}

/// Reference-graph "type name → its group's canonical name" map-building
/// phase of `mangle_qualified_type_names`, extracted verbatim (cog>30
/// decomposition, sequential-phase pattern — every later phase only reads
/// this map, never feeds back into it). STRUCTURAL TWINS — the checker
/// unifies two record/variant decls that share the same BASE name and the
/// same shape (almai: the root `Message` and every provider's `Message`
/// are byte-identical and flow into each other freely, and `check`
/// accepts). Which nominal name a given SITE resolves to is then an
/// accident of constraint order, so mangling each twin to its own struct
/// produced `expected almide_rt_openai_Message, found Message` (E0308) on
/// whichever sites landed on the other twin. Realize the checker's
/// semantics: map every dotted twin to ONE canonical name — the bare root
/// decl when one exists with the same fingerprint, else the first twin
/// (sorted) — and dedup the now-identical decls. Types with a unique shape
/// keep the plain per-module mangle, so genuinely distinct same-name types
/// stay distinct.
fn build_type_rename_map(type_decls: &[IrTypeDecl]) -> HashMap<String, Sym> {
    // Group decls by (base name, fingerprint).
    let mut groups: HashMap<(String, String), Vec<String>> = HashMap::new();
    for td in type_decls {
        let n = td.name.as_str();
        let base = n.rsplit('.').next().unwrap_or(n).to_string();
        groups.entry((base, td.structural_fingerprint())).or_default().push(n.to_string());
    }

    let mut map: HashMap<String, Sym> = HashMap::new();
    for ((_base, _fp), mut members) in groups {
        members.sort();
        // Canonical target: the bare member if present, else the first dotted
        // member's standard mangle.
        let canonical: Sym = match members.iter().find(|m| !m.contains('.')) {
            Some(bare) => sym(bare),
            None => sym(&format!("almide_rt_{}", members[0].replace('.', "_"))),
        };
        for m in &members {
            if m.contains('.') {
                map.insert(m.clone(), canonical);
            }
        }
    }
    map
}

/// NAME-KEYED-annotation remap phase of `mangle_qualified_type_names`,
/// extracted verbatim (cog>30 decomposition). The flatten rename must
/// reach every NAME-KEYED annotation too: the walker looks up
/// default/boxed fields by the ctor name it sees POST-flatten
/// (`almide_rt_mod_Type`), while the producing passes registered the
/// pre-flatten `mod.Type` — so a flattened module type's field DEFAULTS
/// were silently skipped (almai: `Message { role, content }` missing its
/// defaulted `tool_calls` → generated-Rust E0063).
fn remap_codegen_annotations(ann: &mut CodegenAnnotations, map: &HashMap<String, Sym>) {
    let remap = |n: &str| map.get(n).map(|s| s.as_str().to_string()).unwrap_or_else(|| n.to_string());
    ann.default_fields = std::mem::take(&mut ann.default_fields).into_iter()
        .map(|((c, f), e)| ((remap(&c), f), e)).collect();
    ann.boxed_fields = std::mem::take(&mut ann.boxed_fields).into_iter()
        .map(|(c, f)| (remap(&c), f)).collect();
    ann.ctor_to_enum = std::mem::take(&mut ann.ctor_to_enum).into_iter()
        .map(|(c, e)| (remap(&c), remap(&e))).collect();
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
                rename_variant_case_kind(&mut c.kind, map);
            }
        }
    }
}

/// `IrVariantKind` case of `rename_type_decl_kind`'s `Variant` arm,
/// extracted verbatim (cog>30 decomposition).
fn rename_variant_case_kind(kind: &mut IrVariantKind, map: &HashMap<String, Sym>) {
    match kind {
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
/// Rename the declared type of every `Bind` statement in `stmts`. Shared by
/// the `Block` / `While` / `ForIn` arms of [`rename_expr`] — loop bodies hold
/// `Vec<IrStmt>` directly (not a Block child), so each needs this same
/// `let`-binding type-annotation pass. Without it, a `let p = mod.f()` inside
/// a `while`/`for` keeps the unmangled type name and the walker emits
/// `let p: P` against the flat struct `almide_rt_mod_P` → E0425
/// (cross-module record bound in a loop).
fn rename_bind_tys_in_stmts(stmts: &mut [IrStmt], map: &HashMap<String, Sym>) {
    for s in stmts.iter_mut() {
        if let IrStmtKind::Bind { ty, .. } = &mut s.kind {
            *ty = rename_ty(ty, map);
        }
    }
}

// Lambda params, closure captures, call type-args, and boxing casts carry
// Tys OUTSIDE expr.ty — the walker renders them verbatim into Rust closure
// signatures and `as Rc<dyn Fn>` casts, so a missed rename here surfaces as
// E0425 on the unmangled name (#681: a linked module's record flowing
// through a fold lambda's `(acc, l)` params). Each of the next three
// helpers renames one such carrier, for the matching `rename_expr` arm.

fn rename_lambda_param_tys(params: &mut [(VarId, Ty)], map: &HashMap<String, Sym>) {
    for (_, ty) in params.iter_mut() {
        *ty = rename_ty(ty, map);
    }
}

fn rename_closure_capture_tys(captures: &mut [(VarId, Ty)], map: &HashMap<String, Sym>) {
    for (_, ty) in captures.iter_mut() {
        *ty = rename_ty(ty, map);
    }
}

fn rename_call_type_args(type_args: &mut [Ty], map: &HashMap<String, Sym>) {
    for ty in type_args.iter_mut() {
        *ty = rename_ty(ty, map);
    }
}

/// `InlineRust { template, .. }` arm of [`rename_expr`]: a user package's
/// `@inline_rust` template is raw Rust text that can reference the
/// package's OWN structs. StdlibLowering requalified those tokens to the
/// canonical dotted name (`aes.Cfb8State`); mangle them to the flat struct
/// name here, exactly like every Ty reference. A dotted token cannot occur
/// in valid Rust, so plain textual replacement is unambiguous — longest
/// keys first so `m.Cfg` never clips `m.CfgSet`.
fn rename_inline_rust_template(template: &mut String, map: &HashMap<String, Sym>) {
    if template.contains('.') {
        let mut keys: Vec<&String> = map.keys()
            .filter(|k| template.contains(k.as_str()))
            .collect();
        keys.sort_by_key(|k| std::cmp::Reverse(k.len()));
        for k in keys {
            *template = template.replace(k.as_str(), map[k].as_str());
        }
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
        IrExprKind::Block { stmts, .. } => rename_bind_tys_in_stmts(stmts, map),
        IrExprKind::While { body, .. } => rename_bind_tys_in_stmts(body, map),
        IrExprKind::ForIn { body, .. } => rename_bind_tys_in_stmts(body, map),
        IrExprKind::Record { name: Some(n), .. } => {
            // A struct literal carries its (now-qualified) type name as the ctor
            // (`mod.Type`, pinned by lowering); mangle it to the flat struct name.
            if let Some(nn) = map.get(n.as_str()) {
                *n = *nn;
            }
        }
        IrExprKind::Lambda { params, .. } => rename_lambda_param_tys(params, map),
        IrExprKind::ClosureCreate { captures, .. } => rename_closure_capture_tys(captures, map),
        IrExprKind::Call { type_args, .. } => rename_call_type_args(type_args, map),
        IrExprKind::RcWrap { cast_ty: Some(ty), .. } => {
            **ty = rename_ty(ty, map);
        }
        IrExprKind::InlineRust { template, .. } => rename_inline_rust_template(template, map),
        _ => {}
    }
    e
}
