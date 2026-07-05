// ── Full program rendering ──

pub fn render_program(ctx: &RenderContext, program: &IrProgram) -> String {
    // Build constructor → enum name map
    // Build type alias map for transparent expansion
    let mut type_aliases = std::collections::HashMap::new();
    let mut generic_types = std::collections::HashSet::new();
    // Collect type aliases and generic types from ALL sources
    // (top-level + modules) so render_type can expand them everywhere.
    let all_type_decls = program.type_decls.iter()
        .chain(program.modules.iter().flat_map(|m| m.type_decls.iter()));
    for td in all_type_decls {
        match &td.kind {
            IrTypeDeclKind::Alias { target } => {
                // Opaque (mod/local) aliases are newtypes — don't expand transparently
                if matches!(td.visibility, IrVisibility::Public) {
                    type_aliases.insert(td.name, target.clone());
                }
            }
            _ => {}
        }
        // Track types with generic parameters
        if td.generics.as_ref().map_or(false, |g| !g.is_empty()) {
            generic_types.insert(td.name);
        }
    }
    // Record/variant types that get a generated `AlmideRepr` impl — a value of
    // such a type interpolated in a string renders to its literal form. Gate
    // mirrors `render_repr_impl` (closure-bearing types are excluded).
    let mut repr_named_types = std::collections::HashSet::new();
    for td in program.type_decls.iter()
        .chain(program.modules.iter().flat_map(|m| m.type_decls.iter()))
    {
        if declarations::type_has_repr_impl(td) {
            repr_named_types.insert(td.name);
        }
    }
    let mut ann = ctx.ann.clone();
    // Compute which user types cannot derive PartialEq (contain Matrix,
    // Fn, or a field whose type itself blocks equality). Must consider
    // type decls from every module, not just the top-level program,
    // because user programs reference types defined in other modules.
    let all_type_decls: Vec<IrTypeDecl> = program.type_decls.iter()
        .chain(program.modules.iter().flat_map(|m| m.type_decls.iter()))
        .cloned()
        .collect();
    ann.eq_blocked_types = super::walker::declarations::compute_eq_blocked_types(&all_type_decls);
    ann.phantom_param_structs = super::walker::declarations::compute_phantom_param_structs(&all_type_decls);
    // §4 endgame: the legacy pre-index (lazy_top_let_names /
    // eager_force_top_lets / const_top_let_vars) and the mutable-storage
    // register block are GONE — every consumer reads the TopLetStorage
    // attribute computed by TopLetStoragePass, and the agreement verifier
    // that soaked the flip (v0.27.2) retired with the predicates it
    // compared. One rule, one place.
    // Classify function-local `var` bindings:
    //   LocalMut (let mut T)  — not captured by closures, no RcCow overhead
    //   RcCow                 — captured by a lambda, needs COW semantics
    //
    // Scan IR Bind statements for `var` of non-Copy types, then check if
    // any lambda in the same function captures that var.
    {
        use almide_ir::annotations::VarStorage;
        let mut exclude: std::collections::HashSet<u32> = std::collections::HashSet::new();
        for tl in &program.top_lets {
            if tl.mutable { exclude.insert(tl.var.0); }
        }
        for module in &program.modules {
            for tl in &module.top_lets {
                if tl.mutable { exclude.insert(tl.var.0); }
            }
        }
        for func in &program.functions {
            for p in &func.params { exclude.insert(p.var.0); }
        }
        for module in &program.modules {
            for func in &module.functions {
                for p in &func.params { exclude.insert(p.var.0); }
            }
        }

        // Phase 1: Collect all non-Copy `var` bindings
        struct VarBindCollector { vars: std::collections::HashSet<u32> }
        impl almide_ir::visit::IrVisitor for VarBindCollector {
            fn visit_stmt(&mut self, stmt: &IrStmt) {
                if let IrStmtKind::Bind { var, mutability: almide_ir::Mutability::Var, ty, .. } = &stmt.kind {
                    // §4 stage 2c (#531): derived from THE copy-ness
                    // classifier (projection table in top_let_storage).
                    if !almide_ir::top_let_storage::rccow_copyish(ty) {
                        self.vars.insert(var.0);
                    }
                }
                almide_ir::visit::walk_stmt(self, stmt);
            }
            fn visit_expr(&mut self, expr: &IrExpr) {
                almide_ir::visit::walk_expr(self, expr);
            }
        }
        let mut collector = VarBindCollector { vars: std::collections::HashSet::new() };
        use almide_ir::visit::IrVisitor;
        for func in &program.functions {
            collector.visit_expr(&func.body);
        }
        for module in &program.modules {
            for func in &module.functions {
                collector.visit_expr(&func.body);
            }
        }

        // Phase 2: Find vars captured by any lambda — via the single shared
        // free-variable analysis (`almide_ir::free_vars`), the same one the WASM
        // closure path uses. A lambda's captures are the free vars of its body
        // relative to its params; the union over every lambda is the full captured
        // set. `free_vars` tracks all binders (block lets incl. destructure, match
        // arms, for-in vars, nested lambdas), so this is strictly more accurate than
        // the old hand-rolled lambda-depth walker. (Closure v2, P4: one capture
        // analysis for both targets.)
        struct CaptureUnion { captured: std::collections::HashSet<u32> }
        impl almide_ir::visit::IrVisitor for CaptureUnion {
            fn visit_expr(&mut self, expr: &IrExpr) {
                if let IrExprKind::Lambda { params, body, .. } = &expr.kind {
                    let param_set: std::collections::HashSet<VarId> =
                        params.iter().map(|(v, _)| *v).collect();
                    for v in almide_ir::free_vars::free_vars(body, &param_set) {
                        self.captured.insert(v.0);
                    }
                }
                almide_ir::visit::walk_expr(self, expr);
            }
        }
        let mut cap = CaptureUnion { captured: std::collections::HashSet::new() };
        for func in &program.functions { cap.visit_expr(&func.body); }
        for module in &program.modules {
            for func in &module.functions { cap.visit_expr(&func.body); }
        }

        // Phase 3: Only vars captured by lambdas get RcCow; rest are LocalMut (let mut)
        for var_id in collector.vars {
            if exclude.contains(&var_id) { continue; }
            // Captured mutable vars that became shared cells (`Rc<Cell>` for Copy via
            // P3, `SharedMut` for non-Copy via P6) are driven by the shared-mut path,
            // NOT RcCow — RcCow's copy-on-write would lose a mutation made through the
            // closure. (Closure v2 P6.)
            if ann.is_shared_mut(&VarId(var_id)) { continue; }
            if cap.captured.contains(&var_id) {
                ann.var_storage.insert(VarId(var_id), VarStorage::RcCow);
            }
            // LocalMut: no entry in var_storage → walker emits plain `let mut T`
        }
        // Note: captured Copy-type mutable vars (Int/Float/Bool) are classified as
        // `shared_mut_vars` (→ `Rc<Cell<T>>`) by CaptureClonePass, which runs before
        // it must decide whether to clone-wrap the (now non-Copy) capture. Those
        // flow in via `program.codegen_annotations` → `ctx.ann`. (Closure v2, P3.)
    }
    let mut ctx = RenderContext {
        templates: ctx.templates,
        var_table: ctx.var_table,
        indent: ctx.indent,
        target: ctx.target,
        auto_unwrap: ctx.auto_unwrap,
        is_test: ctx.is_test,
        ann,
        type_aliases,
        generic_types,
        minimal_generic_bounds: false,
        repr_c: ctx.repr_c,
        ref_params: std::collections::HashSet::new(),
        ref_mut_params: std::collections::HashSet::new(),
        repr_named_types,
        fn_err_ty: None,
    };
    for td in &program.type_decls {
        if let IrTypeDeclKind::Variant { cases, .. } = &td.kind {
            for c in cases {
                ctx.ann.ctor_to_enum.insert(c.name.to_string(), td.name.to_string());
            }
        }
    }
    // Also register constructors from imported modules
    for module in &program.modules {
        for td in &module.type_decls {
            if let IrTypeDeclKind::Variant { cases, .. } = &td.kind {
                for c in cases {
                    ctx.ann.ctor_to_enum.insert(c.name.to_string(), td.name.to_string());
                }
            }
        }
    }

    // Build anonymous record maps (populated by target-specific pipeline)
    ctx.ann.named_records = collect_named_records(program);
    ctx.ann.anon_records = collect_anon_records(program, &ctx.ann.named_records);
    ctx.ann.anon_records_with_fn = declarations::take_anon_fn_keys();
    ctx.ann.record_field_counts = collect_record_field_counts(program);

    let mut parts = Vec::new();

    // Anonymous record struct definitions (only if anon_records is populated).
    // SORTED iteration: anon_records is a HashMap, and emitting in its raw
    // iteration order made the generated Rust SOURCE nondeterministic
    // run-to-run (three runs, three different bytes) — semantically neutral
    // for rustc but fatal for reproducible builds and any byte-diff gate.
    if !ctx.ann.anon_records.is_empty() {
        let mut sorted_anon: Vec<(&Vec<String>, &String)> = ctx.ann.anon_records.iter().collect();
        sorted_anon.sort_by(|a, b| a.1.cmp(b.1));
        for (field_names, struct_name) in sorted_anon {
            // A closure field can't be Debug/PartialEq, so such a struct derives
            // Clone only (the `has_fn_fields` struct_decl) and drops the
            // `Debug + PartialEq` generic bounds — derive(Clone) re-adds `T: Clone`
            // itself. Mirrors the `type`-declared record path. (Cross-target gaps.)
            let has_fn = ctx.ann.anon_records_with_fn.contains(field_names);
            let generics: Vec<String> = (0..field_names.len())
                .map(|i| {
                    let name_s = format!("T{}", i);
                    if has_fn {
                        name_s
                    } else {
                        ctx.templates.render_with("generic_bound_full", None, &[], &[("name", name_s.as_str())])
                            .unwrap_or_else(|| format!("T{}", i))
                    }
                })
                .collect();
            let fields: Vec<String> = field_names.iter().enumerate()
                .map(|(i, name)| {
                    let type_s = format!("T{}", i);
                    ctx.templates.render_with("struct_field", None, &[], &[("name", name.as_str()), ("type", type_s.as_str())])
                        .unwrap_or_else(|| format!("    pub {}: T{}", name, i))
                })
                .collect();
            let fields_str = fields.join("\n");
            let full_name = format!("{}<{}>", struct_name, generics.join(", "));
            let mut decl_attrs: Vec<&str> = if ctx.repr_c { vec!["repr_c"] } else { vec![] };
            if has_fn { decl_attrs.push("has_fn_fields"); }
            let repr_prefix = if ctx.repr_c { "#[repr(C)]\n" } else { "" };
            parts.push(ctx.templates.render_with("struct_decl", None, &decl_attrs, &[("name", full_name.as_str()), ("fields", fields_str.as_str())])
                .unwrap_or_else(|| format!("{}pub struct {} {{\n{}\n}}", repr_prefix, struct_name, fields_str)));

            // `AlmideRepr` impl for the anonymous struct: `"${rec}"` renders an
            // anonymous record to `{ x: 1, y: 2 }` — NO type name, because it HAS
            // none — byte-identically with the WASM anon-record walk. A
            // closure-bearing anon record (`has_fn`) is not `AlmideRepr`, so it is
            // skipped (it never reaches compound interp). Fields render in the
            // struct's own field order, which is the SORTED field-name list (this
            // `field_names` is the sorted key from `collect_anon_records`); the
            // WASM walk sorts to match (see `emit_repr_record`).
            if !has_fn {
                let bare_generics: Vec<String> = (0..field_names.len())
                    .map(|i| format!("T{}", i)).collect();
                // The anon struct declares each param via `generic_bound_full`
                // (`T: Clone + Debug + PartialEq`); the impl must satisfy those
                // same bounds, plus `AlmideRepr` so the field reprs compose.
                // Reuse the template so the bounds stay in lock-step with the decl.
                let impl_bounds = bare_generics.iter()
                    .map(|t| {
                        let own = ctx.templates.render_with("generic_bound_full", None, &[], &[("name", t.as_str())])
                            .unwrap_or_else(|| format!("{}: Clone + std::fmt::Debug + PartialEq", t));
                        match own.split_once(':') {
                            Some((name, rest)) => format!("{}: AlmideRepr +{}", name.trim_end(), rest),
                            None => format!("{}: AlmideRepr", t),
                        }
                    })
                    .collect::<Vec<_>>().join(", ");
                let target = format!("{}<{}>", struct_name, bare_generics.join(", "));
                let fmt = field_names.iter().enumerate()
                    .map(|(i, name)| format!("{}{}: {{}}", if i > 0 { ", " } else { "" }, name))
                    .collect::<Vec<_>>().join("");
                let args = field_names.iter()
                    .map(|name| format!("self.{}.almide_repr()", name))
                    .collect::<Vec<_>>().join(", ");
                parts.push(format!(
                    "impl<{}> AlmideRepr for {} {{ fn almide_repr(&self) -> String {{ format!(\"{{{{ {} }}}}\", {}) }} }}",
                    impl_bounds, target, fmt, args
                ));
            }
        }
    }

    // Type declarations — track emitted names to deduplicate across modules
    let mut emitted_types: std::collections::HashSet<String> = std::collections::HashSet::new();
    for td in &program.type_decls {
        emitted_types.insert(td.name.as_str().to_string());
        let mut rendered = render_type_decl(&ctx, td);
        if let Some(ref doc) = td.doc {
            let doc_lines: String = doc.lines()
                .map(|line| if line.is_empty() { "///".to_string() } else { format!("/// {}", line) })
                .collect::<Vec<_>>()
                .join("\n");
            rendered = format!("{}\n{}", doc_lines, rendered);
        }
        parts.push(rendered);
    }

    // Top-level lets and vars — §4 Stage 2: the declaration consumes the
    // SAME GlobalInfo every reference site dispatches on (storage class and
    // static name decided once, in the attribute pass). The former
    // `lazy_vars` mid-emission write is gone — no reader remains.
    for tl in &program.top_lets {
        let ty_str = render_type_fn(&ctx, &tl.ty);
        let val_str = render_expr_fn(&ctx, &tl.value);
        let info = ctx.ann.globals.get(&tl.var).unwrap_or_else(|| panic!(
            "[COMPILER BUG] top-let `{}` missing from the storage attribute",
            ctx.var_table.get(tl.var).name.as_str()
        ));
        use almide_ir::top_let_storage::TopLetStorage as Tls;
        let name_upper = info.static_name.as_str();
        let mut rendered = match info.storage {
            Tls::Cell =>
                format!("thread_local! {{ static {}: std::cell::Cell<{}> = std::cell::Cell::new({}); }}", name_upper, ty_str, val_str),
            Tls::RcRefCell =>
                format!("thread_local! {{ static {}: std::cell::RefCell<std::rc::Rc<{}>> = std::cell::RefCell::new(std::rc::Rc::new({})); }}", name_upper, ty_str, val_str),
            Tls::Const | Tls::Lazy { .. } => {
                let construct = match info.storage {
                    Tls::Const => "top_let_const",
                    _ => "top_let_lazy",
                };
                ctx.templates.render_with(construct, None, &[], &[("name", name_upper), ("type", ty_str.as_str()), ("value", val_str.as_str())])
                    .unwrap_or_else(|| format!("const {} = {};", name_upper, val_str))
            }
        };
        if let Some(ref doc) = tl.doc {
            let doc_lines: String = doc.lines()
                .map(|line| if line.is_empty() { "///".to_string() } else { format!("/// {}", line) })
                .collect::<Vec<_>>()
                .join("\n");
            rendered = format!("{}\n{}", doc_lines, rendered);
        }
        parts.push(rendered);
    }

    // Functions (non-test): separate extern fn imports from regular functions
    let mut import_parts = Vec::new();
    let mut fn_parts = Vec::new();
    for func in program.functions.iter().filter(|f| !f.is_test) {
        let rendered = render_function(&ctx, func);
        if !func.extern_attrs.is_empty() {
            import_parts.push(rendered);
        } else {
            fn_parts.push(rendered);
        }
    }
    // Emit imports first as a group, then functions
    if !import_parts.is_empty() {
        parts.push(import_parts.join("\n"));
    }
    parts.extend(fn_parts);

    // Test functions
    let test_fns: Vec<&IrFunction> = program.functions.iter().filter(|f| f.is_test).collect();
    if !test_fns.is_empty() {
        let test_parts: Vec<String> = test_fns.iter()
            .map(|f| render_function(&ctx, f))
            .collect();
        let tests_s = test_parts.join("\n\n");
        let indented_tests = indent_lines(&tests_s, 4);
        let wrapped = ctx.templates.render_with("test_module", None, &[], &[("tests", indented_tests.as_str())])
            .unwrap_or_else(|| test_parts.join("\n\n"));
        parts.push(wrapped);
    }

    // Modules are flattened by ir_link_flatten into root functions/types/top_lets.
    // No per-module iteration needed.
    debug_assert!(program.modules.is_empty(), "ir_link_flatten should have emptied modules");

    parts.join("\n\n")
}
