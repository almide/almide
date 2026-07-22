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
    /// qualified decl key → structural fingerprint (see
    /// `IrTypeDecl::structural_fingerprint`) — lets the repair treat a bare
    /// reference whose owners are all STRUCTURAL TWINS as unambiguous.
    fingerprints: std::collections::HashMap<String, String>,
}

fn index_decls(program: &IrProgram) -> DeclIndex {
    let mut bare = HashSet::new();
    let mut qualified: std::collections::HashMap<String, Vec<String>> = Default::default();
    let mut fingerprints: std::collections::HashMap<String, String> = Default::default();
    let mut add = |td: &almide_ir::IrTypeDecl| {
        let s = td.name.as_str();
        match s.rsplit_once('.') {
            Some((_, base)) => {
                qualified.entry(base.to_string()).or_default().push(s.to_string());
                fingerprints.insert(s.to_string(), td.structural_fingerprint());
            }
            None => { bare.insert(td.name); }
        }
    };
    for td in &program.type_decls { add(td); }
    for m in &program.modules {
        for td in &m.type_decls { add(td); }
    }
    DeclIndex { bare, qualified, fingerprints }
}

struct TyChecker<'a> {
    decls: &'a DeclIndex,
    offenders: Vec<UnresolvableName>,
    where_: String,
}

impl TyChecker<'_> {
    /// `Ty::Named` case of `check_ty`, extracted verbatim (cog>30
    /// decomposition, pattern 2: uniform match arms as inherent methods —
    /// mirrors the `Concretizer`/`BranchBalance` extraction shape since
    /// the arm body uses `self.*` fields).
    fn check_ty_named(&mut self, n: &Sym, args: &[Ty]) {
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

    /// `Ty::Variant` case of `check_ty`, extracted verbatim. Variant cases
    /// inside a Ty value carry payload tys.
    fn check_ty_variant(&mut self, cases: &[almide_lang::types::VariantCase]) {
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

    fn check_ty(&mut self, ty: &Ty) {
        match ty {
            Ty::Named(n, args) => self.check_ty_named(n, args),
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
            Ty::Variant { cases, .. } => self.check_ty_variant(cases),
            // Scalars / TypeVar / Unknown / Never etc. carry no Named children.
            _ => {}
        }
    }
}

impl IrVisitor for TyChecker<'_> {
    fn visit_expr(&mut self, expr: &IrExpr) {
        let saved = self.where_.clone();
        self.where_ = format!("{} / expr `{}` ty", saved, expr_kind_tag(&expr.kind));
        self.check_ty(&expr.ty);
        self.where_ = saved.clone();
        // Ty positions outside expr.ty — the same set repair/mangle rewrite.
        match &expr.kind {
            IrExprKind::Lambda { params, .. } => {
                for (_, ty) in params {
                    self.check_ty(ty);
                }
            }
            IrExprKind::ClosureCreate { captures, .. } => {
                for (_, ty) in captures {
                    self.check_ty(ty);
                }
            }
            IrExprKind::Call { type_args, .. } => {
                for ty in type_args {
                    self.check_ty(ty);
                }
            }
            IrExprKind::RcWrap { cast_ty: Some(ty), .. } => self.check_ty(ty),
            _ => {}
        }
        walk_expr(self, expr);
        self.where_ = saved;
    }
    fn visit_stmt(&mut self, stmt: &IrStmt) {
        if let IrStmtKind::Bind { ty, .. } = &stmt.kind {
            let saved = self.where_.clone();
            self.where_ = format!("{} / bind-stmt ty", saved);
            self.check_ty(ty);
            self.where_ = saved;
        }
        walk_stmt(self, stmt);
    }
}

fn expr_kind_tag(k: &IrExprKind) -> &'static str {
    match k {
        IrExprKind::Var { .. } => "Var",
        IrExprKind::Call { .. } => "Call",
        IrExprKind::RuntimeCall { .. } => "RuntimeCall",
        IrExprKind::TailCall { .. } => "TailCall",
        IrExprKind::Match { .. } => "Match",
        IrExprKind::If { .. } => "If",
        IrExprKind::Block { .. } => "Block",
        IrExprKind::Member { .. } => "Member",
        IrExprKind::IndexAccess { .. } => "IndexAccess",
        IrExprKind::List { .. } => "List",
        IrExprKind::Record { .. } => "Record",
        IrExprKind::Lambda { .. } => "Lambda",
        IrExprKind::ForIn { .. } => "ForIn",
        IrExprKind::While { .. } => "While",
        IrExprKind::BinOp { .. } => "BinOp",
        IrExprKind::Clone { .. } => "Clone",
        IrExprKind::Borrow { .. } => "Borrow",
        IrExprKind::FnRef { .. } => "FnRef",
        IrExprKind::TupleIndex { .. } => "TupleIndex",
        _ => "other",
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
        for p in &func.params {
            chk.where_ = format!("fn `{}` / param `{}`", func.name, p.name);
            chk.check_ty(&p.ty);
        }
        chk.where_ = format!("fn `{}` / return ty", func.name);
        chk.check_ty(&func.ret_ty);
        chk.where_ = format!("fn `{}`", func.name);
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

// ── Repair: complete unambiguous bare references ──
//
// Producers (checker/lowering) are supposed to pin canonical qualified
// names, but several positions still leak bare ones — lambda param types,
// alias-qualified annotations (`q.Cfg` with `import m as q`), and generic
// instantiations. When a bare base name has EXACTLY ONE qualified
// declaration and no bare twin, it can only mean that declaration, so we
// rewrite it to the canonical qualified name before the gate runs. The
// gate then only fires on genuinely ambiguous references. This is a
// consumer-side completion, not a producer fix — the verifier still
// machine-checks the end state either way.

fn build_repair_map(decls: &DeclIndex) -> std::collections::HashMap<Sym, Sym> {
    let mut map = std::collections::HashMap::new();
    for (base, owners) in &decls.qualified {
        if decls.bare.contains(&almide_base::intern::sym(base)) {
            continue;
        }
        if owners.len() == 1 {
            map.insert(
                almide_base::intern::sym(base),
                almide_base::intern::sym(&owners[0]),
            );
            continue;
        }
        // Multiple owners that are ALL STRUCTURAL TWINS (same fingerprint) are
        // one type to the checker — it unifies same-shape records freely, and
        // the flatten pass merges them into one canonical struct. A bare
        // reference to such a base is therefore unambiguous: pick the first
        // (sorted) owner; the flatten twin-merge maps every owner to the same
        // canonical name anyway. (almai: 8 provider modules + the package root
        // each declare identical Tool/ToolCall/Usage/LLMResponse — bare refs in
        // its spec files used to trip the gate.)
        let mut sorted: Vec<&String> = owners.iter().collect();
        sorted.sort();
        let first_fp = decls.fingerprints.get(sorted[0]);
        if first_fp.is_some()
            && sorted.iter().all(|o| decls.fingerprints.get(*o) == first_fp)
        {
            map.insert(
                almide_base::intern::sym(base),
                almide_base::intern::sym(sorted[0]),
            );
        }
    }
    map
}

fn repair_ty(ty: &Ty, map: &std::collections::HashMap<Sym, Sym>) -> Ty {
    let t = ty.map_children(&|c| repair_ty(c, map));
    match t {
        Ty::Named(n, args) => match map.get(&n) {
            Some(q) => Ty::Named(*q, args),
            None => Ty::Named(n, args),
        },
        Ty::Variant { name, cases } => match map.get(&name) {
            Some(q) => Ty::Variant { name: *q, cases },
            None => Ty::Variant { name, cases },
        },
        other => other,
    }
}

/// Canonical-walker repair: the SAME traversal family as the verifier
/// (`IrMutVisitor`), so every position the gate scans is a position the
/// repair rewrites — by construction, not by keeping two hand-maintained
/// enumerations in sync. (#783: the old `map_children`-based rewrite fixed
/// `Bind` tys only when the Bind was a DIRECT child of a `Block` expr; a
/// `let child: Node = …` inside a ForIn/While body — `Vec<IrStmt>`, not a
/// Block — kept its bare ty and tripped the gate four functions deep in
/// ceangal's layout module.)
struct RepairVisitor<'a> {
    map: &'a std::collections::HashMap<Sym, Sym>,
}

impl almide_ir::visit_mut::IrMutVisitor for RepairVisitor<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        e.ty = repair_ty(&e.ty, self.map);
        match &mut e.kind {
            // Record literals carry their type name as the ctor; a bare ctor
            // would render a bare (post-mangle nonexistent) struct name.
            IrExprKind::Record { name: Some(n), .. } => {
                if let Some(q) = self.map.get(n) {
                    *n = *q;
                }
            }
            // Lambda params / closure captures / call type-args / boxing casts
            // carry Tys outside expr.ty.
            IrExprKind::Lambda { params, .. } => {
                for (_, ty) in params.iter_mut() {
                    *ty = repair_ty(ty, self.map);
                }
            }
            IrExprKind::ClosureCreate { captures, .. } => {
                for (_, ty) in captures.iter_mut() {
                    *ty = repair_ty(ty, self.map);
                }
            }
            IrExprKind::Call { type_args, .. } => {
                for ty in type_args.iter_mut() {
                    *ty = repair_ty(ty, self.map);
                }
            }
            IrExprKind::RcWrap { cast_ty: Some(ty), .. } => {
                **ty = repair_ty(ty, self.map);
            }
            _ => {}
        }
        almide_ir::visit_mut::walk_expr_mut(self, e);
    }
    fn visit_stmt_mut(&mut self, s: &mut IrStmt) {
        if let IrStmtKind::Bind { ty, .. } = &mut s.kind {
            *ty = repair_ty(ty, self.map);
        }
        almide_ir::visit_mut::walk_stmt_mut(self, s);
    }
    fn visit_pattern_mut(&mut self, p: &mut IrPattern) {
        if let IrPattern::Bind { ty, .. } = p {
            *ty = repair_ty(ty, self.map);
        }
        almide_ir::visit_mut::walk_pattern_mut(self, p);
    }
}

fn repair_expr_in_place(e: &mut IrExpr, map: &std::collections::HashMap<Sym, Sym>) {
    use almide_ir::visit_mut::IrMutVisitor;
    RepairVisitor { map }.visit_expr_mut(e);
}

fn repair_fn(f: &mut IrFunction, map: &std::collections::HashMap<Sym, Sym>) {
    for p in &mut f.params {
        p.ty = repair_ty(&p.ty, map);
    }
    f.ret_ty = repair_ty(&f.ret_ty, map);
    repair_expr_in_place(&mut f.body, map);
}

fn repair_decl_kind(kind: &mut IrTypeDeclKind, map: &std::collections::HashMap<Sym, Sym>) {
    match kind {
        IrTypeDeclKind::Record { fields } => {
            for f in fields {
                f.ty = repair_ty(&f.ty, map);
            }
        }
        IrTypeDeclKind::Alias { target } => *target = repair_ty(target, map),
        IrTypeDeclKind::Variant { cases, .. } => {
            for c in cases {
                match &mut c.kind {
                    IrVariantKind::Unit => {}
                    IrVariantKind::Tuple { fields } => {
                        for t in fields {
                            *t = repair_ty(t, map);
                        }
                    }
                    IrVariantKind::Record { fields } => {
                        for f in fields {
                            f.ty = repair_ty(&f.ty, map);
                        }
                    }
                }
            }
        }
    }
}

/// Rewrite every unambiguous bare type reference to its canonical qualified
/// name, in place. Runs at codegen entry, before `assert_names_resolvable`.
pub fn repair_bare_type_names(program: &mut IrProgram) {
    let decls = index_decls(program);
    let map = build_repair_map(&decls);
    if std::env::var_os("ALMIDE_NAMES_DEBUG").is_some() {
        eprintln!("[names-debug] repair: bare decls = {:?}", decls.bare.iter().map(|s| s.as_str()).collect::<Vec<_>>());
        eprintln!("[names-debug] repair: qualified = {:?}", decls.qualified);
        eprintln!("[names-debug] repair: map = {:?}", map.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect::<Vec<_>>());
    }
    if map.is_empty() {
        return;
    }
    for td in &mut program.type_decls {
        repair_decl_kind(&mut td.kind, &map);
    }
    for f in &mut program.functions {
        repair_fn(f, &map);
    }
    for tl in &mut program.top_lets {
        tl.ty = repair_ty(&tl.ty, &map);
        repair_expr_in_place(&mut tl.value, &map);
    }
    for v in &mut program.var_table.entries {
        v.ty = repair_ty(&v.ty, &map);
    }
    for d in &mut program.def_table.entries {
        d.ty = repair_ty(&d.ty, &map);
    }
    for m in &mut program.modules {
        for td in &mut m.type_decls {
            repair_decl_kind(&mut td.kind, &map);
        }
        for f in &mut m.functions {
            repair_fn(f, &map);
        }
        for tl in &mut m.top_lets {
            tl.ty = repair_ty(&tl.ty, &map);
            repair_expr_in_place(&mut tl.value, &map);
        }
        for v in &mut m.var_table.entries {
            v.ty = repair_ty(&v.ty, &map);
        }
    }
}

/// HARD codegen-entry gate (both targets, debug AND release). A bare type
/// name whose only declaration is module-qualified can never resolve after
/// link — refusing the build here turns the silent E0425 / wasm-trap class
/// into a structured compiler-bug report. Controlled error, not an ICE.
pub fn assert_names_resolvable(program: &IrProgram) {
    let offenders = collect_unresolvable_names(program);
    if std::env::var_os("ALMIDE_NAMES_DEBUG").is_some() {
        let decls = index_decls(program);
        eprintln!("[names-debug] gate: bare decls = {:?}", decls.bare.iter().map(|s| s.as_str()).collect::<Vec<_>>());
        eprintln!("[names-debug] gate: qualified = {:?}", decls.qualified);
    }
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
    fn repair_reaches_binds_in_loop_bodies() {
        // #783: a `let v: Node = …` Bind inside a ForIn/While body (Vec<IrStmt>,
        // not a Block expr) must be repaired like any Block-level Bind — the
        // old map_children rewrite skipped it and the gate fired on ceangal's
        // layout functions.
        let mut program = IrProgram::default();
        program.modules.push(module_with_decl("m.Cfg"));
        let unit = || IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None };
        let bind = IrStmt {
            kind: IrStmtKind::Bind {
                var: VarId(0),
                mutability: Mutability::Let,
                ty: named("Cfg"),
                value: unit(),
            },
            span: None,
        };
        let body = IrExpr {
            kind: IrExprKind::While {
                cond: Box::new(IrExpr { kind: IrExprKind::LitBool { value: false }, ty: Ty::Bool, span: None, def_id: None }),
                body: vec![bind],
            },
            ty: Ty::Unit, span: None, def_id: None,
        };
        program.modules[0].functions.push(IrFunction {
            name: sym("m.f"),
            params: vec![],
            ret_ty: Ty::Unit,
            body,
            is_effect: false,
            is_async: false,
            is_test: false,
            generics: None,
            extern_attrs: vec![],
            export_attrs: vec![],
            attrs: vec![],
            visibility: IrVisibility::Public,
            doc: None,
            blank_lines_before: 0,
            def_id: None,
            mutated_params: vec![],
            module_origin: Some("m".to_string()),
        });
        assert_eq!(collect_unresolvable_names(&program).len(), 1,
            "the loop-body Bind ty must be scanned");
        repair_bare_type_names(&mut program);
        assert!(collect_unresolvable_names(&program).is_empty(),
            "repair must rewrite the loop-body Bind ty to the qualified name");
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

    #[test]
    fn repair_completes_unambiguous_bare_ref() {
        let mut program = IrProgram::default();
        program.modules.push(module_with_decl("m.Cfg"));
        program.var_table.alloc(sym("v"), named("Cfg"), Mutability::Let, None);
        repair_bare_type_names(&mut program);
        assert_eq!(program.var_table.entries[0].ty, named("m.Cfg"),
            "bare `Cfg` with a unique qualified owner is completed to `m.Cfg`");
        assert!(collect_unresolvable_names(&program).is_empty(),
            "the gate passes after repair");
    }

    #[test]
    fn repair_leaves_ambiguous_bare_ref_for_the_gate() {
        // The two owners must have DIFFERENT shapes: same-shape twins are merged by the
        // structural twin-merge (the checker unifies them), so a bare ref to them IS
        // unambiguous and repair legitimately completes it. Genuine ambiguity = two
        // qualified owners whose fingerprints differ.
        let mut program = IrProgram::default();
        program.modules.push(module_with_decl("m.Cfg"));
        let mut n = module_with_decl("n.Cfg");
        if let IrTypeDeclKind::Record { fields } = &mut n.type_decls[0].kind {
            fields.push(almide_ir::IrFieldDecl {
                name: sym("extra"),
                ty: Ty::Int,
                default: None,
                alias: None,
                attrs: vec![],
            });
        }
        program.modules.push(n);
        program.var_table.alloc(sym("v"), named("Cfg"), Mutability::Let, None);
        repair_bare_type_names(&mut program);
        assert_eq!(program.var_table.entries[0].ty, named("Cfg"),
            "two different-shaped qualified owners — repair must not guess");
        assert_eq!(collect_unresolvable_names(&program).len(), 1,
            "ambiguous bare ref is still rejected by the gate");
    }

    #[test]
    fn repair_respects_bare_decl_shadowing() {
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
        repair_bare_type_names(&mut program);
        assert_eq!(program.var_table.entries[0].ty, named("Cfg"),
            "a root-level bare decl owns the bare reference — no rewrite");
    }

    #[test]
    fn repair_reaches_lambda_params_inside_module_fns() {
        let mut program = IrProgram::default();
        let mut m = module_with_decl("m.Cfg");
        let lambda = IrExpr {
            kind: IrExprKind::Lambda {
                params: vec![(VarId(0), named("Cfg"))],
                body: Box::new(IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None }),
                lambda_id: None,
            },
            ty: Ty::Fn { params: vec![named("Cfg")], ret: Box::new(Ty::Unit) },
            span: None,
            def_id: None,
        };
        m.functions.push(IrFunction {
            name: sym("m.user"),
            params: vec![],
            ret_ty: Ty::Unit,
            body: lambda,
            is_effect: false,
            is_async: false,
            is_test: false,
            generics: None,
            extern_attrs: vec![],
            export_attrs: vec![],
            attrs: vec![],
            visibility: IrVisibility::Public,
            doc: None,
            blank_lines_before: 0,
            def_id: None,
            mutated_params: vec![],
            module_origin: Some("m".to_string()),
        });
        program.modules.push(m);
        repair_bare_type_names(&mut program);
        let f = &program.modules[0].functions[0];
        if let IrExprKind::Lambda { params, .. } = &f.body.kind {
            assert_eq!(params[0].1, named("m.Cfg"), "lambda param ty must be completed");
        } else {
            panic!("expected lambda body");
        }
    }
}
