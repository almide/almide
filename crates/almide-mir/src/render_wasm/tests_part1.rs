// render_wasm test suite — part 1 of 3 (renderer unit tests + shared helpers).
// Textually included by render_wasm/tests.rs (one module: helpers/tests share scope).

    fn heap() -> Repr {
        Repr::Ptr { layout: PLACEHOLDER_LAYOUT }
    }

    /// Same program as the Rust-side test: `fn add(a,b)=a+b` + a `main` calling
    /// it. Both targets must print the same `5` — the dual-renderer thesis for
    /// USER FUNCTIONS (the mechanism that lets the runtime be self-hosted).
    fn add_program() -> MirProgram {
        let scalar = Repr::Scalar { width: ScalarWidth::Double };
        let add = MirFunction {
            name: "add".into(),
            params: vec![
                MirParam { value: ValueId(0), repr: scalar },
                MirParam { value: ValueId(1), repr: scalar },
            ],
            ops: vec![Op::IntBinOp {
                dst: ValueId(2),
                op: IntOp::Add,
                a: ValueId(0),
                b: ValueId(1),
            }],
            ret: Some(ValueId(2)),
            ..Default::default()
        };
        let main = MirFunction {
            name: "main".into(),
            params: vec![],
            ops: vec![
                Op::CallFn {
                    dst: Some(ValueId(0)),
                    name: "add".into(),
                    args: vec![CallArg::Imm(2), CallArg::Imm(3)],
                result: None },
                Op::Call { dst: None, func: RtFn::PrintInt, args: vec![CallArg::Scalar(ValueId(0))] , result: None },
            ],
            ret: None,
            ..Default::default()
        };
        MirProgram { functions: vec![add, main] }
    }

    #[test]
    fn function_call_lowers_and_runs_on_wasm() {
        let prog = add_program();
        if let Some(out) = build_and_run("fncall", &render_wasm_program(&prog)) {
            assert_eq!(out, "5");
        }
    }

    #[test]
    fn func_ref_resolves_a_lambda_slot_by_name_then_call_indirect() {
        // FuncRef materializes a lifted function's table slot BY NAME (the lambda-lifting
        // binding for `let f = (x) => …`), then CallIndirect dispatches through it. main is
        // slot 0, add1 slot 1; `f = FuncRef("add1")` must resolve to 1 (not a hardcoded
        // index), so `f(5)` = 6.
        let scalar = Repr::Scalar { width: ScalarWidth::Double };
        let main = MirFunction {
            name: "main".into(),
            params: vec![],
            ops: vec![
                Op::FuncRef { dst: ValueId(0), name: "add1".into() },
                Op::CallIndirect {
                    dst: Some(ValueId(1)),
                    table_idx: ValueId(0),
                    args: vec![CallArg::Imm(5)],
                    result: Some(scalar),
                },
                Op::Call {
                    dst: None,
                    func: RtFn::PrintInt,
                    args: vec![CallArg::Scalar(ValueId(1))],
                    result: None,
                },
            ],
            ret: None,
            ..Default::default()
        };
        let add1 = MirFunction {
            name: "add1".into(),
            params: vec![MirParam { value: ValueId(0), repr: scalar }],
            ops: vec![
                Op::ConstInt { dst: ValueId(1), value: 1 },
                Op::IntBinOp { dst: ValueId(2), op: IntOp::Add, a: ValueId(0), b: ValueId(1) },
            ],
            ret: Some(ValueId(2)),
            ..Default::default()
        };
        // main at slot 0, add1 at slot 1 — FuncRef("add1") must resolve to 1.
        let prog = MirProgram { functions: vec![main, add1] };
        if let Some(out) = build_and_run("func_ref", &render_wasm_program(&prog)) {
            assert_eq!(out, "6");
        }
    }

    #[test]
    fn call_indirect_dispatches_through_the_function_table() {
        // Closures execution floor: a lifted lambda `add1(x) = x + 1` at table slot 0,
        // invoked by `main` via Op::CallIndirect with a runtime table index (the slot the
        // lambda would be bound to). main: fidx = 0; y = (table[fidx])(5); print y -> 6.
        let scalar = Repr::Scalar { width: ScalarWidth::Double };
        let add1 = MirFunction {
            name: "add1".into(),
            params: vec![MirParam { value: ValueId(0), repr: scalar }],
            ops: vec![
                Op::ConstInt { dst: ValueId(1), value: 1 },
                Op::IntBinOp { dst: ValueId(2), op: IntOp::Add, a: ValueId(0), b: ValueId(1) },
            ],
            ret: Some(ValueId(2)),
            ..Default::default()
        };
        let main = MirFunction {
            name: "main".into(),
            params: vec![],
            ops: vec![
                // add1 is function/table index 0 (first in prog.functions).
                Op::ConstInt { dst: ValueId(0), value: 0 },
                Op::CallIndirect {
                    dst: Some(ValueId(1)),
                    table_idx: ValueId(0),
                    args: vec![CallArg::Imm(5)],
                    result: Some(scalar),
                },
                Op::Call {
                    dst: None,
                    func: RtFn::PrintInt,
                    args: vec![CallArg::Scalar(ValueId(1))],
                    result: None,
                },
            ],
            ret: None,
            ..Default::default()
        };
        let prog = MirProgram { functions: vec![add1, main] };
        if let Some(out) = build_and_run("call_indirect", &render_wasm_program(&prog)) {
            assert_eq!(out, "6");
        }
    }

    /// A HEAP-returning user call (`fn mk() -> String = "hi"`): the result is a Ptr
    /// (an i32 `$alloc` handle), so the caller's local must be typed i32 — NOT the
    /// scalar i64 default. Regression for `value_reprs_wasm` reading the call op's
    /// `result` repr; typing it i64 made `local.set` reject `$mk`'s i32 handle. Also
    /// pins the `Init::Str` un-defer: the literal's bytes are materialized.
    fn heap_call_program() -> MirProgram {
        let mk = MirFunction {
            name: "mk".into(),
            params: vec![],
            ops: vec![Op::Alloc { dst: ValueId(0), repr: heap(), init: Init::Str("hi".into()) }],
            ret: Some(ValueId(0)),
            ..Default::default()
        };
        let main = MirFunction {
            name: "main".into(),
            params: vec![],
            ops: vec![
                Op::CallFn {
                    dst: Some(ValueId(0)),
                    name: "mk".into(),
                    args: vec![],
                    result: Some(heap()),
                },
                // The returned object (rc 1) is released here — ownership-balanced.
                Op::Drop { v: ValueId(0) },
            ],
            ret: None,
            ..Default::default()
        };
        MirProgram { functions: vec![mk, main] }
    }

    #[test]
    fn heap_returning_call_types_result_as_i32_handle() {
        let prog = heap_call_program();
        let wat = render_wasm_program(&prog);
        // The fix: the call-result local is an i32 handle, never the scalar i64.
        assert!(wat.contains("(local $v0 i32)"), "call-result local must be i32:\n{wat}");
        assert!(!wat.contains("(local $v0 i64)"), "call-result local must NOT be i64:\n{wat}");
        // The Init::Str un-defer materialized the literal's bytes: 'h'=104, 'i'=105.
        assert!(
            wat.contains("(i32.const 104)") && wat.contains("(i32.const 105)"),
            "Init::Str bytes not materialized:\n{wat}"
        );
        // End-to-end: validates, runs clean (exit 0, no output) where wasmtime exists.
        if let Some(out) = build_and_run("heapcall", &wat) {
            assert_eq!(out, "");
        }
    }

    /// An `@extern(wasm, module, name)` body-less fn lowers to a host-IMPORT call:
    /// the module DECLARES `(import "module" "name" (func …))` and the wrapper body
    /// `(call $__import_…)`s it, coercing each i64-uniform MIR local to the import
    /// valtype (Float ↔ f64 reinterpret). The module is structurally valid wasm; a
    /// browser host (not wasmtime) satisfies the import — so these fns LOWER (🟡).
    #[test]
    fn extern_wasm_fn_lowers_to_host_import() {
        let prog = lower_source(
            r#"
            @extern(wasm, "dom", "create_element")
            fn create_element(tag_id: Int) -> Int = _

            @extern(wasm, "console", "log_float")
            fn log_float(value: Float) -> Unit = _

            @extern(wasm, "dom", "get_offset_width")
            fn get_offset_width(el_id: Int) -> Float = _

            fn main() -> Unit = {
              let el = create_element(7)
              let w = get_offset_width(el)
              log_float(w)
            }
            "#,
        );
        let wat = render_wasm_program(&prog);
        // The import section declares each host function with its mapped signature
        // (Int→i64, Float→f64, Unit→no result), under the mangled `$__import_…` symbol.
        assert!(
            wat.contains(r#"(import "dom" "create_element" (func $__import_dom_create_element (param i64) (result i64)))"#),
            "missing/incorrect create_element import:\n{wat}"
        );
        assert!(
            wat.contains(r#"(import "console" "log_float" (func $__import_console_log_float (param f64)))"#),
            "missing/incorrect log_float import (Unit return = no result, Float param = f64):\n{wat}"
        );
        assert!(
            wat.contains(r#"(import "dom" "get_offset_width" (func $__import_dom_get_offset_width (param i64) (result f64)))"#),
            "missing/incorrect get_offset_width import:\n{wat}"
        );
        // The wrapper body calls the import. The Float arg is reinterpreted from the
        // i64 MIR local that holds its bits; the f64 result is reinterpreted back.
        assert!(
            wat.contains("(call $__import_console_log_float (f64.reinterpret_i64"),
            "Float import arg must reinterpret i64 bits → f64:\n{wat}"
        );
        assert!(
            wat.contains("(i64.reinterpret_f64 (call $__import_dom_get_offset_width"),
            "f64 import result must reinterpret back to the i64 MIR local:\n{wat}"
        );
        // The emitted module is structurally VALID wasm (it just needs a browser host
        // to instantiate). Validate when a wasm toolchain is present; skip otherwise.
        assert_wasm_valid("extern_import", &wat);
    }

    /// A `@extern(rs, …)`/`@extern(rust, …)` (NATIVE) extern has NO wasm host — emitting
    /// an import would be a hollow lie. So it must NOT lower to a `CallImport`: it WALLS
    /// (its body-less Hole stays Unsupported), leaving NO import declaration. Only the
    /// `wasm` target becomes a host import.
    #[test]
    fn extern_rust_target_does_not_become_a_wasm_import() {
        let prog = lower_source(
            r#"
            @extern(rs, "native_lib", "reverse")
            fn reverse(s: Int) -> Int = _

            fn main() -> Unit = ()
            "#,
        );
        let wat = render_wasm_program(&prog);
        assert!(
            !wat.contains("__import_native_lib_reverse") && !wat.contains(r#"(import "native_lib""#),
            "a @extern(rs, …) must NOT emit a wasm import (no wasm host):\n{wat}"
        );
    }

    /// Validate `wat` as a real wasm module with whatever toolchain is on PATH
    /// (`wasm-tools validate` or `wat2wasm`). A browser-import module FAILS to
    /// INSTANTIATE on wasmtime (no host) — that is expected — so this checks PARSE/
    /// VALIDATE only. Skips cleanly when no toolchain is present (CI may lack one).
    fn assert_wasm_valid(label: &str, wat: &str) {
        let dir = std::env::temp_dir().join(format!("almide_mir_validate_{label}"));
        std::fs::create_dir_all(&dir).unwrap();
        let wat_path = dir.join("m.wat");
        std::fs::write(&wat_path, wat).unwrap();
        for (bin, args) in [("wasm-tools", vec!["validate"]), ("wat2wasm", vec!["--no-check"])] {
            match Command::new(bin).args(&args).arg(&wat_path).output() {
                Ok(o) if o.status.code() != Some(127) => {
                    assert!(
                        o.status.success(),
                        "{bin} rejected the module:\n{}\n--- wat ---\n{wat}",
                        String::from_utf8_lossy(&o.stderr)
                    );
                    return; // validated by the first available tool
                }
                _ => {} // tool unavailable → try the next
            }
        }
        // No wasm toolchain present — skip (the text assertions above still ran).
    }

    /// Lower real `.almd` SOURCE to a `MirProgram` through the existing frontend
    /// feeder (the same cut point as `examples/render_program.rs`) — for end-to-end
    /// tests over REAL lowering rather than hand-built MIR. Dev-only deps.
    fn lower_source(src: &str) -> MirProgram {
        // Match the PRODUCTION condition (render_program sets strict values): the
        // deferred-Const fallback is retired on real render paths, so the tests pin
        // the same lowering the shipped pipeline runs.
        crate::lower::STRICT_VALUES.store(true, std::sync::atomic::Ordering::Relaxed);
        use almide_frontend::check::Checker;
        use almide_frontend::lower::lower_program;
        use almide_frontend::{canonicalize, ir_link};
        use almide_lang::lexer::Lexer;
        use almide_lang::parser::Parser;
        use almide_optimize::{mono, optimize};
        let to_ir = |s: &str| {
            let tokens = Lexer::tokenize(s);
            let mut prog = Parser::new(tokens).parse().expect("parse");
            let canon = canonicalize::canonicalize_program(&prog, std::iter::empty());
            let mut checker = Checker::from_env(canon.env);
            let _ = checker.infer_program(&mut prog);
            let mut ir = lower_program(&prog, &checker.env, &checker.type_map);
            optimize::optimize_program(&mut ir);
            mono::monomorphize(&mut ir);
            ir_link::ir_link(&mut ir);
            ir
        };
        let ir = to_ir(src);
        // ADT brick 5b: generate the per-type recursive-drop fns for nested-variant types and
        // re-lower with them in scope (the same two-pass as examples/render_program.rs).
        let anon_recs = crate::lower::collect_recursive_anon_records(&ir);
        let uses_result_opt_str = crate::lower::program_uses_result_option_str(&ir);
        // First-class function values need the uniform `$__drop_closure` (same
        // injection as the production pipeline).
        let closure_drop = if crate::lower::program_uses_closures(&ir) {
            crate::lower::CLOSURE_DROP_SRC
        } else {
            ""
        };
        let lenlist_drop = if crate::lower::program_uses_lenlist_elem_lists(&ir) {
            crate::lower::LENLIST_DROP_SRC
        } else {
            ""
        };
        let drops = format!(
            "{}{}{}{}{}",
            crate::lower::generate_variant_drop_sources(&ir.type_decls),
            crate::lower::generate_record_drop_sources(&ir.type_decls, &anon_recs, uses_result_opt_str),
            crate::lower::generate_variant_repr_sources(&ir.type_decls),
            closure_drop,
            lenlist_drop,
        );
        let ir = if drops.trim().is_empty() { ir } else { to_ir(&format!("{src}\n{drops}")) };
        let mut globals: std::collections::HashMap<almide_ir::VarId, almide_lang::types::Ty> =
            std::collections::HashMap::new();
        for tl in &ir.top_lets {
            globals.insert(tl.var, tl.ty.clone());
        }
        for m in &ir.modules {
            for tl in &m.top_lets {
                globals.insert(tl.var, tl.ty.clone());
            }
        }
        // (single-file test helper: no cross-module bridge needed — no sibling modules)
        let mut record_layouts = crate::lower::build_record_layouts(&ir.type_decls);
        for m in &ir.modules {
            record_layouts.extend(crate::lower::build_record_layouts(&m.type_decls));
        }
        // Variant (custom-ADT) layouts — threaded exactly as `examples/render_program.rs` does,
        // so a variant construct / `match` lowers here too (empty for non-variant programs ⇒
        // byte-identical to the record-only path for every existing test).
        let mut variant_layouts = crate::lower::build_variant_layouts(&ir.type_decls);
        for m in &ir.modules {
            let m_vl = crate::lower::build_variant_layouts(&m.type_decls);
            variant_layouts.by_type.extend(m_vl.by_type);
            variant_layouts.ctor_to_type.extend(m_vl.ctor_to_type);
        }
        let mut functions: Vec<MirFunction> = ir
            .functions
            .iter()
            .filter_map(|f| {
                crate::lower::lower_function_all_with_layouts(
                    f,
                    &globals,
                    &record_layouts,
                    &variant_layouts,
                )
                .ok()
            })
            .flatten()
            .collect();
        // Auto-link the self-hosted stdlib runtime: for each registry entry CALLED but not
        // defined, lower its Almide source and rename the impl fn to the call name (so
        // `(call $module.func)` resolves AND the caps gate reads it as a known-pure stdlib
        // `module.func` — no caps-coverage regression). Conditional, so non-using programs
        // are not bloated by builders + helpers.
        for (source, entries) in self_host_runtime() {
            let mut any_called = entries.iter().any(|(_, call)| calls_fn(&functions, call));
            // A Value drop renders `(call $__drop_value …)`, a value_core helper pulled only WITH
            // value_core. A program building Values via `json.*` (not `value.*`) reaches no
            // value_core call_name yet still drops Values — force value_core on any Value-drop op.
            if entries.iter().any(|(_, c)| *c == "value.null") {
                any_called = any_called
                    || functions.iter().any(|f| {
                        f.ops.iter().any(|op| {
                            matches!(
                                op,
                                crate::Op::DropValue { .. }
                                    | crate::Op::DropListValue { .. }
                                    | crate::Op::DropListStrValue { .. }
                                    | crate::Op::DropListStrStr { .. }
                                    | crate::Op::DropResultValue { .. }
                                    | crate::Op::DropResultListValue { .. }
                            )
                        })
                    });
            }
            // Recognize a source as already provided if EITHER its renamed call name OR its
            // original IMPL name is present. The IMPL-name check is the re-entry guard: when this
            // recursive `lower_source` is lowering value_core ITSELF, value_core's drop helpers emit
            // DropValue ops, so the Value-drop force above sets any_called — but value_core's own
            // impl fns (value_null, …) are present (not yet renamed), so any_defined is now true and
            // we skip re-linking value_core into its own lowering (was an infinite recursion).
            let any_defined = entries
                .iter()
                .any(|(impl_fn, call)| functions.iter().any(|f| &f.name == call || &f.name == impl_fn));
            if any_called && !any_defined {
                let mut rt = lower_source(source);
                for f in rt.functions.iter_mut() {
                    if let Some((_, call)) = entries.iter().find(|(impl_fn, _)| &f.name == impl_fn) {
                        f.name = call.to_string();
                    }
                }
                functions.extend(rt.functions);
            }
        }
        // Auto-link the self-hosted print_str runtime (the v1 linker step) so a plain
        // `println(…)` program — which lowers to a PrintStr → `(call $print_str)` —
        // resolves, matching how render_program links it. Skip if already defined.
        if !functions.iter().any(|f| f.name == "print_str") {
            let rt = lower_source(include_str!("../../../../stdlib/print_str.almd"));
            functions.extend(rt.functions);
        }
        // A recursively-linked impl re-runs this auto-link, so it can carry its own copy of
        // print_str (or, later, a shared helper). Keep the FIRST definition of each name —
        // they are identical (same source), so dedup is a no-op on behavior, not a merge.
        let mut seen = std::collections::HashSet::new();
        functions.retain(|f| seen.insert(f.name.clone()));
        // A self-hosted fn may call ANOTHER registered impl by its IMPL name (value_core's
        // `__vstr_arr` recursing through `value_stringify`), but the auto-link RENAMED that def to
        // its call_name (`value.stringify`). Rewrite those call sites so the internal recursion
        // resolves to the renamed def, not a dangling impl-name call (mirrors render_program).
        let impl_to_call: std::collections::HashMap<&str, &str> = self_host_runtime()
            .iter()
            .flat_map(|(_, es)| es.iter().map(|(i, c)| (*i, *c)))
            .collect();
        for f in functions.iter_mut() {
            for op in f.ops.iter_mut() {
                if let crate::Op::CallFn { name, .. } = op {
                    if let Some(&c) = impl_to_call.get(name.as_str()) {
                        *name = c.to_string();
                    }
                }
            }
        }
        MirProgram { functions }
    }

    /// Does any function CALL `name` (a `CallFn` to it)? Drives conditional auto-linking.
    fn calls_fn(functions: &[MirFunction], name: &str) -> bool {
        functions.iter().any(|f| {
            f.ops.iter().any(|op| matches!(op, Op::CallFn { name: n, .. } if n == name))
        })
    }

    /// A scalar-result user call (`let _r = add(2, 3)`) lowered from REAL source is an
    /// EXECUTABLE `CallFn` — immediate args + a bound scalar result — not the pre-
    /// execution `Const` + empty elided marker. Regression for `try_lower_scalar_call`
    /// (the scalar-call execution slice). The swap is adversarially-verified SOUND: the
    /// real CallFn replaces the marker 1:1 (same callee NAME → caps fold unchanged) and
    /// a scalar result registers no ownership object.
    #[test]
    fn scalar_user_call_lowers_to_executable_callfn() {
        let prog = lower_source(
            "fn add(a: Int, b: Int) -> Int = a + b\nfn main() -> Unit = { let _r = add(2, 3) }\n",
        );
        let main = prog.functions.iter().find(|f| f.name == "main").expect("main lowered");
        // A real CallFn to `add`: a bound result repr + both literal args as immediates.
        let (args, result) = main
            .ops
            .iter()
            .find_map(|op| match op {
                Op::CallFn { dst: Some(_), name, args, result } if name == "add" => {
                    Some((args.clone(), *result))
                }
                _ => None,
            })
            .expect("a real CallFn to add with a bound dst");
        assert!(result.is_some(), "the scalar call result repr must be set");
        assert!(
            args.iter().any(|a| matches!(a, CallArg::Imm(2)))
                && args.iter().any(|a| matches!(a, CallArg::Imm(3))),
            "both literal args must be immediates, got {args:?}"
        );
        // ...and NOT also an empty elided marker for add (the call is real, not elided).
        assert!(
            !main.ops.iter().any(|op| matches!(op,
                Op::CallFn { dst: None, name, args, .. } if name == "add" && args.is_empty())),
            "add must not also appear as an empty elided caps marker"
        );
        // End-to-end: renders to a valid module that runs cleanly (where wasmtime is present).
        if let Some(out) = build_and_run("scalar_user_call", &render_wasm_program(&prog)) {
            assert_eq!(out, "");
        }
    }

    /// An `Int` literal in value position materializes its REAL value (`Op::ConstInt`
    /// → `(local.set $dst (i64.const v))`), not the deferred-`Const` zero — the
    /// scalar-value foundation that lets a self-hosted runtime fn compute real
    /// addresses/lengths. Regression: lowering emits ConstInt, render emits the const.
    #[test]
    fn int_literal_materializes_its_value() {
        let prog = lower_source(
            "fn answer() -> Int = 42\nfn main() -> Unit = { let _a = answer() }\n",
        );
        let answer = prog.functions.iter().find(|f| f.name == "answer").expect("answer lowered");
        assert!(
            answer.ops.iter().any(|op| matches!(op, Op::ConstInt { value: 42, .. })),
            "answer must materialize 42 via ConstInt, got {:?}",
            answer.ops
        );
        let wat = render_wasm_program(&prog);
        assert!(wat.contains("(i64.const 42)"), "render must emit the constant:\n{wat}");
        if let Some(out) = build_and_run("int_literal", &wat) {
            assert_eq!(out, "");
        }
    }

    /// Scalar `Int` arithmetic COMPUTES (`fn add(a, b) = a + b` → `IntBinOp{Add}`,
    /// rendered `i64.add` over the param locals) — not the deferred-Const zero. With
    /// the literal-materialization above, this is the foundation a self-hosted runtime
    /// fn needs to compute real addresses (`s + LIST_HEADER`). The add_program test
    /// already proves `i64.add` returns the right value end-to-end.
    #[test]
    fn scalar_arithmetic_computes_via_intbinop() {
        let prog = lower_source(
            "fn add(a: Int, b: Int) -> Int = a + b\nfn main() -> Unit = { let _r = add(2, 3) }\n",
        );
        let add = prog.functions.iter().find(|f| f.name == "add").expect("add lowered");
        assert!(
            add.ops.iter().any(|op| matches!(op, Op::IntBinOp { op: IntOp::Add, .. })),
            "add must compute a+b via IntBinOp, got {:?}",
            add.ops
        );
        let wat = render_wasm_program(&prog);
        assert!(wat.contains("(i64.add"), "render must emit i64.add:\n{wat}");
        if let Some(out) = build_and_run("scalar_arith", &wat) {
            assert_eq!(out, "");
        }
    }

    /// THE PRIM-FLOOR PROOF (sub-slice 1): a hand-built `print_str` MirFunction —
    /// written ENTIRELY over the prim floor (handle / load / store / fd_write) + the
    /// scalar-value foundation (ConstInt / IntBinOp) — reads a heap String's bytes and
    /// writes them + a newline to stdout via a 2-element iovec `fd_write`. main allocs
    /// "hi" and calls it. This proves the prim ops render to valid wasm and ACTUALLY
    /// PRINT, with NO new preamble runtime func (the discipline) — the whole mechanism
    /// for self-hosted print, validated in isolation before the frontend `prim` module.
    #[test]
    fn prim_floor_print_str_prints() {
        // print_str(s: String): writes the string's bytes + "\n" to stdout.
        let print_str = MirFunction {
            name: "print_str".into(),
            params: vec![MirParam { value: ValueId(0), repr: heap() }],
            ops: vec![
                // h = prim.handle(s)  — the block's i64 address
                Op::Prim { kind: PrimKind::Handle, dst: Some(ValueId(1)), args: vec![ValueId(0)] },
                // len = prim.load32(h + 4)   (LIST_LEN_OFFSET)
                Op::ConstInt { dst: ValueId(2), value: 4 },
                Op::IntBinOp { dst: ValueId(3), op: IntOp::Add, a: ValueId(1), b: ValueId(2) },
                Op::Prim { kind: PrimKind::Load { width: 4 }, dst: Some(ValueId(4)), args: vec![ValueId(3)] },
                // data = h + 12   (LIST_HEADER)
                Op::ConstInt { dst: ValueId(5), value: 12 },
                Op::IntBinOp { dst: ValueId(6), op: IntOp::Add, a: ValueId(1), b: ValueId(5) },
                // iovec[0] = { ptr=data @ 8, len @ 12 }
                Op::ConstInt { dst: ValueId(7), value: 8 },
                Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![ValueId(7), ValueId(6)] },
                Op::ConstInt { dst: ValueId(8), value: 12 },
                Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![ValueId(8), ValueId(4)] },
                // "\n" (10) at scratch 512; iovec[1] = { ptr=512 @ 16, len=1 @ 20 }
                Op::ConstInt { dst: ValueId(9), value: 512 },
                Op::ConstInt { dst: ValueId(10), value: 10 },
                Op::Prim { kind: PrimKind::Store { width: 1 }, dst: None, args: vec![ValueId(9), ValueId(10)] },
                Op::ConstInt { dst: ValueId(11), value: 16 },
                Op::ConstInt { dst: ValueId(12), value: 512 },
                Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![ValueId(11), ValueId(12)] },
                Op::ConstInt { dst: ValueId(13), value: 20 },
                Op::ConstInt { dst: ValueId(14), value: 1 },
                Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![ValueId(13), ValueId(14)] },
                // fd_write(stdout=1, iovec@8, count=2, nwritten@0)
                Op::ConstInt { dst: ValueId(15), value: 1 },
                Op::ConstInt { dst: ValueId(16), value: 8 },
                Op::ConstInt { dst: ValueId(17), value: 2 },
                Op::ConstInt { dst: ValueId(18), value: 0 },
                Op::Prim {
                    kind: PrimKind::FdWrite,
                    dst: Some(ValueId(19)),
                    args: vec![ValueId(15), ValueId(16), ValueId(17), ValueId(18)],
                },
            ],
            ret: None,
            declared_caps: vec![crate::Capability::Stdout],
            heap_slot_masks: Default::default(),
        };
        let main = MirFunction {
            name: "main".into(),
            params: vec![],
            ops: vec![
                Op::Alloc { dst: ValueId(0), repr: heap(), init: Init::Str("hi".into()) },
                Op::CallFn {
                    dst: None,
                    name: "print_str".into(),
                    args: vec![CallArg::Handle(ValueId(0))],
                    result: None,
                },
                Op::Drop { v: ValueId(0) },
            ],
            ret: None,
            ..Default::default()
        };
        let prog = MirProgram { functions: vec![print_str, main] };
        // The prim ops render to valid wasm and print "hi\n" (trimmed to "hi").
        if let Some(out) = build_and_run("prim_print_str", &render_wasm_program(&prog)) {
            assert_eq!(out, "hi");
        }
    }

    /// THE OBSERVABILITY KEYSTONE (sub-slice 2+3): `println("hello")` from SOURCE runs
    /// through v1's SELF-HOSTED print_str — written in ALMIDE over the `prim` floor
    /// (prim.handle/load32/store*/fd_write → Op::Prim, mapped from the bundled `prim`
    /// module), compiled through v1's own lower→MIR→render pipeline — and prints
    /// "hello", byte-matching v0's native println. NO hand-written WAT growth (the
    /// discipline). The self-host runtime vision, realized for print.
    #[test]
    fn selfhosted_print_str_from_source_prints() {
        let src = "fn print_str(s: String) -> Unit = {\n  \
            let h = prim.handle(s)\n  \
            let len = prim.load32(h + 4)\n  \
            let data = h + 12\n  \
            prim.store32(8, data)\n  \
            prim.store32(12, len)\n  \
            prim.store8(512, 10)\n  \
            prim.store32(16, 512)\n  \
            prim.store32(20, 1)\n  \
            let _w = prim.fd_write(1, 8, 2, 0)\n\
            }\n\
            fn main() -> Unit = println(\"hello\")\n";
        let prog = lower_source(src);
        // print_str lowered to real prim-floor ops (not the deferred Const).
        let ps = prog.functions.iter().find(|f| f.name == "print_str").expect("print_str lowered");
        assert!(
            ps.ops.iter().any(|op| matches!(op, Op::Prim { kind: PrimKind::FdWrite, .. })),
            "print_str must reach Op::Prim FdWrite from source, got {:?}",
            ps.ops
        );
        // End-to-end: it prints "hello" (matching v0's native println).
        if let Some(out) = build_and_run("selfhost_println", &render_wasm_program(&prog)) {
            assert_eq!(out, "hello");
        }
    }

    /// SEAMLESS v1=v0: a PLAIN `println` program — byte-identical to what runs on v0,
    /// with NO print_str defined — works on v1 because the self-hosted print_str
    /// runtime is AUTO-LINKED (the v1 linker step). Two printlns include the newline
    /// between them (print_str writes string + "\n" via two single-iovec fd_writes).
    #[test]
    fn plain_println_auto_links_and_prints() {
        let prog = lower_source(
            "fn main() -> Unit = {\n  println(\"line one\")\n  println(\"line two\")\n}\n",
        );
        // print_str was auto-linked (the source did not define it).
        assert!(
            prog.functions.iter().any(|f| f.name == "print_str"),
            "the self-hosted print_str must be auto-linked"
        );
        // build_and_run trims the trailing newline; the MIDDLE newline must remain.
        if let Some(out) = build_and_run("plain_println", &render_wasm_program(&prog)) {
            assert_eq!(out, "line one\nline two");
        }
    }

