pub(crate) fn emit(program: &IrProgram) -> Vec<u8> {
    let mut emitter = WasmEmitter::new();

    // Pre-scan: detect filesystem usage to conditionally include init_preopen_dirs
    emitter.needs_fs = program_uses_fs(program);

    // Copy the COW-target var set (AliasCowPass) onto the emitter as bare u32s,
    // mirroring `mutable_captures`. Read at every in-place mutation emit site.
    emitter.needs_cow = program.codegen_annotations.needs_cow.iter().map(|v| v.0).collect();
    emitter.global_alias = program.codegen_annotations.global_alias.iter()
        .map(|(k, v)| (k.0, v.0)).collect();

    // Phase 0: Collect `@intrinsic(symbol)` → (module, fn_name) from every
    // bundled stdlib source so the `RuntimeCall` fallback path can route
    // dispatch by the Almide fn name rather than by naively decoding the
    // runtime symbol. Needed when the runtime symbol differs from the
    // Almide fn name (e.g. `map.map` → `almide_rt_map_map_values`).
    {
        use almide_lang::ast::{AttrValue, Decl};
        for &mod_name in almide_lang::stdlib_info::BUNDLED_MODULES {
            let Some(source) = almide_lang::stdlib_info::bundled_source(mod_name) else { continue };
            let Some(parsed) = almide_lang::parse_cached(source) else { continue };
            for decl in &parsed.decls {
                let Decl::Fn { name, attrs, .. } = decl else { continue };
                let Some(attr) = attrs.iter().find(|a| a.name.as_str() == "intrinsic") else { continue };
                let Some(first) = attr.args.first() else { continue };
                let AttrValue::String { value: symbol } = &first.value else { continue };
                emitter.intrinsic_symbol_to_fn.insert(
                    symbol.clone(),
                    (mod_name.to_string(), name.to_string()),
                );
            }
        }
    }

    // Embed the Unicode case-mapping tables at the FRONT of the data section
    // (while data_bytes is still just the newline byte) when the program uses any
    // string case op. Gated to keep non-case-folding modules lean (~51KB tables).
    if program_uses_case_op(program) {
        emitter.embed_case_tables();
    }
    // Embed the libm 2/pi / PIO2 tables (front protected region, after case
    // tables, before any string interning) when the program uses trig.
    if program_uses_trig(program) {
        emitter.embed_libm_tables();
    }

    // Phase 1: Register types and function indices
    // Step 1a: WASI imports (must come first — all imports before any defined functions)
    runtime::register_runtime_imports(&mut emitter);

    // Store import info for fd_write
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "fd_write".to_string(),
        type_idx: emitter.types.iter().position(|(p, r)| {
            p == &[ValType::I32, ValType::I32, ValType::I32, ValType::I32]
                && r == &[ValType::I32]
        }).unwrap() as u32,
    });

    // Import clock_time_get: (id: i32, precision: i64, time_ptr: i32) -> i32
    let clock_type_idx = emitter.register_type(
        vec![ValType::I32, ValType::I64, ValType::I32],
        vec![ValType::I32],
    );
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "clock_time_get".to_string(),
        type_idx: clock_type_idx,
    });

    // Import proc_exit: (code: i32) -> ()
    let proc_exit_type_idx = emitter.register_type(vec![ValType::I32], vec![]);
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "proc_exit".to_string(),
        type_idx: proc_exit_type_idx,
    });

    // Import random_get: (buf: i32, len: i32) -> i32
    let random_get_type_idx = emitter.register_type(
        vec![ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "random_get".to_string(),
        type_idx: random_get_type_idx,
    });

    // Import path_open
    let path_open_type_idx = emitter.register_type(
        vec![
            ValType::I32, ValType::I32, ValType::I32, ValType::I32,
            ValType::I32, ValType::I64, ValType::I64, ValType::I32,
            ValType::I32,
        ],
        vec![ValType::I32],
    );
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "path_open".to_string(),
        type_idx: path_open_type_idx,
    });

    // Import fd_read
    let fd_read_type_idx = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "fd_read".to_string(),
        type_idx: fd_read_type_idx,
    });

    // Import fd_close
    let fd_close_type_idx = emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "fd_close".to_string(),
        type_idx: fd_close_type_idx,
    });

    // Import fd_seek
    let fd_seek_type_idx = emitter.register_type(
        vec![ValType::I32, ValType::I64, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "fd_seek".to_string(),
        type_idx: fd_seek_type_idx,
    });

    // Import fd_filestat_get
    let fd_filestat_get_type_idx = emitter.register_type(
        vec![ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "fd_filestat_get".to_string(),
        type_idx: fd_filestat_get_type_idx,
    });

    // Import path_filestat_get
    let path_filestat_get_type_idx = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "path_filestat_get".to_string(),
        type_idx: path_filestat_get_type_idx,
    });

    // Import path_create_directory
    let path_create_directory_type_idx = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "path_create_directory".to_string(),
        type_idx: path_create_directory_type_idx,
    });

    // Import path_rename
    let path_rename_type_idx = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "path_rename".to_string(),
        type_idx: path_rename_type_idx,
    });

    // Import path_unlink_file
    let path_unlink_file_type_idx = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "path_unlink_file".to_string(),
        type_idx: path_unlink_file_type_idx,
    });

    // Import path_remove_directory
    let path_remove_directory_type_idx = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "path_remove_directory".to_string(),
        type_idx: path_remove_directory_type_idx,
    });

    // Import fd_prestat_get
    let fd_prestat_get_type_idx = emitter.register_type(
        vec![ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "fd_prestat_get".to_string(),
        type_idx: fd_prestat_get_type_idx,
    });

    // Import fd_prestat_dir_name
    let fd_prestat_dir_name_type_idx = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "fd_prestat_dir_name".to_string(),
        type_idx: fd_prestat_dir_name_type_idx,
    });

    // Import fd_readdir
    let fd_readdir_type_idx = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32, ValType::I64, ValType::I32],
        vec![ValType::I32],
    );
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "fd_readdir".to_string(),
        type_idx: fd_readdir_type_idx,
    });

    // Import args_sizes_get: (argc_ptr: i32, argv_buf_size_ptr: i32) -> errno
    let args_sizes_get_type_idx = emitter.register_type(
        vec![ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "args_sizes_get".to_string(),
        type_idx: args_sizes_get_type_idx,
    });

    // Import args_get: (argv_ptr: i32, argv_buf_ptr: i32) -> errno
    let args_get_type_idx = emitter.register_type(
        vec![ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "args_get".to_string(),
        type_idx: args_get_type_idx,
    });

    // Step 1b: @extern(wasm, ...) imports — must be registered before any
    // defined functions so import indices are contiguous at the start.
    // Scan both program.functions and module functions.
    let mut extern_wasm_set: HashSet<usize> = HashSet::new();
    for (i, func) in program.functions.iter().enumerate() {
        if let Some(attr) = func.extern_attrs.iter().find(|a| a.target.as_str() == "wasm") {
            let params: Vec<ValType> = func.params.iter()
                .filter_map(|p| values::ty_to_valtype(&p.ty))
                .collect();
            let results = values::ret_type(&func.ret_ty);
            let type_idx = emitter.register_type(params, results);
            let func_idx = emitter.register_import(type_idx);
            emitter.imports.push(ImportInfo {
                module: attr.module.as_str().to_string(),
                name: attr.function.as_str().to_string(),
                type_idx,
            });
            emitter.func_map.insert(func.name.to_string(), func_idx);
            if func.is_effect {
                emitter.effect_fns.insert(func.name.to_string());
            }
            extern_wasm_set.insert(i);
        }
    }
    // Module @extern(wasm) imports: key = (module_idx, func_idx)
    let mut extern_wasm_module_set: HashSet<(usize, usize)> = HashSet::new();
    for (mi, module) in program.modules.iter().enumerate() {
        emitter.module_names.push(module.name.to_string());
        let mod_ident = module.versioned_name
            .map(|v| v.to_string().replace('.', "_"))
            .unwrap_or_else(|| module.name.to_string().replace('.', "_"));
        for (fi, func) in module.functions.iter().enumerate() {
            if let Some(attr) = func.extern_attrs.iter().find(|a| a.target.as_str() == "wasm") {
                let params: Vec<ValType> = func.params.iter()
                    .filter_map(|p| values::ty_to_valtype(&p.ty))
                    .collect();
                let results = values::ret_type(&func.ret_ty);
                let type_idx = emitter.register_type(params, results);
                let func_idx = emitter.register_import(type_idx);
                emitter.imports.push(ImportInfo {
                    module: attr.module.as_str().to_string(),
                    name: attr.function.as_str().to_string(),
                    type_idx,
                });
                // Register by prefixed, qualified, and bare name for call dispatch
                let func_name_sanitized = func.name.to_string().replace(' ', "_").replace('-', "_").replace('.', "_");
                let prefixed_name = format!("almide_rt_{}_{}", mod_ident, func_name_sanitized);
                emitter.func_map.insert(prefixed_name, func_idx);
                // Qualified name: "{module}.{func}" — preferred for disambiguation
                let module_name = module.name.to_string();
                let qualified_name = format!("{}.{}", module_name, func.name);
                emitter.func_map.insert(qualified_name, func_idx);
                // Bare name: last-write-wins (later modules override earlier ones
                // so intra-module calls resolve to the local function, not an
                // imported module's function with the same name)
                let bare_name = func.name.to_string();
                emitter.func_map.insert(bare_name, func_idx);
                if func.is_effect {
                    let effect_prefixed = format!("almide_rt_{}_{}", mod_ident, func_name_sanitized);
                    emitter.effect_fns.insert(effect_prefixed);
                }
                extern_wasm_module_set.insert((mi, fi));
            }
        }
    }

    // Step 1c: Runtime defined functions (after all imports are registered)
    runtime::register_runtime_functions(&mut emitter);

    // Register type declarations (record and variant field layouts).
    // Include both the main program and all imported modules so nominal
    // types from `import mod` resolve during codegen.
    // Register module type_decls first, then program's own (self) type_decls.
    // This ensures self types win over same-named dependency types in record_fields.
    let all_type_decls = program.modules.iter().flat_map(|m| m.type_decls.iter())
        .chain(program.type_decls.iter());
    for td in all_type_decls {
        match &td.kind {
            almide_ir::IrTypeDeclKind::Record { fields } => {
                let field_list: Vec<(String, almide_lang::types::Ty)> = fields.iter()
                    .map(|f| (f.name.to_string(), f.ty.clone()))
                    .collect();
                // Index by sorted field-name set so a structural record literal
                // can recover its declared nominal name (mirrors native, #627).
                let mut sorted_names: Vec<String> = fields.iter().map(|f| f.name.to_string()).collect();
                sorted_names.sort();
                emitter.named_records.insert(sorted_names, td.name.to_string());
                emitter.record_fields.insert(td.name.to_string(), field_list);
            }
            almide_ir::IrTypeDeclKind::Variant { cases, .. } => {
                let mut variant_cases = Vec::new();
                for (tag, case) in cases.iter().enumerate() {
                    let fields: Vec<(String, almide_lang::types::Ty)> = match &case.kind {
                        almide_ir::IrVariantKind::Record { fields } => {
                            fields.iter().map(|f| (f.name.to_string(), f.ty.clone())).collect()
                        }
                        almide_ir::IrVariantKind::Tuple { fields } => {
                            fields.iter().enumerate()
                                .map(|(i, ty)| (format!("_{}", i), ty.clone()))
                                .collect()
                        }
                        almide_ir::IrVariantKind::Unit => vec![],
                    };
                    // Also register each case name in record_fields for field access
                    emitter.record_fields.insert(case.name.to_string(), fields.clone());
                    variant_cases.push(VariantCase {
                        name: case.name.to_string(),
                        tag: tag as u32,
                        fields,
                    });
                }
                emitter.variant_info.insert(td.name.to_string(), variant_cases);
            }
            almide_ir::IrTypeDeclKind::Alias { .. } => {
                // Alias types are erased by ConcretizeTypesPass — nothing to register.
            }
        }
    }

    // Stdlib runtime types that aren't declared as Almide records but must
    // resolve for Member access (e.g. `resp.status`). Field offsets must
    // match the layout chosen by the corresponding stdlib emit (see
    // calls_http.rs `response`/`json`).
    use almide_lang::types::Ty as _Ty;
    emitter.record_fields.insert("HttpResponse".to_string(), vec![
        ("status".to_string(),  _Ty::Int),     // i64 @ 0
        ("body".to_string(),    _Ty::String),  // i32 ptr @ 8
        ("headers".to_string(),
            _Ty::Applied(almide_lang::types::TypeConstructorId::List, vec![
                _Ty::Tuple(vec![_Ty::String, _Ty::String]),
            ])),                                // i32 ptr @ 12
    ]);
    emitter.record_fields.insert("HttpRequest".to_string(), vec![
        ("method".to_string(),  _Ty::String),
        ("path".to_string(),    _Ty::String),
        ("body".to_string(),    _Ty::String),
        ("headers".to_string(),
            _Ty::Applied(almide_lang::types::TypeConstructorId::List, vec![
                _Ty::Tuple(vec![_Ty::String, _Ty::String]),
            ])),
    ]);

    // Also register all anonymous record shapes found in the IR under synthetic
    // names so `emit_member`'s Unknown-type fallback (which searches
    // `record_fields` by field name) can resolve Member access on Lambda
    // parameters whose type inference left them as TypeVar/Unknown.
    register_anonymous_records(program, &mut emitter);

    // Build default_fields from type declarations
    for td in &program.type_decls {
        match &td.kind {
            almide_ir::IrTypeDeclKind::Variant { cases, .. } => {
                for case in cases {
                    if let almide_ir::IrVariantKind::Record { fields } = &case.kind {
                        for f in fields {
                            if let Some(def) = &f.default {
                                emitter.default_fields.insert(
                                    (case.name.to_string(), f.name.to_string()), def.clone()
                                );
                            }
                        }
                    }
                }
            }
            almide_ir::IrTypeDeclKind::Record { fields } => {
                for f in fields {
                    if let Some(def) = &f.default {
                        emitter.default_fields.insert(
                            (td.name.to_string(), f.name.to_string()), def.clone()
                        );
                    }
                }
            }
            _ => {}
        }
    }

    // Register top-level let bindings as globals
    for tl in &program.top_lets {
        let global_idx = emitter.next_global;
        emitter.next_global += 1;
        let vt = values::ty_to_valtype(&tl.ty).unwrap_or(ValType::I64);
        // Extract const value for direct initialization (store as i64 bits)
        let const_bits: i64 = match &tl.value.kind {
            almide_ir::IrExprKind::LitInt { value } => *value,
            almide_ir::IrExprKind::LitFloat { value } => value.to_bits() as i64,
            almide_ir::IrExprKind::LitBool { value } => *value as i64,
            _ => 0, // computed values default to 0
        };
        emitter.top_let_globals.insert(tl.var.0, (global_idx, vt));
        let name = program.var_table.get(tl.var).name.to_string();
        emitter.top_let_globals_by_name.insert(name, (global_idx, vt));
        emitter.top_let_init.push((global_idx, vt, const_bits));
    }
    // Also register module top_lets as globals so cross-module access (synthetic
    // Var with `ALMIDE_RT_<MOD>_<NAME>` name) can resolve at WASM emit time.
    for module in &program.modules {
        for tl in &module.top_lets {
            let global_idx = emitter.next_global;
            emitter.next_global += 1;
            let vt = values::ty_to_valtype(&tl.ty).unwrap_or(ValType::I64);
            let const_bits: i64 = match &tl.value.kind {
                almide_ir::IrExprKind::LitInt { value } => *value,
                almide_ir::IrExprKind::LitFloat { value } => value.to_bits() as i64,
                almide_ir::IrExprKind::LitBool { value } => *value as i64,
                _ => 0,
            };
            let name = program.var_table.get(tl.var).name.to_string();
            emitter.top_let_globals_by_name.insert(name.clone(), (global_idx, vt));
            emitter.top_let_globals.insert(tl.var.0, (global_idx, vt));
            emitter.top_let_init.push((global_idx, vt, const_bits));
            // Register by DefId for direct cross-package resolution
            if let Some(def_id) = tl.def_id {
                emitter.def_globals.insert(def_id.0, (global_idx, vt));
            }

            // Also register under the ALMIDE_RT_<MOD>_<NAME> synthetic name
            // that cross-module access creates during lowering. Without this,
            // the name-keyed fallback in expressions.rs can't find the global.
            let mod_name = module.name.as_str();
            if !mod_name.is_empty() {
                // Register ALMIDE_RT_<MOD>_<NAME> under multiple name forms:
                // - Full module path: ALMIDE_RT_SNAIDHM_WEB_GPU_STORAGE
                // - VarTable name as-is (may include _V0_ versioning)
                // - Leaf segment only: ALMIDE_RT_GPU_STORAGE
                let segments: Vec<&str> = mod_name.split('.').collect();
                let leaf = segments.last().copied().unwrap_or(mod_name);
                for alias in [mod_name, leaf] {
                    let synthetic = format!(
                        "ALMIDE_RT_{}_{}",
                        alias.to_uppercase().replace('.', "_"),
                        name.to_uppercase(),
                    );
                    emitter.top_let_globals_by_name.insert(synthetic, (global_idx, vt));
                }
                // Also register the VarTable name itself (handles versioned names like ALMIDE_RT_SNAIDHM_V0_...)
                if name.starts_with("ALMIDE_RT_") {
                    emitter.top_let_globals_by_name.insert(name.clone(), (global_idx, vt));
                    // Strip version suffix: ALMIDE_RT_SNAIDHM_V0_WEB_GPU_STORAGE → ALMIDE_RT_SNAIDHM_WEB_GPU_STORAGE
                    // so that the unversioned lowering synthetic name can also match.
                    let stripped = name.replacen("_V0_", "_", 1);
                    if stripped != name {
                        emitter.top_let_globals_by_name.insert(stripped, (global_idx, vt));
                    }
                }
            }
        }
    }

    // Register function signatures.
    // Library mode (no main): skip test functions so the WASM module
    // can be loaded without a _start entry point.
    let mut user_meta: Vec<u32> = Vec::new();
    let mut user_func_indices: Vec<u32> = Vec::new();
    let mut test_func_indices: Vec<(u32, String)> = Vec::new();
    let has_main = program.functions.iter().any(|f| f.name == "main" && !f.is_test);
    let has_tests = program.functions.iter().any(|f| f.is_test);
    let library_mode = !has_main && !has_tests;

    for (func_enum_idx, func) in program.functions.iter().enumerate() {
        // Skip @extern(wasm) — already registered as imports above
        if extern_wasm_set.contains(&func_enum_idx) {
            continue;
        }
        // Library mode: skip test functions entirely
        if library_mode && func.is_test {
            continue;
        }
        // Resolve param and ret types: Unknown/TypeVar can leak through from
        // lifted lambdas whose outer `Ty::Fn` had unresolved entries. Fall back
        // to VarTable (for params) and expression inspection (for ret).
        let params: Vec<ValType> = func.params.iter()
            .filter_map(|p| {
                if func.name.contains("closure") || func.name.contains("lambda") {
                }
                let pty = if p.ty.is_unresolved_structural() {
                    let vt_ty = &program.var_table.get(p.var).ty;
                    if !vt_ty.is_unresolved_structural() {
                        vt_ty.clone()
                    } else {
                        p.ty.clone()
                    }
                } else {
                    p.ty.clone()
                };
                values::ty_to_valtype(&pty)
            })
            .collect();
        if func.name.contains("closure") || func.name.contains("lambda") {
        }
        // Function return type: use declared ret_ty, fall back to body.ty
        // (concretized by the ConcretizeTypes pass) when declared is Unknown.
        let resolved_ret_ty = if func.ret_ty.is_unresolved() {
            func.body.ty.clone()
        } else {
            func.ret_ty.clone()
        };
        let results = values::ret_type(&resolved_ret_ty);
        let type_idx = emitter.register_type(params, results);
        // Test blocks already carry `TEST_NAME_PREFIX` from lowering so
        // they cannot collide with user fns — use the name as-is.
        let reg_name = func.name.to_string();
        let func_idx = emitter.register_func(&reg_name, type_idx);
        user_meta.push(type_idx);
        user_func_indices.push(func_idx);
        if func.is_test {
            test_func_indices.push((func_idx, func.display_name().to_string()));
        }
        if func.is_effect {
            emitter.effect_fns.insert(func.name.to_string());
        }
    }

    // Register module functions (user packages, not stdlib)
    let mut module_func_meta: Vec<(usize, usize, u32)> = Vec::new(); // (module_idx, func_idx, type_idx)
    for (mi, module) in program.modules.iter().enumerate() {
        let mod_ident = module.versioned_name
            .map(|v| v.to_string().replace('.', "_"))
            .unwrap_or_else(|| module.name.to_string().replace('.', "_"));
        for (fi, func) in module.functions.iter().enumerate() {
            // Skip @extern(wasm) — already registered as imports
            if extern_wasm_module_set.contains(&(mi, fi)) {
                continue;
            }
            // Skip test functions defined in dependency modules: they are
            // only relevant when running tests on that module directly,
            // not when it's imported by another file. Including them would
            // emit extra closures whose function-table layout can conflict
            // with the top-level program's own closures.
            if func.is_test {
                continue;
            }
            // Stdlib Unification Stage 1: `@inline_rust` / `@wasm_intrinsic`
            // bundled fns are dispatch-only declarations. On the WASM
            // target, the call dispatch still goes through
            // `calls_<module>.rs` (TOML-backed intrinsics); the bundled
            // fn's body (typically `_` / Hole) is never needed and would
            // fail to compile. Skip registration + emission.
            //
            // BUT a USER package's `@inline_rust` fn can carry a REAL Almide
            // body as its portable implementation (aes cfb8_encrypt) — there
            // the attr is a NATIVE-target optimization only, and wasm must
            // compile the body like any module fn. Previously these were
            // skipped too, so a cross-module call ICE'd with `no WASM
            // dispatch`. Skip only the dispatch-only (Hole-bodied) form;
            // `@wasm_intrinsic`/`@intrinsic` always skip (the wasm emitter
            // itself IS their implementation).
            let has_intrinsic_attr = func.attrs.iter().any(|a|
                matches!(a.name.as_str(), "wasm_intrinsic" | "intrinsic"));
            let has_inline_rust = func.attrs.iter().any(|a| a.name.as_str() == "inline_rust");
            let body_is_hole = matches!(func.body.kind,
                almide_ir::IrExprKind::Hole | almide_ir::IrExprKind::Todo { .. });
            if has_intrinsic_attr || (has_inline_rust && body_is_hole) {
                continue;
            }
            let func_name_sanitized = func.name.to_string().replace(' ', "_").replace('-', "_").replace('.', "_");
            // Test blocks carry `TEST_NAME_PREFIX` from lowering — no
            // additional conditional prefix needed here.
            let prefixed_name = format!("almide_rt_{}_{}", mod_ident, func_name_sanitized);
            let params: Vec<ValType> = func.params.iter()
                .filter_map(|p| values::ty_to_valtype(&p.ty))
                .collect();
            let results = values::ret_type(&func.ret_ty);
            let type_idx = emitter.register_type(params, results);
            let func_idx = emitter.register_func(&prefixed_name, type_idx);
            // Register qualified name: "{module}.{func}" for intra-module resolution
            let module_name_str = module.name.to_string();
            let qualified_name = format!("{}.{}", module_name_str, func.name);
            emitter.func_map.insert(qualified_name, func_idx);
            // Also register by bare name so lifted closures from this module
            // can call module-local functions. ClosureConversion lifts lambdas
            // from modules to program.functions, but their Named call targets
            // use the unqualified function name. Skip tests — tests must not
            // shadow user functions.
            if !func.is_test {
                let bare_name = func.name.to_string();
                if !emitter.func_map.contains_key(&bare_name) {
                    emitter.func_map.insert(bare_name, func_idx);
                }
            }
            module_func_meta.push((mi, fi, type_idx));
            user_func_indices.push(func_idx);
            if func.is_effect {
                emitter.effect_fns.insert(prefixed_name);
            }
        }
    }

    // Check if any top-level let needs dynamic initialization (non-constant values).
    // LitStr needs init because string pointers are resolved at runtime via data section.
    let is_dyn = |tl: &almide_ir::IrTopLet| !matches!(&tl.value.kind,
        almide_ir::IrExprKind::LitInt { .. } | almide_ir::IrExprKind::LitFloat { .. } |
        almide_ir::IrExprKind::LitBool { .. }
    );
    let needs_init = program.top_lets.iter().any(is_dyn)
        || program.modules.iter().any(|m| m.top_lets.iter().any(is_dyn));
    let init_globals_idx: Option<u32> = if needs_init {
        let void_ty = emitter.register_type(vec![], vec![]);
        let idx = emitter.register_func("__init_globals", void_ty);
        Some(idx)
    } else {
        None
    };

    // If no main but has tests, register a test runner as _start
    let test_runner_idx = if !has_main && !test_func_indices.is_empty() {
        let void_ty = emitter.register_type(vec![], vec![]);
        let idx = emitter.register_func("__test_runner", void_ty);
        Some(idx)
    } else {
        None
    };

    // If `main` exists, wrap it in a void `__main_runner` so the exported
    // `_start` is a clean WASI command `() -> ()`. `main` is an effect fn that
    // returns a Result (an i32 at the wasm boundary); exporting it directly as
    // `_start` leaves the entry non-void, so wasmtime runs it via `--invoke`
    // and prints the return value to stdout — corrupting any observable-output
    // capture (e.g. a cross-target equivalence diff). This occupies exactly the
    // `__test_runner` slot (mutually exclusive with it), inheriting the same
    // proven registration/compile ordering relative to closures and globals.
    let main_runner_idx = if has_main {
        let void_ty = emitter.register_type(vec![], vec![]);
        Some(emitter.register_func("__main_runner", void_ty))
    } else {
        None
    };

    // Phase 1.9: Reachability prune (#644). Compute which user/module function
    // bodies the entry surface can actually reach, BEFORE any body (or the
    // lambdas inside it) is scanned/compiled. The post-compile
    // `dce::eliminate_dead_code` cannot help here: an unreachable body that
    // references a native-only intrinsic (e.g. a matrix Q8 op with no WASM
    // runtime) PANICS the emitter while *compiling* it, long before DCE runs. So
    // unreachable bodies are emitted as `unreachable` stubs instead of compiled —
    // they can never be called, and the native target already drops them (the
    // Rust linker discards the dead fn). Roots: `main`, every exported `pub fn`,
    // every test (the test runner calls them), and every fn named by a top-level
    // `let` initializer (run by `__init_globals`). The set OVER-approximates
    // reachability (see reachability.rs), so a body is stubbed only when truly
    // unreachable. Computed before `pre_scan_closures` so that pass can skip the
    // lambdas of dead functions too (their bodies can equally hit a native-only
    // intrinsic) while keeping the lambda-table index aligned with
    // `compile_lambda_bodies` (both consult the SAME set, same iteration order).
    // Shared with the CLI native-only-op pre-check (lib.rs) so both agree which
    // bodies are dead — an unreachable native-only intrinsic must neither ICE the
    // emit nor fail the pre-check (#644).
    let reachable_fns = reachability::reachable_fn_names(program);
    // True iff a function registered under `keys` is reachable (any spelling).
    let is_reachable = |keys: &[String]| keys.iter().any(|k| reachable_fns.contains(k));

    // Pre-scan for lambdas and FnRefs — only these need element table entries.
    // (Previously all user functions were added unconditionally, bloating the
    // element table and preventing DCE from eliminating unused functions.)
    // Lambdas of unreachable functions (#644) get no table slot / body —
    // `compile_lambda_bodies` applies the identical reachable-fn filter to keep
    // the `emitter.lambdas[i]` ↔ body index alignment (Closure v2 P0).
    closures::pre_scan_closures(program, &mut emitter, &reachable_fns);

    // Pre-register variant deep-equality functions (must be before compilation starts)
    register_variant_eq_funcs(&mut emitter);

    // Pre-register per-type Almide-literal repr functions (recursive ADTs walk via
    // call, not inline expansion). Must reserve indices before compilation starts.
    register_repr_funcs(&mut emitter, program);

    // Phase 2: Compile function bodies (order must match registration order)
    runtime::compile_runtime(&mut emitter);

    // User + test functions (skip @extern(wasm) — they are imports, not defined)
    let mut user_idx = 0;
    for (func_enum_idx, func) in program.functions.iter().enumerate() {
        if extern_wasm_set.contains(&func_enum_idx) {
            continue;
        }
        if library_mode && func.is_test {
            continue;
        }
        let type_idx = user_meta[user_idx];
        // #644: unreachable top-level fns are emitted as trapping stubs — their
        // real body may reference a native-only intrinsic that would panic emit.
        // `main`/tests/exports are roots, so they are always reachable here.
        let compiled = if is_reachable(&reachability::registered_keys(None, func.name.as_str())) {
            // Pass init_globals_idx to main function so top-level lets get initialized
            let is_main = func.name == "main" && !func.is_test;
            let init_idx = if is_main { init_globals_idx } else { None };
            functions::compile_function_with_init(&mut emitter, func, &program.var_table, type_idx, init_idx)
        } else {
            CompiledFunc::trap_stub(type_idx)
        };
        emitter.add_compiled(compiled);
        user_idx += 1;
    }

    // Module functions (user packages). VarIds already point into the
    // unified `program.var_table` (see `pass_unify_var_tables`).
    for &(mi, fi, type_idx) in &module_func_meta {
        let module = &program.modules[mi];
        let func = &module.functions[fi];
        let mod_name = module.name.to_string();
        // #644: a merely-imported module's unused fns (e.g. tensor loaders that
        // call native-only matrix intrinsics) are stubbed when unreachable, so
        // importing such a module no longer forces WASM-compiling code the entry
        // never runs — the exact import-graph trap from the issue.
        let compiled = if is_reachable(&reachability::registered_keys(Some(&mod_name), func.name.as_str())) {
            functions::compile_module_function(&mut emitter, func, &program.var_table, type_idx, &mod_name)
        } else {
            CompiledFunc::trap_stub(type_idx)
        };
        emitter.add_compiled(compiled);
    }

    // Init globals (dynamic top-level let initialization, must come before test runner)
    if init_globals_idx.is_some() {
        compile_init_globals(&mut emitter, program);
    }

    // Test runner (if needed)
    if let Some(_runner_idx) = test_runner_idx {
        compile_test_runner(&mut emitter, &test_func_indices, init_globals_idx);
    }

    // Main runner (mirrors the test-runner slot; mutually exclusive with it).
    // `main` already runs `__init_globals` itself (init_idx passed at its
    // compilation), so the runner only calls `main` and drops its Result.
    if main_runner_idx.is_some() {
        let main_idx = *emitter.func_map.get("main")
            .expect("has_main implies a registered `main`");
        let main_func = program.functions.iter().find(|f| f.name == "main" && !f.is_test);
        // Only `effect fn main` returns a `Result` that can carry an unhandled
        // error. A plain `fn main` (`Unit`) cannot fail — never tag-check it, or
        // its `Unit` payload would be misread as an `Err` tag and abort every run.
        let is_effect = main_func.map(|f| f.is_effect).unwrap_or(false);
        let drop_count = main_func
            .map(|f| {
                let ret = if f.ret_ty.is_unresolved() { f.body.ty.clone() } else { f.ret_ty.clone() };
                values::ret_type(&ret).len()
            })
            .unwrap_or(0);
        compile_main_runner(&mut emitter, main_idx, drop_count, is_effect);
    }

    // Lambda bodies and FnRef wrappers
    closures::compile_lambda_bodies(program, &mut emitter, &reachable_fns);

    // Compile variant deep-equality functions (bodies, after all user code)
    compile_variant_eq_funcs(&mut emitter, &program.var_table);

    // Compile per-type repr function bodies (after eq funcs, same sorted-order
    // index/body contract). Order relative to eq funcs matches the registration
    // order above (eq funcs reserved first, then repr funcs).
    compile_repr_funcs(&mut emitter, &program.var_table);

    // Collect public user functions for WASM export (skip imports) BEFORE DCE.
    // A host-driven export (`render_frame`, `on_pointer_*`, any JS-called `pub fn`)
    // is often unreachable from `main`/`_start`; if DCE runs first it stubs the body
    // to `unreachable`, and the export then traps on the first host call (#457). By
    // populating `user_exports` here, DCE seeds these as roots and keeps their bodies.
    // @export(wasm, "symbol") overrides the export name; otherwise use fn name.
    for (func_enum_idx, func) in program.functions.iter().enumerate() {
        if extern_wasm_set.contains(&func_enum_idx) { continue; }
        if func.is_test { continue; }
        if !matches!(func.visibility, almide_ir::IrVisibility::Public) { continue; }
        if func.generics.as_ref().map_or(false, |g| !g.is_empty()) { continue; }
        if func.name.as_str() == "main" { continue; }
        let internal_name = func.name.to_string();
        let export_name = func.export_attrs.iter()
            .find(|a| a.target.as_str() == "wasm")
            .map(|a| a.symbol.to_string())
            .unwrap_or_else(|| internal_name.clone());
        emitter.user_exports.push((export_name, internal_name));
    }

    // Phase 2.5: Dead Code Elimination (exported `pub fn`s above are roots)
    let dce_count = dce::eliminate_dead_code(&mut emitter);

    // Phase 2.6: Dead Data Elimination — remove unreferenced string constants
    let _data_dce_bytes = dce::eliminate_dead_data(&mut emitter);

    // Phase 3: Assemble (DCE already ran in Phase 2.5: {} functions eliminated)
    let _ = dce_count;
    let bytes = assemble(&mut emitter);

    // Phase 4: Validate — mechanical guarantee of structural correctness.
    // ALWAYS-ON and FATAL (release-parity, completeness §10): this used to be
    // debug-only and print-only, and an invalid module (a Unit tail var
    // pushing a phantom value — caught by the §2 matrix gate) shipped through
    // it for as long as the shape existed, runnable only because wasm-opt
    // happened to repair it on machines that have binaryen installed. The
    // wasmtime-facing artifact must never depend on an optional external
    // sanitizer; validation costs milliseconds at these module sizes.
    if let Err(e) = wasmparser::validate(&bytes) {
        eprintln!("error: [COMPILER BUG] emitted WASM failed structural validation");
        eprintln!("  {e}");
        eprintln!("  The module would be rejected by any spec-compliant runtime. This is a");
        eprintln!("  compiler bug, not an error in your program.");
        eprintln!("  Please report this at https://github.com/almide/almide/issues");
        // Debug aid: dump the invalid module (name section included) so the
        // failing function can be identified with wasm-tools. Never a release
        // artifact — the path must be requested explicitly.
        if let Ok(p) = std::env::var("ALMIDE_DUMP_INVALID_WASM") {
            let _ = std::fs::write(&p, &bytes);
            eprintln!("  (invalid module dumped to {p})");
        }
        std::process::exit(1);
    }

    // Phase 5: RC balance verification — mathematical double-free prevention.
    //
    // For each user function, count RcDec statements in the IR and
    // call(rc_dec) instructions in the emitted WASM. If the WASM has MORE
    // rc_dec calls than the IR specifies (accounting for typed child drops),
    // it's a compiler bug that could cause double-free.
    //
    // This is a static, post-emit check — no runtime overhead, no function
    // index perturbation. Combined with PerceusVerifyPass (Lean 4 certified
    // IR-level balance) and Verified<'_> type-state gate, this closes the
    // gap between IR verification and WASM emission. ALWAYS-ON (§10): it is
    // a cheap per-function instruction count, and a violation in the
    // double-free direction must stop a release build too.
    verify_rc_balance(program, &emitter);

    bytes
}
