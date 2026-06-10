//! NameResolutionTotal — completeness-by-construction §1(a).
//!
//! Detects the #433/#484 bug class at codegen entry: a BARE `Ty::Named(n)`
//! reference whose only declaration is module-qualified (`m.n`). Such a name
//! cannot resolve after link — the declaration mangles to `almide_rt_m_n`
//! while the reference renders bare `n` — surfacing as rustc `E0425` on
//! native or a `record_fields` lookup miss on wasm. Every producer is
//! supposed to pin canonical qualified names during checking/lowering; this
//! verifier makes "supposed to" machine-checked, the same verifier-first step
//! that preceded the `Verified`/`Canonical` type-states.
//!
//! Runs BEFORE the nanopass pipeline (both targets), while declarations are
//! still in their canonical state: root decls bare, module decls qualified.
//!
//! The detection is a pure function (unit-testable); the wrapper adds the
//! formatted `[COMPILER BUG]` abort, mirroring `assert_types_concretized`.

use std::collections::HashSet;
use almide_ir::*;
use almide_ir::visit::{IrVisitor, walk_expr, walk_stmt};
use almide_lang::types::Ty;
use almide_base::intern::Sym;

/// A bare type reference that no bare declaration satisfies while a
/// module-qualified declaration of the same base name exists.
#[derive(Debug)]
pub struct UnresolvableName {
    pub bare: String,
    pub qualified_candidates: Vec<String>,
    pub where_: String,
}

struct DeclIndex {
    bare: HashSet<Sym>,
    /// base name → qualified decl keys ("m.Cfg") that own it
    qualified: std::collections::HashMap<String, Vec<String>>,
}

fn index_decls(program: &IrProgram) -> DeclIndex {
    let mut bare = HashSet::new();
    let mut qualified: std::collections::HashMap<String, Vec<String>> = Default::default();
    let mut add = |name: Sym| {
        let s = name.as_str();
        match s.rsplit_once('.') {
            Some((_, base)) => qualified.entry(base.to_string()).or_default().push(s.to_string()),
            None => { bare.insert(name); }
        }
    };
    for td in &program.type_decls { add(td.name); }
    for m in &program.modules {
        for td in &m.type_decls { add(td.name); }
    }
    DeclIndex { bare, qualified }
}

struct TyChecker<'a> {
    decls: &'a DeclIndex,
    offenders: Vec<UnresolvableName>,
    where_: String,
}

impl TyChecker<'_> {
    fn check_ty(&mut self, ty: &Ty) {
        match ty {
            Ty::Named(n, args) => {
                let s = n.as_str();
                if !s.contains('.') && !self.decls.bare.contains(n) {
                    if let Some(cands) = self.decls.qualified.get(s) {
                        // Cap per-site duplicates: one report per (name, where) is enough.
                        if !self.offenders.iter().any(|o| o.bare == s && o.where_ == self.where_) {
                            self.offenders.push(UnresolvableName {
                                bare: s.to_string(),
                                qualified_candidates: cands.clone(),
                                where_: self.where_.clone(),
                            });
                        }
                    }
                }
                for a in args { self.check_ty(a); }
            }
            Ty::Applied(_, args) | Ty::Tuple(args) => {
                for a in args { self.check_ty(a); }
            }
            Ty::Fn { params, ret } => {
                for p in params { self.check_ty(p); }
                self.check_ty(ret);
            }
            Ty::Record { fields } | Ty::OpenRecord { fields } => {
                for (_, t) in fields { self.check_ty(t); }
            }
            // Variant cases inside a Ty value carry payload tys
            Ty::Variant { cases, .. } => {
                for c in cases {
                    match &c.payload {
                        almide_lang::types::VariantPayload::Tuple(ts) => {
                            for t in ts { self.check_ty(t); }
                        }
                        almide_lang::types::VariantPayload::Record(fs) => {
                            for (_, t) in fs { self.check_ty(t); }
                        }
                        _ => {}
                    }
                }
            }
            // Scalars / TypeVar / Unknown / Never etc. carry no Named children.
            _ => {}
        }
    }
}

impl IrVisitor for TyChecker<'_> {
    fn visit_expr(&mut self, expr: &IrExpr) {
        self.check_ty(&expr.ty);
        walk_expr(self, expr);
    }
    fn visit_stmt(&mut self, stmt: &IrStmt) {
        if let IrStmtKind::Bind { ty, .. } = &stmt.kind {
            self.check_ty(ty);
        }
        walk_stmt(self, stmt);
    }
}

/// Pure detector: every Ty position in the program (type decls, signatures,
/// var tables, top-lets, expression types) is scanned for bare names whose
/// only declaration is qualified.
pub fn collect_unresolvable_names(program: &IrProgram) -> Vec<UnresolvableName> {
    let decls = index_decls(program);
    let mut chk = TyChecker { decls: &decls, offenders: Vec::new(), where_: String::new() };

    let check_decl_tys = |chk: &mut TyChecker, td: &IrTypeDecl| {
        chk.where_ = format!("type decl `{}`", td.name);
        match &td.kind {
            IrTypeDeclKind::Record { fields } => {
                for f in fields { chk.check_ty(&f.ty); }
            }
            IrTypeDeclKind::Variant { cases, .. } => {
                for c in cases {
                    match &c.kind {
                        IrVariantKind::Tuple { fields } => for t in fields { chk.check_ty(t); },
                        IrVariantKind::Record { fields } => for f in fields { chk.check_ty(&f.ty); },
                        IrVariantKind::Unit => {}
                    }
                }
            }
            IrTypeDeclKind::Alias { target } => chk.check_ty(target),
        }
    };

    let check_fn = |chk: &mut TyChecker, func: &IrFunction| {
        chk.where_ = format!("fn `{}`", func.name);
        for p in &func.params { chk.check_ty(&p.ty); }
        chk.check_ty(&func.ret_ty);
        chk.visit_expr(&func.body);
    };

    for td in &program.type_decls { check_decl_tys(&mut chk, td); }
    for f in &program.functions { check_fn(&mut chk, f); }
    for tl in &program.top_lets {
        chk.where_ = "top-level let".to_string();
        chk.check_ty(&tl.ty);
        chk.visit_expr(&tl.value);
    }
    for (i, vi) in program.var_table.entries.iter().enumerate() {
        chk.where_ = format!("var #{} `{}`", i, vi.name);
        chk.check_ty(&vi.ty);
    }
    for m in &program.modules {
        for td in &m.type_decls { check_decl_tys(&mut chk, td); }
        for f in &m.functions { check_fn(&mut chk, f); }
        for tl in &m.top_lets {
            chk.where_ = format!("module `{}` top-level let", m.name);
            chk.check_ty(&tl.ty);
            chk.visit_expr(&tl.value);
        }
        for (i, vi) in m.var_table.entries.iter().enumerate() {
            chk.where_ = format!("module `{}` var #{} `{}`", m.name, i, vi.name);
            chk.check_ty(&vi.ty);
        }
    }
    chk.offenders
}

/// HARD codegen-entry gate (both targets, debug AND release). A bare type
/// name whose only declaration is module-qualified can never resolve after
/// link — refusing the build here turns the silent E0425 / wasm-trap class
/// into a structured compiler-bug report. Controlled error, not an ICE.
pub fn assert_names_resolvable(program: &IrProgram) {
    let offenders = collect_unresolvable_names(program);
    if offenders.is_empty() { return; }

    let mut msg = String::new();
    msg.push_str("error: [COMPILER BUG] unresolvable bare type name(s) reached codegen\n");
    msg.push_str(&format!(
        "  {} reference(s) use a BARE type name whose only declaration is module-qualified.\n",
        offenders.len()
    ));
    msg.push_str("  After link the declaration is mangled while the reference stays bare, so the\n");
    msg.push_str("  build would fail as generated-Rust E0425 or trap at runtime on wasm. A name\n");
    msg.push_str("  producer (checker/lowering) failed to pin the canonical qualified name (#433).\n");
    msg.push_str("  This is a compiler bug, not an error in your program.\n");
    const MAX_LISTED: usize = 10;
    for o in offenders.iter().take(MAX_LISTED) {
        msg.push_str(&format!(
            "    - `{}` in {} (qualified candidate(s): {})\n",
            o.bare, o.where_, o.qualified_candidates.join(", ")
        ));
    }
    if offenders.len() > MAX_LISTED {
        msg.push_str(&format!("    … and {} more\n", offenders.len() - MAX_LISTED));
    }
    msg.push_str("  Please report this at https://github.com/almide/almide/issues\n");
    eprint!("{}", msg);
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use almide_base::intern::sym;

    fn named(n: &str) -> Ty { Ty::Named(sym(n), vec![]) }

    fn module_with_decl(decl: &str) -> IrModule {
        IrModule {
            name: sym("m"),
            versioned_name: None,
            type_decls: vec![IrTypeDecl {
                name: sym(decl),
                kind: IrTypeDeclKind::Record { fields: vec![] },
                deriving: None, generics: None,
                visibility: IrVisibility::Public,
                doc: None, blank_lines_before: 0,
            }],
            functions: vec![], top_lets: vec![], var_table: VarTable::new(),
            exports: vec![], imports: vec![],
        }
    }

    #[test]
    fn bare_ref_with_only_qualified_decl_is_flagged() {
        let mut program = IrProgram::default();
        program.modules.push(module_with_decl("m.Cfg"));
        program.var_table.alloc(sym("v"), named("Cfg"), Mutability::Let, None);
        let offenders = collect_unresolvable_names(&program);
        assert_eq!(offenders.len(), 1, "bare `Cfg` with only `m.Cfg` declared must be flagged");
        assert_eq!(offenders[0].bare, "Cfg");
        assert_eq!(offenders[0].qualified_candidates, vec!["m.Cfg".to_string()]);
    }

    #[test]
    fn bare_ref_with_bare_decl_is_fine() {
        let mut program = IrProgram::default();
        program.type_decls.push(IrTypeDecl {
            name: sym("Cfg"),
            kind: IrTypeDeclKind::Record { fields: vec![] },
            deriving: None, generics: None,
            visibility: IrVisibility::Public,
            doc: None, blank_lines_before: 0,
        });
        program.modules.push(module_with_decl("m.Cfg"));
        program.var_table.alloc(sym("v"), named("Cfg"), Mutability::Let, None);
        assert!(collect_unresolvable_names(&program).is_empty(),
            "a bare decl satisfies the bare reference even when a qualified twin exists");
    }

    #[test]
    fn nested_named_inside_containers_is_scanned() {
        let mut program = IrProgram::default();
        program.modules.push(module_with_decl("m.Cfg"));
        program.var_table.alloc(
            sym("v"),
            Ty::Applied(almide_lang::types::TypeConstructorId::List, vec![named("Cfg")]),
            Mutability::Let, None,
        );
        assert_eq!(collect_unresolvable_names(&program).len(), 1,
            "List[Cfg] must be scanned through the container");
    }
}
