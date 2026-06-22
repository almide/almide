/// A named record/variant type is repr-backed unless it (transitively, through a
/// container) holds a closure field — a closure has no Almide-literal form, so it
/// never reaches compound interpolation. Mirrors the native `type_has_repr_impl`
/// closure gate so the two targets agree on exactly which types get a repr.
fn ty_field_has_closure(ty: &almide_lang::types::Ty) -> bool {
    matches!(ty, almide_lang::types::Ty::Fn { .. })
        || ty.children().into_iter().any(ty_field_has_closure)
}

/// Collect the names of named types referenced anywhere inside `ty` (directly or
/// nested in a container / tuple / record), into `out`.
fn collect_named_refs(ty: &almide_lang::types::Ty, out: &mut HashSet<String>) {
    if let almide_lang::types::Ty::Named(n, _) = ty {
        out.insert(n.to_string());
    }
    for child in ty.children() {
        collect_named_refs(child, out);
    }
}

/// The set of named record/variant types that lie on a reference CYCLE — i.e. a
/// type that can reach itself through its fields/cases (self-recursion like
/// `Tree = … Node(Tree, Tree)`, or mutual recursion `A → B → A`). Only these need
/// a per-type repr function: a NON-recursive type stays on the inline walk, where
/// the concrete `Ty::Named(_, type_args)` at the interpolation site resolves its
/// fields' types (so a generic non-recursive type like `Box[Int]` reprs its `T`
/// payload correctly). A recursive type's inline walk would instead expand its
/// type graph forever at compile time, so it routes through its repr fn where the
/// self-reference is a runtime CALL.
fn recursive_type_names(emitter: &WasmEmitter) -> HashSet<String> {
    // Edge set: type name → directly-referenced named types (through fields).
    let mut edges: HashMap<String, HashSet<String>> = HashMap::new();
    for (name, cases) in &emitter.variant_info {
        let mut refs = HashSet::new();
        for c in cases {
            for (_, ft) in &c.fields { collect_named_refs(ft, &mut refs); }
        }
        edges.insert(name.clone(), refs);
    }
    let case_names: HashSet<String> = emitter.variant_info.values()
        .flat_map(|cases| cases.iter().map(|c| c.name.clone()))
        .collect();
    for (name, fields) in &emitter.record_fields {
        if name.starts_with("__anon_record_") || case_names.contains(name)
            || emitter.variant_info.contains_key(name) { continue; }
        let mut refs = HashSet::new();
        for (_, ft) in fields { collect_named_refs(ft, &mut refs); }
        edges.insert(name.clone(), refs);
    }

    // A type is recursive iff it can reach itself. Reachability via DFS over the
    // edge set (only edges to nodes that are themselves typed are followed).
    let mut recursive = HashSet::new();
    for start in edges.keys() {
        let mut stack: Vec<String> = edges[start].iter().cloned().collect();
        let mut seen: HashSet<String> = HashSet::new();
        while let Some(n) = stack.pop() {
            if &n == start { recursive.insert(start.clone()); break; }
            if !seen.insert(n.clone()) { continue; }
            if let Some(next) = edges.get(&n) {
                for m in next { stack.push(m.clone()); }
            }
        }
    }
    recursive
}

/// Pre-register one `__repr_<TypeName>(ptr: i32) -> i32` per repr-backed NAMED
/// record/variant type that lies on a reference cycle. Recursion (self / mutual)
/// becomes a CALL into the callee's repr fn, so a finite runtime value terminates
/// exactly like the native trait dispatch — inline expansion of a recursive type
/// graph would loop forever at compile time. NON-recursive types are walked
/// inline (the concrete type args at the interpolation site resolve their fields).
///
/// Reserve indices in SORTED name order so they are a pure function of the
/// program (the same 32-bit/64-bit host-determinism contract as
/// `register_variant_eq_funcs`); the (also sorted) compile order must match.
fn register_repr_funcs(emitter: &mut WasmEmitter, program: &IrProgram) {
    let type_idx = emitter.register_type(vec![ValType::I32], vec![ValType::I32]);

    let recursive = recursive_type_names(emitter);

    // Repr-backed RECURSIVE named types: every recursive variant type, plus every
    // recursive named record. A record case-name is also stored in `record_fields`
    // (for field access), so restrict records to declared record types — not the
    // synthetic `__anon_record_*` shapes (walked inline) and not the variant CASE
    // names (a `Node` value reprs through its variant type's fn, by tag). A type
    // with a closure field is excluded.
    let mut base_names: HashSet<String> = HashSet::new();
    for (name, cases) in &emitter.variant_info {
        let has_closure = cases.iter().any(|c| c.fields.iter().any(|(_, ft)| ty_field_has_closure(ft)));
        if recursive.contains(name) && !has_closure {
            base_names.insert(name.clone());
        }
    }
    // Variant CASE names that are also keyed in record_fields — skip them as
    // standalone record reprs (they are reached via the owning variant type).
    let case_names: HashSet<String> = emitter.variant_info.values()
        .flat_map(|cases| cases.iter().map(|c| c.name.clone()))
        .collect();
    for (name, fields) in &emitter.record_fields {
        let is_anon = name.starts_with("__anon_record_");
        let is_variant_case = case_names.contains(name);
        let is_variant_type = emitter.variant_info.contains_key(name);
        let has_closure = fields.iter().any(|(_, ft)| ty_field_has_closure(ft));
        if recursive.contains(name) && !is_anon && !is_variant_case && !is_variant_type && !has_closure {
            base_names.insert(name.clone());
        }
    }

    // Discover the concrete INSTANTIATIONS of these recursive types used in the
    // program (`Tree[Int]`, `Tree[String]`, `Tree[List[Int]]`). Each needs its
    // own repr fn keyed by the mangled name — a monomorphic by-bare-name fn
    // reads the `T` payload as a raw `TypeVar`. The recursive references inside
    // a fn body resolve to the SAME instantiation (`Node`'s children are
    // `Tree[T]` → `Tree[Int]` for the `Tree[Int]` fn), so the site-level
    // instantiations are the full set — no new ones appear transitively.
    // A non-generic recursive type (`type IntTree = Leaf(Int) | ...`) yields the
    // bare-name key (empty args → mangle is just the name), preserving behavior.
    let mut instantiations: BTreeMap<String, Ty> = BTreeMap::new();
    // Always register the bare-name fn for every recursive type: a recursive type
    // with NO interpolation site still needs its reserved slot iff something else
    // (e.g. a nested non-generic recursive field) routes to it, and the
    // non-generic case is reached via the bare name at dispatch.
    for name in &base_names {
        let bare = Ty::Named(almide_base::intern::sym(name), Vec::new());
        instantiations.insert(name.clone(), bare);
    }
    collect_repr_instantiations(program, &base_names, &mut instantiations);

    // Reserve indices in SORTED key order (host-determinism contract); the
    // compile order in `compile_repr_funcs` must match.
    for (mangled, ty) in instantiations {
        let func_idx = emitter.register_func(&format!("__repr_{}", mangled), type_idx);
        emitter.repr_funcs.insert(mangled.clone(), func_idx);
        emitter.repr_func_tys.insert(mangled, ty);
    }
}

/// Mangle a concrete type into the suffix used to key per-instantiation repr
/// fns (`Tree[Int]` → `Tree_Int`, `Tree[List[Int]]` → `Tree_List_Int`). Mirrors
/// the Rust-walker `mangle_ty_for_mono` convention so the two targets name
/// instantiations identically. A type with no args mangles to its bare name.
pub(super) fn mangle_repr_ty(ty: &Ty) -> String {
    use almide_lang::types::constructor::TypeConstructorId;
    match ty {
        Ty::Int => "Int".into(),
        Ty::Float => "Float".into(),
        Ty::String => "String".into(),
        Ty::Bool => "Bool".into(),
        Ty::Int8 => "Int8".into(),
        Ty::Int16 => "Int16".into(),
        Ty::Int32 => "Int32".into(),
        Ty::UInt8 => "UInt8".into(),
        Ty::UInt16 => "UInt16".into(),
        Ty::UInt32 => "UInt32".into(),
        Ty::UInt64 => "UInt64".into(),
        Ty::Float32 => "Float32".into(),
        Ty::Bytes => "Bytes".into(),
        Ty::Unit => "Unit".into(),
        Ty::Named(name, args) => {
            if args.is_empty() { name.to_string() }
            else { format!("{}_{}", name, args.iter().map(mangle_repr_ty).collect::<Vec<_>>().join("_")) }
        }
        Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 =>
            format!("List_{}", mangle_repr_ty(&args[0])),
        Ty::Applied(id, args) => {
            let name = format!("{:?}", id);
            if args.is_empty() { name } else {
                format!("{}_{}", name, args.iter().map(mangle_repr_ty).collect::<Vec<_>>().join("_"))
            }
        }
        _ => "Unknown".into(),
    }
}

/// Walk every expression type in the program, recording each concrete
/// instantiation `Ty::Named(base, non-empty-args)` of a recursive repr-backed
/// type (`base` ∈ `base_names`), keyed by its mangled name. Nested instantiations
/// (`List[Tree[Int]]` → `Tree[Int]`) are found by scanning each type's subtree.
fn collect_repr_instantiations(
    program: &IrProgram,
    base_names: &HashSet<String>,
    out: &mut BTreeMap<String, Ty>,
) {
    use almide_ir::visit::{IrVisitor, walk_expr};
    struct Collector<'a> {
        base_names: &'a HashSet<String>,
        out: &'a mut BTreeMap<String, Ty>,
    }
    impl<'a> Collector<'a> {
        fn scan_ty(&mut self, ty: &Ty) {
            // A generic recursive type used concretely: record the instantiation.
            if let Ty::Named(name, args) = ty {
                if !args.is_empty()
                    && self.base_names.contains(name.as_str())
                    && !args.iter().any(ty_is_unresolved_repr)
                {
                    out_insert(self.out, ty);
                }
            }
            // Descend into every child type (List/Tuple/Map/Option/Result/Named
            // args, Record fields) so nested instantiations surface too.
            for child in ty.children() {
                self.scan_ty(child);
            }
        }
    }
    fn out_insert(out: &mut BTreeMap<String, Ty>, ty: &Ty) {
        out.insert(mangle_repr_ty(ty), ty.clone());
    }
    impl<'a> IrVisitor for Collector<'a> {
        fn visit_expr(&mut self, expr: &almide_ir::IrExpr) {
            self.scan_ty(&expr.ty);
            walk_expr(self, expr);
        }
    }
    let mut c = Collector { base_names, out };
    for func in &program.functions {
        c.visit_expr(&func.body);
    }
    for module in &program.modules {
        for func in &module.functions {
            c.visit_expr(&func.body);
        }
    }
}

/// A type still carrying an unresolved `TypeVar`/`Unknown` is not a real
/// instantiation — skip it (the corresponding bare-name fn handles the
/// degenerate case, and we must not mangle a `TypeVar` into a fn name).
fn ty_is_unresolved_repr(ty: &Ty) -> bool {
    match ty {
        Ty::TypeVar(_) | Ty::Unknown => true,
        _ => ty.children().iter().any(|c| ty_is_unresolved_repr(c)),
    }
}

/// Compile each `__repr_<TypeName>` body: load the value pointer (param 0) and
/// run the SAME structural walk the inline path uses (`emit_repr_record` /
/// `emit_repr_variant`). Nested named-type fields recurse as a CALL because
/// `emit_repr_value` routes a repr-backed `Ty::Named` through its repr fn (see
/// `calls_string_repr.rs`). Body-emit order is sorted to match the (sorted)
/// index reservation in `register_repr_funcs`.
fn compile_repr_funcs(emitter: &mut WasmEmitter, var_table: &almide_ir::VarTable) {
    let mut entries: Vec<(String, u32)> = emitter.repr_funcs.iter()
        .map(|(n, &idx)| (n.clone(), idx))
        .collect();
    entries.sort();

    for (mangled, _func_idx) in &entries {
        let type_idx = emitter.register_type(vec![ValType::I32], vec![ValType::I32]);

        // One repr fn walks a SINGLE level (children recurse via call), so its
        // scratch demand is bounded by one type's field/case count — the generous
        // caps below mirror the eq-fn setup and never approach the inline-expansion
        // overflow that motivated these functions.
        let mut local_decls = Vec::new();
        let scratch_i32_cap = 32usize;
        let scratch_i64_cap = 8usize;
        let scratch_f64_cap = 2usize;
        let scratch_i32_base = 1u32; // after the single `ptr` param
        for _ in 0..scratch_i32_cap { local_decls.push((1, ValType::I32)); }
        let scratch_i64_base = scratch_i32_base + scratch_i32_cap as u32;
        for _ in 0..scratch_i64_cap { local_decls.push((1, ValType::I64)); }
        let scratch_f64_base = scratch_i64_base + scratch_i64_cap as u32;
        for _ in 0..scratch_f64_cap { local_decls.push((1, ValType::F64)); }

        let wasm_func = TrackedFunction::new(local_decls);
        let mut scratch_alloc = scratch::ScratchAllocator::new();
        scratch_alloc.set_bases_with_capacity(
            scratch_i32_base, scratch_i32_cap,
            scratch_i64_base, scratch_i64_cap,
            scratch_f64_base, scratch_f64_cap,
        );

        // The repr emitters dispatch on the static `Ty` of the value: a variant
        // type → `emit_repr_variant`, otherwise a record → `emit_repr_record`.
        // The instantiation `Ty` (e.g. `Tree[Int]`) carries the concrete type
        // args so the variant/record walk substitutes them into each payload —
        // this is the whole point of keying by instantiation. The dispatch base
        // name is the type's own name (`Tree`), not the mangled key.
        let ty = emitter.repr_func_tys.get(mangled).cloned()
            .unwrap_or_else(|| panic!(
                "[ICE] repr dispatch for `{}` has no registered instantiation — \
                 fabricating a Named from the MANGLED key silently printed an \
                 empty repr (#525)",
                mangled
            ));
        let base_name = match &ty {
            almide_lang::types::Ty::Named(n, _) => n.to_string(),
            _ => mangled.clone(),
        };
        let is_variant = emitter.variant_info.contains_key(base_name.as_str());

        let compiled_func = {
            let mut compiler = FuncCompiler {
                emitter: &mut *emitter,
                func: wasm_func,
                var_map: std::collections::HashMap::new(),
                depth: 0,
                loop_stack: Vec::new(),
                scratch: scratch_alloc,
                var_table,
                stub_ret_ty: almide_lang::types::Ty::Unit,
                current_module_name: None,
                live_heap: Vec::new(),
            };

            // Push the value pointer (param 0); the walk consumes it from the
            // stack and leaves the result string pointer (the return value).
            wasm!(compiler.func, { local_get(0); });
            if is_variant {
                compiler.emit_repr_variant(&ty);
            } else {
                let fields = compiler.extract_record_fields(&ty);
                compiler.emit_repr_record(Some(base_name.as_str()), &fields);
            }
            compiler.func.instruction(&wasm_encoder::Instruction::End);
            compiler.func
        };

        emitter.add_compiled(CompiledFunc::tracked(type_idx, compiled_func));
    }
}

use std::collections::HashSet;

/// Content-derived, host-deterministic name for an anonymous record shape.
///
/// The name is a pure function of the field shape — the same
/// `(field_name, Debug(ty))` key used for dedup below — so two structurally
/// identical records get one name and the name is invariant to the IR walk
/// order in which a record happens to be discovered. The previous
/// `__anon_record_{record_fields.len()}` counter coupled the name to walk
/// position, which is deterministic only as long as the entire upstream walk
/// order is; the Determinism Belt prefers names that are a function of content,
/// not provenance.
///
/// FNV-1a/64 is used deliberately instead of `std`'s `DefaultHasher`, whose
/// `RandomState` seed varies per process (it would reintroduce exactly the
/// non-determinism this is meant to remove). See
/// docs/roadmap/active/determinism-belt.md.
fn anon_record_name(key: &[(String, String)]) -> String {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut h = FNV_OFFSET;
    let mut mix = |bytes: &[u8]| {
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(FNV_PRIME);
        }
        // Field separator so `["ab","c"]` and `["a","bc"]` can't alias.
        h ^= 0xff;
        h = h.wrapping_mul(FNV_PRIME);
    };
    for (n, t) in key {
        mix(n.as_bytes());
        mix(t.as_bytes());
    }
    format!("__anon_record_{h:016x}")
}

/// Walk all IR expressions/statements and collect anonymous record shapes
/// (i.e. `Ty::Record { fields }`). Each unique field-set is registered in
/// `emitter.record_fields` under a content-derived name (see
/// [`anon_record_name`]) so the emit-phase Member access fallback (which
/// iterates record_fields looking for a match by field name) can find them
/// when a lambda param's own type was left as Unknown/TypeVar by inference.
fn register_anonymous_records(program: &IrProgram, emitter: &mut WasmEmitter) {
    use almide_ir::{IrExpr, IrExprKind, IrStmt, IrStmtKind};
    let mut seen: HashSet<Vec<(String, String)>> = HashSet::new();
    // Seed with already-registered records to avoid redundant anonymous entries.
    for fields in emitter.record_fields.values() {
        let key: Vec<(String, String)> = fields.iter().map(|(n, t)| (n.clone(), format!("{:?}", t))).collect();
        seen.insert(key);
    }

    fn walk_ty(
        ty: &Ty,
        seen: &mut HashSet<Vec<(String, String)>>,
        record_fields: &mut BTreeMap<String, Vec<(String, Ty)>>,
    ) {
        match ty {
            Ty::Record { fields } | Ty::OpenRecord { fields } => {
                let field_vec: Vec<(String, Ty)> = fields.iter()
                    .map(|(n, t)| (n.to_string(), t.clone()))
                    .collect();
                let key: Vec<(String, String)> = field_vec.iter()
                    .map(|(n, t)| (n.clone(), format!("{:?}", t)))
                    .collect();
                let name = anon_record_name(&key);
                if seen.insert(key) {
                    record_fields.insert(name, field_vec.clone());
                }
                for (_, fty) in fields.iter() { walk_ty(fty, seen, record_fields); }
            }
            Ty::Applied(_, args) => { for a in args { walk_ty(a, seen, record_fields); } }
            Ty::Tuple(elems) => { for e in elems { walk_ty(e, seen, record_fields); } }
            Ty::Fn { params, ret } => {
                for p in params { walk_ty(p, seen, record_fields); }
                walk_ty(ret, seen, record_fields);
            }
            _ => {}
        }
    }

    fn walk_expr(
        expr: &IrExpr,
        seen: &mut HashSet<Vec<(String, String)>>,
        record_fields: &mut BTreeMap<String, Vec<(String, Ty)>>,
    ) {
        walk_ty(&expr.ty, seen, record_fields);
        match &expr.kind {
            IrExprKind::Block { stmts, expr: tail } => {
                for s in stmts { walk_stmt(s, seen, record_fields); }
                if let Some(t) = tail { walk_expr(t, seen, record_fields); }
            }
            IrExprKind::Call { args, .. } => { for a in args { walk_expr(a, seen, record_fields); } }
            IrExprKind::If { cond, then, else_ } => {
                walk_expr(cond, seen, record_fields);
                walk_expr(then, seen, record_fields);
                walk_expr(else_, seen, record_fields);
            }
            IrExprKind::Match { subject, arms } => {
                walk_expr(subject, seen, record_fields);
                for arm in arms {
                    if let Some(g) = &arm.guard { walk_expr(g, seen, record_fields); }
                    walk_expr(&arm.body, seen, record_fields);
                }
            }
            IrExprKind::Record { fields, .. } => {
                // Build field-type list from the literal's field expressions.
                let field_vec: Vec<(String, Ty)> = fields.iter()
                    .map(|(n, e)| (n.to_string(), e.ty.clone()))
                    .collect();
                let key: Vec<(String, String)> = field_vec.iter()
                    .map(|(n, t)| (n.clone(), format!("{:?}", t)))
                    .collect();
                let name = anon_record_name(&key);
                if field_vec.iter().all(|(_, t)| !t.is_unresolved()) && seen.insert(key) {
                    record_fields.insert(name, field_vec);
                }
                for (_, e) in fields.iter() { walk_expr(e, seen, record_fields); }
            }
            IrExprKind::SpreadRecord { base, fields } => {
                walk_expr(base, seen, record_fields);
                for (_, e) in fields.iter() { walk_expr(e, seen, record_fields); }
            }
            IrExprKind::List { elements } => { for e in elements { walk_expr(e, seen, record_fields); } }
            IrExprKind::Tuple { elements } => { for e in elements { walk_expr(e, seen, record_fields); } }
            IrExprKind::Lambda { body, .. } => { walk_expr(body, seen, record_fields); }
            IrExprKind::ClosureCreate { captures, .. } => {
                for (_, t) in captures { walk_ty(t, seen, record_fields); }
            }
            IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
            | IrExprKind::OptionSome { expr } => walk_expr(expr, seen, record_fields),
            IrExprKind::Member { object, .. } => { walk_expr(object, seen, record_fields); }
            IrExprKind::IndexAccess { object, index } => {
                walk_expr(object, seen, record_fields);
                walk_expr(index, seen, record_fields);
            }
            IrExprKind::BinOp { left, right, .. } => {
                walk_expr(left, seen, record_fields);
                walk_expr(right, seen, record_fields);
            }
            IrExprKind::UnOp { operand, .. } => walk_expr(operand, seen, record_fields),
            IrExprKind::Try { expr } | IrExprKind::Unwrap { expr } => walk_expr(expr, seen, record_fields),
            IrExprKind::ForIn { iterable, body, .. } => {
                walk_expr(iterable, seen, record_fields);
                for s in body { walk_stmt(s, seen, record_fields); }
            }
            IrExprKind::While { cond, body } => {
                walk_expr(cond, seen, record_fields);
                for s in body { walk_stmt(s, seen, record_fields); }
            }
            _ => {}
        }
    }

    fn walk_stmt(
        stmt: &IrStmt,
        seen: &mut HashSet<Vec<(String, String)>>,
        record_fields: &mut BTreeMap<String, Vec<(String, Ty)>>,
    ) {
        match &stmt.kind {
            IrStmtKind::Bind { value, ty, .. } => {
                walk_ty(ty, seen, record_fields);
                walk_expr(value, seen, record_fields);
            }
            IrStmtKind::BindDestructure { value, .. } => walk_expr(value, seen, record_fields),
            IrStmtKind::Assign { value, .. } => walk_expr(value, seen, record_fields),
            IrStmtKind::Expr { expr } => walk_expr(expr, seen, record_fields),
            _ => {}
        }
    }

    for func in &program.functions {
        walk_ty(&func.ret_ty, &mut seen, &mut emitter.record_fields);
        for p in &func.params { walk_ty(&p.ty, &mut seen, &mut emitter.record_fields); }
        walk_expr(&func.body, &mut seen, &mut emitter.record_fields);
    }
    for tl in &program.top_lets {
        walk_expr(&tl.value, &mut seen, &mut emitter.record_fields);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_program_produces_valid_wasm() {
        let program = IrProgram {
            functions: vec![],
            top_lets: vec![],
            type_decls: vec![],
            var_table: almide_ir::VarTable::new(),
            def_table: Default::default(),
            modules: vec![],
            type_registry: Default::default(),
            effect_fn_names: Default::default(),
            effect_map: Default::default(),
            codegen_annotations: Default::default(),
            used_stdlib_modules: Default::default(),
        };
        let bytes = emit(&program);
        assert_eq!(&bytes[0..4], b"\0asm");
        assert_eq!(&bytes[4..8], &[1, 0, 0, 0]);
    }
}

/// Scan IR program for filesystem module calls (fs.read_text, fs.write_text, etc.).
/// True iff the program references `string.to_upper` / `to_lower` / `capitalize`
/// in any form (resolved module call, unresolved method call, or runtime call).
/// Conservative by design: matching every dispatch form the emitter handles means
/// the pre-scan never misses a reachable case op (a miss would leave the always-
/// compiled case lookup functions baking stale offsets — silently wrong).
fn program_uses_case_op(program: &IrProgram) -> bool {
    use almide_ir::{IrExprKind, CallTarget};
    use almide_ir::visit::{IrVisitor, walk_expr, walk_stmt};

    fn is_case_fn(name: &str) -> bool {
        // Accept both the bare method name and a "string."-qualified one: the
        // Module arm sees a bare `func`, but the unresolved Method arm (and the
        // calls.rs UFCS fallback) can carry a dotted "string.to_upper". Missing a
        // form would leave the case tables un-embedded while the runtime fns stay
        // DCE-live — silently wrong, so keep this strictly broader than dispatch.
        let name = name.strip_prefix("string.").unwrap_or(name);
        matches!(name, "to_upper" | "to_lower" | "capitalize")
    }

    struct CaseScanner { found: bool }
    impl IrVisitor for CaseScanner {
        fn visit_expr(&mut self, expr: &almide_ir::IrExpr) {
            if self.found { return; }
            match &expr.kind {
                IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. }
                    if module.as_str() == "string" && is_case_fn(func.as_str()) =>
                {
                    self.found = true;
                    return;
                }
                IrExprKind::Call { target: CallTarget::Method { method, .. }, .. }
                    if is_case_fn(method.as_str()) =>
                {
                    self.found = true;
                    return;
                }
                IrExprKind::RuntimeCall { symbol, .. }
                    if matches!(
                        symbol.as_str(),
                        "almide_rt_string_to_upper"
                            | "almide_rt_string_to_lower"
                            | "almide_rt_string_capitalize"
                    ) =>
                {
                    self.found = true;
                    return;
                }
                _ => {}
            }
            walk_expr(self, expr);
        }
        fn visit_stmt(&mut self, stmt: &almide_ir::IrStmt) {
            if self.found { return; }
            walk_stmt(self, stmt);
        }
    }

    let mut scanner = CaseScanner { found: false };
    for func in &program.functions {
        scanner.visit_expr(&func.body);
        if scanner.found { return true; }
    }
    false
}

/// True iff the program references `math.sin` / `math.cos` / `math.tan` in any
/// dispatch form (resolved module call, unresolved method call, or runtime call).
/// Conservative — same contract as `program_uses_case_op`: a miss would leave the
/// always-compiled trig runtime baking stale table offsets (silently wrong), so
/// this is strictly broader than dispatch and also walks module bodies, not just
/// top-level functions.
fn program_uses_trig(program: &IrProgram) -> bool {
    use almide_ir::{IrExprKind, CallTarget};
    use almide_ir::visit::{IrVisitor, walk_expr, walk_stmt};

    fn is_trig_fn(name: &str) -> bool {
        let name = name.strip_prefix("math.").unwrap_or(name);
        matches!(name, "sin" | "cos" | "tan")
    }

    struct TrigScanner { found: bool }
    impl IrVisitor for TrigScanner {
        fn visit_expr(&mut self, expr: &almide_ir::IrExpr) {
            if self.found { return; }
            match &expr.kind {
                IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. }
                    if module.as_str() == "math" && is_trig_fn(func.as_str()) =>
                {
                    self.found = true;
                    return;
                }
                IrExprKind::Call { target: CallTarget::Method { method, .. }, .. }
                    if is_trig_fn(method.as_str()) =>
                {
                    self.found = true;
                    return;
                }
                IrExprKind::RuntimeCall { symbol, .. }
                    if matches!(
                        symbol.as_str(),
                        "almide_rt_math_sin" | "almide_rt_math_cos" | "almide_rt_math_tan"
                    ) =>
                {
                    self.found = true;
                    return;
                }
                _ => {}
            }
            walk_expr(self, expr);
        }
        fn visit_stmt(&mut self, stmt: &almide_ir::IrStmt) {
            if self.found { return; }
            walk_stmt(self, stmt);
        }
    }

    let mut scanner = TrigScanner { found: false };
    for func in &program.functions {
        scanner.visit_expr(&func.body);
        if scanner.found { return true; }
    }
    for module in &program.modules {
        for func in &module.functions {
            scanner.visit_expr(&func.body);
            if scanner.found { return true; }
        }
    }
    false
}

fn program_uses_fs(program: &IrProgram) -> bool {
    use almide_ir::{IrExprKind, CallTarget};
    use almide_ir::visit::{IrVisitor, walk_expr, walk_stmt};

    struct FsScanner { found: bool }
    impl IrVisitor for FsScanner {
        fn visit_expr(&mut self, expr: &almide_ir::IrExpr) {
            if self.found { return; }
            if let IrExprKind::Call { target: CallTarget::Module { module, .. }, .. } = &expr.kind {
                if module == "fs" { self.found = true; return; }
            }
            if let IrExprKind::RuntimeCall { symbol, .. } = &expr.kind {
                if symbol.starts_with("almide_rt_fs_") { self.found = true; return; }
            }
            walk_expr(self, expr);
        }
        fn visit_stmt(&mut self, stmt: &almide_ir::IrStmt) {
            if self.found { return; }
            walk_stmt(self, stmt);
        }
    }

    let mut scanner = FsScanner { found: false };
    for func in &program.functions {
        scanner.visit_expr(&func.body);
        if scanner.found { return true; }
    }
    false
}
