
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
            matches!(func, "get" | "first" | "last" | "index_of" | "binary_search" | "max" | "min" | "find" | "find_int_str" | "find_index" | "reduce" | "get_str" | "first_str" | "last_str")
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
        "option" => matches!(func, "map" | "filter" | "flat_map" | "or_else" | "flatten" | "zip" | "collect"),
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
        Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => match &args[0] {
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
            // `${List[Map[String, List[Int]]]}` → `[["a": [1, 2]], ["b": [3]]]` — each
            // map through its own interp (stdlib/map_hval.almd's list_to_string_lmh).
            Ty::Applied(TypeConstructorId::Map, kv)
                if kv.len() == 2 && matches!(kv[0], Ty::String)
                    && matches!(&kv[1], Ty::Applied(TypeConstructorId::List, b)
                        if b.len() == 1 && matches!(b[0], Ty::Int)) =>
            {
                ("list", "to_string_lmh")
            }
            // Any other unsupported element type (`List[Map]`, deeper nesting, …) routes to an
            // UNLINKED variant name so the interp DESUGARS to a real `list.to_string_x` CallFn that
            // the render wall then REJECTS — the function walls cleanly. Returning `None` here would
            // instead leave the interp Opaque and the `println` would emit NOTHING (a silent empty
            // miscompile); routing-to-unlinked preserves the all-or-nothing wall. NEVER registered.
            _ => ("list", "to_string_x"),
        },
        // `${Option[T]}` renders v0's `some(<T-repr>)` / `none`, routed to a PER-ELEMENT self-host
        // (mirrors List): the inner value uses its own interp form — Int decimal, String QUOTED,
        // Float drop-`.0`, Bool `true`/`false`. An unsupported element (a heap/nested payload) routes
        // to the UNLINKED `option.to_string_x` so the function walls cleanly (never a wrong byte).
        Ty::Applied(TypeConstructorId::Option, args) if args.len() == 1 => match &args[0] {
            Ty::Int => ("option", "to_string"),
            Ty::String => ("option", "to_string_s"),
            Ty::Float => ("option", "to_string_f"),
            Ty::Bool => ("option", "to_string_b"),
            // `${Option[List[Int]]}` → `some([1, 2, 3])` / `none` — the inner list renders like
            // `${list}`, wrapped in `some(…)`. A deeper element routes to the UNLINKED `_x`.
            Ty::Applied(TypeConstructorId::List, e) if e.len() == 1 && matches!(e[0], Ty::Int) => {
                ("option", "to_string_li")
            }
            Ty::Applied(TypeConstructorId::List, e) if e.len() == 1 && matches!(e[0], Ty::String) => {
                ("option", "to_string_ls")
            }
            Ty::Applied(TypeConstructorId::Option, e) if e.len() == 1 && matches!(e[0], Ty::Int) => {
                ("option", "to_string_oi")
            }
            Ty::Applied(TypeConstructorId::Option, e) if e.len() == 1 && matches!(e[0], Ty::Bool) => {
                ("option", "to_string_ob")
            }
            Ty::Applied(TypeConstructorId::Option, e) if e.len() == 1 && matches!(e[0], Ty::String) => {
                ("option", "to_string_os")
            }
            Ty::Applied(TypeConstructorId::List, e) if e.len() == 1 && matches!(e[0], Ty::Bool) => {
                ("option", "to_string_lb")
            }
            Ty::Applied(TypeConstructorId::List, e) if e.len() == 1 && matches!(e[0], Ty::Float) => {
                ("option", "to_string_lf")
            }
            Ty::Applied(TypeConstructorId::Map, e)
                if e.len() == 2 && matches!(e[0], Ty::String) && matches!(e[1], Ty::Int) =>
            {
                ("option", "to_string_msi")
            }
            Ty::Applied(TypeConstructorId::Result, e)
                if e.len() == 2 && matches!(e[0], Ty::Int) && matches!(e[1], Ty::String) =>
            {
                ("option", "to_string_ri")
            }
            Ty::Applied(TypeConstructorId::Result, e)
                if e.len() == 2 && matches!(e[0], Ty::String) && matches!(e[1], Ty::String) =>
            {
                ("option", "to_string_rs")
            }
            Ty::Applied(TypeConstructorId::Option, e)
                if e.len() == 1
                    && matches!(&e[0], Ty::Applied(TypeConstructorId::Option, e2)
                        if e2.len() == 1 && matches!(e2[0], Ty::Int)) =>
            {
                ("option", "to_string_ooi")
            }
            Ty::Applied(TypeConstructorId::Option, e)
                if e.len() == 1
                    && matches!(&e[0], Ty::Applied(TypeConstructorId::List, e2)
                        if e2.len() == 1 && matches!(e2[0], Ty::Int)) =>
            {
                ("option", "to_string_ooli")
            }
            Ty::Applied(TypeConstructorId::Result, e)
                if e.len() == 2
                    && matches!(&e[0], Ty::Applied(TypeConstructorId::List, e2)
                        if e2.len() == 1 && matches!(e2[0], Ty::Int))
                    && matches!(e[1], Ty::String) =>
            {
                ("option", "to_string_rli")
            }
            _ => ("option", "to_string_x"),
        },
        // `${Result[T, E]}` renders v0's `ok(<T>)` / `err(<E>)`, routed per (T, E) pair (err is almost
        // always String). Self-hosted: (Int, String) and (String, String). Any other pairing routes to
        // the UNLINKED `result.to_string_x` so the function walls cleanly (never a wrong byte).
        Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => {
            match (&args[0], &args[1]) {
                (Ty::Int, Ty::String) => ("result", "to_string"),
                (Ty::String, Ty::String) => ("result", "to_string_ss"),
                // `${Result[List[Int], String]}` → `ok([1, 2, 3])` / `err("<quoted>")`.
                (Ty::Applied(TypeConstructorId::List, e), Ty::String)
                    if e.len() == 1 && matches!(e[0], Ty::Int) =>
                {
                    ("result", "to_string_li")
                }
                (Ty::Applied(TypeConstructorId::List, e), Ty::String)
                    if e.len() == 1 && matches!(e[0], Ty::String) =>
                {
                    ("result", "to_string_ls")
                }
                (Ty::Bool, Ty::String) => ("result", "to_string_b"),
                (Ty::Applied(TypeConstructorId::Option, e), Ty::String)
                    if e.len() == 1 && matches!(e[0], Ty::Int) =>
                {
                    ("result", "to_string_oi")
                }
                (Ty::Applied(TypeConstructorId::Option, e), Ty::String)
                    if e.len() == 1 && matches!(e[0], Ty::String) =>
                {
                    ("result", "to_string_os")
                }
                (Ty::Applied(TypeConstructorId::Result, e), Ty::String)
                    if e.len() == 2 && matches!(e[0], Ty::Int) && matches!(e[1], Ty::String) =>
                {
                    ("result", "to_string_ri")
                }
                (Ty::Applied(TypeConstructorId::List, e), Ty::String)
                    if e.len() == 1 && matches!(e[0], Ty::Bool) =>
                {
                    ("result", "to_string_lb")
                }
                (Ty::Float, Ty::String) => ("result", "to_string_f"),
                (Ty::Applied(TypeConstructorId::List, e), Ty::String)
                    if e.len() == 1 && matches!(e[0], Ty::Float) =>
                {
                    ("result", "to_string_lf")
                }
                (Ty::Applied(TypeConstructorId::Option, e), Ty::String)
                    if e.len() == 1
                        && matches!(&e[0], Ty::Applied(TypeConstructorId::List, e2)
                            if e2.len() == 1 && matches!(e2[0], Ty::String)) =>
                {
                    ("result", "to_string_osl")
                }
                (Ty::Applied(TypeConstructorId::Map, e), Ty::String)
                    if e.len() == 2 && matches!(e[0], Ty::String) && matches!(e[1], Ty::Int) =>
                {
                    ("result", "to_string_msi")
                }
                (Ty::Applied(TypeConstructorId::Option, e), Ty::String)
                    if e.len() == 1
                        && matches!(&e[0], Ty::Applied(TypeConstructorId::List, e2)
                            if e2.len() == 1 && matches!(e2[0], Ty::Int)) =>
                {
                    ("result", "to_string_oli")
                }
                _ => ("result", "to_string_x"),
            }
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

/// Does a record/tuple/list/scalar VALUE of type `ty` materialize with REAL slots the recursive
/// Display can read — the STATIC (IR-type-only) predicate the gate and the lowering BOTH consult so
/// they agree on expand-vs-wrap BY CONSTRUCTION (no runtime-`materialized_aggregates` divergence).
/// Matches exactly what the construction path materializes:
///   - Int/Bool/Float/String          → yes (scalar / single heap leaf)
///   - List[scalar]                    → yes (scalar-element block); List[heap] → NO (not materialized)
///   - a registered record/tuple whose every field is itself `field_displayable` → yes (the
///     nested-aggregate construction admits a SCALAR-ONLY nested block; a heap-IN-nested field would
///     leak under the single-level mask, so it is NO)
///   - Map/Set/Option/Result/variant/unresolved → NO
fn field_displayable(ty: &Ty, registry: &RecordLayouts) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    match ty {
        Ty::Int | Ty::Bool | Ty::Float | Ty::String => true,
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => !is_heap_ty(&a[0]),
        Ty::Record { .. } | Ty::Tuple(_) | Ty::Named(..) => match resolve_aggregate(ty, registry) {
            // A NESTED aggregate must be SCALAR-ONLY (the construction's `lower_owned_heap_field`
            // admits only a scalar-only nested block — a heap-in-nested field would leak).
            Some((_, _, fields)) => fields.iter().all(|(_, t)| !is_heap_ty(t)),
            None => false,
        },
        _ => false,
    }
}

/// Is a record/tuple interpolation PART statically EXPAND-foldable — i.e. the lowering will
/// materialize it and read its real slots? True iff the part expr is a `Var` (a materialized
/// aggregate binding; a literal/call result is not a tracked block) AND every field of the
/// (resolvable) aggregate is `field_displayable`. The gate and the lowering both gate on THIS, so
/// the synthetic-call count the gate credits equals the calls the lowering emits — for both the
/// EXPAND path (recursive tree) and the WALL path (one `compound.to_string`).
pub(crate) fn aggregate_part_expandable(expr: &IrExpr, registry: &RecordLayouts) -> bool {
    if !matches!(expr.kind, IrExprKind::Var { .. }) {
        return false; // a literal `${P{..}}` / a call `${f()}` is not a tracked materialized block
    }
    match resolve_aggregate(&expr.ty, registry) {
        Some((_, _, fields)) => fields.iter().all(|(_, t)| field_displayable(t, registry)),
        None => false,
    }
}

/// Build the String-producing LEAF for ONE interpolation part, by type:
///   - a literal text part → a `LitStr` (no call),
///   - a String-typed part → the expr itself (identity, no call),
///   - an EXPAND-foldable RECORD/TUPLE part (a materialized Var with displayable fields) → the
///     recursive layout-driven Display ([`display_aggregate`]), an INLINE `ConcatStr` tree of
///     per-field formatters; a NON-expandable record/tuple part → ONE unlinked `compound.to_string`
///     wrapper (the function walls at render — never a wrong byte),
///   - any other part with a pure `module.to_string` → `module.to_string(expr)`.
/// Returns `None` for a part whose type has no admitted Display at all (an unresolved type) — the
/// caller then declines the whole desugar.
fn interp_part_leaf(p: &IrStringPart, registry: &RecordLayouts) -> Option<IrExpr> {
    match p {
        IrStringPart::Lit { value } => Some(lit_str(value)),
        IrStringPart::Expr { expr } if matches!(expr.ty, Ty::String) => Some(expr.clone()),
        // A record/tuple part: EXPAND if the lowering will materialize it; else wrap in the
        // unlinked `compound.to_string` so the function walls (the SAME decision the gate makes).
        IrStringPart::Expr { expr }
            if matches!(expr.ty, Ty::Record { .. } | Ty::Tuple(_) | Ty::Named(..))
                && resolve_aggregate(&expr.ty, registry).is_some() =>
        {
            // An ANONYMOUS record ALWAYS takes the generated sorted-field repr: v0 sorts
            // anon fields by name, while the inline display_aggregate expansion reads the
            // STRUCTURAL (source) order — expanding it would emit wrong bytes.
            if let Ty::Record { fields } = &expr.ty {
                // An ANONYMOUS record part — route to the generated
                // `__repr_anonrec_<hash>` (sorted-field render); an unemitted shape
                // (a nested payload) leaves the call unlinked = the honest wall.
                Some(IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Named {
                            name: sym(&format!(
                                "__repr_{}",
                                crate::lower::anon_record_drop_name(fields)
                            )),
                        },
                        args: vec![expr.clone()],
                        type_args: Vec::new(),
                    },
                    ty: Ty::String,
                    span: None,
                    def_id: None,
                })
            } else if aggregate_part_expandable(expr, registry) {
                display_aggregate(expr, &expr.ty, registry)
            } else if let Ty::Named(name, _) = &expr.ty {
                // A NAMED record outside the inline-expand subset (a recursive record, a
                // List[record] field — the compound_repr class): route to the GENERATED
                // `__repr_rec_<R>` (render-pipeline-injected). A record the generator does
                // not emit leaves the call unlinked — the same honest render wall the
                // `compound.to_string` wrapper gives, with the SAME call-count (one node).
                Some(IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Named {
                            name: sym(&format!(
                                "__repr_rec_{}",
                                crate::lower::drop_fn_ident(name.as_str())
                            )),
                        },
                        args: vec![expr.clone()],
                        type_args: Vec::new(),
                    },
                    ty: Ty::String,
                    span: None,
                    def_id: None,
                })
            } else {
                Some(to_string_call("compound", "to_string", expr.clone()))
            }
        }
        // A custom-VARIANT part (`"${Overflow(\"x\")}"` / a bound variant var): route
        // to the GENERATED `__repr_<V>` (render-pipeline-injected; the classify gate
        // counts the same call node). A variant the generator does not emit (a field
        // outside Int/Bool/String/nested-variant) leaves an unlinked call — the same
        // honest render wall the `compound.to_string` wrapper gives records.
        IrStringPart::Expr { expr }
            if matches!(&expr.ty, Ty::Named(..))
                && resolve_aggregate(&expr.ty, registry).is_none() =>
        {
            let Ty::Named(name, _) = &expr.ty else { unreachable!() };
            Some(IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Named {
                        name: sym(&format!(
                            "__repr_{}",
                            crate::lower::drop_fn_ident(name.as_str())
                        )),
                    },
                    args: vec![expr.clone()],
                    type_args: Vec::new(),
                },
                ty: Ty::String,
                span: None,
                def_id: None,
            })
        }
        IrStringPart::Expr { expr } => {
            let (module, func) = interp_to_string_call(&expr.ty)?;
            Some(to_string_call(module, func, expr.clone()))
        }
    }
}

/// Desugar a STRING INTERPOLATION `"…${e}…"` into a left-nested `ConcatStr` fold,
/// seeded by an empty `""` literal: `(((("" ++ p0) ++ p1) … ) ++ p_{K-1})`. Each
/// part is wrapped in its type's `to_string` ([`interp_part_leaf`]) — a Lit/String
/// part is a no-call leaf, every other part a single `module.to_string` call.
/// Concatenating with the leading `""` is byte-identical to v0's `emit_string_interp`
/// (`"" ++ bytes == bytes`), so the folded String matches v0 in EVERY position.
///
/// This is the SINGLE source the lowering ([`LowerCtx::try_lower_string_interp`])
/// AND the corpus caps gate (`count_ir_calls` in classify_corpus) BOTH consult: the
/// gate counts the call NODES of the very tree the lowering emits, so the synthetic
/// MIR `Op::CallFn`s are 1:1 backed by IR call nodes — `mir_calls == ir_calls` for an
/// in-profile interp BY CONSTRUCTION (no `mir > ir` over-count, no spurious caps
/// taint). Soundness rests on one invariant: when this returns `Some(tree)`, every
/// leaf lowers to exactly one `CallFn` (a pure `module.to_string`, admitted by
/// `purity::is_pure`) or a no-call passthrough — so `try_lower_concat_str` never
/// rolls back. Returns `None` (the interp stays the deferred Opaque, credited 0 by
/// the gate) iff a part has no admitted `to_string` module — a memory-safe defer.
///
/// THE WALL DOES THE HEAVY LIFTING: a part whose `to_string` is UNLINKED (Float /
/// compound — registered in `PURE_MODULES` but not in the self-host runtime) still
/// desugars to a real `CallFn`, so the enclosing function emits an unlinked call and
/// the render wall (`try_render_wasm_program`) REJECTS it as `Unsupported`. Such a
/// function is OUT of profile, so it can never contribute a `count != lower`
/// mismatch — the only IN-profile interps are the fully-linkable ones (Lit/String/
/// Int/Bool), where `count == lower` is trivially exact.
pub fn desugar_string_interp(parts: &[IrStringPart], registry: &RecordLayouts) -> Option<IrExpr> {
    let mut acc = lit_str("");
    for p in parts {
        let leaf = interp_part_leaf(p, registry)?;
        acc = IrExpr {
            kind: IrExprKind::BinOp {
                op: almide_ir::BinOp::ConcatStr,
                left: Box::new(acc),
                right: Box::new(leaf),
            },
            ty: Ty::String,
            span: None,
            def_id: None,
        };
    }
    Some(acc)
}

/// The SYNTHETIC call names the recursive Display ([`display_value`]) introduces for a
/// single value of type `ty` — the `<module>.to_string`-family wrappers, recursively. A
/// scalar/string/float/list value contributes ONE name; a record/tuple value contributes
/// none itself but recurses via [`aggregate_synthetic_names`] into its fields. This DOES
/// NOT count the value's OWN inner calls (it counts the WRAPPERS the desugar adds, not the
/// operand) — keeping the `count_ir_calls` operand-descent free of double counting.
fn value_synthetic_names(ty: &Ty, registry: &RecordLayouts, out: &mut Vec<String>) {
    match ty {
        // A nested record/tuple expands INLINE (recursive `__str_concat` + field formatters).
        Ty::Record { .. } | Ty::Tuple(_) | Ty::Named(..) if resolve_aggregate(ty, registry).is_some() => {
            aggregate_synthetic_names(ty, registry, out);
        }
        // Every OTHER value type routes to exactly ONE `to_string`-family call — the SAME single
        // wrapper [`display_value`] / [`interp_part_leaf`] emit (Int → int.to_string, Float →
        // float.to_string_compound, String → string.quote, List → list.to_string*, Map/Set/Option/
        // Result → the unlinked `<module>.to_string` that walls). Keyed off `display_leaf_call` so
        // the gate's count is BY CONSTRUCTION the lowering's emitted call set.
        _ => {
            if let Some((m, f)) = display_leaf_call(ty) {
                out.push(format!("{m}.{f}"));
            }
        }
    }
}

/// The SYNTHETIC call names the recursive Display ([`display_aggregate`]) introduces for an
/// aggregate of type `ty`: one `__str_concat` per `ConcatStr` fold the expansion builds
/// (= the number of `concat_all` parts at this level) plus the field formatters recursively.
/// MIRRORS `display_aggregate`'s structure EXACTLY so the gate credits precisely the
/// synthetic CallFns the lowering emits (count == lower for the aggregate, by construction).
fn aggregate_synthetic_names(ty: &Ty, registry: &RecordLayouts, out: &mut Vec<String>) {
    // A non-resolvable aggregate (structural record, unregistered) yields no Display tree —
    // the part declines and the whole interp credits 0 (matched by `interp_synthetic_call_names`).
    let Some((type_name, is_tuple, fields)) = resolve_aggregate(ty, registry) else {
        return;
    };
    if !is_tuple && type_name.is_none() {
        return; // structural record has no Display → walls, credits 0
    }
    // `concat_all` parts at this level: opening + (per field: a leading ", " for idx>0,
    // a "field: " label for a record, the field formatter) + closing.
    //   record: 1 (open) + Σ_i [ (i>0 → 1) + 1 (label) + 1 (formatter) ] + 1 (close)
    //   tuple:  1 (open) + Σ_i [ (i>0 → 1) +            1 (formatter) ] + 1 (close)
    let mut concat_parts = 2; // open + close
    for (idx, _) in fields.iter().enumerate() {
        if idx > 0 {
            concat_parts += 1; // ", "
        }
        if !is_tuple {
            concat_parts += 1; // "field: "
        }
        concat_parts += 1; // the field formatter expression
    }
    for _ in 0..concat_parts {
        out.push("__str_concat".to_string());
    }
    for (_, fty) in &fields {
        value_synthetic_names(fty, registry, out);
    }
}

/// Count the synthetic `CallFn`s [`desugar_string_interp`] yields for `parts` — the
/// `ConcatStr` and `module.to_string`-family call NODES of the desugared tree. The corpus
/// gate adds exactly this to its IR call count for each interp (it counts the same tree),
/// so the MIR calls the lowering emits are 1:1 backed. `None` (a part with no admitted
/// Display) ⇒ 0 (the interp stays Opaque, lowering emits no synthetic call).
pub fn interp_str_synthetic_call_count(parts: &[IrStringPart], registry: &RecordLayouts) -> usize {
    interp_synthetic_call_names(parts, registry).len()
}

/// The SYNTHETIC call names [`desugar_string_interp`] introduces for `parts`: one
/// `__str_concat` per TOP-LEVEL fold step (= `parts.len()`: K parts over the `""` seed ⇒ K
/// concats) and, per non-passthrough part, the Display wrappers it adds — a scalar part one
/// `<module>.to_string`, a RECORD/TUPLE part the full recursive `__str_concat` + field-
/// formatter set ([`aggregate_synthetic_names`]). It DOES NOT include the operands' OWN
/// inner calls (a `${g(x)}` callee) — those live in the original part exprs and are reached
/// separately by `count_ir_calls`'s descent, so no double count. Empty (a `None` desugar —
/// a part with no admitted Display) ⇒ the interp stays Opaque, crediting none.
pub fn interp_synthetic_call_names(parts: &[IrStringPart], registry: &RecordLayouts) -> Vec<String> {
    // A part with no admitted Display ⇒ the whole interp is non-desugarable (the lowering
    // returns `None` and defers to Opaque), so it credits zero synthetic calls.
    if desugar_string_interp(parts, registry).is_none() {
        return Vec::new();
    }
    let mut names = Vec::with_capacity(parts.len() * 2);
    // The TOP-LEVEL fold: K parts over the `""` seed ⇒ K `__str_concat` (the interp's own
    // outer concatenation — a record/tuple part is ONE top-level part here, its INNER
    // `__str_concat`s are added by `value_synthetic_names` below).
    for _ in 0..parts.len() {
        names.push("__str_concat".to_string());
    }
    for p in parts {
        if let IrStringPart::Expr { expr } = p {
            if matches!(expr.ty, Ty::String) {
                continue; // a String part is a no-call passthrough
            }
            // A TOP-LEVEL record/tuple part mirrors `interp_part_leaf`'s decision tree
            // EXACTLY (the mir == ir contract): an ANON record is ALWAYS one generated
            // `__repr_anonrec_<hash>` call; an expand-foldable named/tuple part credits the
            // full recursive tree; a non-expandable NAMED record one `__repr_rec_<R>`; any
            // other non-expandable aggregate one `compound.to_string` (the wall).
            if matches!(expr.ty, Ty::Record { .. } | Ty::Tuple(_) | Ty::Named(..))
                && resolve_aggregate(&expr.ty, registry).is_some()
            {
                if let Ty::Record { fields } = &expr.ty {
                    names.push(format!(
                        "__repr_{}",
                        crate::lower::anon_record_drop_name(fields)
                    ));
                } else if aggregate_part_expandable(expr, registry) {
                    aggregate_synthetic_names(&expr.ty, registry, &mut names);
                } else if let Ty::Named(name, _) = &expr.ty {
                    names.push(format!(
                        "__repr_rec_{}",
                        crate::lower::drop_fn_ident(name.as_str())
                    ));
                } else {
                    names.push("compound.to_string".to_string());
                }
            } else {
                value_synthetic_names(&expr.ty, registry, &mut names);
            }
        }
    }
    names
}

/// Is a WHOLE interpolation DESUGARABLE (every part has an admitted Display)? When true, the
/// lowering folds it to a `ConcatStr` chain; when false, it stays the deferred Opaque.
/// (Desugarable does NOT imply LINKABLE — a Float part desugars but float.to_string is
/// unlinked, so the function walls at render. Use the registry to split proven-vs-walled;
/// this predicate only answers "does the lowering fold it".)
pub fn interp_str_desugarable(parts: &[IrStringPart], registry: &RecordLayouts) -> bool {
    desugar_string_interp(parts, registry).is_some()
}

/// Does `module.func` return a real MATERIALIZED `Result[Int, String]` (the DynListStr len-as-tag
/// layout)? Its result may be tracked in `materialized_results` so an `Ok`/`Err` `match` over it
/// EXECUTES. NARROW to fns actually self-hosted — any other Result is a deferred `Opaque` (len 0,
/// would misread as `Ok`). `int.parse` is the canonical for string.to_int/to_integer/parse_int.
/// The CallFn name for a stdlib `module.func` call, routing the REPR-POLYMORPHIC list combinators
/// to their `_str` variant when the RESULT is a `List[heap]` (e.g. `list.map` over a `List[String]`
/// → `list.map_str`, a DynListStr-result impl). The element repr (i64 vs i32 handle) demands a
/// separate variant; the variant reads/writes via the heap-aware prim ops. Scalar-result lists keep
/// the plain name. `module.func` is unchanged for everything else.
pub(crate) fn list_heap_call_name(module: &str, func: &str, arg_tys: &[Ty], result_ty: &Ty) -> String {
    use almide_lang::types::constructor::TypeConstructorId;
    // `random.choice` / `random.shuffle` key on the LIST ELEMENT: a scalar element uses the
    // flat self-host (i64 slots, flat drops), a String element the rc-aware `_str` variant
    // (random_choice.almd / random_shuffle.almd). Any other element class routes to the
    // UNLINKED `random.<func>_x` → a clean render wall (never a wrong-typed link).
    if module == "random" && matches!(func, "choice" | "shuffle") {
        if let Some(Ty::Applied(TypeConstructorId::List, a)) = arg_tys.first() {
            if a.len() == 1 && !is_heap_ty(&a[0]) {
                return format!("random.{func}");
            }
            if a.len() == 1 && matches!(a[0], Ty::String) {
                return format!("random.{func}_str");
            }
        }
        return format!("random.{func}_x");
    }
    // `fan.map` selects a monomorphic self-host by (input element A, output element B) — `fan.map<sfx>`
    // where A/B in {Int (""), String ("s")}. The input A is `arg_tys[0] = List[A]`; the output B is
    // `result_ty = Result[List[B], String]`. An unsupported pairing routes to the UNLINKED
    // `fan.map_x` → a clean render wall (never a wrong-typed link = never invalid wasm).
    if module == "fan" && func == "map" {
        fn elem_of(t: Option<&Ty>) -> Option<&Ty> {
            match t {
                Some(Ty::Applied(TypeConstructorId::List, e)) if e.len() == 1 => Some(&e[0]),
                _ => None,
            }
        }
        let a = elem_of(arg_tys.first());
        let b = match result_ty {
            Ty::Applied(TypeConstructorId::Result, r) if r.len() == 2 => elem_of(Some(&r[0])),
            _ => None,
        };
        let sfx = match (a, b) {
            (Some(Ty::Int), Some(Ty::Int)) => "",
            (Some(Ty::Int), Some(Ty::String)) => "_is",
            (Some(Ty::String), Some(Ty::String)) => "_ss",
            (Some(Ty::String), Some(Ty::Int)) => "_si",
            _ => "_x",
        };
        return format!("fan.map{sfx}");
    }
    // `fold` threads an ACCUMULATOR (= the result type). A HEAP accumulator (e.g. a String built up
    // across the fold) needs the closure-result + accumulator to be an i32 handle, not the i64 the
    // scalar-accumulator fold variants hardcode — emitting an i32 there is invalid wasm. No heap-
    // accumulator fold variant is self-hosted yet, so route it to an UNREGISTERED name: render walls
    // it cleanly (a controlled reject) rather than emitting a repr-mismatched module. (Soundness-
    // preserving: a wall is never a miscompile.)
    if func == "fold" && matches!(module, "list" | "map" | "set") && is_heap_ty(result_ty) {
        return format!("{module}.fold_hacc");
    }
    // `list.enumerate` keys on its SOURCE element: scalar → the flat-pair self-host;
    // String → the rc-share pair variant (`DropListIntStr` at the call site frees each
    // pair's key ref); any other heap element routes to an UNREGISTERED name (walls
    // cleanly — a flat pair drop would leak a rich element's children).
    if module == "list" && func == "enumerate" {
        if let Some(Ty::Applied(TypeConstructorId::List, a)) = arg_tys.first() {
            if a.len() == 1 {
                if !is_heap_ty(&a[0]) {
                    return "list.enumerate".to_string();
                }
                if matches!(a[0], Ty::String) {
                    return "list.enumerate_str".to_string();
                }
                return "list.enumerate_h".to_string();
            }
        }
    }
    // `list.zip` keys on BOTH sources: scalar/scalar → flat pairs; FLAT-heap/FLAT-heap
    // (String or List[scalar] each side — matrix rows) → the rc-share variant (the
    // call-site `DropListStrStr` releases both acquired refs); anything else walls.
    if module == "list" && func == "zip" && arg_tys.len() == 2 {
        let elem = |t: &Ty| match t {
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => Some(a[0].clone()),
            _ => None,
        };
        if let (Some(ea), Some(eb)) = (elem(&arg_tys[0]), elem(&arg_tys[1])) {
            let flat_heap = |t: &Ty| matches!(t, Ty::String)
                || matches!(t, Ty::Applied(TypeConstructorId::List, b)
                    if b.len() == 1 && !is_heap_ty(&b[0]));
            if !is_heap_ty(&ea) && !is_heap_ty(&eb) {
                return "list.zip".to_string();
            }
            if flat_heap(&ea) && flat_heap(&eb) {
                return "list.zip_rc".to_string();
            }
            return "list.zip_h".to_string();
        }
    }
    // `map.entries` / `map.from_list` over the skv repr (`Map[String, scalar]` — the
    // tokenizer vocab): route to the skv self-hosts; the all-String repr keeps its
    // existing `map.entries_str`. Other reprs wall (unregistered).
    if module == "map" && func == "entries" {
        if let Some(Ty::Applied(TypeConstructorId::Map, a)) = arg_tys.first() {
            if a.len() == 2 && matches!(a[0], Ty::String) && !is_heap_ty(&a[1]) {
                return "map.entries_skv".to_string();
            }
        }
    }
    if module == "map" && func == "from_list" {
        if let Some(Ty::Applied(TypeConstructorId::List, a)) = arg_tys.first() {
            if a.len() == 1
                && matches!(&a[0], Ty::Tuple(ts) if ts.len() == 2
                    && matches!(ts[0], Ty::String) && !is_heap_ty(&ts[1]))
            {
                return "map.from_list_skv".to_string();
            }
        }
    }
    // `list.repeat` over a HEAP element (`list.repeat(h, n_rep)` where `h: Matrix` — the
    // nn repeat_kv GQA duplication): each result slot must CO-OWN the element (rc_inc per
    // copy) — the scalar impl's raw alias would make every slot an uncounted owner the
    // recursive result drop double-frees.
    if module == "list" && func == "repeat" {
        if let Some(t) = arg_tys.first() {
            if is_heap_ty(t) {
                return "list.repeat_rc".to_string();
            }
        }
    }
    // `list.flatten` over HEAP-element sublists: the copied slots are handles the
    // result must CO-OWN — route to the rc_inc-on-copy variant (the scalar variant's
    // raw copy would make the result a second uncounted owner = a double free).
    if module == "list" && func == "flatten" {
        if let Ty::Applied(TypeConstructorId::List, inner) = result_ty {
            if inner.len() == 1 && is_heap_ty(&inner[0]) {
                return "list.flatten_rc".to_string();
            }
        }
    }
    // `option.unwrap_or(o, d)` over an `Option[String]` (the pipe/UFCS form
    // `list.get(xs, i) |> option.unwrap_or("")`, NOT the `??` operator that
    // `try_lower_option_unwrap_or` desugars) must route to `option.unwrap_or_str`: the
    // generic `option.unwrap_or` takes its default as an i64 SCALAR (`Option[Int]`), so a
    // String fallback (an i32 handle) and a String result repr-mismatch it — invalid wasm
    // (`expected i64, found i32` in the call + the i64 result). `option.unwrap_or_str`
    // (param i32 i32) (result i32) is the rc-correct String variant (deep-copies the kept
    // payload so result + source can both drop). Keyed on the Option payload being String.
    // The same repr-poly routing for `result.unwrap_or` (the pipe/direct form —
    // `result.unwrap_or(json.parse(s), json.null())`, json_gltf_walk): the generic
    // impl takes an i64 scalar default; a heap Ok/default needs the rc-correct
    // self-hosts registered for the `??` desugar.
    if module == "result" && func == "unwrap_or" {
        if let Some(Ty::Applied(TypeConstructorId::Result, a)) = arg_tys.first() {
            if a.len() == 2 && is_value_ty(&a[0]) {
                return "result.value_unwrap_or".to_string();
            }
            if a.len() == 2
                && matches!(&a[0], Ty::Applied(TypeConstructorId::List, e)
                    if e.len() == 1 && is_value_ty(&e[0]))
            {
                return "result.list_value_unwrap_or".to_string();
            }
            if a.len() == 2 && matches!(a[0], Ty::String) {
                return "result.str_unwrap_or".to_string();
            }
        }
    }
    if module == "option" && func == "unwrap_or" {
        if let Some(Ty::Applied(TypeConstructorId::Option, a)) = arg_tys.first() {
            if a.len() == 1 && matches!(a[0], Ty::String) {
                return "option.unwrap_or_str".to_string();
            }
            // The remaining heap payloads route to their rc-correct self-hosts
            // (already registered for the `??` desugar): Value / List[Value] /
            // List[String]. The generic unwrap_or takes an i64 scalar default,
            // so a handle default (`json.get_array(v,k) |> option.unwrap_or([])`,
            // json_gltf_walk's count_floats) repr-mismatched — invalid wasm.
            if a.len() == 1 && is_value_ty(&a[0]) {
                return "option.value_unwrap_or".to_string();
            }
            if a.len() == 1
                && matches!(&a[0], Ty::Applied(TypeConstructorId::List, e)
                    if e.len() == 1 && is_value_ty(&e[0]))
            {
                return "option.listvalue_unwrap_or".to_string();
            }
            if a.len() == 1
                && matches!(&a[0], Ty::Applied(TypeConstructorId::List, e)
                    if e.len() == 1 && matches!(e[0], Ty::String))
            {
                return "option.liststr_unwrap_or".to_string();
            }
            // A FLAT scalar-element list payload (`map.get(groups, "0") ?? []` —
            // Option[List[Int]], the group_by class): the rc-correct flat variant.
            if a.len() == 1
                && matches!(&a[0], Ty::Applied(TypeConstructorId::List, e)
                    if e.len() == 1 && !is_heap_ty(&e[0]))
            {
                return "option.listint_unwrap_or".to_string();
            }
        }
    }
    if module == "list" {
        // List[Float] ordering uses IEEE-754 totalOrder (f64::total_cmp), NOT a signed-int slot
        // compare. Float is SCALAR (is_heap_ty false), so the heap routes below never fire for it —
        // route sort/min/max explicitly on the element being Ty::Float (C-055). sort_by keys on the
        // CLOSURE (arg 1) RETURN type being Float — the element list may be any type (e.g. List[R]).
        if matches!(func, "sort" | "min" | "max") {
            if let Some(Ty::Applied(TypeConstructorId::List, a)) = arg_tys.first() {
                if a.len() == 1 && a[0] == Ty::Float {
                    return format!("list.{func}_float");
                }
                // list.sort/min/max over a List[String] compare by CONTENT, not the i64 handle the
                // generic impls compare (→ arbitrary/handle order or wrong element, a silent bug).
                // _str variants do a lexicographic byte compare.
                if a.len() == 1 && matches!(a[0], Ty::String) {
                    return format!("list.{func}_str");
                }
            }
        }
        if func == "sort_by" {
            if let Some(Ty::Fn { ret, .. }) = arg_tys.get(1) {
                if **ret == Ty::Float {
                    // A HEAP element (List[R] of records) must be CO-OWNED by the
                    // result list (rc_inc per copied handle) — the raw-copy variant
                    // shares without acquiring and the two recursive drops double-free.
                    let heap_elem = matches!(arg_tys.first(),
                        Some(Ty::Applied(TypeConstructorId::List, a)) if a.len() == 1 && is_heap_ty(&a[0]));
                    return if heap_elem {
                        "list.sort_by_float_rc".to_string()
                    } else {
                        "list.sort_by_float".to_string()
                    };
                }
                // A STRING-key sort_by has NO registered typed variant: the generic
                // impl compares scalar slots (a String key = handle order, and a
                // heap-elem list traps the funcref signature — the ll1 indirect-call
                // mismatch). Route to the unlinkable `_x` name so the caller WALLS
                // honestly instead of trapping or handle-sorting.
                if **ret == Ty::String {
                    return "list.sort_by_str_key_x".to_string();
                }
            }
        }
        // `list.map` is the one combinator whose SOURCE and RESULT element reprs may DIFFER (the
        // closure transforms the type). A heap RESULT over a SCALAR source (`float.to_string` over a
        // List[Float], `int.to_string` over a List[Int]) must read the source slot as a raw i64
        // scalar (load64), not as a String handle (load_str) — that is `map_s2h`; a heap result over
        // a heap source is the all-String `map_str`.
        if func == "map" {
            if let Ty::Applied(TypeConstructorId::List, rargs) = result_ty {
                if rargs.len() == 1 && is_heap_ty(&rargs[0]) {
                    let src_heap = matches!(
                        arg_tys.first(),
                        Some(Ty::Applied(TypeConstructorId::List, s)) if s.len() == 1 && is_heap_ty(&s[0])
                    );
                    return if src_heap {
                        "list.map_str".to_string()
                    } else {
                        "list.map_s2h".to_string()
                    };
                }
            }
        }
        // `list.enumerate` over a List[String] → `list.enumerate_str` (result List[(Int, String)]).
        // Keyed on the SOURCE arg being List[String] (the yaml `lines |> list.enumerate` shape).
        if func == "enumerate" {
            if let Some(Ty::Applied(TypeConstructorId::List, s)) = arg_tys.first() {
                if s.len() == 1 && matches!(s[0], Ty::String) {
                    return "list.enumerate_str".to_string();
                }
            }
        }
        // chunk/windows/window (build a NESTED List[List[heap]] whose recursive drop is a separate
        // gap) and the HIGHER-ORDER take_while/drop_while/reduce over a HEAP element (String/Value):
        // the generic i64 self-host impls copy element handles WITHOUT rc_inc → a DOUBLE-FREE at
        // scope end (the result and source both free the shared handle), and the HO closure ABI is
        // i64-scalar so a String/Value i32 handle mismatches the indirect call. Both are memory-
        // safety bugs the prim-region ownership cert cannot see (it treats prim rc as a no-op), so
        // they slipped past corpus-wall and trapped only at runtime. Route to an UNREGISTERED name →
        // render walls cleanly (a controlled reject, never a miscompile or double-free) until the
        // rc-correct heap variants land. Scalar element lists (Int/Float/Bool) are unaffected.
        // take_while/drop_while over a List[String] now have rc-correct _str variants (each kept
        // String DEEP-COPIED so result + source can both drop without a double-free); route to them
        // BEFORE the heap-element wall below. (List[Value] / the other combinators still wall.)
        // `reduce` over a List[String] joins them too: `list.reduce_str` is the self-host tail-`if`
        // (None on empty, else Some) whose loop is the proven closure-call heap-accumulator (#738 Path
        // C). Routed to the rc-correct _str variant BEFORE the heap-element wall.
        if matches!(func, "take_while" | "drop_while" | "chunk" | "windows" | "window" | "reduce") {
            if let Some(Ty::Applied(TypeConstructorId::List, s)) = arg_tys.first() {
                if s.len() == 1 && matches!(s[0], Ty::String) {
                    return format!("list.{func}_str");
                }
            }
        }
        if matches!(func, "chunk" | "windows" | "window" | "take_while" | "drop_while" | "reduce") {
            if let Some(Ty::Applied(TypeConstructorId::List, s)) = arg_tys.first() {
                if s.len() == 1 && is_heap_ty(&s[0]) {
                    return format!("list.{func}_heapelem");
                }
            }
        }
        // `list.set` over a List[String] → `list.set_str` (the val is a String HANDLE, not an i64
        // Int — the generic list.set's i64 val param mismatches; set_str rc-copies + co-owns). The
        // yaml `list.set(lines, dp, …)` shape.
        // The heap-element list MODIFIERS (set/insert/remove_at/swap/update) over a List[String]/
        // List[Value]: the generic i64 impls copy slots WITHOUT rc_inc (→ double-free when
        // result+source both drop) and i32/i64-mismatch on a typed element (set/insert's `x`; for
        // update the CLOSURE — a (String/Value)->same lambda renders an i32 handle RESULT while the
        // generic f: (Int)->Int call_indirect expects i64 → runtime type-mismatch trap, #736). The
        // _str/_value variants rc-copy each element to co-own and recursively free any replaced
        // element. The element type is the first arg's List parameter.
        if matches!(func, "set" | "insert" | "remove_at" | "swap" | "update") {
            if let Some(Ty::Applied(TypeConstructorId::List, s)) = arg_tys.first() {
                if s.len() == 1 && matches!(s[0], Ty::String) {
                    return format!("list.{func}_str");
                }
                if s.len() == 1 && is_value_ty(&s[0]) {
                    return format!("list.{func}_value");
                }
            }
        }
        // The element-PRESERVING List[heap]-returning combinators (source elem == result elem).
        if matches!(func, "filter" | "reverse" | "take" | "drop" | "unique" | "dedup" | "intersperse") {
            if let Ty::Applied(TypeConstructorId::List, args) = result_ty {
                // A List[List[String]] result element is itself a heap list — the `_str` deep-copy
                // (string.repeat) would read its length word as a byte count. take/drop SHARE the inner
                // lists by handle via the `_liststr` variant; the other combinators are a later brick.
                if args.len() == 1
                    && matches!(func, "take" | "drop")
                    && matches!(&args[0], Ty::Applied(TypeConstructorId::List, e)
                        if e.len() == 1 && matches!(e[0], Ty::String))
                {
                    return format!("list.{func}_liststr");
                }
                if args.len() == 1 && matches!(args[0], Ty::String) {
                    return format!("list.{func}_str");
                }
                // A NON-String heap element (a custom variant / record / Value handle):
                // the `_str` deep-copy would read the block's length word as a byte
                // count (garbage handles — the closures_and_variants UAF). `filter`
                // routes to the rc-sharing `_rc` variant; the other combinators have
                // no handle-sharing self-host yet, so route them to an UNREGISTERED
                // `_hshare` name — the render walls it cleanly (the fold_hacc
                // precedent), never a miscompile.
                if args.len() == 1 && is_heap_ty(&args[0]) {
                    if func == "filter" {
                        return "list.filter_rc".to_string();
                    }
                    return format!("list.{func}_hshare");
                }
            }
        }
        // Element-RETURNING accessors / search over a List[heap] (the result is an Option[heap]):
        // get/first/last (positional) + find (predicate higher-order).
        if matches!(func, "get" | "first" | "last" | "find") {
            if let Ty::Applied(TypeConstructorId::Option, args) = result_ty {
                // A List[Value] element is a dynamic Value, NOT a String — the `_str` variant DEEP-
                // COPIES via `string.repeat` (corrupting an Object to {}). Route get/first/last to the
                // Value accessor, which SHARES the element (rc_inc, like value.get's Ok). (find's
                // closure-keyed Value form is a later brick — only the positional accessors here.)
                if args.len() == 1 && is_value_ty(&args[0]) && matches!(func, "get" | "first" | "last")
                {
                    return format!("list.{func}_value");
                }
                // `find` over a `List[(Int, String)]` (the `enumerate |> find(closure)` shape): the
                // element is a tuple, NOT a String — route to `find_int_str`, which loads each element
                // as a TUPLE HANDLE and hands it to the predicate closure (reading e.g. `e.1`). The
                // `_str` fallback would load it as a String handle (garbage).
                if func == "find"
                    && args.len() == 1
                    && matches!(&args[0],
                        Ty::Tuple(tys) if tys.len() == 2 && matches!(tys[0], Ty::Int) && matches!(tys[1], Ty::String))
                {
                    return "list.find_int_str".to_string();
                }
                // A `List[List[String]]` element is itself a heap list, NOT a String — the `_str`
                // variant would DEEP-COPY it via `string.repeat`, reading the inner list's length word
                // as a byte count (garbage). Route to the handle-SHARE `_liststr` accessor (the
                // `List[String]` analogue of `_value`); the inner list is co-owned, dropped DropListStr.
                if args.len() == 1
                    && matches!(func, "get" | "first" | "last")
                    && matches!(&args[0], Ty::Applied(TypeConstructorId::List, e)
                        if e.len() == 1 && matches!(e[0], Ty::String))
                {
                    return format!("list.{func}_liststr");
                }
                // A RECORD/aggregate element (`list.get(vars, idx)` over `List[EnvVar]`
                // — porta dedup_env): SHARE the element handle exactly like the
                // Value/liststr accessors (the record stays owned by the list; the
                // `_str` deep copy would read its block as string bytes). The `_value`
                // impl is layout-identical (load_handle + Some), so reuse it. Keyed by
                // elimination (this is a free fn, no layout registry): a heap element
                // that is NOT a String/List/Value is a nominal record/tuple/variant
                // block — all of which are single-handle share-safe.
                if args.len() == 1
                    && matches!(func, "get" | "first" | "last")
                    && is_heap_ty(&args[0])
                    && !matches!(args[0], Ty::String)
                    && !is_value_ty(&args[0])
                    && !matches!(&args[0], Ty::Applied(TypeConstructorId::List | TypeConstructorId::Map | TypeConstructorId::Set, _))
                {
                    return format!("list.{func}_value");
                }
                // `find`'s Some() result is a DEEP COPY of the found element (`string.repeat`,
                // list_find_str's `__lfs_some`) — correct only for an actual String element; any
                // other heap type here (a flat scalar tuple, a record) falls through to a WALL
                // rather than corrupting the copy (the same class of bug fixed above for
                // contains/index_of — `find` just hasn't grown a `_hshare` copy variant yet).
                if args.len() == 1 && matches!(args[0], Ty::String) {
                    return format!("list.{func}_str");
                }
                // Any remaining heap-element shape (find over Value/List/Map/Set; get/first/last
                // over a non-String heap List element) has no covering arm above — route to a
                // deliberately UNREGISTERED `_x` name. The bare `list.{func}` name is NOT a safe
                // fallback here: it links against the Int-typed generic self-host (list_search.almd
                // et al), which silently "succeeds" via raw i64-slot / pointer-identity comparison
                // instead of refusing to link (the fallthrough danger confirmed by probe elsewhere
                // in this dispatch — see the contains/index_of comment above).
                if args.len() == 1 && is_heap_ty(&args[0]) {
                    return format!("list.{func}_x");
                }
            }
        }
        // get_or returns the ELEMENT directly (not an Option). Over a List[heap] it must return
        // an i32 handle (a deep copy), so it is keyed on the heap RESULT being the element type.
        if func == "get_or" && is_heap_ty(result_ty) {
            // A Value element SHARES (the _str deep copy corrupts a Value block —
            // json_gltf_walk's `list.get_or(meshes, 0, json.null())` returned "?").
            if is_value_ty(result_ty) {
                return "list.get_or_value".to_string();
            }
            if matches!(result_ty, Ty::String) {
                return "list.get_or_str".to_string();
            }
            // No handle-sharing self-host for other heap elements yet — an
            // UNREGISTERED name walls cleanly (the fold_hacc precedent).
            return "list.get_or_hshare".to_string();
        }
        // SUBJECT-keyed (arg 0) over a List[heap], where the result is scalar (Bool/Int/Option[Int])
        // so it can't be keyed on the result type: search (contains/index_of) does an ELEMENT-
        // EQUALITY comparison — String routes to the byte-eq `_str` family (correct only for actual
        // String elements: __str_eq reads the length FIELD as a byte count); a flat scalar-slot
        // block (all-scalar tuple / List[scalar]) routes to the slot-wise `_hshare` family (B32's
        // `__uh_eq`, which reads length as an ELEMENT count and compares raw i64 slots — exact for
        // this shape). Any OTHER heap element (record, Value, nested heap list, String-bearing
        // tuple) has no correct comparison variant yet — falls through to a WALL (never routed to
        // `_str`, which would silently produce WRONG results: a tuple/list `len` misread as a byte
        // count truncates the compare to its first ~2 bytes, a confirmed false-positive collision).
        // NOTE: falling through to the bare `list.contains`/`list.index_of` name here is NOT a
        // safe wall — it links against the Int-typed generic (list_search.almd), which silently
        // "succeeds" (raw i64-slot compare, i.e. POINTER-IDENTITY on a heap handle — the exact
        // OLD C-015 bug) rather than refusing to link (CONFIRMED by probe: a record-element
        // `list.contains` produced invalid wasm via this path, and a tuple-element `set.from_list`
        // sharing the same generic-fallthrough shape silently mis-deduped). Any excluded heap
        // element must route to a deliberately UNREGISTERED name (`_x`, the established
        // wall-suffix convention) so the render step's unlinked-call check catches it.
        if matches!(func, "contains" | "index_of") {
            if let Some(Ty::Applied(TypeConstructorId::List, a)) = arg_tys.first() {
                if a.len() == 1 && is_heap_ty(&a[0]) {
                    if matches!(a[0], Ty::String) {
                        return format!("list.{func}_str");
                    }
                    if is_flat_scalar_block_ty(&a[0]) {
                        return format!("list.{func}_hshare");
                    }
                    return format!("list.{func}_x");
                }
            }
        }
        // all/any/count/fold are pure closure-passthrough (each element handle is loaded and handed
        // to the predicate/accumulator closure — no internal equality compare or deep copy), so the
        // byte-eq `_str` family's ITERATION shape is safe for ANY heap element type, not just String.
        if matches!(func, "all" | "any" | "count" | "fold") {
            if let Some(Ty::Applied(TypeConstructorId::List, a)) = arg_tys.first() {
                if a.len() == 1 && is_heap_ty(&a[0]) {
                    return format!("list.{func}_str");
                }
            }
        }
    }
    if module == "set" {
        // `Set[heap]`-RETURNING constructors key on the RESULT element type; `set.to_list` over a
        // `Set[heap]` returns a `List[heap]`; the predicate `set.contains` keys on its SUBJECT
        // (arg 0) element type (its result is Bool). Every one of these funcs relies on `__str_eq`
        // (byte-level String-layout equality, for membership/dedup) AND/OR `string.repeat` (a
        // String-specific deep copy) internally (set_str.almd) — BOTH are unsound for a non-String
        // heap element: `__str_eq` misreads a block's slot-count `len` as a byte count (a confirmed
        // false-positive collision past the first ~2 bytes), and `string.repeat` would corrupt a
        // tuple/record/list block's bytes. Restrict the `_str` route to an ACTUAL String element;
        // any other heap element (tuple, record, nested list, Value) falls through to a WALL — no
        // correct Set variant exists yet for those (the flat-scalar `_hshare` family list.contains
        // just grew does not cover Set's dedup-on-build/algebra ops).
        let result_elem_is_string = matches!(
            result_ty,
            Ty::Applied(TypeConstructorId::Set | TypeConstructorId::List, a)
                if a.len() == 1 && matches!(a[0], Ty::String)
        );
        // RESULT-keyed: constructors / Set-returning algebra over heap elements. The bare
        // `set.{func}` fallback is NOT a safe wall for a non-String heap element here — it links
        // against the Int-typed generic (set_core.almd), which silently "succeeds" via raw i64-slot
        // / pointer-identity comparison (the OLD C-015 bug) instead of refusing to link (CONFIRMED
        // by probe: `set.from_list` over a `List[(Int,Int)]` silently mis-deduped, len 3 instead of
        // 2, then trapped later). Route explicitly to the UNREGISTERED `_x` wall name instead.
        let result_elem_is_heap = matches!(
            result_ty,
            Ty::Applied(TypeConstructorId::Set | TypeConstructorId::List, a)
                if a.len() == 1 && is_heap_ty(&a[0])
        );
        if matches!(
            func,
            "from_list" | "to_list" | "union" | "intersection" | "difference"
                | "new" | "insert" | "remove" | "symmetric_difference" | "filter"
        ) && result_elem_is_heap
        {
            return if result_elem_is_string {
                format!("set.{func}_str")
            } else {
                format!("set.{func}_x")
            };
        }
        // `all`/`any`/`fold` are pure closure-passthrough (no internal eq compare or deep copy) —
        // safe for any heap element type, same as the list-module analogue above.
        let arg0_elem_is_heap = matches!(
            arg_tys.first(),
            Some(Ty::Applied(TypeConstructorId::Set, a)) if a.len() == 1 && is_heap_ty(&a[0])
        );
        if matches!(func, "all" | "any" | "fold") && arg0_elem_is_heap {
            return format!("set.{func}_str");
        }
        // ARG-keyed eq/membership: a Bool-returning fn over a `Set[heap]` subject (arg 0) — String
        // element only (the `__str_eq`/`__set_has_str` unsoundness above); any other heap element
        // routes to the `_x` wall (the same fallthrough danger as the RESULT-keyed family).
        let arg0_elem_is_string = matches!(
            arg_tys.first(),
            Some(Ty::Applied(TypeConstructorId::Set, a)) if a.len() == 1 && matches!(a[0], Ty::String)
        );
        if matches!(func, "contains" | "is_subset" | "is_disjoint" | "eq") && arg0_elem_is_heap {
            return if arg0_elem_is_string {
                format!("set.{func}_str")
            } else {
                format!("set.{func}_x")
            };
        }
    }
    if module == "map" {
        // A map's REPR is set by its (key, value) heap-ness, read from whichever Map type the call
        // exposes: arg 0 (the SUBJECT of set/get/fold/filter/…) takes priority, else the RESULT
        // (map.new() has no args). The two repr families:
        //   key heap, value heap  → `_str` (map_str: interleaved all-String entries)
        //   key heap, value scalar → `_skv` (map_skv: String keys + i64 values, serves
        //                            Map[String,Int] AND Map[String,Float] — the value is one i64)
        //   key scalar             → the plain map_core (Map[Int,Int]); a scalar-key heap-value map
        //                            has no variant yet, so it falls through (walled by repr).
        // The element-returning forms (get → Option[V], keys/values → List[elem]) read the same
        // key/value reprs off the subject map (arg 0).
        let map_kv = |ty: &Ty| match ty {
            Ty::Applied(TypeConstructorId::Map, a) if a.len() == 2 => {
                Some((is_heap_ty(&a[0]), is_heap_ty(&a[1])))
            }
            _ => None,
        };
        let kv = arg_tys
            .first()
            .and_then(&map_kv)
            .or_else(|| map_kv(result_ty));
        if let Some((key_heap, val_heap)) = kv {
            // Each repr family routes ONLY the funcs its self-hosted variant file actually defines
            // (an unlisted func keeps the plain name — never a dangling `_str`/`_skv` reference).
            // Whether the VALUE type is exactly String — the `_str` variant stores
            // interleaved all-String entries, so a heap-but-not-String value
            // (`Map[String, List[Int]]`) must NOT route there (its deep-copy would
            // read the list block as string bytes). It routes to an UNREGISTERED
            // `_hval` name instead — a clean render wall, never invalid wasm.
            let val_is_string = matches!(
                arg_tys.first().or(Some(result_ty)),
                Some(Ty::Applied(TypeConstructorId::Map, a)) if a.len() == 2 && matches!(a[1], Ty::String)
            );
            // Both the `_str` (all-String interleaved entries) and `_skv` (String key + i64 value)
            // families do KEY EQUALITY via `__str_eq`/`__skv_eq` — a byte-level compare that
            // misreads a non-String heap block's slot-count `len` as a byte count (the same
            // confirmed false-positive-collision class fixed above for list/set). A non-String heap
            // KEY (a tuple, record, nested list) must NOT route to either family — CONFIRMED via
            // probe to currently produce INVALID WASM (an i32/i64 ABI-width mismatch on `map.set`
            // with a tuple key), not silently wrong bytes, but still not the honest wall this repr
            // gate is meant to guarantee. Gate both families on an ACTUAL String key.
            let key_is_string = matches!(
                arg_tys.first().or(Some(result_ty)),
                Some(Ty::Applied(TypeConstructorId::Map, a)) if a.len() == 2 && matches!(a[0], Ty::String)
            );
            // The FLAT-heap-value class map_hval actually implements (List[scalar]).
            let val_is_flat_list = matches!(
                arg_tys.first().or(Some(result_ty)),
                Some(Ty::Applied(TypeConstructorId::Map, a))
                    if a.len() == 2 && matches!(&a[1],
                        Ty::Applied(TypeConstructorId::List, e) if e.len() == 1 && !is_heap_ty(&e[0]))
            );
            let variant = match (key_heap, val_heap) {
                // `Map[String, List[scalar]]` — the implemented subset of the heap-value
                // family (new/set/eq; other funcs keep the unregistered wall name).
                (true, true) if val_is_flat_list && matches!(func, "new" | "set" | "eq") => {
                    Some("_hval")
                }
                // `Map[String, List[Int]]` from_list / display (the map-of-lists literal):
                // keyed on the RESULT/first-arg map; to_string_hval passes through
                // verbatim (the B22 suffix guard).
                (true, true)
                    if !val_is_string
                        && func == "from_list"
                        && matches!(result_ty, Ty::Applied(TypeConstructorId::Map, a)
                            if a.len() == 2
                                && matches!(a[0], Ty::String)
                                && matches!(&a[1], Ty::Applied(TypeConstructorId::List, b)
                                    if b.len() == 1 && matches!(b[0], Ty::Int))) =>
                {
                    Some("_hval")
                }
                (true, true) if func == "to_string_hval" => Some(""),
                (true, true) if !val_is_string => Some("_hval_wall"),
                (true, true) if key_is_string => matches!(
                    func,
                    "new" | "set" | "remove" | "merge" | "update" | "filter" | "get" | "keys"
                        | "values" | "len" | "is_empty" | "contains" | "all" | "any" | "count" | "fold"
                        | "entries"
                )
                .then_some("_str"),
                (true, false) if key_is_string => matches!(
                    func,
                    "new" | "set" | "remove" | "filter" | "get" | "get_or" | "keys" | "values"
                        | "len" | "is_empty" | "contains" | "all" | "any" | "count" | "fold" | "eq"
                        | "find"
                )
                .then_some("_skv"),
                // A non-String heap KEY (tuple/record/nested list) reaching here has no correct
                // variant — route to an explicit UNREGISTERED wall name rather than falling through
                // to the bare `map.{func}` name, which links against the scalar-key map_core generic
                // and produces INVALID WASM (an i32/i64 ABI-width mismatch, CONFIRMED by probe) —
                // a crash, not the honest compile-time wall this repr gate exists to guarantee.
                (true, true) | (true, false) => Some("_key_wall"),
                // `Map[Int, String]` — the implemented scalar-key/heap-value variant
                // (new/set/eq). Other funcs, and other heap value types, keep an
                // UNREGISTERED wall name (never the plain Map[Int,Int] i64-slot link
                // that emitted invalid wasm — map_set_eq's original failure).
                // An ALREADY-SUFFIXED synthesized display call (`map.to_string_ivh` from
                // the interp leaf table) — pass through verbatim (re-suffixing would
                // fabricate `to_string_ivh_ivh_wall`).
                (false, true) if func == "to_string_ivh" => Some(""),
                (false, true)
                    if {
                        // `from_list`'s FIRST arg is the pairs List, not the Map — key
                        // its admission on the RESULT type instead (either probe works
                        // for the Map-first fns).
                        let is_ivh = |t: &Ty| {
                            matches!(t, Ty::Applied(TypeConstructorId::Map, a)
                                if a.len() == 2
                                    && matches!(a[0], Ty::Int)
                                    && matches!(a[1], Ty::String))
                        };
                        (arg_tys.first().is_some_and(is_ivh) || is_ivh(result_ty))
                            && matches!(func, "new" | "set" | "eq" | "from_list")
                    } =>
                {
                    Some("_ivh")
                }
                (false, true) => Some("_ivh_wall"),
                // `Map[Int, Float]` from_list (the float_map literal): ONE scalar-KV impl
                // (map_if.almd) — the paired i64 slots carry f64 bits verbatim. Display
                // routes separately (the interp table's to_string_if). Other scalar-scalar
                // maps keep the plain (unlinked) name — walls cleanly.
                (false, false)
                    if func == "from_list"
                        && matches!(result_ty, Ty::Applied(TypeConstructorId::Map, a)
                            if a.len() == 2
                                && matches!(a[0], Ty::Int)
                                && matches!(a[1], Ty::Float)) =>
                {
                    Some("_if")
                }
                (false, false) if func == "to_string_if" => Some(""),
                _ => None,
            };
            if let Some(suffix) = variant {
                return format!("map.{func}{suffix}");
            }
        }
    }
    format!("{module}.{func}")
}

