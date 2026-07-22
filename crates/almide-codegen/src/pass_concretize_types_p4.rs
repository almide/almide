
/// True when `expr` is a value-position `Ty::Never` violation. `Ty::Never` is
/// legitimate for *divergent* expressions — `break` / `continue` / `todo()` /
/// a hole never yield a value, and a call to a `-> Never` function diverges. It
/// is a BUG only when a node that DOES produce a usable runtime value is typed
/// `Never`: emit would then have to materialize a value of an uninhabited type,
/// the value-Never analogue of the `Unknown→i32` fallback. Mirrors the wasm
/// `ty_to_valtype` convention where `Never` maps to "no value" (`None`).
fn is_value_never(expr: &IrExpr) -> bool {
    if expr.ty != Ty::Never { return false; }
    // Inherently-divergent kinds are *allowed* to be Never.
    !matches!(&expr.kind,
        IrExprKind::Break | IrExprKind::Continue
        | IrExprKind::Hole | IrExprKind::Todo { .. }
        // A call / runtime-call may legitimately be a `-> Never` divergent
        // function (panic, exit). Control-flow joins (If/Match/Block) inherit
        // Never from a diverging branch and are fine. Returning/propagation
        // wrappers likewise carry through a diverging inner.
        | IrExprKind::Call { .. } | IrExprKind::TailCall { .. }
        | IrExprKind::RuntimeCall { .. } | IrExprKind::RustMacro { .. }
        | IrExprKind::If { .. } | IrExprKind::Match { .. }
        | IrExprKind::Block { .. }
        | IrExprKind::Try { .. } | IrExprKind::Unwrap { .. }
    )
}

/// Walk every reachable expression and collect residual unresolved-type (or
/// value-`Never`) sites that are NOT covered by [`is_legit_unresolved`]. This
/// is the shared engine behind the soft audit and the hard gate.
pub fn collect_unresolved_sites(program: &IrProgram) -> Vec<UnresolvedSite> {
    struct Auditor<'a> {
        location: String,
        sites: Vec<UnresolvedSite>,
        var_table: &'a VarTable,
    }
    impl<'a> Auditor<'a> {
        fn detail_of(&self, expr: &IrExpr) -> String {
            match &expr.kind {
                IrExprKind::Var { id } => {
                    if (id.0 as usize) < self.var_table.entries.len() {
                        let info = &self.var_table.entries[id.0 as usize];
                        format!("var_id={} name={} stored_ty={:?}", id.0, info.name.as_str(), info.ty)
                    } else {
                        format!("var_id={}", id.0)
                    }
                }
                IrExprKind::Member { field, .. } => format!("member={}", field.as_str()),
                IrExprKind::Call { .. } => "(call)".to_string(),
                _ => String::new(),
            }
        }
        fn record(&mut self, expr: &IrExpr, value_never: bool) {
            let detail = self.detail_of(expr);
            self.sites.push(UnresolvedSite {
                location: self.location.clone(),
                kind: kind_name(&expr.kind),
                ty: format!("{:?}", expr.ty),
                span: expr.span,
                detail,
                value_never,
            });
        }
    }
    impl<'a> IrVisitor for Auditor<'a> {
        fn visit_expr(&mut self, expr: &IrExpr) {
            if (expr.ty).has_unresolved_deep() {
                if !is_legit_unresolved(expr) {
                    self.record(expr, false);
                }
            } else if is_value_never(expr) {
                self.record(expr, true);
            }
            walk_expr(self, expr);
        }
        fn visit_stmt(&mut self, stmt: &IrStmt) {
            walk_stmt(self, stmt);
        }
    }
    /// Split into 4 groups (cog>30 decomposition, pattern 1 — independent
    /// name-router arms, mirrors the `list_call_name` recipe) since the
    /// original 57-arm match was itself the whole complexity source.
    fn kind_name(k: &IrExprKind) -> &'static str {
        kind_name_group_a(k)
            .or_else(|| kind_name_group_b(k))
            .or_else(|| kind_name_group_c(k))
            .unwrap_or_else(|| kind_name_group_d(k))
    }
    fn kind_name_group_a(k: &IrExprKind) -> Option<&'static str> {
        Some(match k {
            IrExprKind::LitInt { .. } => "LitInt",
            IrExprKind::LitFloat { .. } => "LitFloat",
            IrExprKind::LitStr { .. } => "LitStr",
            IrExprKind::LitBool { .. } => "LitBool",
            IrExprKind::Unit => "Unit",
            IrExprKind::Var { .. } => "Var",
            IrExprKind::FnRef { .. } => "FnRef",
            IrExprKind::BinOp { .. } => "BinOp",
            IrExprKind::UnOp { .. } => "UnOp",
            IrExprKind::If { .. } => "If",
            IrExprKind::Match { .. } => "Match",
            IrExprKind::Block { .. } => "Block",
            IrExprKind::Fan { .. } => "Fan",
            IrExprKind::ForIn { .. } => "ForIn",
            _ => return None,
        })
    }
    fn kind_name_group_b(k: &IrExprKind) -> Option<&'static str> {
        Some(match k {
            IrExprKind::While { .. } => "While",
            IrExprKind::Call { .. } => "Call",
            IrExprKind::TailCall { .. } => "TailCall",
            IrExprKind::List { .. } => "List",
            IrExprKind::MapLiteral { .. } => "MapLiteral",
            IrExprKind::Record { .. } => "Record",
            IrExprKind::SpreadRecord { .. } => "SpreadRecord",
            IrExprKind::Tuple { .. } => "Tuple",
            IrExprKind::Range { .. } => "Range",
            IrExprKind::Member { .. } => "Member",
            IrExprKind::TupleIndex { .. } => "TupleIndex",
            IrExprKind::IndexAccess { .. } => "IndexAccess",
            IrExprKind::MapAccess { .. } => "MapAccess",
            IrExprKind::Lambda { .. } => "Lambda",
            _ => return None,
        })
    }
    fn kind_name_group_c(k: &IrExprKind) -> Option<&'static str> {
        Some(match k {
            IrExprKind::ClosureCreate { .. } => "ClosureCreate",
            IrExprKind::EnvLoad { .. } => "EnvLoad",
            IrExprKind::ResultOk { .. } => "ResultOk",
            IrExprKind::ResultErr { .. } => "ResultErr",
            IrExprKind::Try { .. } => "Try",
            IrExprKind::Unwrap { .. } => "Unwrap",
            IrExprKind::UnwrapOr { .. } => "UnwrapOr",
            IrExprKind::ToOption { .. } => "ToOption",
            IrExprKind::OptionalChain { .. } => "OptionalChain",
            IrExprKind::OptionSome { .. } => "OptionSome",
            IrExprKind::OptionNone => "OptionNone",
            IrExprKind::Break => "Break",
            IrExprKind::Continue => "Continue",
            IrExprKind::StringInterp { .. } => "StringInterp",
            _ => return None,
        })
    }
    fn kind_name_group_d(k: &IrExprKind) -> &'static str {
        match k {
            IrExprKind::RenderedCall { .. } => "RenderedCall",
            IrExprKind::RuntimeCall { .. } => "RuntimeCall",
            IrExprKind::InlineRust { .. } => "InlineRust",
            IrExprKind::RustMacro { .. } => "RustMacro",
            IrExprKind::Clone { .. } => "Clone",
            IrExprKind::Deref { .. } => "Deref",
            IrExprKind::Borrow { .. } => "Borrow",
            IrExprKind::BoxNew { .. } => "BoxNew",
            IrExprKind::RcWrap { .. } => "RcWrap",
            IrExprKind::ToVec { .. } => "ToVec",
            IrExprKind::Await { .. } => "Await",
            IrExprKind::Todo { .. } => "Todo",
            IrExprKind::Hole => "Hole",
            IrExprKind::IterChain { .. } => "IterChain",
            IrExprKind::EmptyMap => "EmptyMap",
            _ => "(unknown-variant)",
        }
    }
    let mut a = Auditor { location: String::new(), sites: Vec::new(), var_table: &program.var_table };
    for f in &program.functions {
        a.location = format!("fn {}", f.name);
        a.visit_expr(&f.body);
    }
    for m in &program.modules {
        let mname = m.name.to_string();
        for f in &m.functions {
            a.location = format!("{}::{}", mname, f.name);
            a.visit_expr(&f.body);
        }
    }
    a.sites
}

/// Render one site as a one-line span-tagged description.
fn render_site(s: &UnresolvedSite) -> String {
    let loc = match s.span {
        Some(sp) => format!("{}:{}:{}", s.location, sp.line, sp.col),
        None => format!("{} <no span>", s.location),
    };
    let detail = if s.detail.is_empty() { String::new() } else { format!(" {}", s.detail) };
    let what = if s.value_never { "value-Never" } else { "unresolved" };
    format!("[{}] {} {} ty={}{}", loc, what, s.kind, s.ty, detail)
}

/// Postcondition audit (soft): used by the mid-pipeline `ConcretizeTypes`
/// postcondition. Returns a single summary violation string when any residual
/// site survives, formatted like the historical message so existing log
/// scrapers (`grep POSTCONDITION VIOLATION`) keep working.
fn audit_remaining_unresolved(program: &IrProgram) -> Vec<String> {
    let sites = collect_unresolved_sites(program);
    if sites.is_empty() { return Vec::new(); }
    let samples: Vec<String> = sites.iter().take(5).map(render_site).collect();
    vec![format!("[ConcretizeTypes] {} expression(s) remain with unresolved types. Samples: {}",
        sites.len(), samples.join(" | "))]
}

/// HARD codegen-entry gate. Runs on EVERY build (debug AND release, Rust AND
/// WASM) right before emit. If any reachable expression still carries a
/// `Ty::Unknown`/`Ty::TypeVar` (or a value-position `Ty::Never`) that the
/// concretization machinery could not resolve, this is a COMPILER bug — emit
/// would otherwise fall back to `i32` on WASM (the `fan.map` silent-miscompile
/// class) or to an arbitrary type on Rust. We refuse to emit and abort with a
/// clean, structured diagnostic that names the function + span. This is a
/// compiler-bug detector, so the message targets compiler developers ("please
/// report"); it is a controlled error, NOT an ICE (no panic, no backtrace).
///
/// The detection ([`collect_unresolved_sites`]) is a pure function, unit-tested
/// directly; this wrapper only adds the formatting + abort so the test process
/// is never killed.
pub fn assert_types_concretized(program: &IrProgram) {
    let sites = collect_unresolved_sites(program);
    if sites.is_empty() { return; }

    let mut msg = String::new();
    msg.push_str("error: [COMPILER BUG] internal type resolution failed before codegen\n");
    msg.push_str(&format!(
        "  {} expression(s) still carry an unresolved (Unknown/TypeVar) or value-Never type\n",
        sites.len()
    ));
    msg.push_str("  after the ConcretizeTypes pass. Emitting these would silently fall back to a\n");
    msg.push_str("  wrong runtime representation (e.g. the WASM Unknown→i32 fallback), so the build\n");
    msg.push_str("  is refused instead. This is a compiler bug, not an error in your program.\n");
    // Cap the listed sites so a pathological program can't flood the terminal;
    // the count above always reflects the true total.
    const MAX_LISTED: usize = 20;
    for s in sites.iter().take(MAX_LISTED) {
        msg.push_str(&format!("    {}\n", render_site(s)));
    }
    if sites.len() > MAX_LISTED {
        msg.push_str(&format!("    ... and {} more\n", sites.len() - MAX_LISTED));
    }
    msg.push_str("  hint: please report this at https://github.com/almide/almide/issues\n");
    msg.push_str("        with the source above — include the function name(s) and span(s) shown.\n");

    eprint!("{}", msg);
    // Controlled abort: print the diagnostic, then terminate the build with a
    // non-zero status. This mirrors the established codegen failure convention
    // (`main.rs` build paths, the generated div-by-zero runtime) — a clean
    // process exit, NOT a `panic!` that would dump a Rust backtrace (an ICE) or
    // be swallowed as a "skip" by the spec harness's `catch_unwind`. The
    // collector is unit-tested separately so this branch never runs under
    // `cargo test` for a well-formed program.
    std::process::exit(1);
}

#[cfg(test)]
mod hard_gate_tests {
    use super::*;
    use almide_ir::{IrFunction, IrVisibility, Mutability};

    fn expr(kind: IrExprKind, ty: Ty) -> IrExpr {
        IrExpr { kind, ty, span: None, def_id: None }
    }

    fn make_fn(name: &str, body: IrExpr) -> IrFunction {
        IrFunction {
            name: name.into(),
            params: vec![],
            ret_ty: body.ty.clone(),
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
            module_origin: None,
        }
    }

    fn program(body: IrExpr, var_table: VarTable) -> IrProgram {
        IrProgram {
            functions: vec![make_fn("main", body)],
            top_lets: vec![],
            type_decls: vec![],
            var_table,
            def_table: Default::default(),
            modules: vec![],
            type_registry: Default::default(),
            effect_fn_names: Default::default(),
            effect_map: Default::default(),
            codegen_annotations: Default::default(),
            used_stdlib_modules: Default::default(),
        }
    }

    /// A synthetic Unknown-carrying IR (a `Member` access typed Unknown — NOT
    /// in the legitimate-residual skip list) must fail the postcondition / gate.
    #[test]
    fn synthetic_unknown_member_is_a_violation() {
        let mut vt = VarTable::new();
        let rec = vt.alloc("rec".into(), Ty::Unknown, Mutability::Let, None);
        let body = expr(
            IrExprKind::Member {
                object: Box::new(expr(IrExprKind::Var { id: rec }, Ty::Unknown)),
                field: "field".into(),
            },
            Ty::Unknown,
        );
        let prog = program(body, vt);
        let sites = collect_unresolved_sites(&prog);
        // The Member node and the Var node both carry Unknown → at least one
        // violation, none of them whitelisted.
        assert!(!sites.is_empty(), "Unknown Member must be flagged");
        assert!(sites.iter().any(|s| s.kind == "Member"), "Member site expected: {sites:?}");
        assert!(sites.iter().all(|s| !s.value_never), "these are Unknown, not Never");
        // The soft audit must agree with the hard collector.
        assert!(!audit_remaining_unresolved(&prog).is_empty());
    }

    /// A fully-concrete program produces zero sites (the gate is silent).
    #[test]
    fn concrete_program_has_no_sites() {
        let body = expr(IrExprKind::LitInt { value: 7 }, Ty::Int);
        let prog = program(body, VarTable::new());
        assert!(collect_unresolved_sites(&prog).is_empty());
        assert!(audit_remaining_unresolved(&prog).is_empty());
    }

    /// Whitelisted residuals (empty list literal, OptionNone) are NOT flagged,
    /// so the hard gate does not regress programs the soft audit accepted.
    #[test]
    fn whitelisted_residuals_are_not_violations() {
        let empty_list = expr(
            IrExprKind::List { elements: vec![] },
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, vec![Ty::Unknown]),
        );
        let prog = program(empty_list, VarTable::new());
        assert!(collect_unresolved_sites(&prog).is_empty(), "empty `[]` is whitelisted");

        let none = expr(IrExprKind::OptionNone, Ty::Applied(
            almide_lang::types::constructor::TypeConstructorId::Option, vec![Ty::Unknown]));
        let prog = program(none, VarTable::new());
        assert!(collect_unresolved_sites(&prog).is_empty(), "OptionNone is whitelisted");
    }

    /// A value-position `Ty::Never` (a `Var` typed Never) is a violation, but a
    /// divergent call typed Never is allowed — distinguishing "uninhabited value
    /// materialized" from "expression diverges".
    #[test]
    fn value_never_var_flagged_but_divergent_call_allowed() {
        let mut vt = VarTable::new();
        let v = vt.alloc("v".into(), Ty::Never, Mutability::Let, None);
        let body = expr(IrExprKind::Var { id: v }, Ty::Never);
        let prog = program(body, vt);
        let sites = collect_unresolved_sites(&prog);
        assert!(sites.iter().any(|s| s.value_never && s.kind == "Var"),
            "value-Never Var must be flagged: {sites:?}");

        // A diverging `todo()`-style hole / break is allowed to be Never.
        let brk = expr(IrExprKind::Break, Ty::Never);
        let prog = program(brk, VarTable::new());
        assert!(collect_unresolved_sites(&prog).is_empty(), "Break:Never is allowed");
    }

    /// An unannotated record whose Unknowns live ONLY in `Option`/`List` payload
    /// slots (an only-ever-`none` field, `let leaf = { value: 1, left: none }`)
    /// is whitelisted; the same record with an Unknown in a load-bearing slot
    /// (a `Result` Ok, a tuple element) is NOT — the gate stays strict there.
    #[test]
    fn unknown_only_in_empty_container_payloads_is_whitelisted() {
        use almide_lang::types::constructor::TypeConstructorId as TCI;
        let opt_unknown = Ty::Applied(TCI::Option, vec![Ty::Unknown]);
        let rec_ty = Ty::Record { fields: vec![
            ("value".into(), Ty::Int),
            ("left".into(), opt_unknown.clone()),
            ("right".into(), opt_unknown.clone()),
        ] };
        // A Var carrying this record type is whitelisted (payload-only Unknown).
        let mut vt = VarTable::new();
        let leaf = vt.alloc("leaf".into(), rec_ty.clone(), Mutability::Let, None);
        let prog = program(expr(IrExprKind::Var { id: leaf }, rec_ty.clone()), vt);
        assert!(collect_unresolved_sites(&prog).is_empty(),
            "record with Unknown only in Option payloads is whitelisted");

        // Direct predicate checks. The ONLY whitelisted bare-Unknown leaf is an
        // `Option` payload (a never-materialized `none`); every other undecidable
        // empty collection is now rejected in the frontend (E018) before it can
        // reach this gate, so the gate no longer whitelists them.
        assert!(unresolved_only_in_empty_payloads(&opt_unknown));
        assert!(unresolved_only_in_empty_payloads(&rec_ty));
        // A bare-Unknown List/Set element or Map value is NO LONGER whitelisted —
        // it would be an undecidable empty collection, which E018 rejects first,
        // so reaching here is a compiler bug.
        assert!(!unresolved_only_in_empty_payloads(&Ty::Applied(TCI::List, vec![Ty::Unknown])),
            "bare List[Unknown] is now an E018 the frontend owns — a gate violation");
        assert!(!unresolved_only_in_empty_payloads(&Ty::Applied(TCI::Set, vec![Ty::Unknown])),
            "bare Set[Unknown] is now an E018 the frontend owns — a gate violation");
        assert!(!unresolved_only_in_empty_payloads(&Ty::Applied(TCI::Map, vec![Ty::String, Ty::Unknown])),
            "bare Map[_, Unknown] is now an E018 the frontend owns — a gate violation");
        // A List/Set of only-`none` elements stays legit THROUGH the Option.
        assert!(unresolved_only_in_empty_payloads(&Ty::Applied(TCI::List, vec![opt_unknown.clone()])),
            "List[Option[Unknown]] (a list of nones) is legit via the Option payload");
        // Load-bearing Unknowns are NOT whitelisted.
        assert!(!unresolved_only_in_empty_payloads(&Ty::Unknown), "bare Unknown is load-bearing");
        assert!(!unresolved_only_in_empty_payloads(&Ty::Tuple(vec![Ty::Int, Ty::Unknown])),
            "tuple element Unknown is load-bearing");
        assert!(!unresolved_only_in_empty_payloads(&Ty::Applied(TCI::Result, vec![Ty::Unknown, Ty::String])),
            "Result Ok Unknown is load-bearing");
        // Map KEY Unknown is load-bearing.
        assert!(!unresolved_only_in_empty_payloads(&Ty::Applied(TCI::Map, vec![Ty::Unknown, Ty::Int])),
            "Map key Unknown is load-bearing");
        // A fully concrete type is never reported as payload-only.
        assert!(!unresolved_only_in_empty_payloads(&Ty::Int));
    }
}
