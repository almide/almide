
/// Does a statement list contain a `break`/`continue` that targets THIS loop — i.e.
/// not nested inside another loop (which captures its own)? Used to wall a loop body
/// whose early-exit path would skip the per-iteration frame's drops (a leak).
pub(crate) fn body_breaks_or_continues(stmts: &[IrStmt]) -> bool {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct Scan {
        found: bool,
    }
    impl IrVisitor for Scan {
        fn visit_expr(&mut self, e: &IrExpr) {
            match &e.kind {
                IrExprKind::Break | IrExprKind::Continue => self.found = true,
                // A nested loop captures its OWN break/continue — do not descend.
                IrExprKind::ForIn { .. } | IrExprKind::While { .. } => {}
                _ => walk_expr(self, e),
            }
        }
    }
    let mut s = Scan { found: false };
    for stmt in stmts {
        s.visit_stmt(stmt);
    }
    s.found
}

/// Does a loop body REASSIGN a HEAP variable (`acc = acc + "x"`, `xs = xs + [e]`) in a
/// position the THIS-loop model-one-iteration fallback would reach (not nested inside an
/// inner loop, which manages its own)? Such a reassignment is the loop ACCUMULATOR: the
/// fallback DEFERS it (it emits no rebind, `value_of[acc]` stays pinned to the pre-loop
/// handle) — memory-safe but the accumulation is DROPPED, so the loop prints the initial
/// value (e.g. `var acc="S"; while i<3 { acc=acc+"x" }` → v0 `Sxxx`, the fallback `S`).
/// The executable `try_lower_scalar_while`/`_for_*` paths already decline a heap reassign
/// and roll back, so a body reaching the fallback with one cannot be faithfully run — the
/// caller WALLs it instead of silently eliding the accumulation.
/// VarIds that are the TARGET of an `Assign` (`x = …`) lexically INSIDE a `while`/`for` loop in
/// `body` — the loop-carried (option-C) reassignment slots. Used to wall a mutable `var x = r.field`
/// owned-field-`Dup` bind whose `x` is loop-reassigned (the initial owned copy + the per-iteration
/// option-C drop are an unproven ownership coordination the kernel cert REJECTS). A straight-line
/// (top-level) reassignment is NOT included — that owned-Dup + scope-end drop is balanced and sound.
pub(crate) fn loop_reassigned_vars(body: &IrExpr) -> std::collections::HashSet<VarId> {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct Scan {
        in_loop: u32,
        found: std::collections::HashSet<VarId>,
    }
    impl IrVisitor for Scan {
        fn visit_stmt(&mut self, stmt: &IrStmt) {
            if self.in_loop > 0 {
                if let IrStmtKind::Assign { var, .. } = &stmt.kind {
                    self.found.insert(*var);
                }
            }
            almide_ir::visit::walk_stmt(self, stmt);
        }
        fn visit_expr(&mut self, e: &IrExpr) {
            match &e.kind {
                IrExprKind::ForIn { .. } | IrExprKind::While { .. } => {
                    self.in_loop += 1;
                    walk_expr(self, e);
                    self.in_loop -= 1;
                }
                _ => walk_expr(self, e),
            }
        }
    }
    let mut s = Scan { in_loop: 0, found: std::collections::HashSet::new() };
    s.visit_expr(body);
    s.found
}

pub(crate) fn body_reassigns_heap(stmts: &[IrStmt]) -> bool {
    use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};
    struct Scan {
        found: bool,
    }
    impl IrVisitor for Scan {
        fn visit_stmt(&mut self, stmt: &IrStmt) {
            if self.found {
                return;
            }
            if let IrStmtKind::Assign { value, .. } = &stmt.kind {
                if is_heap_ty(&value.ty) {
                    self.found = true;
                    return;
                }
            }
            walk_stmt(self, stmt);
        }
        fn visit_expr(&mut self, e: &IrExpr) {
            if self.found {
                return;
            }
            match &e.kind {
                // A nested loop captures its OWN accumulator — do not descend.
                IrExprKind::ForIn { .. } | IrExprKind::While { .. } => {}
                _ => walk_expr(self, e),
            }
        }
    }
    let mut s = Scan { found: false };
    for stmt in stmts {
        s.visit_stmt(stmt);
    }
    s.found
}

/// Collect every var a loop body HEAP-REASSIGNS — directly (`Assign` with a heap
/// value, `FieldAssign`/`MapInsert` targets whose write lowers to a rebind) or via
/// the functional-rebind rewrites (`list.push(v, x)` / `string.push(s, x)` /
/// `bytes.push(b, x)` and their Member-receiver forms, which rewrite into
/// Assign/FieldAssign during lowering). Descends into nested control flow but NOT
/// into nested loops (each loop pre-copies its own borrowed slots when reached).
pub(crate) fn collect_heap_reassign_vars(stmts: &[IrStmt], out: &mut Vec<VarId>) {
    use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};
    struct Scan<'a> {
        out: &'a mut Vec<VarId>,
    }
    impl Scan<'_> {
        fn push(&mut self, v: VarId) {
            if !self.out.contains(&v) {
                self.out.push(v);
            }
        }
        fn push_receiver(&mut self, recv: &IrExpr) {
            match &recv.kind {
                IrExprKind::Var { id } => self.push(*id),
                IrExprKind::Member { object, .. } => {
                    if let IrExprKind::Var { id } = &object.kind {
                        self.push(*id);
                    }
                }
                _ => {}
            }
        }
    }
    impl IrVisitor for Scan<'_> {
        fn visit_stmt(&mut self, stmt: &IrStmt) {
            match &stmt.kind {
                IrStmtKind::Assign { var, value } => {
                    if is_heap_ty(&value.ty) {
                        self.push(*var);
                    }
                }
                IrStmtKind::FieldAssign { target, value, .. } => {
                    if is_heap_ty(&value.ty) {
                        self.push(*target);
                    }
                }
                IrStmtKind::MapInsert { target, .. } => {
                    self.push(*target);
                }
                _ => {}
            }
            walk_stmt(self, stmt);
        }
        fn visit_expr(&mut self, e: &IrExpr) {
            match &e.kind {
                IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
                    if func.as_str() == "push"
                        && matches!(module.as_str(), "list" | "string" | "bytes")
                        && !args.is_empty() =>
                {
                    self.push_receiver(&args[0]);
                    walk_expr(self, e);
                }
                // A nested loop pre-copies its OWN borrowed slots — do not descend.
                IrExprKind::ForIn { .. } | IrExprKind::While { .. } => {}
                _ => walk_expr(self, e),
            }
        }
    }
    let mut s = Scan { out };
    for stmt in stmts {
        s.visit_stmt(stmt);
    }
}

/// Does `body` directly project a FIELD/INDEX off `v` — a `v.field` Member or `v.N` TupleIndex
/// EXPRESSION whose object is `Var(v)`? A `for-in` heap-AGGREGATE element is bound as the whole
/// element handle, which is correct for a `let (x, y) = v` destructure (a tuple PATTERN) or passing
/// `v` whole, but a direct `v.field` / `v.N` projection on the loop element currently reads off the
/// wrong handle (a silent miscompile). Only that projecting case must wall.
pub(crate) fn body_reads_var_field(body: &[IrStmt], v: VarId) -> bool {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct Scan {
        v: VarId,
        found: bool,
    }
    impl IrVisitor for Scan {
        fn visit_expr(&mut self, e: &IrExpr) {
            if self.found {
                return;
            }
            let obj = match &e.kind {
                IrExprKind::Member { object, .. } => Some(object),
                IrExprKind::TupleIndex { object, .. } => Some(object),
                _ => None,
            };
            if let Some(o) = obj {
                if matches!(&o.kind, IrExprKind::Var { id } if *id == self.v) {
                    self.found = true;
                    return;
                }
            }
            walk_expr(self, e);
        }
    }
    let mut s = Scan { v, found: false };
    for stmt in body {
        s.visit_stmt(stmt);
    }
    s.found
}

/// Find the type a variable is USED at in a body (its first reference's `ty`) — for
/// a `for-in` loop variable, this is its element type (the `ForIn` node carries no
/// explicit element type). `None` if the variable is unused (then its heap-ness does
/// not matter — nothing references it to manage).
pub(crate) fn find_var_ty(stmts: &[IrStmt], var: VarId) -> Option<Ty> {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct Find {
        var: VarId,
        ty: Option<Ty>,
    }
    impl IrVisitor for Find {
        fn visit_expr(&mut self, e: &IrExpr) {
            if self.ty.is_some() {
                return;
            }
            if let IrExprKind::Var { id } = &e.kind {
                if *id == self.var {
                    self.ty = Some(e.ty.clone());
                    return;
                }
            }
            walk_expr(self, e);
        }
    }
    let mut f = Find { var, ty: None };
    for stmt in stmts {
        if f.ty.is_some() {
            break;
        }
        f.visit_stmt(stmt);
    }
    f.ty
}

/// Extract a concrete initializer from a fresh-heap bind value. A `List[Int]`
/// literal yields [`Init::IntList`]; everything else is [`Init::Opaque`] (the
/// computation is carried by a later brick).
/// Does the stdlib `module.func` call return a real MATERIALIZED 0-or-1-element-list
/// Option (a self-host Option fn whose impl returns through tail-materialized `Some`/
/// `None`)? Its result may be tracked in `materialized_options` so a `match` over it
/// EXECUTES. The SINGLE SOURCE for both the bound-var path (binds.rs) and the direct-
/// subject path (control.rs) — keep them in sync to avoid tracking a non-materialized
/// call (which would misread as `None`). Add a name only when its self-host impl lands.
pub fn is_self_host_option_module_fn(module: &str, func: &str) -> bool {
    match module {
        "list" => {
            // `fold` is here for the ONE Option-returning variant (`list.fold_ols`,
            // Option[List[String]] acc) — a scalar-acc fold subject never reaches the
            // variant-tracking sites (they gate on a heap/variant subject type first).
            // `pop` — the in-place self-host (list_pop.almd) returns a real materialized
            // Option built by the same Some/None ctor rails as `get`; a heap-element pop
            // routes to the unregistered `list.pop_x` and walls at render, so tracking
            // its bound result is never a misread.
            matches!(func, "get" | "first" | "last" | "index_of" | "binary_search" | "max" | "min" | "find" | "find_int_str" | "find_index" | "reduce" | "fold" | "get_str" | "first_str" | "last_str" | "pop")
        }
        "string" => matches!(func, "index_of" | "last_index_of" | "codepoint" | "first" | "last" | "get" | "strip_prefix" | "strip_suffix"),
        "bytes" => matches!(func, "get" | "index_of"),
        // regex.find builds a materialized Option[String] via the self-hosted
        // engine's ordinary some()/none ctors (stdlib/regex_engine.almd) — a
        // `match` over the bound result EXECUTES.
        "regex" => matches!(func, "find" | "captures"),
        // random.choice delegates to the generic list.get (a materialized Option) /
        // returns a literal `none` — either way the bound result is a real len-tag block.
        "random" => func == "choice",
        // result.to_option builds a materialized Option[Int] from a Result's len-tag (Ok → Some,
        // Err → None); option.map rebuilds a materialized Option (Some(f(x)) / None) — a `match`
        // over either result EXECUTES.
        "result" => matches!(func, "to_option" | "to_err_option"),
        "option" => matches!(func, "map" | "filter" | "flat_map" | "or_else" | "flatten" | "zip" | "collect" | "collect_map"),
        // map.get(m, k) builds a materialized Option[Int] (Some(value) when the key is found via
        // the paired-slot scan, None otherwise) — a `match` over it EXECUTES.
        // map.find(m, pred) — the predicate-search HOF — builds a materialized
        // Option[(K, V)] (Some((key, value)) on the first predicate hit, None otherwise); a
        // `match` over it should ALSO execute. See the paired routing in control.rs (near
        // `is_self_host_option_call`), which detects an Option[(String, <scalar>)] SUBJECT
        // and routes its DROP to the type-specific generated `$__drop_opt_str_int` instead of
        // the generic flat one-level-exact path — the payload is a TUPLE that itself owns a
        // heap slot (the String), not a single flat handle a blind `rc_dec` would free.
        "map" => matches!(func, "get" | "find"),
        // int.to_{int,uint}N_checked builds a materialized Option[Int] (Some(n) when n fits the
        // N-bit range, None otherwise) — a `match` over it EXECUTES.
        "int" => matches!(
            func,
            "to_int8_checked"
                | "to_int16_checked"
                | "to_int32_checked"
                | "to_uint8_checked"
                | "to_uint16_checked"
                | "to_uint32_checked"
                | "to_uint64_checked"
                | "to_float32_checked"
        ),
        // float.to_{int,uint}N_checked builds a materialized Option[IntN] (Some(to_T(n)) when n is
        // an exact integer in range, None otherwise) — a `match` over it EXECUTES. Same scalar shape
        // as the int variants (IntN is i64-repr); to_int64/to_uint64/to_float32 are not yet hosted.
        "float" => matches!(
            func,
            "to_int8_checked"
                | "to_int16_checked"
                | "to_int32_checked"
                | "to_uint8_checked"
                | "to_uint16_checked"
                | "to_uint32_checked"
                | "to_int64_checked"
                | "to_uint64_checked"
                | "to_float32_checked"
        ),
        // json.as_int/as_float/as_bool build a materialized Option (Some(scalar) / None) by reading
        // the shared Value tag (@4) — a `match`/`??` over the result EXECUTES. as_int/as_float WIDEN
        // across Int/Float exactly like v0. json.as_string is the heap-payload case: Some(a deep copy
        // of the Str payload @12) / None — the repr-poly Option[String] materialization (a 0-or-1-
        // element DynListStr, same path as list.get_str); as_array (List[Value]) is a refinement.
        // json.get is self-hosted (`match value.get(j,key) { ok(v) => some(v), err(_) => none }`),
        // so it returns a materialized Option[Value] — `json.get(v,k) ?? d` (→ option.value_unwrap_or)
        // and a `match` over it EXECUTE. The ubiquitous json-accessor idiom, the root of the
        // wasm-bindgen get_str/get_kind cascade.
        // json.get_<T>(j, key) is self-hosted (`match value.get(j,key) { ok(v) => __r2o(value.as_<T>(v)),
        // err(_) => none }`), returning a materialized Option[T] — `json.get_string(v,k) ?? ""`,
        // `json.get_bool(v,k) ?? false`, `json.get_array(v,k) ?? []` and a `match` over any of them
        // EXECUTE (the typed-accessor sibling of json.get, the manifest/jsonrpc parser idiom root).
        // http.get_header is self-hosted (stdlib/http_response.almd) returning a real
        // materialized Option[String] — a `match` over it EXECUTES.
        "http" => matches!(func, "get_header"),
        "json" => matches!(
            func,
            "as_int" | "as_float" | "as_bool" | "as_string" | "get" | "as_array"
                | "get_string" | "get_int" | "get_float" | "get_bool" | "get_array"
                | "get_path"
        ),
        _ => false,
    }
}

/// A `Sym`-interning shorthand for the recursive Display builders below.
fn sym(s: &str) -> almide_lang::intern::Sym {
    almide_lang::intern::sym(s)
}

/// A `LitStr` IR leaf (a static text fragment of a Display expansion — `"Point { "`,
/// `", "`, `" }"`, `"("`, `")"`). No call, the no-op leaf of the `ConcatStr` fold.
fn lit_str(s: &str) -> IrExpr {
    IrExpr { kind: IrExprKind::LitStr { value: s.to_string() }, ty: Ty::String, span: None, def_id: None }
}

/// Left-nest `parts` into a `ConcatStr` fold seeded by `""` — the SAME shape
/// [`desugar_string_interp`] builds, reused for a record/tuple body so the whole
/// expansion is one uniform `ConcatStr` tree (K parts ⇒ K `__str_concat` folds).
fn concat_all(parts: Vec<IrExpr>) -> IrExpr {
    let mut acc = lit_str("");
    for p in parts {
        acc = IrExpr {
            kind: IrExprKind::BinOp {
                op: almide_ir::BinOp::ConcatStr,
                left: Box::new(acc),
                right: Box::new(p),
            },
            ty: Ty::String,
            span: None,
            def_id: None,
        };
    }
    acc
}

/// Wrap `value` in `module.func(value)` (a single `Call { Module }` node), the Display
/// leaf for a scalar/list/string field — `int.to_string(r.x)`, `string.quote(r.name)`,
/// `list.to_string(r.items)`, `float.to_string_compound(r.v)`.
fn to_string_call(module: &str, func: &str, value: IrExpr) -> IrExpr {
    IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Module { module: sym(module), func: sym(func), def_id: None },
            args: vec![value],
            type_args: Vec::new(),
        },
        ty: Ty::String,
        span: None,
        def_id: None,
    }
}

/// The DECLARATION-ordered fields of an aggregate `ty`, for the recursive Display
/// expansion: `(opt_type_name, Vec<(opt_field_name, field_ty)>)`. A `Ty::Named(name, args)`
/// resolves its fields via the layout `registry` (substituting generics) and carries the
/// type NAME (records print `Point { … }`); a structural `Ty::Record`/`Ty::Tuple` carries
/// no name. Returns `None` for a non-aggregate or unregistered type (the Display then
/// declines, the interp walls). MIRRORS `LowerCtx::aggregate_field_tys` exactly so the
/// desugar and the lowering agree on field count, order, and types.
fn resolve_aggregate(
    ty: &Ty,
    registry: &RecordLayouts,
) -> Option<(Option<String>, bool, Vec<(Option<String>, Ty)>)> {
    // `(type_name, is_tuple, [(field_name, field_ty)])`.
    match ty {
        Ty::Tuple(elems) => {
            Some((None, true, elems.iter().map(|t| (None, t.clone())).collect()))
        }
        Ty::Record { fields } => Some((
            None,
            false,
            fields.iter().map(|(n, t)| (Some(n.as_str().to_string()), t.clone())).collect(),
        )),
        Ty::Named(name, args) => {
            // Only registry-declared records resolve here; a `Ty::Named` that names no
            // record layout (an enum / alias / unknown) returns `None` and walls.
            let (generics, decl_fields) = registry.get(name.as_str())?;
            let mut subst: HashMap<almide_lang::intern::Sym, Ty> = HashMap::new();
            for (g, a) in generics.iter().zip(args.iter()) {
                subst.insert(*g, a.clone());
            }
            let fields = decl_fields
                .iter()
                .map(|(n, t)| (Some(n.as_str().to_string()), calls::subst_type_var(t, &subst)))
                .collect();
            Some((Some(name.as_str().to_string()), false, fields))
        }
        _ => None,
    }
}

/// Build the Display IR expression for an aggregate VALUE `obj` of type `ty` (a record or
/// tuple) — the recursive heart of `${record}` / `${tuple}`. Expands to a `ConcatStr` tree:
///   record: `"Name { " ++ "f0: " ++ fmt(obj.f0) ++ ", " ++ "f1: " ++ fmt(obj.f1) ++ " }"`
///   tuple:  `"(" ++ fmt(obj.0) ++ ", " ++ fmt(obj.1) ++ ")"`
/// where `fmt(field)` is [`display_value`] over the field-access node (`Member`/`TupleIndex`).
/// Returns `None` (the whole interp walls — NEVER wrong bytes) if `ty` is not a resolvable
/// aggregate or ANY field's type has no Display leaf. The `Member`/`TupleIndex` nodes lower
/// through the EXISTING value-model field access (scalar slot load / heap-field borrow), so
/// no new lowering machinery is needed — only this IR shape.
fn display_aggregate(obj: &IrExpr, ty: &Ty, registry: &RecordLayouts) -> Option<IrExpr> {
    let (type_name, is_tuple, fields) = resolve_aggregate(ty, registry)?;
    let mut parts: Vec<IrExpr> = Vec::new();
    // Opening: `Name { ` for a record, `(` for a tuple. A structural (un-named) record has
    // no v0 Display form (v0 only Displays a NAMED record), so wall it.
    if is_tuple {
        parts.push(lit_str("("));
    } else {
        let name = type_name?;
        parts.push(lit_str(&format!("{name} {{ ")));
    }
    for (idx, (fname, fty)) in fields.iter().enumerate() {
        if idx > 0 {
            parts.push(lit_str(", "));
        }
        if let Some(fname) = fname {
            parts.push(lit_str(&format!("{fname}: ")));
        }
        // The field-access node: `obj.fname` (Member) or `obj.idx` (TupleIndex), typed `fty`.
        let access = if is_tuple {
            IrExpr {
                kind: IrExprKind::TupleIndex { object: Box::new(obj.clone()), index: idx },
                ty: fty.clone(),
                span: None,
                def_id: None,
            }
        } else {
            IrExpr {
                kind: IrExprKind::Member {
                    object: Box::new(obj.clone()),
                    field: sym(fname.as_deref().unwrap_or("")),
                },
                ty: fty.clone(),
                span: None,
                def_id: None,
            }
        };
        parts.push(display_value(&access, registry)?);
    }
    parts.push(lit_str(if is_tuple { ")" } else { " }" }));
    Some(concat_all(parts))
}

/// Build the Display IR (a String-producing expression) for a VALUE `expr` of ANY type —
/// the per-field formatter the record/tuple Display calls recursively. Byte-matches v0's
/// AlmideRepr for the value's type:
///   - `Int`     → `int.to_string(expr)`              (signed decimal)
///   - `Bool`    → `bool.to_string(expr)`             (`true`/`false`)
///   - `Float`   → `float.to_string_compound(expr)`   (compound form — DROPS the `.0`)
///   - `String`  → `string.quote(expr)`               (double-quoted + escaped)
///   - `List[T]` → `list.to_string*(expr)`            (element-type-keyed, as the top-level interp)
///   - Record/Tuple → [`display_aggregate`] recursively (no call — an inline `ConcatStr`)
/// Returns `None` (so the enclosing Display declines and the interp walls) for any type
/// with no Display leaf — a nested `List[List[_]]` element, a Map/Set/Option field, an
/// unresolved var. NEVER emits a wrong-byte fallback.
fn display_value(expr: &IrExpr, registry: &RecordLayouts) -> Option<IrExpr> {
    // A nested record/tuple expands INLINE (recursive `ConcatStr`, no `to_string` call).
    if matches!(expr.ty, Ty::Record { .. } | Ty::Tuple(_) | Ty::Named(..))
        && resolve_aggregate(&expr.ty, registry).is_some()
    {
        return display_aggregate(expr, &expr.ty, registry);
    }
    // Every other value type wraps in its single `to_string`-family call.
    let (module, func) = display_leaf_call(&expr.ty)?;
    Some(to_string_call(module, func, expr.clone()))
}

/// The SINGLE `(module, func)` Display wrapper for a NON-aggregate value type — the source both
/// [`display_value`] (the IR builder) and [`value_synthetic_names`] (the gate counter) consult, so
/// the emitted call and the counted call AGREE by construction:
///   - `Int`     → `int.to_string`            `Bool`  → `bool.to_string`
///   - `Float`   → `float.to_string_compound` (compound form — drops the `.0`)
///   - `String`  → `string.quote`             (double-quoted + escaped)
///   - `List[T]` → `list.to_string*`          (element-type-keyed; unsupported → unlinked, walls)
///   - Map/Set/Option/Result → the unlinked `<module>.to_string` (walls — never wrong bytes)
/// `None` for a type with NO Display leaf at all (a bare unresolved var) — the Display declines.
fn display_leaf_call(ty: &Ty) -> Option<(&'static str, &'static str)> {
    match ty {
        // SIZED ints display like Int: a v1 record/variant slot is a uniform i64
        // (the narrow literal was widened at construction), so int.to_string prints
        // the exact stored value — incl. negative Int8/16/32 (sign carried in the
        // i64). UInt64 is EXCLUDED (a value above i64::MAX would misprint).
        Ty::Int
        | Ty::Int8
        | Ty::Int16
        | Ty::Int32
        | Ty::Int64
        | Ty::UInt8
        | Ty::UInt16
        | Ty::UInt32 => Some(("int", "to_string")),
        Ty::Bool => Some(("bool", "to_string")),
        Ty::Float => Some(("float", "to_string_compound")),
        Ty::String => Some(("string", "quote")),
        // List / Map / Set / Option / Result route through the element-type-keyed
        // `interp_to_string_call` (List → a self-host variant; the rest → an unlinked
        // `<module>.to_string` that walls). A Tuple/Record/variant/unresolved returns the
        // unlinked `compound.to_string` there, so the enclosing aggregate also walls.
        _ => interp_to_string_call(ty),
    }
}

/// The `(module, func)` pair whose call renders a value of type `ty` to its Almide-Display form
/// for the string-interpolation desugar. The MIR `CallFn` name is `"<module>.<func>"`, so this is
/// the SINGLE source both the leaf builder ([`interp_part_leaf`]) and the gate name-lister
/// ([`interp_synthetic_call_names`]) consult — they agree on the exact call name BY CONSTRUCTION,
/// keeping `mir == ir` for the corpus caps gate. The module MUST be pure (`purity::is_pure`).
///
/// For a `List[T]` the func is ELEMENT-TYPE-KEYED so each variant is a monomorphic self-host impl
/// that reads the slot at the right width/repr and formats the element in v0's COMPOUND form (NB:
/// the compound-Float element drops the trailing `.0` — see `list_to_string_f.almd`):
///   - `List[Int]`            → `list.to_string`     (i64 slot, decimal digits)
///   - `List[Float]`          → `list.to_string_f`   (f64-bits slot, compound float, drops `.0`)
///   - `List[Bool]`           → `list.to_string_b`   (i64 0/1 slot, `true`/`false`)
///   - `List[String]`         → `list.to_string_s`   (i32-handle slot, quoted+escaped)
/// Any OTHER element type (NESTED `List[List[_]]`, Map/Set/Option/Record element, an unresolved var)
/// returns `None`: the whole interp declines the desugar and stays cleanly walled — NEVER a wrong
/// byte. Nested lists are walled deliberately: v1 does not yet materialize a `List[List[_]]` literal
/// (the inner handles are never stored), so a nested element formatter would read garbage slots;
/// walling is the sound choice. (Map/Set/Option/Result top-level `to_string` stay unlinked = walled.)
fn interp_to_string_call(ty: &Ty) -> Option<(&'static str, &'static str)> {
    use almide_lang::types::constructor::TypeConstructorId;
    Some(match ty {
        // SIZED ints display like Int: a v1 scalar value is a uniform i64 (widened at
        // the literal/load), so int.to_string prints the exact stored value including
        // negative Int8/16/32. UInt64 is EXCLUDED (above i64::MAX would misprint).
        Ty::Int
        | Ty::Int8
        | Ty::Int16
        | Ty::Int32
        | Ty::Int64
        | Ty::UInt8
        | Ty::UInt16
        | Ty::UInt32 => ("int", "to_string"),
        Ty::Bool => ("bool", "to_string"),
        // Scalar `${f}` interp uses v0's Display format, which DROPS the `.0` for integer-valued
        // floats (`3.0`->`3`, `100.0`->`100`) — exactly the compound formatter
        // `float.to_string_compound`, NOT `float.to_string` (which keeps `.0` for an EXPLICIT
        // `float.to_string(x)` call). Same drop-.0 Display a Float record/list field already uses.
        Ty::Float => ("float", "to_string_compound"),
        // Decomposed (#781, cog 121): the List/Option/Result routings are verbatim
        // text moves into interp_{list,option,result}_to_string.
        Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => {
            interp_list_to_string(&args[0])
        }
        Ty::Applied(TypeConstructorId::Option, args) if args.len() == 1 => {
            interp_option_to_string(&args[0])
        }
        Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => {
            interp_result_to_string(&args[0], &args[1])
        }
        // `${Set[T]}` renders v0's `set.from_list([<elems>])` (insertion order). Self-hosted for Int;
        // any other element routes to the UNLINKED `set.to_string_x` (walls cleanly).
        Ty::Applied(TypeConstructorId::Set, args) if args.len() == 1 => match &args[0] {
            Ty::Int => ("set", "to_string"),
            Ty::String => ("set", "to_string_s"),
            _ => ("set", "to_string_x"),
        },
        // Map top-level `to_string` is not self-hosted → the synthesized call is UNLINKED, so the
        // using function walls at render (never a wrong byte). Keep routing it so the gate accounts the
        // same call name the lowering emits (mir == ir), exactly as before.
        // `${Map[K, V]}` renders v0's `["k": v, …]` (insertion order; empty → `[:]`). Self-hosted for
        // (String, Int); any other pairing routes to the UNLINKED `map.to_string_x` (walls cleanly).
        Ty::Applied(TypeConstructorId::Map, args) if args.len() == 2 => {
            match (&args[0], &args[1]) {
                (Ty::String, Ty::Int) => ("map", "to_string"),
                // `${Map[String, String]}` — quoted keys AND values (stdlib/map_to_string.almd).
                (Ty::String, Ty::String) => ("map", "to_string_ss"),
                // `${Map[String, Bool]}` — quoted keys, `true`/`false` values
                // (option_unwrap_or_else_heap's some(Map) probes).
                (Ty::String, Ty::Bool) => ("map", "to_string_sb"),
                // `${Map[String, List[Option[Int]]]}` — the mlo family display
                // (stdlib/map_mlo.almd; values via the list.to_string_lo composition).
                (Ty::String, Ty::Applied(TypeConstructorId::List, b))
                    if b.len() == 1
                        && matches!(&b[0], Ty::Applied(TypeConstructorId::Option, o)
                            if o.len() == 1 && matches!(o[0], Ty::Int)) =>
                {
                    ("map", "to_string_mlo")
                }
                // `${Map[Int, String]}` — the ivh display (`[10: "x", 20: "y"]`, raw int
                // keys + quoted/escaped String values; stdlib/map_ivh.almd).
                (Ty::Int, Ty::String) => ("map", "to_string_ivh"),
                // `${Map[Int, Float]}` — `[1: 0.5]` (raw int keys, shortest-round-trip
                // float values; stdlib/map_if.almd).
                (Ty::Int, Ty::Float) => ("map", "to_string_if"),
                // `${Map[String, List[Int]]}` — `["xs": [1, 2, 3]]` (quoted keys, list
                // values through their own interp; stdlib/map_hval.almd).
                (Ty::String, Ty::Applied(TypeConstructorId::List, b))
                    if b.len() == 1 && matches!(b[0], Ty::Int) =>
                {
                    ("map", "to_string_hval")
                }
                _ => ("map", "to_string_x"),
            }
        }
        // Tuple / Record / variant / any other type has no self-hosted `to_string` yet.
        // Route to an UNLINKED `to_string` so the interp DESUGARS to a real CallFn that the
        // render wall REJECTS (the function walls cleanly) — NEVER leave it Opaque, which
        // makes `println("${tuple}")` emit NOTHING (a silent empty miscompile). This is the
        // nested-`List` lesson (above) applied UNIFORMLY: no interp Expr part may fall to
        // Opaque. NEVER registered, so every such function walls all-or-nothing.
        _ => ("compound", "to_string"),
    })
}

/// `${List[T]}` interp routing per element type. Verbatim text move (#781).
fn interp_list_to_string(inner: &Ty) -> (&'static str, &'static str) {
    use almide_lang::types::constructor::TypeConstructorId;
    match inner {
        Ty::Int => ("list", "to_string"),
        Ty::Float => ("list", "to_string_f"),
        Ty::Bool => ("list", "to_string_b"),
        Ty::String => ("list", "to_string_s"),
        // A NESTED `List[List[Int/Float]]` renders through the composed self-host
        // (each row via the flat to_string, joined in brackets — byte-matches v0's Debug).
        Ty::Applied(TypeConstructorId::List, inner)
            if inner.len() == 1 && matches!(inner[0], Ty::Int) =>
        {
            ("list", "to_string_ll")
        }
        Ty::Applied(TypeConstructorId::List, inner)
            if inner.len() == 1 && matches!(inner[0], Ty::Float) =>
        {
            ("list", "to_string_llf")
        }
        // `${List[Option[Int]]}` → `[some(1), none, some(3)]` — composed from the
        // per-element option display (stdlib/list_to_string_lo.almd).
        Ty::Applied(TypeConstructorId::Option, inner)
            if inner.len() == 1 && matches!(inner[0], Ty::Int) =>
        {
            ("list", "to_string_lo")
        }
        // `${List[Option[Bool]]}` (the C-149 option.to_list chain).
        Ty::Applied(TypeConstructorId::Option, inner)
            if inner.len() == 1 && matches!(inner[0], Ty::Bool) =>
        {
            ("list", "to_string_lob")
        }
        // `${List[Result[Int, String]]}` → `[ok(1), err("bad")]` — fan.settle's
        // pure-thunk result list, composed from the per-element result display
        // (stdlib/list_to_string_lr.almd).
        Ty::Applied(TypeConstructorId::Result, inner)
            if inner.len() == 2
                && matches!(inner[0], Ty::Int)
                && matches!(inner[1], Ty::String) =>
        {
            ("list", "to_string_lr")
        }
        // `${List[Map[String, List[Option[Int]]]]}` — compound_repr_interp's `deep`,
        // composed from the per-element mlo map display (stdlib/map_mlo.almd).
        Ty::Applied(TypeConstructorId::Map, kv)
            if kv.len() == 2 && matches!(kv[0], Ty::String)
                && matches!(&kv[1], Ty::Applied(TypeConstructorId::List, b)
                    if b.len() == 1
                        && matches!(&b[0], Ty::Applied(TypeConstructorId::Option, o)
                            if o.len() == 1 && matches!(o[0], Ty::Int))) =>
        {
            ("list", "to_string_lmlo")
        }
        // `${List[Map[String, List[Int]]]}` → `[["a": [1, 2]], ["b": [3]]]` — each
        // map through its own interp (stdlib/map_hval.almd's list_to_string_lmh).
        Ty::Applied(TypeConstructorId::Map, kv)
            if kv.len() == 2 && matches!(kv[0], Ty::String)
                && matches!(&kv[1], Ty::Applied(TypeConstructorId::List, b)
                    if b.len() == 1 && matches!(b[0], Ty::Int)) =>
        {
            ("list", "to_string_lmh")
        }
        // `${List[(String, Int)]}` → `[("é", 2), ("a", 1)]` — string.run_length_encode's
        // pair list (stdlib/list_to_string_lsi.almd).
        Ty::Tuple(ts)
            if ts.len() == 2 && matches!(ts[0], Ty::String) && matches!(ts[1], Ty::Int) =>
        {
            ("list", "to_string_lsi")
        }
        // Any other unsupported element type (`List[Map]`, deeper nesting, …) routes to an
        // UNLINKED variant name so the interp DESUGARS to a real `list.to_string_x` CallFn that
        // the render wall then REJECTS — the function walls cleanly. Returning `None` here would
        // instead leave the interp Opaque and the `println` would emit NOTHING (a silent empty
        // miscompile); routing-to-unlinked preserves the all-or-nothing wall. NEVER registered.
        _ => ("list", "to_string_x"),
    }
    }