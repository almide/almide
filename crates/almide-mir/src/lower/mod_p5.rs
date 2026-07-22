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
                IrExprKind::LitFloat { value } => Some(crate::lower::float_lit_bits(*value, &e.ty)),
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
            // LAMBDA PARAMS are binders too (`(entry) => …` — `entry` has a VarId
            // no Var/Bind/pattern hook sees). A fresh synthetic var colliding with
            // one poisons the lift's capture analysis (the C-127 ANF desugar hit
            // exactly this: its let-var collided with the map lambda's param).
            if let IrExprKind::Lambda { params, .. } = &e.kind {
                for (v, _) in params {
                    self.0 = self.0.max(v.0);
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