impl WasmEmitter {
    fn new() -> Self {
        WasmEmitter {
            layout_reg: engine::LayoutRegistry::new(),
            types: Vec::new(),
            type_map: HashMap::new(),
            imports: Vec::new(),
            num_imports: 0,
            next_func_idx: 0,
            func_map: HashMap::new(),
            module_names: Vec::new(),
            intrinsic_symbol_to_fn: HashMap::new(),
            func_type_indices: HashMap::new(),
            compiled: Vec::new(),
            strings: HashMap::new(),
            // First byte is newline at NEWLINE_OFFSET
            data_bytes: vec![0x0A],
            case_tables: None,
            libm_tables: None,
            case_table_bytes: 0,
            rt: RuntimeFuncs {
                fd_write: 0, alloc: 0, rc_inc: 0, rc_dec: 0, cow_check: 0,
                heap_save: 0, heap_restore: 0, alloc_pinned: 0, heap_start_global: 0,
                println_str: 0, println_int: 0,
                int_to_string: 0, float_to_string: 0,
                float_parse: 0, float_to_fixed: 0, float_pow: 0,
                math_sin: 0, math_cos: 0, math_tan: 0,
                math_log: 0, math_log10: 0, math_log2: 0, math_exp: 0,
                bytes_f16_to_f64: 0,
                base64_encode: 0, base64_decode: 0,
                base64_encode_url: 0, base64_decode_url: 0,
                hex_encode: 0, hex_encode_upper: 0, hex_decode: 0,
                concat_str: 0,
                div_trap: 0,
                string_append: 0,
                string_alloc: 0,
                concat_list: 0,
                list_eq: 0, mem_eq: 0, list_list_str_cmp: 0,
                option_eq_i64: 0, option_eq_str: 0,
                result_eq_i64_str: 0, int_parse: 0, int_from_hex: 0,
                string: StringRuntime {
                    eq: 0, contains: 0, trim: 0,
                    slice: 0, reverse: 0, repeat: 0, index_of: 0,
                    replace: 0, split: 0, join: 0, count: 0,
                    pad_start: 0, pad_end: 0,
                    trim_start: 0, trim_end: 0,
                    to_upper: 0, to_lower: 0,
                    chars: 0, lines: 0,
                    from_bytes: 0, to_bytes: 0,
                    replace_first: 0, last_index_of: 0,
                    strip_prefix: 0, strip_suffix: 0,
                    is_digit: 0, is_alpha: 0, is_alnum: 0,
                    is_whitespace: 0, is_unicode_ws: 0, utf8_classify: 0, is_upper: 0, is_lower: 0,
                    cmp: 0,
                    char_count: 0,
                    cp_of_byte: 0,
                    run_length_encode: 0,
                    utf8_width: 0,
                    utf8_scalar: 0,
                    utf8_byte_of_cp: 0,
                    utf8_snap: 0,
                    prop_alpha: 0, prop_alnum: 0, prop_upper: 0, prop_lower: 0,
                    prop_alpha_table: 0, prop_alnum_table: 0,
                    prop_upper_table: 0, prop_lower_table: 0,
                    utf8_emit_scalar: 0,
                    case_map_lookup: 0,
                    set_member: 0,
                    final_sigma: 0,
                    str_case_map: 0,
                    capitalize: 0,
                },
                value_stringify: 0,
                value_eq: 0,
                json_stringify_pretty: 0,
                json_escape_string: 0,
                json_parse: 0,
                json_parse_at: 0,
                json_get_path: 0,
                json_set_path: 0,
                json_remove_path: 0,
                regex: rt_regex::RegexRuntime::default(),
                clock_time_get: 0,
                proc_exit: 0,
                random_get: 0,
                path_open: 0,
                fd_read: 0,
                fd_close: 0,
                fd_seek: 0,
                fd_filestat_get: 0,
                path_filestat_get: 0,
                path_create_directory: 0,
                path_rename: 0,
                path_unlink_file: 0,
                path_remove_directory: 0,
                fd_readdir: 0,
                fd_prestat_get: 0,
                fd_prestat_dir_name: 0,
                args_sizes_get: 0,
                args_get: 0,
                environ_sizes_get: 0,
                environ_get: 0,
                resolve_path: 0,
                init_preopen_dirs: 0,
                dragon: rt_dragon::DragonRuntime::default(),
                decfloat: rt_dec2flt::DecFloatRuntime::default(),
                libm: rt_libm::LibmRuntime::default(),
                repr_str: 0,
                float_display: 0,
            },
            heap_ptr_global: runtime::HEAP_PTR_GLOBAL_IDX,
            free_list_global: runtime::FREE_LIST_GLOBAL_IDX,
            preopen_table_global: runtime::PREOPEN_TABLE_GLOBAL_IDX,
            preopen_count_global: runtime::PREOPEN_COUNT_GLOBAL_IDX,
            top_let_globals: HashMap::new(),
            def_globals: HashMap::new(),
            top_let_globals_by_name: HashMap::new(),
            top_let_init: Vec::new(),
            next_global: runtime::HEAP_START_GLOBAL_IDX + 1, // fixed globals end at HEAP_START
            func_table: Vec::new(),
            func_to_table_idx: HashMap::new(),
            record_fields: BTreeMap::new(),
            named_records: BTreeMap::new(),
            variant_info: BTreeMap::new(),
            default_fields: HashMap::new(),
            lambdas: Vec::new(),
            fn_ref_wrappers: HashMap::new(),
            lambda_counter: std::cell::Cell::new(0),
            effect_fns: HashSet::new(),
            mutable_captures: HashSet::new(),
            needs_cow: HashSet::new(),
            global_alias: HashMap::new(),
            eq_funcs: HashMap::new(),
            repr_funcs: BTreeMap::new(),
            repr_func_tys: BTreeMap::new(),
            user_exports: Vec::new(),
            needs_fs: false,
        }
    }

    /// Register a function type, returning its (deduplicated) type index.
    pub fn register_type(&mut self, params: Vec<ValType>, results: Vec<ValType>) -> u32 {
        let key = (params.clone(), results.clone());
        if let Some(&idx) = self.type_map.get(&key) {
            return idx;
        }
        let idx = self.types.len() as u32;
        self.types.push((params, results));
        self.type_map.insert(key, idx);
        idx
    }

    /// Register a WASI import function, returning its function index.
    pub fn register_import(&mut self, _type_idx: u32) -> u32 {
        let idx = self.next_func_idx;
        self.next_func_idx += 1;
        self.num_imports += 1;
        idx
    }

    /// Register a defined function by name, returning its function index.
    pub fn register_func(&mut self, name: &str, type_idx: u32) -> u32 {
        let idx = self.next_func_idx;
        self.next_func_idx += 1;
        self.func_map.insert(name.to_string(), idx);
        self.func_type_indices.insert(idx, type_idx);
        idx
    }

    /// Add a compiled function body.
    /// THE ctor-field lookup (#525): a registration MISS is a compiler bug
    /// (the registration↔lookup name-skew class — cross-module/mangled ctor
    /// names), not an empty layout. `unwrap_or_default` at the call sites
    /// conflated None with Some(empty): every pattern bind silently read
    /// zero-initialized locals, record map-keys never matched, repr printed
    /// empty. Registered-but-empty stays legal (unit-ish payloads).
    pub fn fields_of(&self, ctor: &str) -> Vec<(String, almide_lang::types::Ty)> {
        self.record_fields.get(ctor).cloned().unwrap_or_else(|| panic!(
            "[ICE] constructor `{}` has no registered field layout (#525 class)",
            ctor
        ))
    }

    pub fn add_compiled(&mut self, compiled: CompiledFunc) {
        if let Some(expected) = compiled.expected_func_idx {
            let landing = self.num_imports + self.compiled.len() as u32;
            assert_eq!(
                landing, expected,
                "[ICE] compiled body for registered func {} landing at index {} — \
                 registration and compile order diverged (#526); a same-signature \
                 neighbor swap binds the wrong body to a name and validates cleanly",
                expected, landing
            );
        }
        self.compiled.push(compiled);
    }

    /// Embed the oracle-derived Unicode case tables at the FRONT of `data_bytes`
    /// (immediately after the newline byte), recording their absolute addresses.
    ///
    /// Placement at the front is MANDATORY: it sits at a fixed low offset that
    /// never moves when interned string literals (appended later, during function
    /// compilation) are compacted by `eliminate_dead_data`. Must be called once,
    /// before any string is interned (asserted), so the baked `i32_const` offsets
    /// in the case runtime functions stay valid for the life of the module.
    fn embed_case_tables(&mut self) {
        debug_assert_eq!(
            self.data_bytes.len(), 1,
            "case tables must be embedded before any string interning"
        );
        let t = rt_string_case::generate_case_tables();

        fn pad4(db: &mut Vec<u8>) {
            while db.len() % 4 != 0 { db.push(0); }
        }
        fn push_u32s(db: &mut Vec<u8>, arr: &[u32]) -> u32 {
            pad4(db);
            let base = NEWLINE_OFFSET + db.len() as u32;
            for &x in arr { db.extend_from_slice(&x.to_le_bytes()); }
            base
        }
        fn push_bytes(db: &mut Vec<u8>, bytes: &[u8]) -> u32 {
            let base = NEWLINE_OFFSET + db.len() as u32;
            db.extend_from_slice(bytes);
            base
        }

        // For each map: place VALS first so its base is known, then bake the OFFS
        // array as ABSOLUTE addresses into VALS, then the KEYS search array.
        let db = &mut self.data_bytes;
        let upper_vals = push_bytes(db, &t.upper.vals);
        let upper_keys = push_u32s(db, &t.upper.keys);
        let upper_offs_abs: Vec<u32> = t.upper.val_offsets.iter().map(|o| upper_vals + o).collect();
        let upper_offs = push_u32s(db, &upper_offs_abs);

        let lower_vals = push_bytes(db, &t.lower.vals);
        let lower_keys = push_u32s(db, &t.lower.keys);
        let lower_offs_abs: Vec<u32> = t.lower.val_offsets.iter().map(|o| lower_vals + o).collect();
        let lower_offs = push_u32s(db, &lower_offs_abs);

        let cased = push_u32s(db, &t.cased);
        let ci = push_u32s(db, &t.case_ignorable);

        self.case_table_bytes = self.data_bytes.len() - 1;
        self.case_tables = Some(CaseTableOffsets {
            upper_keys, upper_offs, upper_n: t.upper.keys.len() as u32,
            lower_keys, lower_offs, lower_n: t.lower.keys.len() as u32,
            cased, cased_n: t.cased.len() as u32,
            ci, ci_n: t.case_ignorable.len() as u32,
        });
    }

    /// Embed the vendored-libm 2/pi (`IPIO2`) and `PIO2` constant tables into the
    /// FRONT protected region of `data_bytes`, immediately after the case tables
    /// (if any) and before any string is interned. Their absolute addresses are
    /// recorded and baked as `i32_const` into the `rt_libm` runtime, so — like the
    /// case tables — they must sit at a fixed low offset that `eliminate_dead_data`
    /// never moves. Adds to the protected `case_table_bytes` prefix.
    fn embed_libm_tables(&mut self) {
        debug_assert!(
            self.data_bytes.len() == 1 + self.case_table_bytes,
            "libm tables must be embedded right after case tables, before string interning"
        );
        fn pad8(db: &mut Vec<u8>) {
            while db.len() % 8 != 0 { db.push(0); }
        }
        // IPIO2: 690 × i32. PIO2: 8 × f64 (8-byte aligned for f64.load).
        let db = &mut self.data_bytes;
        let ipio2_base = NEWLINE_OFFSET + db.len() as u32;
        for &x in rt_libm::IPIO2.iter() {
            db.extend_from_slice(&x.to_le_bytes());
        }
        pad8(db);
        let pio2_base = NEWLINE_OFFSET + db.len() as u32;
        for &x in rt_libm::PIO2.iter() {
            db.extend_from_slice(&x.to_le_bytes());
        }
        self.case_table_bytes = self.data_bytes.len() - 1;
        self.libm_tables = Some(rt_libm::LibmTableOffsets { ipio2_base, pio2_base });
    }
}

/// Label tracking for break/continue in loops.
pub struct LoopLabels {
    pub break_depth: u32,
    pub continue_depth: u32,
}

/// RAII guard for WASM block nesting depth.
/// Created by `depth_push`/`depth_push_n`, consumed by `depth_pop`.
/// `#[must_use]` ensures the guard is not silently dropped.
#[must_use = "call depth_pop() to restore depth"]
pub struct DepthGuard(u32);

impl DepthGuard {
    /// The depth value at the point this guard was created (before push).
    pub fn saved(&self) -> u32 { self.0 }
}

/// Per-function compilation state.
pub struct FuncCompiler<'a> {
    pub emitter: &'a mut WasmEmitter,
    pub func: TrackedFunction,
    pub var_map: HashMap<u32, u32>,
    pub depth: u32,
    pub loop_stack: Vec<LoopLabels>,
    // Scratch local allocator
    pub scratch: scratch::ScratchAllocator,
    // Variable table for name lookups (pattern matching)
    pub var_table: &'a almide_ir::VarTable,
    // Return type for stub calls (set by emit_call before delegating to handlers)
    pub stub_ret_ty: Ty,
    // Module name of the function being compiled (for intra-module call resolution)
    pub current_module_name: Option<String>,
    /// EARLY-RETURN LEAK FIX: the running set of OWNED heap locals — pushed on a heap
    /// `Bind`, removed on its Perceus `RcDec` (so this mirrors Perceus's own liveness as
    /// the body is emitted, in scope-nested order). A `Try`/`Unwrap`/`Fan` Err-path
    /// `return_` jumps PAST the Perceus terminal rc_decs, so it first frees these (the
    /// ones live at that point) — else they leak on wasm (Rust `?` runs Drop). Excludes
    /// env-borrows + donate-only `__*` temps; the returned Err ptr is a scratch temp, not
    /// a member. See emit_early_return_decs + docs/roadmap/active/v0-unwrap-early-return-leak.md.
    pub live_heap: Vec<almide_ir::VarId>,
}

impl FuncCompiler<'_> {
    /// Push depth by 1. Returns a guard that must be passed to `depth_pop`.
    pub fn depth_push(&mut self) -> DepthGuard {
        let g = DepthGuard(self.depth);
        self.depth += 1;
        g
    }

    /// Push depth by N. Returns a guard that restores to the saved depth.
    pub fn depth_push_n(&mut self, n: u32) -> DepthGuard {
        let g = DepthGuard(self.depth);
        self.depth += n;
        g
    }

    /// Restore depth from guard. Debug-asserts that depth hasn't been corrupted.
    pub fn depth_pop(&mut self, guard: DepthGuard) {
        debug_assert!(
            self.depth > guard.0,
            "depth_pop: depth {} should be > saved {}",
            self.depth, guard.0,
        );
        self.depth = guard.0;
    }

    /// Write a freshly-(re)allocated heap object pointer back to the variable an
    /// in-place mutator (`list.push`, `map.insert`, `string.push`, …) operates on.
    ///
    /// Every such op may relocate its target (grow + realloc), so the new pointer
    /// must replace the old binding. There are three storage classes, and getting
    /// any of them wrong is silently wrong (the mutation is lost or, worse, the
    /// next read dereferences a stale pointer):
    ///
    /// - **mutable capture** — the local holds a *cell* pointer, not the object.
    ///   Store the new object pointer *into* the cell (`i32_store(0)`), preserving
    ///   the cell's identity so other closures sharing it observe the update. This
    ///   is the case that the per-op write-backs historically missed (Closure
    ///   Architecture v2, P6): they overwrote the local with the object pointer,
    ///   corrupting the cell so subsequent cell-deref reads returned garbage.
    /// - **local** — overwrite the local with the new pointer.
    /// - **top-level global** — overwrite the global.
    ///
    /// `new_ptr` is a local already holding the new object pointer. No-op when the
    /// target is not a bare `Var` (e.g. `foo().push(x)` has nowhere to write back).
    pub fn emit_mutator_writeback(&mut self, target: &almide_ir::IrExpr, new_ptr: u32) {
        // A field of a record (`list.push(b.xs, v)` on a `mut b`): store the
        // realloc'd pointer back into the field slot `[base + offset]`. The record
        // block is shared (passed by pointer), so the write persists to the caller
        // — the wasm counterpart of native's `&mut b.xs` (#703/#705). Without this
        // the new buffer is dropped and the field keeps the pre-realloc pointer.
        if let almide_ir::IrExprKind::Member { object, field } = &target.kind {
            if let Some(total_offset) = self.member_field_store_offset(object, field) {
                self.emit_expr(object);
                wasm!(self.func, {
                    i32_const(total_offset as i32); i32_add;
                    local_get(new_ptr);
                    i32_store(0);
                });
            }
            return;
        }
        let id = match &target.kind {
            almide_ir::IrExprKind::Var { id } => id.0,
            _ => return,
        };
        if self.emitter.mutable_captures.contains(&id) {
            if let Some(&local_idx) = self.var_map.get(&id) {
                wasm!(self.func, { local_get(local_idx); local_get(new_ptr); i32_store(0); });
            }
        } else if let Some(&local_idx) = self.var_map.get(&id) {
            wasm!(self.func, { local_get(new_ptr); local_set(local_idx); });
        } else if let Some((global_idx, _)) = self.lookup_global(almide_ir::VarId(id)) {
            wasm!(self.func, { local_get(new_ptr); global_set(global_idx); });
        } else {
            // #525 (A6): silently SKIPPING the write-back leaves the var
            // holding a stale pre-realloc pointer — this fn's own doc calls
            // that "silently wrong". Refuse loudly instead.
            panic!(
                "[ICE] mutator write-back target VarId {} resolved to neither local nor global (#525)",
                id
            );
        }
    }

    /// Copy-on-write guard for the in-place mutation of a heap local. If `id` is a
    /// COW target (copy-aliased + mutated; see AliasCowPass) and stored in a plain
    /// local, load it, call `__cow_check` (clones iff rc>1), and write the returned
    /// pointer back to the local BEFORE the mutation reads it. The alias keeps the
    /// old pointer (whose rc __cow_check decremented), so the mutation can no longer
    /// reach it.
    ///
    /// No-op when `id` is not a COW target → the direct mutation path is unchanged
    /// (byte-identical wasm for non-aliasing programs). Also skipped for shared-cell
    /// captures (deliberately reference-shared) and for vars without a plain local
    /// (globals don't alias at the source level; AliasCowPass excludes them).
    pub fn cow_if_needed(&mut self, id: u32) {
        if !self.emitter.needs_cow.contains(&id) { return; }
        if self.emitter.mutable_captures.contains(&id) { return; }
        let Some(&local_idx) = self.var_map.get(&id) else { return };
        let cow_check = self.emitter.rt.cow_check;
        wasm!(self.func, { local_get(local_idx); call(cow_check); local_set(local_idx); });
    }
}

// ── Public API ──────────────────────────────────────────────────────

/// Emit a WASM binary from a fully-certified IR program (WASI mode).
///
/// AlmidePerceusBelt: the sole public door to WASM emission accepts only a
/// [`Canonical`](super::Canonical) program, which is reachable only by refining
/// a [`Verified`](super::Verified) one (see `Canonical::certify`). So neither
/// RC-unverified nor non-canonical IR can reach emission. `emit` below is
/// `pub(crate)` so this stays the only entry — closing the prior bypass where a
/// caller could invoke `emit` directly and skip the gate.
pub fn emit_certified(canonical: super::Canonical<'_>) -> Vec<u8> {
    emit(canonical.0)
}
