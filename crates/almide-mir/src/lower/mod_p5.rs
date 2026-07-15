pub(crate) fn is_self_host_result_module_fn(module: &str, func: &str) -> bool {
    matches!(
        (module, func),
        ("int", "parse")
            // `float.parse` is the same intrinsic-Result shape as `int.parse` (Result[Float, String],
            // a materialized scalar Result read len-as-tag); a `match` over it EXECUTES the same way.
            | ("float", "parse")
            | ("int", "from_hex")
            | ("option", "to_result")
            | ("result", "collect")
            | ("result", "map")
            | ("result", "flat_map")
            | ("result", "map_err")
            | ("result", "filter")
            | ("result", "or_else")
            | ("result", "flatten")
            | ("error", "context")
            // value.as_int/as_bool/as_float build a materialized Result[T, String] (Ok(payload)
            // on a tag match, else Err("expected T")) — a `match` over the result EXECUTES.
            | ("value", "as_int")
            | ("value", "as_bool")
            | ("value", "as_float")
    )
}

/// Does `module.func` return a materialized HEAP-Ok `Result[String, String]` (the cap-as-tag
/// DynListStr layout, both Ok and Err owning a String)? Its result is tracked in
/// `materialized_results_str` so an `Ok`/`Err` `match` over it EXECUTES reading cap@8.
pub fn is_self_host_result_str_module_fn(module: &str, func: &str) -> bool {
    matches!(
        (module, func),
        ("value", "as_string") | ("result", "zip") | ("value", "as_array") | ("value", "get")
            // `json.parse` is the SELF-HOSTED recursive-descent parser
            // (stdlib/json_parse.almd): its `Result[Value, String]` is built by the
            // ordinary Almide ok()/err() ctors = the materialize_result_str layout
            // (payload @12, tag @16). Tracking the bound var routes a later
            // `match r { ok/err }` through try_lower_result_match (tag dispatch)
            // instead of the linearization, and `is_value_result_ty` picks the
            // recursive DropResultValue for the Ok Value.
            | ("json", "parse")
            // `fs.read_text` returns the cap-as-tag `Result[String, String]` ($read_text_file builds
            // it in the EXACT `materialize_result_str` layout — payload @12, Ok/Err tag @16). So a
            // `match`/`!` over it must read tag @16 + bind the @12 payload handle (the str-result
            // path), NOT len-as-tag @4. Without this the subject was untracked, so `try_lower_result_
            // match` bailed and the unwrap bound the WHOLE Result block where the Ok String was
            // expected — a 1-byte garbage print (low byte of the payload pointer) / an i64↔i32 width
            // mismatch downstream in csv-to-json.
            | ("fs", "read_text")
            // `fs.read_bytes_raw` — the raw-bytes twin (same cap-as-tag Result block; the Ok
            // payload is Bytes instead of String).
            | ("fs", "read_bytes_raw")
            // `fs.read_bytes` — the List[Int]-expanded sibling (the self-host re-wraps the
            // prim's Result in the SAME materialize_result_str layout via ok()/err() ctors).
            | ("fs", "read_bytes")
            // `fs.list_dir` returns the cap-as-tag `Result[List[String], String]` ($read_dir builds
            // it in the same `materialize_result_str` layout — payload @12 a List[String], tag @16).
            // So a `match`/`!` over it must read tag @16 + bind the @12 payload list handle, exactly
            // like fs.read_text (only the Ok payload type differs: a List[String], not a String).
            | ("fs", "list_dir")
            // `fs.stat` returns the cap-as-tag `Result[FileStat, String]` (the self-host builds
            // it with the ordinary ok()/err() ctors — payload @12, tag @16). The Ok payload is a
            // SCALAR-ONLY record block (size/is_dir/is_file/modified — no heap fields), so the
            // flat DropListStr @12 free is exact on both arms (record block on Ok, msg on Err).
            | ("fs", "stat")
            // `fs.write` returns the cap-as-tag `Result[Unit, String]` ($write_text_file builds it in
            // the same layout — Ok with len@4=0 + @12=0 + tag@16=0, Err with len@4=1 + @12=msg +
            // tag@16=1). So a `match`/`!` over it must read tag @16 (NOT len-as-tag @4 — that would
            // MISREAD the Ok len-0 block AND linearize both arms, a silent miscompile printing both).
            // The Ok arm has NO @12 payload (Unit), so `ok(_)` discards a null handle (never used);
            // the flat DropListStr frees nothing on Ok, the @12 msg on Err — exact for both arms.
            | ("fs", "write")
            // `fs.mkdir_p` returns the SAME cap-as-tag `Result[Unit, String]` shape as fs.write
            // ($make_dir builds it identically — Ok with len@4=0 + @12=0 + tag@16=0, Err with
            // len@4=1 + @12=msg + tag@16=1). So a `match`/`!` over it reads tag @16, exactly like
            // fs.write — same Ok-has-no-payload discipline, same flat DropListStr for both arms.
            | ("fs", "mkdir_p")
            // `fs.remove_all` returns the SAME cap-as-tag `Result[Unit, String]` shape as fs.write
            // ($remove_all builds it identically — Ok with len@4=0 + @12=0 + tag@16=0, Err with
            // len@4=1 + @12=msg + tag@16=1). So a `match`/`!` over it reads tag @16, exactly like
            // fs.write — same Ok-has-no-payload discipline, same flat DropListStr for both arms.
            | ("fs", "remove_all")
            // `fan.map` returns the cap-as-tag `Result[List[Int], String]` (the self-host `fan_map`
            // builds it with the ordinary `ok(acc)`/`err(e)` ctors — a heap-Ok Result in the exact
            // `materialize_result_str` layout, like `fs.list_dir`'s `Result[List[String], String]`).
            // So a `match`/`!` over it reads tag @16 + binds the @12 payload list handle.
            | ("fan", "map")
            | ("fan", "map_is")
            | ("fan", "map_ss")
            | ("fan", "map_si")
    )
}

/// Is `ty` a `value.as_array`-style Result whose Ok arm is a `List[Value]` (a heap-Ok Result with a
/// LIST-of-Value payload)? Such a Result reuses the cap@16 str-result MATCH machinery, but its DROP
/// must free the list RECURSIVELY (`Op::DropResultListValue`/`value_result_lists`), not flat
/// (`DropListStr` would leak the list's element Values). The DISTINGUISHER from `value.as_string`'s
/// `Result[String, String]` is the Ok-arm being a `List`, so the tracking is TYPE-driven (sound
/// wherever only the `ValueId` + its `ty` are known — seed_variant_param, the match subject).
pub fn is_result_listval_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    // The Ok arm must be a `List[Value]` SPECIFICALLY — those elements are dynamic Values that
    // `DropResultListValue` frees recursively. A `List[scalar]` (e.g. `List[Int]` from base64
    // decode) is a FLAT block whose `DropListStr` rc_dec is correct, AND is how its
    // `materialize_result_str(value_ok=false)` construction tracks it — so it must fall to the
    // `heap_elem_lists` branch at every call site, NOT this recursive-Value one (a `List[Int]`
    // routed here gets a wrong recursive drop that reads each Int as a Value handle).
    matches!(ty, Ty::Applied(TypeConstructorId::Result, a)
        if a.len() == 2 && matches!(&a[0], Ty::Applied(TypeConstructorId::List, le)
            if le.len() == 1 && is_value_ty(&le[0])))
}

/// Is `ty` a `Result[String, String]` (the value.as_string shape — both arms a flat String)? The
/// PRECISE str-str distinguisher (vs the broader `is_heap_ok_result`, which also matches a tuple-Ok
/// `result.zip`), so the `??` routes only a genuine String-payload Result to `result.str_unwrap_or`.
pub fn is_result_str_str_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::Result, a)
        if a.len() == 2 && matches!(&a[0], Ty::String) && matches!(&a[1], Ty::String))
}

/// Is `ty` an `Option[Value]` (the `list.get(rows, i)` shape — a dynamic Value Some-payload)? Its
/// `??` routes to `option.value_unwrap_or` (the prim-based unwrap, since the value-match Some-arm's
/// scalar_bind rejects a heap Value payload).
pub fn is_option_value_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::Option, a)
        if a.len() == 1 && is_value_ty(&a[0]))
}

/// Is `ty` an `Option[List[String]]` (the `list.get_liststr(rows, i)` shape — a nested-heap-list
/// Some-payload)? Its `??` routes to `option.liststr_unwrap_or`, the List[String] analogue of
/// `option.value_unwrap_or`.
pub fn is_option_liststr_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::Option, a)
        if a.len() == 1 && matches!(&a[0], Ty::Applied(TypeConstructorId::List, e)
            if e.len() == 1 && matches!(e[0], Ty::String)))
}

/// Is `ty` an `Option[List[<scalar>]]` (the `map.get(groups, k) ?? []` group_by shape)? Its `??`
/// routes to `option.listint_unwrap_or`, the FLAT scalar-element analogue of
/// `option.liststr_unwrap_or` (the payload list owns nothing — a flat rc drop is exact).
pub fn is_option_listscalar_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::Option, a)
        if a.len() == 1 && matches!(&a[0], Ty::Applied(TypeConstructorId::List, e)
            if e.len() == 1 && !is_heap_ty(&e[0])))
}

/// Is `ty` an `Option[List[Value]]` (the `json.as_array(v)` shape)? Its `??` routes to
/// `option.listvalue_unwrap_or`, the List[Value] analogue of `option.liststr_unwrap_or`.
pub fn is_option_listvalue_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::Option, a)
        if a.len() == 1 && matches!(&a[0], Ty::Applied(TypeConstructorId::List, e)
            if e.len() == 1 && is_value_ty(&e[0])))
}

pub(crate) fn alloc_init(value: &IrExpr) -> Init {
    if let IrExprKind::LitStr { value } = &value.kind {
        return Init::Str(value.clone());
    }
    // A list OR tuple of scalar literals materializes its slots: an Int element stores its value, a
    // Float element stores its f64 BITS (the i64-uniform Float repr — read back via load64 +
    // ffrombits). A `(3, 7)` tuple is physically a 2-slot block [3@12, 7@20], exactly a List[Int]
    // literal — so a scalar-literal-field tuple shares the IntList materialization. A mixed/
    // non-literal list or tuple stays Opaque.
    if let IrExprKind::List { elements } | IrExprKind::Tuple { elements } = &value.kind {
        let ints: Option<Vec<i64>> = elements
            .iter()
            .map(|e| match &e.kind {
                IrExprKind::LitInt { value } => Some(*value),
                IrExprKind::LitFloat { value } => Some(value.to_bits() as i64),
                // A Bool literal occupies its 8-byte slot as 0/1 (the i64-uniform Bool repr), so a
                // `[true, false]` literal materializes exactly like an IntList of [1, 0] — read back
                // via load64 as 0/1. (`${bool_list}` → list.to_string_b reads these slots.)
                IrExprKind::LitBool { value } => Some(*value as i64),
                _ => None,
            })
            .collect();
        if let Some(ints) = ints {
            return Init::IntList(ints);
        }
    }
    Init::Opaque
}

pub(crate) fn stmt_kind_name(k: &IrStmtKind) -> &'static str {
    match k {
        IrStmtKind::Bind { .. } => "Bind",
        IrStmtKind::BindDestructure { .. } => "BindDestructure",
        IrStmtKind::Assign { .. } => "Assign",
        IrStmtKind::IndexAssign { .. } => "IndexAssign",
        IrStmtKind::MapInsert { .. } => "MapInsert",
        IrStmtKind::FieldAssign { .. } => "FieldAssign",
        IrStmtKind::Guard { .. } => "Guard",
        IrStmtKind::Expr { .. } => "Expr",
        IrStmtKind::Comment { .. } => "Comment",
        IrStmtKind::RcInc { .. } => "RcInc",
        IrStmtKind::RcDec { .. } => "RcDec",
        IrStmtKind::ListSwap { .. } => "ListSwap",
        IrStmtKind::ListReverse { .. } => "ListReverse",
        IrStmtKind::ListRotateLeft { .. } => "ListRotateLeft",
        IrStmtKind::ListCopySlice { .. } => "ListCopySlice",
    }
}

/// The CONTAINER expression of a field/element/tuple/map extraction, if `expr`
/// is one — the source whose object the extracted value aliases (the
/// container-grain field access, see [`LowerCtx::lower_heap_extraction`]).
pub(crate) fn extraction_container(expr: &IrExpr) -> Option<&IrExpr> {
    match &expr.kind {
        IrExprKind::Member { object, .. }
        | IrExprKind::IndexAccess { object, .. }
        | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::MapAccess { object, .. } => Some(object),
        _ => None,
    }
}

/// Rebuild a field/element/tuple/map EXTRACTION `expr` with its container (object) replaced by
/// `new_container`, preserving the extracted field/index and the result type/span. Used to ANF-lift
/// a Call-result container (`f(x).field`) to a synthetic temp Var before re-running the extraction
/// (see [`LowerCtx::lower_heap_extraction`]). Precondition: `expr` is one of the four extraction
/// kinds (the caller checked via [`extraction_container`]); any other kind is returned unchanged.
pub(crate) fn rebuild_extraction(expr: &IrExpr, new_container: IrExpr) -> IrExpr {
    let kind = match &expr.kind {
        IrExprKind::Member { field, .. } => IrExprKind::Member {
            object: Box::new(new_container),
            field: *field,
        },
        IrExprKind::TupleIndex { index, .. } => IrExprKind::TupleIndex {
            object: Box::new(new_container),
            index: *index,
        },
        IrExprKind::IndexAccess { index, .. } => IrExprKind::IndexAccess {
            object: Box::new(new_container),
            index: index.clone(),
        },
        IrExprKind::MapAccess { key, .. } => IrExprKind::MapAccess {
            object: Box::new(new_container),
            key: key.clone(),
        },
        _ => return expr.clone(),
    };
    IrExpr { kind, ty: expr.ty.clone(), span: expr.span, def_id: expr.def_id }
}

/// True if any argument is a FUNCTION-typed value (a closure / lambda / fn-ref).
/// A stdlib call with such an argument invokes USER code, so its effective
/// capabilities are its-own ∪ the closure's — unmodelled in the pure-only Module
/// slice — and a captured-heap closure carries ownership this brick does not
/// track. Such calls are walled. The TYPE test catches every form (a lambda
/// literal, a fn-ref, OR a variable of function type) under the AllTypesConcrete
/// precondition; the kind test is a belt-and-suspenders for any arg whose type
/// was not concretized.
pub(crate) fn is_higher_order(args: &[IrExpr]) -> bool {
    args.iter().any(|a| {
        matches!(a.ty, Ty::Fn { .. })
            || matches!(
                a.kind,
                IrExprKind::Lambda { .. }
                    | IrExprKind::ClosureCreate { .. }
                    | IrExprKind::FnRef { .. }
            )
    })
}

/// TAIL-DUPLICATION desugar for a `let s = <heap-result if/match>; <rest>` in a NON-tail,
/// let-bound position — the shape `lower_bind` walls (a merged-dst heap value has no sound
/// scope-end drop in the flat certificate).
///
/// This is a PURE IR→IR rewrite applied to a function BODY *before* both lowering and the
/// caps `count_ir_calls` gate ("desugar-before-both"): they see the IDENTICAL node tree, so the
/// duplicated continuation's calls are counted exactly as the lowering emits them and the
/// `mir == ir` 1:1 invariant holds BY CONSTRUCTION — no special-casing in either side, no risk
/// of an IR-structure count formula leaking a false `mir > ir` (or masking an elision).
///
/// Scan the body block's `(stmts, tail)` for the FIRST `Bind { s, ty, value }` whose `value` is a
/// heap-result `if`/`match` and `ty` is heap. Found at index `i`, push the continuation `<rest>`
/// (`stmts[i+1..] ++ tail`) into each arm:
///   `… ; let s = if c then A else B; <rest>`  →  `… ; if c then { let s = A; <rest> } else { let s = B; <rest> }`
/// (and the `match` analog — each literal-pattern arm, via `desugar_match_to_if`, binds its value
/// then runs `<rest>`). The rewritten branch becomes the block's TAIL, so the EXISTING `lower_tail`
/// machinery executes it by result kind (Unit/scalar/heap `if`) — each arm independently binds `s`
/// (cert `i`), runs `<rest>` and drops `s` + the continuation's locals at the arm frame end (cert
/// `d`): the per-arm `i…d` balance the proven checker already accepts. Only ONE arm runs at runtime,
/// so duplicating `<rest>` is semantically identical to v0. NO certificate / Coq change.
///
/// GATE (bounded + sound — WALL what cannot be duplicated cleanly; the rewritten tree still routes
/// through the per-position `if` machinery, which itself rolls back to an explicit wall on an
/// unfaithful arm/cond):
///  - The continuation `<rest>` must NOT itself carry another unresolved heap let-bound `if`/`match`
///    (duplicating a duplicating continuation risks exponential blow-up) — left to the wall.
///  - A `match` not reducible to a literal-pattern else-if chain (`desugar_match_to_if`) — left to
///    the wall.
///
/// Returns `Some(rewritten_body)` when the desugar applies, `None` (the body is unchanged) otherwise.
/// The max `VarId` used anywhere in `body` (0 if none) — so a fresh synthetic var can be
/// allocated as `max + 1` without a frontend var-table round-trip.
pub(crate) fn max_var_id(body: &IrExpr) -> u32 {
    use almide_ir::visit::IrVisitor;
    use almide_ir::IrPattern;
    // A pattern binds variables (`some(ch)`, `ok(x)`, `(a, b)`) that are NOT `IrExprKind::Var` /
    // `IrStmtKind::Bind` nodes, so the visitor's expr/stmt hooks miss them. A fresh synthetic var
    // (`rk`/`idx` = max+1/+2) MUST clear them too — else it COLLIDES with a pattern bind and the
    // renderer reuses one local for two types (an i32 element handle AND an i64 flag = invalid wasm).
    fn pat_max(p: &IrPattern, acc: &mut u32) {
        match p {
            IrPattern::Bind { var, .. } => *acc = (*acc).max(var.0),
            IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => {
                pat_max(inner, acc)
            }
            IrPattern::Tuple { elements } | IrPattern::List { elements }
            | IrPattern::Constructor { args: elements, .. } => {
                for e in elements {
                    pat_max(e, acc);
                }
            }
            IrPattern::RecordPattern { fields, .. } => {
                for f in fields {
                    if let Some(fp) = &f.pattern {
                        pat_max(fp, acc);
                    }
                }
            }
            IrPattern::Wildcard | IrPattern::None | IrPattern::Literal { .. } => {}
        }
    }
    struct M(u32);
    impl IrVisitor for M {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Var { id } = &e.kind {
                self.0 = self.0.max(id.0);
            }
            if let IrExprKind::Match { arms, .. } = &e.kind {
                for arm in arms {
                    pat_max(&arm.pattern, &mut self.0);
                }
            }
            almide_ir::visit::walk_expr(self, e);
        }
        fn visit_stmt(&mut self, s: &IrStmt) {
            if let IrStmtKind::Bind { var, .. } = &s.kind {
                self.0 = self.0.max(var.0);
            }
            almide_ir::visit::walk_stmt(self, s);
        }
    }
    let mut m = M(0);
    m.visit_expr(body);
    m.0
}

/// Is `e` a HEAP-result `if`/`match` (the form `lower_bind` walls / the tail-dup recovers)?
fn is_heap_branch(e: &IrExpr) -> bool {
    is_heap_ty(&e.ty) && matches!(e.kind, IrExprKind::If { .. } | IrExprKind::Match { .. })
}

// ─────────────────── TCO: tail-self-recursion → scalar loop ───────────────────
// A tail-self-recursive `f(p…) = <if/block tree whose leaves are self-calls f(p'…) or base
// exprs>` is rewritten to the GATE-VERIFIABLE cert-clean shape: a SCALAR-only top-test loop
// (the loop body only reassigns the scalar loop-carried params + a `result_kind` flag) followed
// by a POST-LOOP dispatch that builds the heap result from `result_kind` + the final scalars.
// No new MIR primitive, no cert change — the existing scalar-while + heap-result-if lowering
// verify it. Replaces the self-rec-guard wall for the reconstructible-base subset (scan_quote,
// find_colon_at, …). See docs/roadmap/active/v1-tco-self-recursion.md.

fn tco_ir(kind: IrExprKind, ty: Ty) -> IrExpr {
    IrExpr { kind, ty, span: None, def_id: None }
}

/// An empty value of `ty` for the TCO result accumulator's INITIAL binding — a placeholder the first
/// base case overwrites (its scope-end-style drop on reassignment must be a no-op-equivalent, so it is
/// a genuine empty heap block, not a deferred Opaque). `List → []`, `String → ""`. Other heap results
/// (Value, Result) have no clean empty literal, so the accumulator path declines (`None`) and the
/// caller keeps the post-loop dispatch (or walls, when a base references a loop-body-local).
fn tco_empty_for(ty: &Ty) -> Option<IrExpr> {
    use almide_lang::types::constructor::TypeConstructorId;
    match ty {
        Ty::String => Some(tco_ir(IrExprKind::LitStr { value: String::new() }, Ty::String)),
        Ty::Applied(TypeConstructorId::List, _) => {
            Some(tco_ir(IrExprKind::List { elements: vec![] }, ty.clone()))
        }
        // brick 1: scalar accumulators empty to their zero value (no ownership).
        Ty::Int => Some(tco_ir(IrExprKind::LitInt { value: 0 }, Ty::Int)),
        Ty::Float => Some(tco_ir(IrExprKind::LitFloat { value: 0.0 }, Ty::Float)),
        Ty::Bool => Some(tco_ir(IrExprKind::LitBool { value: false }, Ty::Bool)),
        // brick 1: a tuple accumulator empties componentwise (recursive) — declines if any field
        // has no clean empty.
        Ty::Tuple(tys) => {
            let elements: Option<Vec<IrExpr>> = tys.iter().map(tco_empty_for).collect();
            elements.map(|elements| tco_ir(IrExprKind::Tuple { elements }, ty.clone()))
        }
        // brick 1: a Value result accumulator empties to `value.null()`. It lowers INLINE to a tag-0
        // Value block (commit 6ca50e85), so it is gate-neutral (no synthetic mir CallFn) — the
        // CallTarget::Module path my value.null inline intercepts.
        _ if is_value_ty(ty) => Some(tco_ir(
            IrExprKind::Call {
                target: CallTarget::Module { module: sym("value"), func: sym("null"), def_id: None },
                args: vec![],
                type_args: vec![],
            },
            ty.clone(),
        )),
        // A `Result[_, String]` accumulator (the unwrap-`!`-desugar's TCO over a `match` — base64
        // decode_chunks returns `Result[List[Int], String]`): empty to `err("")`, a valid cap-tag
        // Result block. PLACEHOLDER ONLY — a base (`ok(acc)`/`err(e)`) overwrites the result slot
        // before the post-loop reads it (recursion always terminates at a base), and the overwrite
        // drops this `""` via DropListStr. Gated to a String Err so `err("")` typechecks.
        Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 && matches!(&a[1], Ty::String) => {
            Some(tco_ir(
                IrExprKind::ResultErr {
                    expr: Box::new(tco_ir(IrExprKind::LitStr { value: String::new() }, Ty::String)),
                },
                ty.clone(),
            ))
        }
        _ => None,
    }
}

fn tco_contains_self(e: &IrExpr, fn_name: &str) -> bool {
    use almide_ir::visit::IrVisitor;
    struct S<'a>(&'a str, bool);
    impl IrVisitor for S<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Call { target: CallTarget::Named { name }, .. } = &e.kind {
                if name.as_str() == self.0 {
                    self.1 = true;
                }
            }
            almide_ir::visit::walk_expr(self, e);
        }
    }
    let mut s = S(fn_name, false);
    s.visit_expr(e);
    s.1
}

/// Does expression `e` read variable `v` anywhere (a `Var { id: v }` node)?
fn expr_reads_var(e: &IrExpr, v: VarId) -> bool {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct R {
        v: VarId,
        found: bool,
    }
    impl IrVisitor for R {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Var { id } = &e.kind {
                if *id == self.v {
                    self.found = true;
                }
            }
            walk_expr(self, e);
        }
    }
    let mut r = R { v, found: false };
    r.visit_expr(e);
    r.found
}

/// Order the changed heap-accumulator param indices `idxs` so that an accumulator whose new value
/// READS another changed heap accumulator is assigned BEFORE that one — the reader must observe the
/// OLD value (a `rows = rows + [cur]` self-call alongside `cur = []` must run rows FIRST, while `cur`
/// still holds the old row). Edge `a → b` (emit a before b) iff `args[idxs[a]]` reads
/// `params[idxs[b]].var`. Kahn's topological sort; `None` if the read-graph is CYCLIC (e.g.
/// `a = a + b; b = b + a` — no order sees both olds; that residual needs owned-temp staging).
fn order_heap_accs_by_read_dep(
    idxs: &[usize],
    args: &[IrExpr],
    params: &[almide_ir::IrParam],
) -> Option<Vec<usize>> {
    let n = idxs.len();
    let mut indeg = vec![0usize; n];
    let mut edges: Vec<Vec<usize>> = vec![Vec::new(); n];
    for a in 0..n {
        for b in 0..n {
            if a != b && expr_reads_var(&args[idxs[a]], params[idxs[b]].var) {
                edges[a].push(b); // idxs[a] reads idxs[b] ⇒ a before b
                indeg[b] += 1;
            }
        }
    }
    let mut queue: Vec<usize> = (0..n).filter(|&i| indeg[i] == 0).collect();
    let mut order: Vec<usize> = Vec::new();
    while let Some(a) = queue.pop() {
        order.push(idxs[a]);
        for &b in &edges[a] {
            indeg[b] -= 1;
            if indeg[b] == 0 {
                queue.push(b);
            }
        }
    }
    if order.len() == n {
        Some(order)
    } else {
        None // a cycle — no read-before-reset order exists
    }
}

/// Walk tail-position leaves: a self-call pushes its args to `calls`; any other tail leaf is a
/// base (pushed to `bases`). `None` if a self-call sits in a NON-tail position (not TCO-able).
fn tco_collect<'a>(
    body: &'a IrExpr,
    fn_name: &str,
    calls: &mut Vec<&'a [IrExpr]>,
    bases: &mut Vec<&'a IrExpr>,
) -> Option<()> {
    match &body.kind {
        IrExprKind::If { then, else_, .. } => {
            tco_collect(then, fn_name, calls, bases)?;
            tco_collect(else_, fn_name, calls, bases)
        }
        // A `match` tail (the unwrap-`!` desugar's `match e { ok(v) => …, err(x) => err(x) }`): the
        // SUBJECT must not itself recurse (a self-call in `e` is not a tail), then each arm is a tail
        // leaf — the ok-arm may recurse, the err-arm is a base. Mirrors the `if`-arm recursion.
        IrExprKind::Match { subject, arms } => {
            if tco_contains_self(subject, fn_name) {
                return None;
            }
            for a in arms {
                tco_collect(&a.body, fn_name, calls, bases)?;
            }
            Some(())
        }
        IrExprKind::Block { expr: Some(tail), .. } => tco_collect(tail, fn_name, calls, bases),
        // The frontend's auto-`?` wraps a tail self-call as `Try{Call self}` (and a spelled
        // `!` as `Unwrap{Call self}`) — #557's "TCO must see THROUGH" requirement: the
        // propagation is the identity on the self-call's own same-repr Result, so the
        // wrapped call is STILL a tail self-call. Without this, `checked(n-1)` under the
        // auto-wrap ABI recursed O(n) and blew the call stack at 2e6 depth (effect_tco).
        IrExprKind::Unwrap { expr } | IrExprKind::Try { expr }
            if matches!(&expr.kind,
                IrExprKind::Call { target: CallTarget::Named { name }, .. }
                    if name.as_str() == fn_name) =>
        {
            let IrExprKind::Call { args, .. } = &expr.kind else { unreachable!() };
            calls.push(args);
            Some(())
        }
        IrExprKind::Call { target: CallTarget::Named { name }, args, .. }
            if name.as_str() == fn_name =>
        {
            calls.push(args);
            Some(())
        }
        _ => {
            if tco_contains_self(body, fn_name) {
                return None; // a self-call buried in a non-tail leaf — not TCO-able here
            }
            bases.push(body);
            Some(())
        }
    }
}

/// Rewrite tail leaves: a self-call → a Block assigning each CARRIED param to its new arg; a base
/// → `result_kind = <its 1-based kind>` (kinds assigned in `tco_collect`'s left-to-right order).
fn tco_rewrite(
    body: &IrExpr,
    fn_name: &str,
    params: &[almide_ir::IrParam],
    carried: &[bool],
    rk: VarId,
    next_kind: &mut i64,
    idx: Option<VarId>,
    next_var: &mut u32,
    result: Option<VarId>,
) -> IrExpr {
    match &body.kind {
        IrExprKind::If { cond, then, else_ } => tco_ir(
            IrExprKind::If {
                cond: cond.clone(),
                then: Box::new(tco_rewrite(then, fn_name, params, carried, rk, next_kind, idx, next_var, result)),
                else_: Box::new(tco_rewrite(else_, fn_name, params, carried, rk, next_kind, idx, next_var, result)),
            },
            Ty::Unit,
        ),
        // A `match` tail: rewrite each arm body (ok-arm → recurse/acc-update, err-arm → base/rk-set),
        // preserving the subject + patterns so the per-iteration match dispatches continue-or-exit.
        IrExprKind::Match { subject, arms } => tco_ir(
            IrExprKind::Match {
                subject: subject.clone(),
                arms: arms
                    .iter()
                    .map(|a| almide_ir::IrMatchArm {
                        pattern: a.pattern.clone(),
                        guard: a.guard.clone(),
                        body: tco_rewrite(&a.body, fn_name, params, carried, rk, next_kind, idx, next_var, result),
                    })
                    .collect(),
            },
            Ty::Unit,
        ),
        IrExprKind::Block { stmts, expr: Some(tail) } => tco_ir(
            IrExprKind::Block {
                stmts: stmts.clone(),
                expr: Some(Box::new(tco_rewrite(tail, fn_name, params, carried, rk, next_kind, idx, next_var, result))),
            },
            Ty::Unit,
        ),
        IrExprKind::Call { target: CallTarget::Named { name }, args, .. }
            if name.as_str() == fn_name =>
        {
            // SIMULTANEOUS UPDATE (the loop carries all params at once): a self-call arg may read ANOTHER
            // carried param (`acc + [string.slice(s, pos, …)]` reads `pos`; `start = pos + 1` reads `pos`),
            // so a plain sequential assign would see already-updated values — an off-by-one. Stage every
            // carried SCALAR's new value in a fresh temp (reading OLD params), THEN do the HEAP
            // accumulator assigns (which read the still-OLD scalar locals), THEN commit the scalar temps.
            // An IDENTITY arg (`acc` passed unchanged) is skipped (the stable local already holds it).
            let changed = |i: usize| {
                carried[i] && !matches!(&args[i].kind, IrExprKind::Var { id } if *id == params[i].var)
            };
            let mut stmts: Vec<IrStmt> = Vec::new();
            let mut finals: Vec<(VarId, VarId, Ty)> = Vec::new();
            // Phase 1: stage carried SCALAR args in temps (read OLD params).
            for i in 0..params.len() {
                if changed(i) && !is_heap_ty(&params[i].ty) {
                    let t = VarId(*next_var);
                    *next_var += 1;
                    stmts.push(IrStmt {
                        kind: IrStmtKind::Bind {
                            var: t,
                            mutability: almide_ir::Mutability::Let,
                            ty: params[i].ty.clone(),
                            value: args[i].clone(),
                        },
                        span: None,
                    });
                    finals.push((params[i].var, t, params[i].ty.clone()));
                }
            }
            // Phase 2: HEAP append/reset accumulator(s) — `acc = acc + [x]` reads the still-OLD scalar
            // locals. Emit in READ-DEPENDENCY order so a heap accumulator that reads ANOTHER heap
            // accumulator (`rows = rows + [cur]` alongside `cur = []`) is assigned BEFORE that one is
            // updated — the reader must observe the old value. try_tco_rewrite already walled the
            // cyclic case, so the order always exists (the unwrap_or is a defensive param-order
            // fallback).
            let heap_changed: Vec<usize> = (0..params.len())
                .filter(|&i| changed(i) && is_heap_ty(&params[i].ty))
                .collect();
            let heap_order = order_heap_accs_by_read_dep(&heap_changed, args, params)
                .unwrap_or(heap_changed);
            for i in heap_order {
                stmts.push(IrStmt {
                    kind: IrStmtKind::Assign { var: params[i].var, value: args[i].clone() },
                    span: None,
                });
            }
            // Phase 3: commit the staged scalar updates.
            for (p, t, ty) in finals {
                stmts.push(IrStmt {
                    kind: IrStmtKind::Assign {
                        var: p,
                        value: tco_ir(IrExprKind::Var { id: t }, ty),
                    },
                    span: None,
                });
            }
            // LIST-ITERATOR self-call: the consumed list param is INVARIANT (carried[ci]=false), so
            // advancing it `list.drop(cs,1)` becomes `idx = idx + 1` — the cert-clean iterator bump.
            if let Some(iv) = idx {
                stmts.push(IrStmt {
                    kind: IrStmtKind::Assign {
                        var: iv,
                        value: tco_ir(
                            IrExprKind::BinOp {
                                op: almide_ir::BinOp::AddInt,
                                left: Box::new(tco_ir(IrExprKind::Var { id: iv }, Ty::Int)),
                                right: Box::new(tco_ir(IrExprKind::LitInt { value: 1 }, Ty::Int)),
                            },
                            Ty::Int,
                        ),
                    },
                    span: None,
                });
            }
            tco_ir(IrExprKind::Block { stmts, expr: None }, Ty::Unit)
        }
        _ => {
            // A BASE case (a non-self tail). Set `rk` to a non-zero kind so the `while rk == 0` loop
            // exits. The base VALUE is delivered one of two ways:
            //   • result accumulator (`result = Some`): assign `<base>` to the carried result var HERE,
            //     IN the loop — where the base's inputs (carried params AND loop-body-local bindings
            //     like a destructured `let (field, _) = pf(…)`) are all live. The post-loop trivially
            //     returns the accumulator. This is the only correct place when the base reads a
            //     loop-body-local (those are dead in the post-loop dispatch — the parse_rows_rec bug).
            //   • post-loop dispatch (`result = None`): just record WHICH base via `rk = k`; the value
            //     is recomputed after the loop. Sound ONLY when the base closes over carried params.
            let k = *next_kind;
            *next_kind += 1;
            let mut stmts: Vec<IrStmt> = Vec::new();
            if let Some(rv) = result {
                stmts.push(IrStmt {
                    kind: IrStmtKind::Assign { var: rv, value: body.clone() },
                    span: None,
                });
            }
            stmts.push(IrStmt {
                kind: IrStmtKind::Assign {
                    var: rk,
                    value: tco_ir(IrExprKind::LitInt { value: k }, Ty::Int),
                },
                span: None,
            });
            tco_ir(IrExprKind::Block { stmts, expr: None }, Ty::Unit)
        }
    }
}

/// Rewrite a tail-self-recursive function body to a scalar loop + post-loop dispatch, or `None`
/// if it is outside the TCO subset (no self-call, a heap loop-carried arg, a self-call in a
/// non-tail position, or no base). The result lowers through the ordinary statements+tail path.
pub(crate) fn try_tco_rewrite(
    fn_name: &str,
    params: &[almide_ir::IrParam],
    body: &IrExpr,
) -> Option<IrExpr> {
    if !is_heap_ty(&body.ty) {
        // SCALAR-result gate: the loop rewrite mishandles a TUPLE-DESTRUCTURE bind in
        // the body (`let (cp, l) = __trim_cp(addr)` — the rewritten loop spun forever,
        // int.parse via the Unicode trim, 2026-07-03). Decline those; they keep the
        // real-recursion lowering (correct, stack-bound) as before. A destructure-free
        // scalar body (the `__split_fill`/`__chunk_outer` byte-walkers) is admitted.
        fn has_tuple_destructure(e: &IrExpr) -> bool {
            use almide_ir::visit::{walk_expr, IrVisitor};
            struct C(bool);
            impl IrVisitor for C {
                fn visit_stmt(&mut self, s: &almide_ir::IrStmt) {
                    if matches!(&s.kind, almide_ir::IrStmtKind::BindDestructure { .. }) {
                        self.0 = true;
                    }
                    almide_ir::visit::walk_stmt(self, s);
                }
            }
            let mut c = C(false);
            c.visit_expr(e);
            c.0
        }
        if has_tuple_destructure(body) {
            return None;
        }
    }
    // A HEAP-result self-rec function (the kind the self-rec guard walls — it returns an
    // Option/Result/Value/String the deep recursion would build then trap on), AND a
    // SCALAR-result one: the latter lowers as REAL recursion (a function-tail self-call)
    // which is correct but STACK-BOUND — the self-host byte-walkers (`__split_fill`,
    // `__chunk_outer`, `__fp_pow10_acc`) exhausted the wasm call stack on large inputs
    // (spec/wasm_cross r5_split / list_count_index_truncation, 2026-07-03). A scalar
    // result is exactly the cert-clean scalar-loop form (`tco_empty_for` has scalar
    // empties since brick 1), so admit it; the collect/carried gates below still decline
    // anything outside the loop subset, falling back to the real recursion as before.
    let n = params.len();
    let max_v = max_var_id(body).max(params.iter().map(|p| p.var.0).max().unwrap_or(0));
    let rk = VarId(max_v + 1);
    // LIST-ITERATOR rewrite (the heap-loop-carried escape): a HEAP carried param `cs` consumed in
    // EVERY self-call ONLY as `list.drop(cs, 1)`, with the body matching on `list.first(cs)`, is a
    // forward list scan. Rewrite it to an INVARIANT borrowed `cs` + a synthetic scalar INDEX `idx`:
    // `match list.first(cs) { none => BASE, some(ch) => BODY }` → `if idx < list.len(cs) then { let
    // ch = cs[idx]; BODY } else BASE`, and each `f(list.drop(cs,1), …)` self-call bumps `idx += 1`
    // (handled in `tco_rewrite`). `cs` becomes invariant, so the loop is the cert-clean scalar form —
    // NO heap back-edge merge, NO cert change. Closes oct_rec/bin_rec. Done BEFORE `tco_collect`
    // (which bails on a `match` body), so the rewritten `if` body is what gets collected + lowered.
    let lit = try_list_iter_rewrite(fn_name, body, params, max_v + 2);
    let work_body: &IrExpr = lit.as_ref().map(|(b, _, _)| b).unwrap_or(body);
    let idx_var = lit.as_ref().map(|(_, iv, _)| *iv);

    // FIRST collection — detect the self-calls + carried params (on the pre-substitution body).
    let mut calls0: Vec<&[IrExpr]> = Vec::new();
    let mut bases0: Vec<&IrExpr> = Vec::new();
    tco_collect(work_body, fn_name, &mut calls0, &mut bases0)?;
    if calls0.is_empty() || bases0.is_empty() {
        return None;
    }
    if calls0.iter().any(|c| c.len() != n) {
        return None;
    }
    // An `err($x)`-of-a-VAR base is the desugared `e!` early-return — a Result-unwrap INSIDE the
    // recursion (read_basic's `let (ch,np)=read_escape(..)!`). The TCO loop cannot carry that mid-body
    // early-exit: desugar_let_unwrap (run before this) turned the `!` into a `match e { ok($p)=>{..;
    // self-call}, err($x)=>err($x) }`, so the self-call sits in a nested match arm whose heap-accumulator
    // reassign then walls (mod_p3 "heap reassignment in a scalar loop body"). BAIL → the function falls
    // to the now-allowed REAL recursive lowering (a function-tail self-call, control_p4 ~188), which is
    // input-bounded and byte-matches v0. A natural `err("literal")` base (not a Var) is unaffected.
    if bases0.iter().any(|b| {
        matches!(&b.kind, IrExprKind::ResultErr { expr } if matches!(&expr.kind, IrExprKind::Var { .. }))
    }) {
        return None;
    }
    let mut carried0 = vec![false; n];
    for c in &calls0 {
        for i in 0..n {
            if !matches!(&c[i].kind, IrExprKind::Var { id } if *id == params[i].var) {
                carried0[i] = true;
            }
        }
    }
    if let Some((_, _, ci)) = &lit {
        carried0[*ci] = false;
    }
    // APPEND ACCUMULATORS (option C producer): a heap carried param whose EVERY self-call value is
    // `acc + [x]` (`BinOp::ConcatList` appending the accumulator to itself). Each becomes an OWNED
    // loop-carried SLOT — a fresh var initialized to `acc + []` (an owned copy: a `__list_concat`
    // Call heap-result, so `of[slot]=slot` and cert `i`), substituted for `acc` throughout, then
    // drop-old/alloc-new per iteration (cert `i(id)m`, accepted by the proven `check_cert_lc`). A heap
    // carried param that is NOT a self-append needs a general heap back-edge merge — still unsupported.
    // A self-call value that GROWS the accumulator from itself: `acc + [x]` (`ConcatList`) OR
    // `acc + s` (`ConcatStr`, the STRING accumulator — `parse_unquoted_field(text, pos+1, acc + c)`).
    // Both allocate a FRESH owned heap value; the TCO makes the accumulator an owned loop-carried
    // slot (drop-old/alloc-new per iter, cert `i(id)m`).
    // `acc + x`, OR a LEFT-NESTED chain `acc + a + b + …` whose leftmost leaf is `acc` (base64
    // encode_chunks: `enc(.., acc + c0 + c1 + c2 + c3)`). Both GROW the accumulator from itself —
    // a fresh owned heap value the loop-carried slot takes via drop-old/alloc-new; the in-loop
    // general-reassign path materializes the (possibly nested) concat via try_lower_concat_str/list,
    // and the OwnershipChecker `i(id)m` proof covers any fresh-owned producer regardless of nesting.
    fn concat_leftmost_is_var(e: &IrExpr, acc: VarId) -> bool {
        match &e.kind {
            IrExprKind::Var { id } => *id == acc,
            IrExprKind::BinOp {
                op: almide_ir::BinOp::ConcatList | almide_ir::BinOp::ConcatStr,
                left,
                ..
            } => concat_leftmost_is_var(left, acc),
            _ => false,
        }
    }
    let is_self_append = |e: &IrExpr, acc: VarId| -> bool {
        matches!(
            &e.kind,
            IrExprKind::BinOp { op: almide_ir::BinOp::ConcatList | almide_ir::BinOp::ConcatStr, .. }
        ) && concat_leftmost_is_var(e, acc)
    };
    let is_identity = |e: &IrExpr, acc: VarId| -> bool {
        matches!(&e.kind, IrExprKind::Var { id } if *id == acc)
    };
    // A PURE Module call WRAPPING the growth (`string.take(acc + "x", 8)` — the
    // churn spin): the callee is a pure stdlib fn (fresh-owned result by the
    // calling convention — self-hosts copy, never alias their inputs), and some
    // argument grows from / passes through the accumulator. The loop-carried slot
    // takes the fresh value via the same drop-old/alloc-new; the in-loop reassign
    // materializes the call through the standard pure-module path.
    let is_wrapped_growth = |e: &IrExpr, acc: VarId| -> bool {
        matches!(&e.kind,
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
                if crate::purity::is_pure(module.as_str(), func.as_str())
                    && args.iter().any(|a|
                        is_self_append(a, acc)
                            || matches!(&a.kind, IrExprKind::Var { id } if *id == acc)))
    };
    // A RESET to a FRESH EMPTY heap value (`cur = []` / `acc = ""`): the parser-row shape resets the
    // current-row accumulator after a delimiter. Like a self-append it is a fresh owned heap value the
    // loop-carried slot takes via drop-old/alloc-new (cert `i(id)m`); the in-loop `Assign` lowering's
    // general `lower_owned_heap_field` path materializes the empty literal.
    let is_reset = |e: &IrExpr| -> bool {
        matches!(&e.kind, IrExprKind::List { elements } if elements.is_empty())
            || matches!(&e.kind, IrExprKind::LitStr { value } if value.is_empty())
    };
    let mut append_accs: Vec<usize> = Vec::new();
    for i in 0..n {
        if carried0[i] && is_heap_ty(&params[i].ty) {
            // Each self-call passes the accumulator UNCHANGED (`acc`, a pass-through branch), APPENDED
            // (`acc + [x]`), or RESET to a fresh empty (`[]`/`""`); at least one grows/resets it (else
            // not carried). A heap carry outside these needs a general back-edge merge — unsupported.
            if calls0.iter().all(|c| {
                is_identity(&c[i], params[i].var)
                    || is_self_append(&c[i], params[i].var)
                    || is_reset(&c[i])
                    || is_wrapped_growth(&c[i], params[i].var)
            }) {
                append_accs.push(i);
            } else {
                return None;
            }
        }
    }
    drop(calls0);
    drop(bases0);

    // Build the (possibly substituted) working body + params + upfront slot-init binds.
    let mut slot_next = max_v + 3;
    let mut upfront: Vec<IrStmt> = Vec::new();
    let mut params_v: Vec<almide_ir::IrParam> = params.to_vec();
    let subst_body: Option<IrExpr> = if append_accs.is_empty() {
        None
    } else {
        let mut b = work_body.clone();
        for &ai in &append_accs {
            let slot = VarId(slot_next);
            slot_next += 1;
            let acc_var = params[ai].var;
            let list_ty = params[ai].ty.clone();
            // upfront: `let slot = acc + <empty>` — a fresh OWNED copy of the borrowed accumulator
            // param (the concat always allocates, so the slot never aliases it). A String
            // accumulator copies via `acc + ""` (`ConcatStr`); a list via `acc + []` (`ConcatList`).
            let (empty, concat_op) = if matches!(list_ty, Ty::String) {
                (tco_ir(IrExprKind::LitStr { value: String::new() }, Ty::String), almide_ir::BinOp::ConcatStr)
            } else {
                (tco_ir(IrExprKind::List { elements: vec![] }, list_ty.clone()), almide_ir::BinOp::ConcatList)
            };
            let copy = tco_ir(
                IrExprKind::BinOp {
                    op: concat_op,
                    left: Box::new(tco_ir(IrExprKind::Var { id: acc_var }, list_ty.clone())),
                    right: Box::new(empty),
                },
                list_ty.clone(),
            );
            upfront.push(IrStmt {
                kind: IrStmtKind::Bind {
                    var: slot,
                    mutability: almide_ir::Mutability::Var,
                    ty: list_ty.clone(),
                    value: copy,
                },
                span: None,
            });
            let slot_ref = tco_ir(IrExprKind::Var { id: slot }, list_ty);
            b = almide_ir::substitute_var_in_expr(&b, acc_var, &slot_ref);
            params_v[ai].var = slot;
        }
        Some(b)
    };
    let work_ref: &IrExpr = subst_body.as_ref().unwrap_or(work_body);
    let params2: &[almide_ir::IrParam] = &params_v;

    // SECOND collection — on the substituted body, with the slot params.
    let mut calls: Vec<&[IrExpr]> = Vec::new();
    let mut bases: Vec<&IrExpr> = Vec::new();
    tco_collect(work_ref, fn_name, &mut calls, &mut bases)?;
    if calls.is_empty() || bases.is_empty() {
        return None;
    }
    if calls.iter().any(|c| c.len() != n) {
        return None;
    }
    // A param is loop-CARRIED iff some self-call passes a value other than the param itself.
    let mut carried = vec![false; n];
    for c in &calls {
        for i in 0..n {
            if !matches!(&c[i].kind, IrExprKind::Var { id } if *id == params2[i].var) {
                carried[i] = true;
            }
        }
    }
    // The list-iterator param is now INVARIANT — its `list.drop(cs,1)` self-call arg is replaced by
    // the `idx` bump (in `tco_rewrite`), so `cs` is never reassigned in the loop.
    if let Some((_, _, ci)) = &lit {
        carried[*ci] = false;
    }
    // A carried HEAP arg is admitted ONLY as an append-accumulator SLOT (handled below by the in-loop
    // `Assign` lowering as drop-old/alloc-new); any other heap carry needs a general back-edge merge.
    let append_slots: std::collections::BTreeSet<VarId> =
        append_accs.iter().map(|&i| params2[i].var).collect();
    if (0..n)
        .any(|i| carried[i] && is_heap_ty(&params2[i].ty) && !append_slots.contains(&params2[i].var))
    {
        return None;
    }
    // SIMULTANEOUS-UPDATE SAFETY. `tco_rewrite` stages scalar updates in temps and runs the heap
    // accumulator assigns BEFORE committing them, so scalar↔scalar and heap-reads-scalar are correct.
    // A HEAP accumulator arg that reads ANOTHER carried HEAP accumulator (`rows = rows + [cur]` while
    // `cur = []`) is handled by emitting the heap assigns in READ-DEPENDENCY order (reader before the
    // accumulator it reads — `order_heap_accs_by_read_dep` in tco_rewrite), so the reader sees the OLD
    // value. WALL only the residual the topological order CANNOT serialize: a CYCLE (`a = a + b`,
    // `b = b + a` — no order sees both olds; needs owned-temp staging, not in this brick).
    {
        for c in &calls {
            let changed_heap: Vec<usize> = (0..n)
                .filter(|&i| {
                    carried[i]
                        && is_heap_ty(&params2[i].ty)
                        && !matches!(&c[i].kind, IrExprKind::Var { id } if *id == params2[i].var)
                })
                .collect();
            if order_heap_accs_by_read_dep(&changed_heap, c, params2).is_none() {
                return None; // a heap-accumulator read cycle — unsupported
            }
        }
        // PURE-VAR ALIAS HAZARD: a carried scalar whose new value is exactly ANOTHER carried param
        // (`start = pos`) cannot be staged in a copy temp — `let t = pos` ALIASES pos's local, so the
        // later `start = t` reads pos's ALREADY-updated value (off-by-one). A COMPUTED arg (`pos + 1`)
        // stages a fresh value and is fine. Wall the pure-var-aliasing form (rare; the parser loops use
        // computed indices like `pos + 1`).
        let carried_scalars: std::collections::BTreeSet<VarId> = (0..n)
            .filter(|&i| carried[i] && !is_heap_ty(&params2[i].ty))
            .map(|i| params2[i].var)
            .collect();
        for c in &calls {
            for i in 0..n {
                if carried[i] {
                    if let IrExprKind::Var { id } = &c[i].kind {
                        if *id != params2[i].var && carried_scalars.contains(id) {
                            return None;
                        }
                    }
                }
            }
        }
    }
    let base_exprs: Vec<IrExpr> = bases.iter().map(|b| (*b).clone()).collect();
    let ret_ty = body.ty.clone();

    // Does ANY base case reference a LOOP-BODY-LOCAL binding — a `let`/destructure in the loop body
    // (e.g. `let (field, np) = pf(…)`) — rather than only carried params? Such a base must be computed
    // IN the loop (the binding is dead in the post-loop dispatch — the parse_rows_rec use-after-free).
    // `free_vars(base)` excludes anything the base binds internally, so the intersection is exactly the
    // loop-body bindings the base READS from an enclosing scope.
    let loop_lets = almide_ir::free_vars::bound_vars(work_ref);
    let base_reads_loop_local = base_exprs.iter().any(|b| {
        almide_ir::free_vars::free_vars(b, &std::collections::HashSet::new())
            .iter()
            .any(|v| loop_lets.contains(v))
    });
    // brick 2: a Value-CONTAINING tuple result must ALSO route to the result accumulator. The
    // post-loop dispatch recomputes a tuple base from the carried params, but a tuple whose base
    // holds a `value.object(..)`/`value.str(..)` CALL alongside a sibling scalar carry reads the
    // scalar STALE (pos=0 not the loop's final pos) — the in-loop accumulator reads the LIVE values.
    // A Value-FREE tuple (csv `pf`'s `(acc, pos)`) works via the dispatch and routing it regresses
    // parse_rows_rec, so gate strictly on a Value-containing tuple.
    let tuple_with_value = matches!(&ret_ty, Ty::Tuple(tys) if tys.iter().any(is_value_ty));
    // When it does (or a base reads a loop-body-local), carry the base value out through a RESULT
    // ACCUMULATOR computed in the loop, and the post-loop is a trivial read. Needs an empty initial
    // value of the result type; without one DECLINE the TCO entirely — the function keeps its
    // memory-safe non-TCO form (a clean wall), never the dispatch's use-after-free.
    // The in-loop RESULT ACCUMULATOR materializes each base via the cap-as-tag heap-Ok Result block
    // (`materialize_result_str` over `lower_result_str_piece`), which needs a HEAP Ok payload. A
    // `Result[Unit/scalar, String]` (`ok(())`) has no heap Ok piece for that path, and routing it
    // through the len-as-tag materializers instead would drift from the empty-init / drop repr (the
    // Result-repr-drift hole) — so the loop-slot reassign walls. DECLINE the TCO for that shape: the
    // function falls to the proven REAL recursive lowering (control_p4 self-call arm), which is
    // input-bounded and byte-matches v0. Only the accumulator path is affected — the post-loop
    // DISPATCH form (no loop-body-local base) keeps lowering its bases via the non-TCO arm path.
    {
        use almide_lang::types::constructor::TypeConstructorId;
        let non_heap_ok_result = matches!(&ret_ty,
            Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 && !is_heap_ty(&a[0]));
        // A `Result[Option[<nested-heap>], String]` Ok payload (read_message: `Option[JsonRpcRequest]`)
        // also has no SOUND in-loop result-accumulator materializer: `materialize_result_str` would mask
        // the Option block with the flat `DropListStr`, LEAKING the nested record/Value inside the Some.
        // DECLINE the TCO for that shape too — the function falls to the proven REAL recursive lowering
        // (control_p4 self-call arm; input-bounded, byte-matches v0), where the `ok(<Option>)`/`ok(none)`
        // bases lower via `try_lower_result_option_ctor` (`resrec:opt_<R>` + the generated `$__drop_opt_<R>`)
        // and the `ok(parse_and_wrap(b)!)` arms via the unwrap-rewrap-identity → bare tail-call. Option[String]
        // (a flat 0-or-1 `DropListStr` block) is EXCLUDED — its accumulator IS sound, so it keeps TCO.
        let option_nested_ok_result = matches!(&ret_ty,
            Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 && matches!(&a[1], Ty::String)
                && matches!(&a[0], Ty::Applied(TypeConstructorId::Option, oa)
                    if oa.len() == 1 && is_heap_ty(&oa[0]) && !matches!(&oa[0], Ty::String)));
        if (base_reads_loop_local || tuple_with_value)
            && (non_heap_ok_result || option_nested_ok_result)
        {
            return None;
        }
    }
    let result_var: Option<VarId> = if base_reads_loop_local || tuple_with_value {
        tco_empty_for(&ret_ty)?;
        let rv = VarId(slot_next);
        slot_next += 1;
        Some(rv)
    } else {
        None
    };

    let mut next_kind = 1i64;
    // `slot_next` is the next free VarId (after rk / list-iter idx / append slots / result) — tco_rewrite
    // draws its simultaneous-update temps from here.
    let loop_body = tco_rewrite(
        work_ref, fn_name, params2, &carried, rk, &mut next_kind, idx_var, &mut slot_next, result_var,
    );

    // `rk == k` (the loop guard uses `rk == 0`; the post-loop dispatch uses `rk == <base kind>`).
    let eq_rk = |k: i64| {
        tco_ir(
            IrExprKind::BinOp {
                op: almide_ir::BinOp::Eq,
                left: Box::new(tco_ir(IrExprKind::Var { id: rk }, Ty::Int)),
                right: Box::new(tco_ir(IrExprKind::LitInt { value: k }, Ty::Int)),
            },
            Ty::Bool,
        )
    };
    // Post-loop: the accumulator path just READS the result the loop computed; otherwise the dispatch
    // `if rk == 1 then base_1 else if … else base_N` recomputes the hit base from the carried params.
    let post = if let Some(rv) = result_var {
        tco_ir(IrExprKind::Var { id: rv }, ret_ty.clone())
    } else {
        let mut post = base_exprs.last()?.clone();
        for (idx, base) in base_exprs.iter().enumerate().rev().skip(1) {
            post = tco_ir(
                IrExprKind::If {
                    cond: Box::new(eq_rk((idx + 1) as i64)),
                    then: Box::new(base.clone()),
                    else_: Box::new(post),
                },
                ret_ty.clone(),
            );
        }
        post
    };

    // `{ [let slot = acc + [];]* [var idx = 0;] var rk = 0; while (rk == 0) { <loop_body> }; <post> }`
    // The append-accumulator slot inits (owned copies of the borrowed `acc` params) come FIRST.
    let mut inits: Vec<IrStmt> = upfront;
    if let Some(iv) = idx_var {
        inits.push(IrStmt {
            kind: IrStmtKind::Bind {
                var: iv,
                mutability: almide_ir::Mutability::Var,
                ty: Ty::Int,
                value: tco_ir(IrExprKind::LitInt { value: 0 }, Ty::Int),
            },
            span: None,
        });
    }
    // The result accumulator (when used) starts at an empty value of the result type — a placeholder
    // the first base case overwrites IN the loop; declared mutable so the in-loop base assigns it.
    if let Some(rv) = result_var {
        inits.push(IrStmt {
            kind: IrStmtKind::Bind {
                var: rv,
                mutability: almide_ir::Mutability::Var,
                ty: ret_ty.clone(),
                value: tco_empty_for(&ret_ty).expect("checked Some above"),
            },
            span: None,
        });
    }
    let init = IrStmt {
        kind: IrStmtKind::Bind {
            var: rk,
            mutability: almide_ir::Mutability::Var,
            ty: Ty::Int,
            value: tco_ir(IrExprKind::LitInt { value: 0 }, Ty::Int),
        },
        span: None,
    };
    inits.push(init);
    let while_stmt = IrStmt {
        kind: IrStmtKind::Expr {
            expr: tco_ir(
                IrExprKind::While {
                    cond: Box::new(eq_rk(0)),
                    body: vec![IrStmt { kind: IrStmtKind::Expr { expr: loop_body }, span: None }],
                },
                Ty::Unit,
            ),
        },
        span: None,
    };
    inits.push(while_stmt);
    Some(tco_ir(
        IrExprKind::Block { stmts: inits, expr: Some(Box::new(post)) },
        ret_ty,
    ))
}

