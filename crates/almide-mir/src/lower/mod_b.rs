
/// Does the BRIDGED module-side type agree with the main-side REFERENCE entry's
/// type, treating the reference side's `Unknown` as a wildcard STRUCTURALLY
/// (`Option[Unknown]` agrees with `Option[Cfg]` — the un-inferred synthesized
/// ref vs the refined module truth)? A concrete mismatch still refuses (the
/// honest unbound wall, never a wrong-typed alias).
fn bridged_ref_ty_agrees(bridged: &Ty, reference: &Ty) -> bool {
    match (bridged, reference) {
        (_, Ty::Unknown) => true,
        (Ty::Applied(a, xs), Ty::Applied(b, ys)) if a == b && xs.len() == ys.len() => {
            xs.iter().zip(ys).all(|(x, y)| bridged_ref_ty_agrees(x, y))
        }
        _ => bridged == reference,
    }
}

/// Refine an UNANNOTATED top-let's Unknown(-payload) type from its OPTION-ctor
/// initializer: `let MAYBE = some(Cfg { .. })` leaves the declared ty `Unknown`
/// (or the checker's partial `Option[Unknown]`) while the ctor's PAYLOAD expr
/// carries its real inferred type — `Option[payload.ty]` is the structural
/// truth, never a guess. Any other shape returns `None` (untouched).
// Named (codopsy cc) — a pure classification, no logic change.
fn is_unknown_option_payload_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    match ty {
        Ty::Unknown => true,
        Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1 && matches!(a[0], Ty::Unknown) => {
            true
        }
        _ => false,
    }
}

pub fn refine_option_toplet_ty(ty: &Ty, init: &almide_ir::IrExpr) -> Option<Ty> {
    if !is_unknown_option_payload_ty(ty) {
        return None;
    }
    let almide_ir::IrExprKind::OptionSome { expr } = &init.kind else { return None };
    if matches!(expr.ty, Ty::Unknown) {
        return None;
    }
    Some(Ty::option(expr.ty.clone()))
}

/// Repair UNKNOWN expression types the frontend leaves on CROSS-MODULE global
/// references (`v.white` — the ref entry's ty is un-inferred, so the whole fn
/// trips the AllTypesConcrete precondition): a `Var` whose id the bridged
/// globals map types gets that type, and a member read off a now-typed
/// STRUCTURAL record object gets its field's type. Types come from the
/// authoritative top-let declaration — never guessed.
pub fn repair_unknown_global_ref_tys(
    func: &mut almide_ir::IrFunction,
    globals: &std::collections::HashMap<almide_ir::VarId, Ty>,
) {
    use almide_ir::{walk_expr_mut, IrMutVisitor};
    // The Member arm's field lookup, named (codopsy cc) — a pure function of the
    // object's type and the field name, no visitor state.
    fn repair_unknown_member_ty(object_ty: &Ty, field: almide_lang::intern::Sym) -> Option<Ty> {
        let Ty::Record { fields } = object_ty else { return None };
        fields
            .iter()
            .find(|(n, _)| n.as_str() == field.as_str())
            .map(|(_, ft)| ft.clone())
    }

    struct R<'a> {
        globals: &'a std::collections::HashMap<almide_ir::VarId, Ty>,
    }
    impl IrMutVisitor for R<'_> {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e); // children first — the object types before the member
            // All 3 arms below share the SAME `e.ty == Unknown` guard (per-arm in the
            // original) — hoisted to one check up front (codopsy cc), semantically
            // identical since a non-Unknown `e.ty` always fell through to `_ => {}`.
            if !matches!(e.ty, Ty::Unknown) {
                return;
            }
            match &e.kind {
                IrExprKind::Var { id } => {
                    if let Some(t) = self.globals.get(id) {
                        e.ty = t.clone();
                    }
                }
                IrExprKind::Member { object, field } => {
                    if let Some(t) = repair_unknown_member_ty(&object.ty, *field) {
                        e.ty = t;
                    }
                }
                // The PARENT of a repaired member read stays Unknown too — a BinOp is
                // TYPE-DISPATCHED (AddFloat vs AddInt), so its result type is intrinsic.
                IrExprKind::BinOp { op, .. } => {
                    if let Some(t) = op.result_ty() {
                        e.ty = t;
                    }
                }
                _ => {}
            }
        }
    }
    let mut r = R { globals };
    r.visit_expr_mut(&mut func.body);
}

pub fn build_variant_layouts(type_decls: &[almide_ir::IrTypeDecl]) -> VariantLayouts {
    // Fold-with-append-only-accumulator split (codopsy8 complexity sweep): each `decl` is
    // processed independently and only APPENDS to `out` (never reads back an earlier
    // decl's contribution) — the established "fold is safer than router" pattern
    // (round5/round7/round8). Pure text-move, no logic change.
    let mut out = VariantLayouts::default();
    for decl in type_decls {
        build_variant_layouts_for_decl(decl, &mut out);
    }
    out
}

/// Extracted from `build_variant_layouts` (codopsy8 complexity sweep, per-decl phase):
/// a plain RECORD's field defaults ride the same map, keyed by the record TYPE name
/// (`AllDefault()` — the paren-empty ctor fills them in `try_lower_record_construct`; a
/// variant record-ctor keys by CTOR name in [`build_variant_layouts_variant_decl`]).
/// Verbatim.
fn build_variant_layouts_for_decl(decl: &almide_ir::IrTypeDecl, out: &mut VariantLayouts) {
    use almide_ir::IrTypeDeclKind;
    if let IrTypeDeclKind::Record { fields } = &decl.kind {
        for f in fields {
            if let Some(d) = &f.default {
                out.ctor_field_defaults
                    .entry(decl.name.as_str().to_string())
                    .or_default()
                    .insert(f.name.as_str().to_string(), d.clone());
            }
        }
        return;
    }
    let IrTypeDeclKind::Variant { cases, .. } = &decl.kind else {
        return;
    };
    build_variant_layouts_variant_decl(decl, cases, out);
}

/// Extracted from `build_variant_layouts` (codopsy8 complexity sweep, the `Variant` arm of
/// the per-decl phase): builds one type's [`VariantLayout`] (all its constructor cases +
/// the shared slot count) and registers each ctor's owning type. Verbatim.
fn build_variant_layouts_variant_decl(
    decl: &almide_ir::IrTypeDecl,
    cases: &[almide_ir::IrVariantDecl],
    out: &mut VariantLayouts,
) {
    let generics =
        decl.generics.as_ref().map(|gs| gs.iter().map(|g| g.name).collect()).unwrap_or_default();
    let type_name = decl.name.as_str().to_string();
    let mut case_layouts = Vec::with_capacity(cases.len());
    let mut max_arity = 0usize;
    for (tag, case) in cases.iter().enumerate() {
        let fields = build_variant_layouts_case_fields(case, out);
        max_arity = max_arity.max(fields.len());
        out.ctor_to_type.insert(case.name.as_str().to_string(), type_name.clone());
        case_layouts.push(VariantCaseLayout { ctor: case.name, tag: tag as u32, fields });
    }
    out.by_type.insert(
        type_name,
        VariantLayout {
            generics,
            cases: case_layouts,
            // slot 0 is the tag; slots 1.. are the widest constructor's fields, so all
            // constructors of the type share one block size (uniform alloc + sound `==`).
            slot_count: 1 + max_arity,
        },
    );
}

/// Extracted from `build_variant_layouts_variant_decl` (codopsy8 complexity sweep): one
/// constructor CASE's field list, by ctor kind. A `Record` case ALSO registers its field
/// defaults (keyed by CTOR name, unlike the plain-record arm above which keys by TYPE
/// name). Verbatim.
fn build_variant_layouts_case_fields(
    case: &almide_ir::IrVariantDecl,
    out: &mut VariantLayouts,
) -> Vec<(almide_lang::intern::Sym, Ty)> {
    use almide_ir::IrVariantKind;
    match &case.kind {
        IrVariantKind::Unit => Vec::new(),
        // A tuple constructor's positional fields get the same `_0`, `_1`, …
        // synthetic names v0 assigns, so field identity is shared across backends.
        IrVariantKind::Tuple { fields } => fields
            .iter()
            .enumerate()
            .map(|(i, ty)| (almide_lang::intern::sym(&format!("_{i}")), ty.clone()))
            .collect(),
        IrVariantKind::Record { fields } => {
            for f in fields {
                if let Some(d) = &f.default {
                    out.ctor_field_defaults
                        .entry(case.name.as_str().to_string())
                        .or_default()
                        .insert(f.name.as_str().to_string(), d.clone());
                }
            }
            fields.iter().map(|f| (f.name, f.ty.clone())).collect()
        }
    }
}

/// If `ty` names a user VARIANT in `variant_names`, return that name (the recursion target for a
/// nested-variant ctor field's drop). Handles the three variant-type surface forms.
fn variant_field_name(ty: &Ty, variant_names: &std::collections::HashSet<String>) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    let n = match ty {
        Ty::Named(n, _) => n.as_str().to_string(),
        Ty::Variant { name, .. } => name.as_str().to_string(),
        Ty::Applied(TypeConstructorId::UserDefined(n), _) => n.clone(),
        _ => return None,
    };
    variant_names.contains(&n).then_some(n)
}

/// A variant type NEEDS a generated recursive drop fn (`Op::DropVariant` → `$__drop_<T>`) iff some
/// ctor field is itself a user variant: a flat `rc_dec` of that nested block would leak its own
/// heap children. A String-only-field variant uses the masked `DropListStr` (ADT brick 5a/5c)
/// instead — no recursive fn. Used by both the generator and `try_lower_variant_ctor` (to choose
/// `DropVariant` tracking), so the two never disagree.
// A ctor field the generated `$__drop_<V>` can free: a nested variant (recurse), a String
// (rc_dec), a List[scalar] (flat rc_dec), an Option[scalar] (flat rc_dec — the 0-or-1 block
// owns no children), a List[<variant>] (per-element), a List[String] (per-element via the
// generic `__drop_list_str` — each element is an OWNED String handle a flat rc_dec of just
// the list block would leak), or a RECORD (recurse via `$__drop_<R>` / a scalar-only record's
// flat rc_dec — see the drop generator's field loop). Named (codopsy cc), pure — no logic
// change from the closure `variant_needs_recursive_drop` used to carry.
fn variant_drop_supported_heap_field(
    t: &Ty,
    variant_names: &std::collections::HashSet<String>,
    record_names: &std::collections::HashSet<String>,
) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    variant_field_name(t, variant_names).is_some()
        || matches!(t, Ty::Named(n, _) if record_names.contains(n.as_str()))
        || matches!(t, Ty::String)
        // A CLOSURE field: the generator's Fn arm frees the self-describing
        // closure block via `__drop_closure`.
        || matches!(t, Ty::Fn { .. })
        || matches!(t, Ty::Applied(TypeConstructorId::List, a)
            if a.len() == 1
                && (!is_heap_ty(&a[0])
                    || matches!(a[0], Ty::String)
                    || variant_field_name(&a[0], variant_names).is_some()))
        || matches!(t, Ty::Applied(TypeConstructorId::Option, a)
            if a.len() == 1 && !is_heap_ty(&a[0]))
}

fn variant_case_field_tys(kind: &almide_ir::IrVariantKind) -> Vec<&Ty> {
    use almide_ir::IrVariantKind;
    match kind {
        IrVariantKind::Unit => vec![],
        IrVariantKind::Tuple { fields } => fields.iter().collect(),
        IrVariantKind::Record { fields } => fields.iter().map(|f| &f.ty).collect(),
    }
}

/// Accumulator for [`variant_needs_recursive_drop`]'s per-case fold — each case's fields only
/// SET these flags (never read one back to decide behavior for a LATER case), so the fold-body
/// is independent per case and safe to factor into [`scan_variant_case_for_drop`] below.
#[derive(Default)]
struct VariantDropScan {
    any_heap: bool,
    all_supported: bool,
    has_variant_field: bool,
}

fn scan_variant_case_for_drop(
    tys: &[&Ty],
    variant_names: &std::collections::HashSet<String>,
    record_names: &std::collections::HashSet<String>,
    scan: &mut VariantDropScan,
) {
    for t in tys {
        if variant_field_name(t, variant_names).is_some() {
            scan.has_variant_field = true;
        }
        if is_heap_ty(t) {
            scan.any_heap = true;
            if !variant_drop_supported_heap_field(t, variant_names, record_names) {
                scan.all_supported = false;
            }
        }
    }
}

pub fn variant_needs_recursive_drop(
    decl: &almide_ir::IrTypeDecl,
    variant_names: &std::collections::HashSet<String>,
    record_names: &std::collections::HashSet<String>,
) -> bool {
    use almide_ir::IrTypeDeclKind;
    let IrTypeDeclKind::Variant { cases, .. } = &decl.kind else {
        return false;
    };
    let mut scan = VariantDropScan { all_supported: true, ..Default::default() };
    for c in cases {
        let tys = variant_case_field_tys(&c.kind);
        scan_variant_case_for_drop(&tys, variant_names, record_names, &mut scan);
    }
    // The ORIGINAL rule (a nested-variant field) OR the widened one: some heap field,
    // ALL of them freeable by the generator (String / List[scalar] / List[variant]) —
    // the GGUFValue shape (ValString + ValArray(List[GGUFValue])). A type with an
    // unsupported heap field (e.g. a Map) keeps needing=false → its list stays WALLED
    // (never a silent leak).
    scan.has_variant_field || (scan.any_heap && scan.all_supported)
}

/// The set of FLAT variant type names — every constructor scalar-only, so the block owns NO inner
/// handle (a nullary enum like `Capability`, or a scalar-payload variant). A `List[flat-variant]`
/// record/anon field is freed per-element by `__drop_list_str` (`rc_dec` of each flat element block +
/// the list block); a variant carrying a `String`/nested/`List` field is NOT flat (its block owns an
/// inner handle) and is excluded — its `List` field stays on the existing flat-block `rc_dec` (the
/// materializer also walls a non-flat-variant list, so such a field is never built). The drop-side
/// mirror of [`crate::lower::VariantLayouts::is_flat_variant_ty`].
pub fn flat_variant_type_names(
    type_decls: &[almide_ir::IrTypeDecl],
) -> std::collections::HashSet<String> {
    use almide_ir::{IrTypeDeclKind, IrVariantKind};
    type_decls
        .iter()
        .filter_map(|d| {
            let IrTypeDeclKind::Variant { cases, .. } = &d.kind else { return None };
            let flat = cases.iter().all(|c| {
                let tys: Vec<&Ty> = match &c.kind {
                    IrVariantKind::Unit => vec![],
                    IrVariantKind::Tuple { fields } => fields.iter().collect(),
                    IrVariantKind::Record { fields } => fields.iter().map(|f| &f.ty).collect(),
                };
                tys.iter().all(|t| !is_heap_ty(t))
            });
            flat.then(|| d.name.as_str().to_string())
        })
        .collect()
}

/// The set of all user-variant type names in `type_decls` — the lookup `variant_field_name` uses.
pub fn variant_type_names(
    type_decls: &[almide_ir::IrTypeDecl],
) -> std::collections::HashSet<String> {
    use almide_ir::IrTypeDeclKind;
    type_decls
        .iter()
        .filter(|d| matches!(d.kind, IrTypeDeclKind::Variant { .. }))
        .map(|d| d.name.as_str().to_string())
        .collect()
}

/// The `__drop_<T>` FUNCTION IDENTIFIER for a (possibly module-prefixed) type name. A cross-module
/// type carries its module prefix in the IR (`self.types.RunResult` → `Ty::Named("types.RunResult")`);
/// a dot is illegal in an Almide function name, so the generated drop fn / its call sites / the
/// rendered `(call $__drop_…)` all sanitize dots to underscores — the SAME mangling v0 codegen
/// applies (`almide_rt_types_RunResult`). For a single-file (dot-free) type this is the identity, so
/// the v0 corpus / spec fixtures render byte-identically. The `Op::DropVariant` renderer applies the
/// IDENTICAL transform, keeping the call site and the definition in lockstep.
pub fn drop_fn_ident(type_name: &str) -> String {
    type_name.replace('.', "_")
}

/// [`lower_function_all`] WITH the program's record-layout registry threaded in —
/// the entry the real pipeline (render_program) uses so a `Ty::Named` record
/// resolves its fields (and `r.x` materializes). The plain [`lower_function_all`]
/// passes an empty registry (the structurally-typed `Ty::Record`/`Ty::Tuple`
/// paths still work; a `Ty::Named` aggregate stays walled without it). Delegates to
/// [`lower_function_all_with_layouts`] with an empty VARIANT registry — so a custom
/// variant stays walled (the ADT bricks call `_with_layouts` to admit it).
pub fn lower_function_all_with_types(
    func: &IrFunction,
    globals: &HashMap<VarId, Ty>,
    record_layouts: &RecordLayouts,
) -> Result<Vec<MirFunction>, LowerError> {
    lower_function_all_with_layouts(func, globals, record_layouts, &VariantLayouts::default())
}

/// [`lower_function_all_with_layouts`] WITH the module-level globals' INITIALIZERS threaded
/// in, so a HEAP global reference materializes its real const value (the base64 alphabet /
/// aes S-box) instead of walling. The `_with_layouts` entry delegates here with empty inits
/// (every heap-global reference there still walls, as before — no regression).
pub fn lower_function_all_with_globals(
    func: &IrFunction,
    globals: &HashMap<VarId, Ty>,
    global_inits: &HashMap<VarId, IrExpr>,
    record_layouts: &RecordLayouts,
    variant_layouts: &VariantLayouts,
) -> Result<Vec<MirFunction>, LowerError> {
    lower_function_all_impl(func, globals, global_inits, record_layouts, variant_layouts)
}

/// [`lower_function_all_with_types`] WITH the program's VARIANT-layout registry threaded in
/// too — the entry the real pipeline uses once custom ADTs participate in the value model
/// (the construct / `match` / drop bricks consult [`LowerCtx::variant_layouts`]). The
/// record-only entry above delegates here with an empty variant registry.
pub fn lower_function_all_with_layouts(
    func: &IrFunction,
    globals: &HashMap<VarId, Ty>,
    record_layouts: &RecordLayouts,
    variant_layouts: &VariantLayouts,
) -> Result<Vec<MirFunction>, LowerError> {
    lower_function_all_impl(func, globals, &HashMap::new(), record_layouts, variant_layouts)
}

fn body_has_stmt_position_propagating_unwrap(body: &IrExpr) -> bool {
    fn stmt_is_propagating(kind: &IrStmtKind) -> bool {
        match kind {
            IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } => {
                matches!(&value.kind, IrExprKind::Unwrap { .. } | IrExprKind::Try { .. })
            }
            IrStmtKind::Expr { expr } => {
                matches!(&expr.kind, IrExprKind::Unwrap { .. } | IrExprKind::Try { .. })
            }
            _ => false,
        }
    }
    fn scan(e: &IrExpr) -> bool {
        match &e.kind {
            IrExprKind::Block { stmts, expr } => {
                stmts.iter().any(|s| stmt_is_propagating(&s.kind))
                    || expr.as_deref().is_some_and(scan)
            }
            IrExprKind::If { then, else_, .. } => scan(then) || scan(else_),
            IrExprKind::Match { arms, .. } => arms.iter().any(|a| scan(&a.body)),
            _ => false,
        }
    }
    scan(body)
}

/// Does `body`'s TAIL (recursing through Block/If/Match, the same control-flow-transparent
/// positions `body_has_stmt_position_propagating_unwrap` scans) end in a bare `!` over an
/// OPTION-typed operand? Such a tail can only compile correctly under a `Result[T, String]`
/// ABI: the desugar (`desugar_tail_effect_unwrap`'s bare-Unwrap case) turns it into
/// `match o { none => err("none"), some(v) => ok(v) }`, which constructs a real Result — under
/// a RAW scalar ABI there is no channel for the none case at all (the old pass-through returned
/// the raw Option handle, a confirmed silent wrong-value/invalid-wasm bug in BOTH the
/// declared-Result and the scalar-lifted case). So this is an AUTO_WRAP_ABI_FNS INCLUSION
/// criterion. Gated to OPTION operands only: a RESULT-typed tail-`!` operand (including a
/// never-err `self()!`/`f()!` — Result-typed at this pre-strip point) is repr-compatible with
/// the pass-through in every ABI (same block IS the propagated Result), so wrapping those would
/// only churn working fns (the yaml TCO cluster's tail self-calls among them).
/// Does `body` carry a TAIL/arm-position `Try`/`Unwrap` over a CAN-ERR Named callee
/// (`if n < 0 then fail("negative") else ... checked(n-1)` — every branch either
/// propagates the callee's Result verbatim or yields a raw scalar)? Such a fn's REAL
/// ABI must be Result (the err channel propagates), so it joins `AUTO_WRAP_ABI_FNS`:
/// the `body.ty` override then makes the SCALAR arms wrap (`0` → `ok(0)` via the
/// heap-result arm machinery) while the Try arms pass the callee's same-repr Result
/// through. Without this, `checked` classified can-err (post the Try fixpoint fix)
/// but its base arm still produced a raw i64 against the i32 Result ABI — the
/// effect_tco invalid-wasm divergence, second layer.
fn body_has_tail_position_canerr_try(
    body: &IrExpr,
    can_err: &std::collections::HashSet<String>,
) -> bool {
    fn scan(e: &IrExpr, can_err: &std::collections::HashSet<String>) -> bool {
        match &e.kind {
            IrExprKind::Unwrap { expr } | IrExprKind::Try { expr } => match &expr.kind {
                IrExprKind::Call { target: CallTarget::Named { name }, .. } => {
                    can_err.contains(name.as_str())
                }
                IrExprKind::Call { target: CallTarget::Module { .. }, .. }
                | IrExprKind::RuntimeCall { .. } => true,
                _ => false,
            },
            IrExprKind::Block { expr, .. } => expr.as_deref().is_some_and(|t| scan(t, can_err)),
            IrExprKind::If { then, else_, .. } => scan(then, can_err) || scan(else_, can_err),
            IrExprKind::Match { arms, .. } => arms.iter().any(|a| scan(&a.body, can_err)),
            _ => false,
        }
    }
    scan(body, can_err)
}

fn body_has_tail_position_option_unwrap(body: &IrExpr) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    fn scan(e: &IrExpr) -> bool {
        match &e.kind {
            IrExprKind::Unwrap { expr } => {
                matches!(&expr.ty, Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1)
            }
            IrExprKind::Block { expr, .. } => expr.as_deref().is_some_and(scan),
            IrExprKind::If { then, else_, .. } => scan(then) || scan(else_),
            IrExprKind::Match { arms, .. } => arms.iter().any(|a| scan(&a.body)),
            _ => false,
        }
    }
    scan(body)
}

/// Desugar `assert(cond)` / `assert_eq(a, b)` / `assert_ne(a, b)` (Unit-typed builtin
/// calls — the test-block floor, also legal in a main body) to the §13 controlled-halt
/// shape the SELF-HOST stdlib already proves out (math.pow's negative-exponent guard):
/// `if <cond> then () else prim.die(prim.handle("assertion failed…"))`. Everything
/// downstream is EXISTING machinery — the stmt-position Unit-`if` executes via
/// `try_lower_unit_if`, `==`/`!=` dispatch through the ordinary BinOp lowering (whatever
/// operand types that subset admits; the rest walls honestly), and `prim.die` is the
/// proven Die prim. Failure = message on stderr + exit 1 — the harness keys on the
/// non-zero exit, exactly like v0's trap. Applied desugar-before-both (same slot as
/// `desugar_heap_branches`), so every driver counts and lowers the SAME tree.
fn desugar_assert_calls(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::{walk_expr_mut, IrMutVisitor};
    use almide_lang::intern::sym;
    fn die_expr(msg: &str) -> IrExpr {
        die_on(IrExpr {
            kind: IrExprKind::LitStr { value: msg.to_string() },
            ty: Ty::String,
            span: None,
            def_id: None,
        })
    }
    /// die on an arbitrary String-typed message EXPRESSION (the computed 2-arg
    /// assert message: `assert(c, "got " + float.to_string(x))`).
    fn die_on(lit: IrExpr) -> IrExpr {
        let handle = IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Module { module: sym("prim"), func: sym("handle"), def_id: None },
                args: vec![lit],
                type_args: Vec::new(),
            },
            ty: Ty::Int,
            span: None,
            def_id: None,
        };
        IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Module { module: sym("prim"), func: sym("die"), def_id: None },
                args: vec![handle],
                type_args: Vec::new(),
            },
            ty: Ty::Unit,
            span: None,
            def_id: None,
        }
    }
    struct S {
        changed: bool,
    }
    impl IrMutVisitor for S {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let is_panic = matches!(&e.kind,
                IrExprKind::Call { target: CallTarget::Named { name }, args, .. }
                    if name.as_str() == "panic" && args.len() == 1
                        && matches!(args[0].ty, Ty::String));
            // `panic` types as the enclosing branch demands (Unit or Never) — it must
            // bypass the Unit gate below.
            if !is_panic && !matches!(e.ty, Ty::Unit) {
                return;
            }
            let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &e.kind
            else {
                return;
            };
            // `panic(msg)` — an UNCONDITIONAL abort: die on "PANIC: " + msg (the v0
            // wasm form: prefix + message, then halt). The message expr is evaluated
            // only here (the abort path), like the computed assert message.
            if name.as_str() == "panic" && args.len() == 1 && matches!(args[0].ty, Ty::String)
            {
                let msg = args[0].clone();
                let text = match &msg.kind {
                    IrExprKind::LitStr { value } => {
                        die_expr(&format!("PANIC: {value}"))
                    }
                    _ => die_on(IrExpr {
                        kind: IrExprKind::BinOp {
                            op: almide_ir::BinOp::ConcatStr,
                            left: Box::new(IrExpr {
                                kind: IrExprKind::LitStr { value: "PANIC: ".to_string() },
                                ty: Ty::String,
                                span: None,
                                def_id: None,
                            }),
                            right: Box::new(msg),
                        },
                        ty: Ty::String,
                        span: None,
                        def_id: None,
                    }),
                };
                *e = text;
                self.changed = true;
                return;
            }
            let (cond, msg) = match (name.as_str(), args.as_slice()) {
                ("assert", [c]) if matches!(c.ty, Ty::Bool) => {
                    (c.clone(), None)
                }
                // The 2-arg form `assert(cond, msg)`: a LITERAL message folds into
                // the die text; a COMPUTED String message dies on the CONCAT
                // `"assertion failed: " + msg` (evaluated only on the failing path).
                ("assert", [c, m]) if matches!(c.ty, Ty::Bool) && matches!(m.ty, Ty::String) => {
                    (c.clone(), Some(m.clone()))
                }
                ("assert_eq", [a, b]) => (
                    IrExpr {
                        kind: IrExprKind::BinOp {
                            op: almide_ir::BinOp::Eq,
                            left: Box::new(a.clone()),
                            right: Box::new(b.clone()),
                        },
                        ty: Ty::Bool,
                        span: None,
                        def_id: None,
                    },
                    None,
                ),
                ("assert_ne", [a, b]) => (
                    IrExpr {
                        kind: IrExprKind::BinOp {
                            op: almide_ir::BinOp::Neq,
                            left: Box::new(a.clone()),
                            right: Box::new(b.clone()),
                        },
                        ty: Ty::Bool,
                        span: None,
                        def_id: None,
                    },
                    None,
                ),
                _ => return,
            };
            let default_text = match name.as_str() {
                "assert_eq" => "assertion failed: left == right",
                "assert_ne" => "assertion failed: left != right",
                _ => "assertion failed: assert(false)",
            };
            let die = match msg {
                None => die_expr(default_text),
                Some(m) => match &m.kind {
                    IrExprKind::LitStr { value } => {
                        die_expr(&format!("assertion failed: {value}"))
                    }
                    _ => die_on(IrExpr {
                        kind: IrExprKind::BinOp {
                            op: almide_ir::BinOp::ConcatStr,
                            left: Box::new(IrExpr {
                                kind: IrExprKind::LitStr {
                                    value: "assertion failed: ".to_string(),
                                },
                                ty: Ty::String,
                                span: None,
                                def_id: None,
                            }),
                            right: Box::new(m),
                        },
                        ty: Ty::String,
                        span: None,
                        def_id: None,
                    }),
                },
            };
            let unit = IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None };
            *e = IrExpr {
                kind: IrExprKind::If {
                    cond: Box::new(cond),
                    then: Box::new(unit),
                    else_: Box::new(die),
                },
                ty: Ty::Unit,
                span: e.span.clone(),
                def_id: e.def_id,
            };
            self.changed = true;
        }
    }
    let mut s = S { changed: false };
    let mut out = body.clone();
    s.visit_expr_mut(&mut out);
    s.changed.then_some(out)
}

/// `m[k]` over a `Map` (the frontend emits `MapAccess` ONLY for `obj.ty.is_map()`) →
/// `map.get(m, k)` — the ordinary self-host map lookup call (`Option[V]` result), which
/// the repr dispatch suffixes (`get_skv`/`get_str`/…) like every other map call site.
/// Applied desugar-before-both (same slot as `desugar_assert_calls`): the counted tree
/// and the lowering see the SAME Call node, so `mir == ir` holds for the one CallFn.
fn desugar_map_access_calls(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::{walk_expr_mut, IrMutVisitor};
    use almide_lang::intern::sym;
    struct S {
        changed: bool,
    }
    impl IrMutVisitor for S {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::MapAccess { object, key } = &e.kind else {
                return;
            };
            *e = IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Module {
                        module: sym("map"),
                        func: sym("get"),
                        def_id: None,
                    },
                    args: vec![(**object).clone(), (**key).clone()],
                    type_args: Vec::new(),
                },
                ty: e.ty.clone(),
                span: e.span.clone(),
                def_id: e.def_id,
            };
            self.changed = true;
        }
    }
    let mut s = S { changed: false };
    let mut out = body.clone();
    s.visit_expr_mut(&mut out);
    s.changed.then_some(out)
}