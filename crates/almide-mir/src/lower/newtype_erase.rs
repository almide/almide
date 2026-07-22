/// TRANSPARENT-NEWTYPE ERASURE (a pre-lowering program pass): `mod type SafeHtml =
/// String` is a purely NOMINAL wrapper — the frontend rejects every operation that
/// could observe the wrapper at runtime (direct `println`, arithmetic, off-type
/// passing), so by IR time the newtype exists ONLY as (1) `Ty::Named(name, [])`
/// tags, (2) a 1-arg ctor CALL `SafeHtml(s)` (which would render as an unlinked
/// `$SafeHtml` — an honest wall), and (3) a 1-arg ctor PATTERN `SafeHtml(s) => …`.
/// Erase all three to the inner type: the value IS its payload (v0's runtime is the
/// same `#[repr(transparent)]` story), equality/print/drop all follow the inner ty.
/// Alias CHAINS (`type A = B; type B = String`) resolve to a fixpoint first.
/// GENERIC aliases keep their decl (the target mentions type params) — untouched.
/// Runs on the WHOLE linked program (pipeline + classify — desugar-before-both:
/// the caps `mir == ir` count sees the erased tree on BOTH sides by construction).
/// The self-host STDLIB opaque-nominal rep table of [`erase_transparent_newtypes`]
/// — verbatim move (each entry is an independent `if !declared.contains(name) {
/// map.insert(...) }` gate; none reads another's insert, so they are a pure
/// name-router with no shared state to thread).
fn seed_selfhost_newtype_reps(
    map: &mut std::collections::HashMap<String, almide_lang::types::Ty>,
    declared: &std::collections::HashSet<&str>,
) {
    use almide_lang::types::Ty;
    // JsonPath — the self-host rep is a List[String] of segments
    // (stdlib/json_path.almd).
    if !declared.contains("JsonPath") {
        map.insert(
            "JsonPath".to_string(),
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, vec![Ty::String]),
        );
    }
    // HttpResponse — the self-host rep is `[status, body, k1, v1, …]`
    // (stdlib/http_response.almd, the same List[String] discipline).
    if !declared.contains("HttpResponse") {
        map.insert(
            "HttpResponse".to_string(),
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, vec![Ty::String]),
        );
    }
    // FileStat — the fs.stat Ok payload. Its decl lives in the BUNDLED stdlib fs module,
    // which `source_to_ir` skips (defs come from the self-host registry), so the nominal
    // `Named(FileStat)` never reaches `record_layouts` and a `meta.size` member read walls.
    // Erase it to the STRUCTURAL record (the SAME field order stdlib/fs.almd declares and
    // stdlib/fs_stat.almd constructs — `aggregate_field_tys` resolves a `Ty::Record`
    // without any registry), so member reads land on the right uniform slots.
    if !declared.contains("FileStat") {
        use almide_lang::intern::sym;
        map.insert(
            "FileStat".to_string(),
            Ty::Record {
                fields: vec![
                    (sym("size"), Ty::Int),
                    (sym("is_dir"), Ty::Bool),
                    (sym("is_file"), Ty::Bool),
                    (sym("modified"), Ty::Int),
                ],
            },
        );
    }
    // ProcessStatus — the process stdlib record (stdlib/process.almd). Its decl lives in
    // the BUNDLED stdlib module `source_to_ir` skips, so an ANNOTATED literal
    // (`let s: process.ProcessStatus = { code: …, stdout: …, stderr: … }`) carried an
    // unresolvable `Named(ProcessStatus)` — the construct declined and every member read
    // walled (the process_named_type walls). Erase it to the STRUCTURAL record (the same
    // field order the decl declares), the exact FileStat treatment above.
    if !declared.contains("ProcessStatus") {
        use almide_lang::intern::sym;
        map.insert(
            "ProcessStatus".to_string(),
            Ty::Record {
                fields: vec![
                    (sym("code"), Ty::Int),
                    (sym("stdout"), Ty::String),
                    (sym("stderr"), Ty::String),
                ],
            },
        );
    }
}

/// The `Ty`-substitution core [`erase_transparent_newtypes`] and
/// [`erase_newtypes_in_type_decls`] both recurse with — module-level (was a
/// nested fn; hoisted so the type_decls-erasure loop below can share it
/// without either duplicating it or threading it as a closure param).
fn subst(ty: &almide_lang::types::Ty, map: &std::collections::HashMap<String, almide_lang::types::Ty>) -> almide_lang::types::Ty {
    use almide_lang::types::Ty;
    match ty {
        Ty::Named(name, args) => subst_named(name, args, map),
        Ty::Applied(id, args) => {
            Ty::Applied(id.clone(), args.iter().map(|a| subst(a, map)).collect())
        }
        Ty::Tuple(ts) => Ty::Tuple(ts.iter().map(|a| subst(a, map)).collect()),
        Ty::Union(ts) => Ty::Union(ts.iter().map(|a| subst(a, map)).collect()),
        Ty::Record { fields } => Ty::Record { fields: subst_ty_fields(fields, map) },
        Ty::OpenRecord { fields } => Ty::OpenRecord { fields: subst_ty_fields(fields, map) },
        Ty::Fn { params, ret } => Ty::Fn {
            params: params.iter().map(|a| subst(a, map)).collect(),
            ret: Box::new(subst(ret, map)),
        },
        Ty::Variant { name, cases } => subst_variant(*name, cases, map),
        Ty::ConstParam { name, ty } => {
            Ty::ConstParam { name: *name, ty: Box::new(subst(ty, map)) }
        }
        Ty::ConstValue { ty, value } => {
            Ty::ConstValue { ty: Box::new(subst(ty, map)), value: *value }
        }
        _ => ty.clone(),
    }
}

/// The `Ty::Named` arm of [`subst`] — extracted (uniform, self-contained
/// match arms, no shared state; same as [`erase_newtypes_in_type_decls`]'s
/// split above). The nominal replaces itself wholesale when it's a
/// zero-arg alias target; otherwise recurse into its type args.
fn subst_named(
    name: &almide_lang::intern::Sym,
    args: &[almide_lang::types::Ty],
    map: &std::collections::HashMap<String, almide_lang::types::Ty>,
) -> almide_lang::types::Ty {
    use almide_lang::types::Ty;
    if args.is_empty() {
        if let Some(t) = map.get(name.as_str()) {
            return t.clone();
        }
    }
    Ty::Named(*name, args.iter().map(|a| subst(a, map)).collect())
}

/// The `(Sym, Ty)` field-list substitution shared by `Ty::Record` and
/// `Ty::OpenRecord` in [`subst`] — was duplicated inline in both arms.
fn subst_ty_fields(
    fields: &[(almide_lang::intern::Sym, almide_lang::types::Ty)],
    map: &std::collections::HashMap<String, almide_lang::types::Ty>,
) -> Vec<(almide_lang::intern::Sym, almide_lang::types::Ty)> {
    fields.iter().map(|(n, t)| (*n, subst(t, map))).collect()
}

/// The `Ty::Variant` arm of [`subst`] — extracted for the same reason as
/// [`subst_named`].
fn subst_variant(
    name: almide_lang::intern::Sym,
    cases: &[almide_lang::types::VariantCase],
    map: &std::collections::HashMap<String, almide_lang::types::Ty>,
) -> almide_lang::types::Ty {
    use almide_lang::types::Ty;
    Ty::Variant {
        name,
        cases: cases.iter().map(|c| subst_variant_case(c, map)).collect(),
    }
}

/// The per-case payload substitution inside [`subst_variant`] — a uniform,
/// self-contained match over `VariantPayload` (each arm reads only its own
/// payload, no cross-arm state).
fn subst_variant_case(
    c: &almide_lang::types::VariantCase,
    map: &std::collections::HashMap<String, almide_lang::types::Ty>,
) -> almide_lang::types::VariantCase {
    use almide_lang::types::VariantPayload;
    almide_lang::types::VariantCase {
        name: c.name,
        payload: match &c.payload {
            VariantPayload::Unit => VariantPayload::Unit,
            VariantPayload::Tuple(ts) => {
                VariantPayload::Tuple(ts.iter().map(|a| subst(a, map)).collect())
            }
            VariantPayload::Record(fs) => VariantPayload::Record(subst_ty_fields(fs, map)),
        },
    }
}

/// Other type decls may carry alias-typed fields (a record holding a
/// SafeHtml) — the tail loop of [`erase_transparent_newtypes`], verbatim move.
fn erase_newtypes_in_type_decls(
    type_decls: &mut [almide_ir::IrTypeDecl],
    map: &std::collections::HashMap<String, almide_lang::types::Ty>,
) {
    for td in type_decls.iter_mut() {
        erase_newtypes_in_one_type_decl(td, map);
    }
}

/// One `td.kind` arm of [`erase_newtypes_in_type_decls`] — extracted, each
/// arm is self-contained (reads only its own `td`, writes only its own
/// fields), so this is a pure name-router split, no behavior change.
fn erase_newtypes_in_one_type_decl(
    td: &mut almide_ir::IrTypeDecl,
    map: &std::collections::HashMap<String, almide_lang::types::Ty>,
) {
    match &mut td.kind {
        almide_ir::IrTypeDeclKind::Record { fields } => {
            for f in fields.iter_mut() {
                f.ty = subst(&f.ty, map);
            }
        }
        almide_ir::IrTypeDeclKind::Variant { cases, .. } => {
            for c in cases.iter_mut() {
                erase_newtypes_in_variant_case(c, map);
            }
        }
        almide_ir::IrTypeDeclKind::Alias { .. } => {}
    }
}

/// The `c.kind` arm of [`erase_newtypes_in_one_type_decl`]'s `Variant` case —
/// extracted for the same reason (uniform, self-contained match arms).
fn erase_newtypes_in_variant_case(
    c: &mut almide_ir::IrVariantDecl,
    map: &std::collections::HashMap<String, almide_lang::types::Ty>,
) {
    match &mut c.kind {
        almide_ir::IrVariantKind::Unit => {}
        almide_ir::IrVariantKind::Tuple { fields } => {
            for t in fields.iter_mut() {
                *t = subst(t, map);
            }
        }
        almide_ir::IrVariantKind::Record { fields } => {
            for f in fields.iter_mut() {
                f.ty = subst(&f.ty, map);
            }
        }
    }
}

/// The eraser's read-only substitution context — hoisted to module scope (was
/// a local `struct`/`impl` nested in [`erase_transparent_newtypes`]) so the
/// sequential-phase helpers below ([`build_newtype_substitution_map`],
/// [`erase_newtypes_in_program_body`]) can share it without threading it as a
/// closure. Depends on nothing local to the caller (`map` is a plain
/// borrowed field), so hoisting is a pure move, no behavior change.
struct NewtypeEraser<'a> {
    map: &'a std::collections::HashMap<String, almide_lang::types::Ty>,
}
impl NewtypeEraser<'_> {
    fn subst(&self, ty: &almide_lang::types::Ty) -> almide_lang::types::Ty {
        subst(ty, self.map)
    }
}
impl almide_ir::visit_mut::IrMutVisitor for NewtypeEraser<'_> {
    fn visit_expr_mut(&mut self, e: &mut almide_ir::IrExpr) {
        almide_ir::visit_mut::walk_expr_mut(self, e);
        e.ty = self.subst(&e.ty);
        self.subst_lambda_param_tys(e);
        self.erase_newtype_ctor_call(e);
        self.erase_unit_bind_match(e);
    }
    fn visit_stmt_mut(&mut self, s: &mut almide_ir::IrStmt) {
        use almide_ir::IrStmtKind;
        almide_ir::visit_mut::walk_stmt_mut(self, s);
        if let IrStmtKind::Bind { ty, .. } = &mut s.kind {
            *ty = self.subst(ty);
        }
    }
    fn visit_pattern_mut(&mut self, p: &mut almide_ir::IrPattern) {
        use almide_ir::IrPattern;
        almide_ir::visit_mut::walk_pattern_mut(self, p);
        if let IrPattern::Bind { ty, .. } = p {
            *ty = self.subst(ty);
        }
        // The 1-arg newtype ctor PATTERN always matches — it IS the inner pattern.
        let inner = if let IrPattern::Constructor { name, args } = p {
            if args.len() == 1 && self.map.contains_key(name.as_str()) {
                Some(args.remove(0))
            } else {
                None
            }
        } else {
            None
        };
        if let Some(ip) = inner {
            *p = ip;
        }
    }
}
impl NewtypeEraser<'_> {
    /// `visit_expr_mut`'s Lambda-param-type substitution — extracted (one of
    /// 3 independent phases run in sequence on the SAME `e`, each re-reading
    /// `e.kind` fresh; none depends on another's outcome except through `e`
    /// itself, which each phase already re-checks). No behavior change.
    fn subst_lambda_param_tys(&self, e: &mut almide_ir::IrExpr) {
        if let almide_ir::IrExprKind::Lambda { params, .. } = &mut e.kind {
            for (_, ty) in params.iter_mut() {
                *ty = self.subst(ty);
            }
        }
    }

    /// The 1-arg newtype ctor CALL is the payload itself (same block, same
    /// ownership — the arg was already erased/visited by the caller).
    fn erase_newtype_ctor_call(&self, e: &mut almide_ir::IrExpr) {
        use almide_ir::{CallTarget, IrExprKind};
        let is_newtype_ctor = matches!(&e.kind,
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. }
                if args.len() == 1 && self.map.contains_key(name.as_str()));
        if is_newtype_ctor {
            if let IrExprKind::Call { args, .. } = &mut e.kind {
                let payload = args.pop().expect("1-arg ctor");
                *e = payload;
            }
        }
    }

    /// A match REDUCED to one bare-Bind arm by the pattern erasure
    /// (`match h { SafeHtml(s) => s }` → `match h { s => s }`) is a `let`:
    /// `{ let s = h; body }`. Count-invariant (subject + body appear once).
    fn erase_unit_bind_match(&self, e: &mut almide_ir::IrExpr) {
        use almide_ir::{IrExpr, IrExprKind, IrPattern, IrStmt};
        let is_unit_bind_match = matches!(&e.kind,
            IrExprKind::Match { arms, .. }
                if arms.len() == 1
                    && arms[0].guard.is_none()
                    && matches!(arms[0].pattern, IrPattern::Bind { .. }));
        if is_unit_bind_match {
            if let IrExprKind::Match { subject, arms } = &mut e.kind {
                let arm = arms.pop().expect("1 arm");
                if let IrPattern::Bind { var, ty } = arm.pattern {
                    let span = e.span.clone();
                    let bind = IrStmt {
                        kind: almide_ir::IrStmtKind::Bind {
                            var,
                            ty,
                            value: (**subject).clone(),
                            mutability: almide_ir::Mutability::Let,
                        },
                        span: span.clone(),
                    };
                    let body_ty = arm.body.ty.clone();
                    let def_id = arm.body.def_id;
                    *e = IrExpr {
                        kind: IrExprKind::Block {
                            stmts: vec![bind],
                            expr: Some(Box::new(arm.body)),
                        },
                        ty: body_ty,
                        span,
                        def_id,
                    };
                }
            }
        }
    }
}

/// [`erase_transparent_newtypes`]'s per-`IrFunction` rewrite (hoisted, was a
/// local fn — needs no local state, a pure move).
fn rewrite_newtype_erased_fns(
    v: &mut NewtypeEraser<'_>,
    map: &std::collections::HashMap<String, almide_lang::types::Ty>,
    fns: &mut [almide_ir::IrFunction],
) {
    use almide_ir::visit_mut::IrMutVisitor;
    for f in fns.iter_mut() {
        for p in f.params.iter_mut() {
            p.ty = subst(&p.ty, map);
        }
        f.ret_ty = subst(&f.ret_ty, map);
        v.visit_expr_mut(&mut f.body);
    }
}

/// Phase 1 of [`erase_transparent_newtypes`]: collect every transparent-alias
/// mapping (program + module type decls + the self-host opaque-nominal
/// table) and resolve alias-of-alias chains to a fixpoint. Extracted
/// verbatim — an empty result short-circuits the caller exactly as the
/// inlined `if map.is_empty() { return; }` did.
fn build_newtype_substitution_map(
    program: &almide_ir::IrProgram,
) -> std::collections::HashMap<String, almide_lang::types::Ty> {
    use almide_ir::IrTypeDeclKind;
    use almide_lang::types::Ty;
    use std::collections::HashMap;

    fn collect(decls: &[almide_ir::IrTypeDecl], map: &mut HashMap<String, Ty>) {
        for td in decls {
            if let IrTypeDeclKind::Alias { target } = &td.kind {
                if td.generics.is_none() {
                    map.insert(td.name.as_str().to_string(), target.clone());
                }
            }
        }
    }

    let mut map: HashMap<String, Ty> = HashMap::new();
    collect(&program.type_decls, &mut map);
    for m in &program.modules {
        collect(&m.type_decls, &mut map);
    }
    // SELF-HOST REP table: opaque STDLIB nominals whose v1 self-host owns the
    // representation (`stdlib/json_path.almd` — JsonPath = a List[String] of
    // segments). Publishing the rep here makes every user-side bind/param of the
    // opaque type route the CORRECT drop (heap_elem_list str). Only when the
    // program does not declare its own type of the same name.
    let declared: std::collections::HashSet<&str> = program
        .type_decls
        .iter()
        .map(|td| td.name.as_str())
        .chain(program.modules.iter().flat_map(|m| m.type_decls.iter().map(|td| td.name.as_str())))
        .collect();
    seed_selfhost_newtype_reps(&mut map, &declared);
    if map.is_empty() {
        return map;
    }
    // Resolve alias-of-alias chains to a fixpoint (bounded — a cycle would be a
    // frontend error; the bound just keeps this total).
    for _ in 0..8 {
        let snapshot = map.clone();
        let mut changed = false;
        for t in map.values_mut() {
            let nt = subst(t, &snapshot);
            if nt != *t {
                *t = nt;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    map
}

/// Phase 2 of [`erase_transparent_newtypes`]: apply the substitution to every
/// region of the program (top-level functions, then each module's functions
/// + top-lets, then the program's own top-lets). Each region is disjoint
/// (writes only its own `program`/`m` field) and reads only the shared
/// read-only `map`/`v` — the established sequential-phase-decomposition
/// pattern, no shared mutable accumulator threaded across regions.
fn erase_newtypes_in_program_body(
    v: &mut NewtypeEraser<'_>,
    map: &std::collections::HashMap<String, almide_lang::types::Ty>,
    program: &mut almide_ir::IrProgram,
) {
    use almide_ir::visit_mut::IrMutVisitor;
    rewrite_newtype_erased_fns(v, map, &mut program.functions);
    let mut modules = std::mem::take(&mut program.modules);
    for m in modules.iter_mut() {
        rewrite_newtype_erased_fns(v, map, &mut m.functions);
        for tl in m.top_lets.iter_mut() {
            tl.ty = subst(&tl.ty, map);
            v.visit_expr_mut(&mut tl.value);
        }
    }
    program.modules = modules;
    for tl in program.top_lets.iter_mut() {
        tl.ty = subst(&tl.ty, map);
        v.visit_expr_mut(&mut tl.value);
    }
}

pub fn erase_transparent_newtypes(program: &mut almide_ir::IrProgram) {
    let map = build_newtype_substitution_map(program);
    if map.is_empty() {
        return;
    }
    let mut v = NewtypeEraser { map: &map };
    erase_newtypes_in_program_body(&mut v, &map, program);
    // Other type decls may carry alias-typed fields (a record holding a SafeHtml).
    erase_newtypes_in_type_decls(&mut program.type_decls, &map);
}

/// INLINE-SUBSTITUTE pure call-bearing GLOBAL initializers at their use sites (a
/// program-level pre-pass, run right after [`erase_transparent_newtypes`] in BOTH the
/// pipeline and classify IR construction — desugar-before-both by construction).
///
/// `let BANNER = make_banner()` (the #632 / C-077 family) cannot materialize at a USE
/// site under the count discipline (the reference is a `Var` = 0 IR calls; injecting the
/// CallFn would breach `mir == ir`), and an eager `__init_globals` prologue is a whole
/// new count/ownership subsystem. But v0 NATIVE globals are LAZY statics — every use
/// evaluates the initializer's VALUE — so for a PURE initializer, substituting the init
/// EXPRESSION at each use site is byte-equivalent (same value each time; v0-wasm's
/// dependency-sorted eager init is pinned observably equal by C-077). The substitution
/// happens in the SHARED IR both the lowering and `count_ir_calls` read, so the call
/// counts stay 1:1.
///
/// GATES: (a) the init CONTAINS a call (const inits keep the existing materialization);
/// (b) the init is transitively PURE — every `Named` callee is a non-`effect` fn whose
/// body (transitively) makes only pure-module/Named calls (no RuntimeCall, no impure
/// Module call) — an effectful init keeps walling (substitution would re-run the
/// effect per use, an observable divergence); (c) REGION-LOCAL — main top-lets
/// substitute into main functions/top-lets, a module's into its own functions — the
/// main/module VarId numbering regions can collide, so cross-region substitution by
/// raw VarId would hit unrelated locals (the bridge owns cross-module reads).
/// Self-referencing inits never substitute into their own init (cycle guard); chained
/// call-globals resolve by a bounded fixpoint.
pub fn inline_pure_call_globals(program: &mut almide_ir::IrProgram) {
    use almide_ir::{CallTarget, IrExpr, IrExprKind};
    use std::collections::{HashMap, HashSet};

    // Function registry by name (program + modules) for the transitive purity scan.
    let mut fns_by_name: HashMap<String, IrExpr> = HashMap::new();
    let mut effect_fns: HashSet<String> = HashSet::new();
    for f in &program.functions {
        fns_by_name.insert(f.name.as_str().to_string(), f.body.clone());
        if f.is_effect {
            effect_fns.insert(f.name.as_str().to_string());
        }
    }
    for m in &program.modules {
        for f in &m.functions {
            let qualified = format!("{}.{}", m.name.as_str(), f.name.as_str());
            fns_by_name.insert(f.name.as_str().to_string(), f.body.clone());
            fns_by_name.insert(qualified.clone(), f.body.clone());
            if f.is_effect {
                effect_fns.insert(f.name.as_str().to_string());
                effect_fns.insert(qualified);
            }
        }
    }

    fn expr_is_pure(
        e: &IrExpr,
        fns: &HashMap<String, IrExpr>,
        effects: &HashSet<String>,
        visiting: &mut HashSet<String>,
    ) -> bool {
        use almide_ir::visit::{walk_expr, IrVisitor};
        struct V<'a> {
            ok: bool,
            fns: &'a HashMap<String, IrExpr>,
            effects: &'a HashSet<String>,
            visiting: &'a mut HashSet<String>,
        }
        impl IrVisitor for V<'_> {
            fn visit_expr(&mut self, e: &IrExpr) {
                if !self.ok {
                    return;
                }
                match &e.kind {
                    IrExprKind::RuntimeCall { .. } => self.ok = false,
                    IrExprKind::Call { target, .. } | IrExprKind::TailCall { target, .. } => {
                        match target {
                            CallTarget::Module { module, func, .. } => {
                                self.mark_impure_if_module_call_impure(
                                    module.as_str(),
                                    func.as_str(),
                                );
                            }
                            CallTarget::Named { name } => {
                                self.mark_impure_if_named_call_impure(name.as_str());
                            }
                            // A Method/Computed callee is unanalyzable here — decline.
                            _ => self.ok = false,
                        }
                    }
                    _ => {}
                }
                walk_expr(self, e);
            }
        }
        impl V<'_> {
            /// The `CallTarget::Module` callee-purity check for `V::visit_expr` — routes an
            /// impure/unknown callee to `self.ok = false`. Verbatim extraction (guard-clause
            /// flattening) of the former inline if-else-if / match nesting, no behavior
            /// change — see docs/roadmap/active/code-health-codopsy.md.
            fn mark_impure_if_module_call_impure(&mut self, module: &str, func: &str) {
                if crate::purity::is_pure(module, func) {
                    return;
                }
                // Not a known-pure STDLIB call — but a USER module's fn (`let gray_50 =
                // v.rgb(…)` calling view.rgb, the ceangal theme class) is in the registry
                // under its QUALIFIED name: recurse into its body exactly like a Named
                // callee (the same cycle guard applies). An unknown qualified name stays
                // impure (declines).
                let q = format!("{module}.{func}");
                if self.effects.contains(&q) {
                    self.ok = false;
                    return;
                }
                if !self.visiting.insert(q.clone()) {
                    return;
                }
                match self.fns.get(&q) {
                    Some(body) => {
                        let body = body.clone();
                        if !expr_is_pure(&body, self.fns, self.effects, self.visiting) {
                            self.ok = false;
                        }
                    }
                    None => self.ok = false,
                }
            }

            /// The `CallTarget::Named` sibling of
            /// [`Self::mark_impure_if_module_call_impure`]. Verbatim extraction, no behavior
            /// change.
            fn mark_impure_if_named_call_impure(&mut self, name: &str) {
                let n = name.to_string();
                if self.effects.contains(&n) {
                    self.ok = false;
                    return;
                }
                if !self.visiting.insert(n.clone()) {
                    return;
                }
                match self.fns.get(&n) {
                    Some(body) => {
                        let body = body.clone();
                        if !expr_is_pure(&body, self.fns, self.effects, self.visiting) {
                            self.ok = false;
                        }
                    }
                    // An unknown callee (a variant ctor is fine — no body, no effect;
                    // anything else unknown declines).
                    None => {}
                }
            }
        }
        let mut v = V { ok: true, fns, effects, visiting };
        v.visit_expr(e);
        v.ok
    }

    // One REGION: substitute qualifying globals into the given fns + sibling inits.
    fn run_region(
        top_lets: &mut [almide_ir::IrTopLet],
        fn_bodies: &mut [almide_ir::IrFunction],
        fns: &HashMap<String, IrExpr>,
        effects: &HashSet<String>,
    ) {
        let qualifying: Vec<(almide_ir::VarId, IrExpr)> = top_lets
            .iter()
            // A MUTABLE `var` must NEVER inline: substituting its INITIALIZER into a use
            // site freezes the read at the init value (writes through the global slot
            // become invisible — `speeds[i]` read `list.repeat(0.0, 4)[i]` = 0.0 forever).
            .filter(|tl| !tl.mutable)
            .filter(|tl| crate::lower::expr_contains_call(&tl.value))
            .filter(|tl| {
                let mut visiting = HashSet::new();
                expr_is_pure(&tl.value, fns, effects, &mut visiting)
            })
            .map(|tl| (tl.var, tl.value.clone()))
            .collect();
        if qualifying.is_empty() {
            return;
        }
        // Bounded fixpoint: a chained call-global (`let A = f(); let B = g(A)`) resolves
        // in ≤ chain-depth rounds; the cycle guard is the self-substitution skip.
        for _ in 0..4 {
            let mut changed = false;
            for (var, init) in &qualifying {
                if substitute_one_global_in_region(*var, init, fn_bodies, top_lets) {
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }
    // One `(var, init)` substitution pass over BOTH regions ([`run_region`]'s
    // former inner two `for` loops) — extracted so the fixpoint driver above
    // only sees a single per-var step. Returns whether anything changed,
    // exactly the OR the inlined `changed = true` writes accumulated —
    // same mutation order, same convergence criterion, no behavior change.
    fn substitute_one_global_in_region(
        var: almide_ir::VarId,
        init: &IrExpr,
        fn_bodies: &mut [almide_ir::IrFunction],
        top_lets: &mut [almide_ir::IrTopLet],
    ) -> bool {
        let mut changed = false;
        for f in fn_bodies.iter_mut() {
            let nb = almide_ir::substitute_var_in_expr(&f.body, var, init);
            if !exprs_eq_shallow(&nb, &f.body) {
                f.body = nb;
                changed = true;
            }
        }
        for tl in top_lets.iter_mut() {
            if tl.var == var {
                continue;
            }
            let nv = almide_ir::substitute_var_in_expr(&tl.value, var, init);
            if !exprs_eq_shallow(&nv, &tl.value) {
                tl.value = nv;
                changed = true;
            }
        }
        changed
    }
    // Cheap change detector: substitution either changes the tree or returns an
    // identical clone — compare the debug forms (bounded corpora; not hot).
    fn exprs_eq_shallow(a: &almide_ir::IrExpr, b: &almide_ir::IrExpr) -> bool {
        format!("{a:?}") == format!("{b:?}")
    }

    let fns_snapshot = fns_by_name;
    let effects_snapshot = effect_fns;
    run_region(
        &mut program.top_lets,
        &mut program.functions,
        &fns_snapshot,
        &effects_snapshot,
    );
    let mut modules = std::mem::take(&mut program.modules);
    for m in modules.iter_mut() {
        run_region(&mut m.top_lets, &mut m.functions, &fns_snapshot, &effects_snapshot);
    }
    program.modules = modules;
}
