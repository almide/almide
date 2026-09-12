// `include!`d part of wasi_p3.rs (codopsy max-lines split, mechanical text move —
// shares the parent module's imports and items; nothing here is pub beyond the parent).

pub fn to_p3(bytes: &[u8], wants_http: bool) -> anyhow::Result<Vec<u8>> {
    // The vendored WIT first: the fs shim's layout facts derive from it,
    // so a WIT/shim drift refuses to emit instead of corrupting stores.
    let mut resolve = wit_parser::Resolve::default();
    for (name, text) in [
        ("clocks.wit", include_str!("../wit/p3/deps/clocks/package.wit")),
        ("random.wit", include_str!("../wit/p3/deps/random/package.wit")),
        ("cli.wit", include_str!("../wit/p3/deps/cli/package.wit")),
        ("filesystem.wit", include_str!("../wit/p3/deps/filesystem/package.wit")),
        ("http.wit", include_str!("../wit/p3/deps/http/package.wit")),
    ] {
        resolve
            .push_str(name, text)
            .map_err(|e| anyhow::anyhow!("wit {name}: {e}"))?;
    }
    let pkg = resolve
        .push_str("world.wit", include_str!("../wit/p3/world.wit"))
        .map_err(|e| anyhow::anyhow!("wit world: {e}"))?;
    // The http-importing world only when the module's op set reaches the
    // http family — a non-http component must not demand `-S http=y`.
    let world_name = if wants_http { "p3-command-http" } else { "p3-command" };
    let world = resolve
        .select_world(&[pkg], Some(world_name))
        .map_err(|e| anyhow::anyhow!("world: {e}"))?;
    let abi = fs_abi(&resolve)?;
    let habi = if wants_http { Some(http_abi(&resolve)?) } else { None };
    // The stat result's WIT-derived footprint must fit its park slot.
    assert!(STATRET + abi.stat_size <= MSG_NOENT, "STATRET reaches the messages");

    let parsed = parse_module(bytes)?;
    let Parsed {
        mut types,
        func_types,
        tables,
        old_mem_min,
        old_mem_max,
        parsed_globals,
        global_count,
        heap_global,
        exports: _,
        main_index,
        elements,
        mut data,
        bodies,
    } = parsed;
    let main_index = main_index.ok_or_else(|| anyhow::anyhow!("no main export"))?;
    let heap_global = heap_global.ok_or_else(|| anyhow::anyhow!("no __heap export"))?;
    let n_funcs = func_types.len() as u32;
    let n_imports = if wants_http { IMPORTS_HTTP } else { IMPORTS };
    let shift = n_imports - 5;
    let shim_base = n_imports + n_funcs;
    // Shim order mirrors the almide.* import order (println, eprintln,
    // exit, fs_call, host_read), then cabi_realloc, run, callback, the
    // #2119 reservation pair, and the optional http shim.
    let f_realloc = shim_base + 5;
    let f_run = shim_base + 6;
    let f_callback = shim_base + 7;
    let f_reserve = shim_base + 8;
    let f_alloc = shim_base + 9;
    let f_eprintln = shim_base + 1;

    let heap_init = parsed_globals[heap_global as usize]
        .1
        .ok_or_else(|| anyhow::anyhow!("__heap init not i32"))? as u32 as u64;
    let park: u64 = heap_init;
    // Globals: originals, then plen, ppos, the stream state: stdout/stderr
    // writable ends + completion futures, stdin readable + its future.
    let (g_plen, g_ppos) = (global_count, global_count + 1);
    let (g_out_tx, g_out_fut) = (global_count + 2, global_count + 3);
    let (g_err_tx, g_err_fut) = (global_count + 4, global_count + 5);
    let (g_in_rx, g_in_fut) = (global_count + 6, global_count + 7);
    let g_pre = global_count + 8; // first preopen descriptor (lazy, -1 = unresolved)
    let g_wset = global_count + 9; // the ONE waitable set (lazy, -1)
    let g_slots = global_count + 10; // fan slot-table base (0 = unallocated)
    let g_slotn = global_count + 11; // slot high-water mark
    let mut globals = GlobalSection::new();
    for (idx, (gt, i32v, i64v, f64v)) in parsed_globals.iter().enumerate() {
        let init = if idx as u32 == heap_global {
            ConstExpr::i32_const((heap_init + PARK_SPAN) as i32)
        } else if let Some(v) = i32v {
            ConstExpr::i32_const(*v)
        } else if let Some(v) = i64v {
            ConstExpr::i64_const(*v)
        } else {
            ConstExpr::f64_const(f64::from_bits(f64v.expect("global init")).into())
        };
        globals.global(*gt, &init);
    }
    let mutable_i32 = GlobalType { val_type: ValType::I32, mutable: true, shared: false };
    globals.global(mutable_i32, &ConstExpr::i32_const(0)); // plen
    globals.global(mutable_i32, &ConstExpr::i32_const(0)); // ppos
    for _ in 0..8 {
        globals.global(mutable_i32, &ConstExpr::i32_const(-1)); // stream state + g_pre + g_wset
    }
    globals.global(mutable_i32, &ConstExpr::i32_const(0)); // g_slots
    globals.global(mutable_i32, &ConstExpr::i32_const(0)); // g_slotn

    // Canonical-ABI core types.
    let t_exit = type_index(&mut types, &[ValType::I32], &[]);
    let t_call = type_index(&mut types, &[ValType::I32], &[ValType::I32]);
    let t_new = type_index(&mut types, &[], &[ValType::I64]);
    let t_rw = type_index(
        &mut types,
        &[ValType::I32, ValType::I32, ValType::I32],
        &[ValType::I32],
    );
    let t_fut_read = type_index(&mut types, &[ValType::I32, ValType::I32], &[ValType::I32]);
    let t_drop = type_index(&mut types, &[ValType::I32], &[]);
    let t_retptr = type_index(&mut types, &[ValType::I32], &[]);
    let t_random = type_index(&mut types, &[ValType::I64, ValType::I32], &[]);
    // Shim types (the almide.* signatures) + realloc + run + callback.
    let t_print = type_index(&mut types, &[ValType::I32, ValType::I32], &[]);
    let t_fs = type_index(&mut types, &[ValType::I32; 5], &[ValType::I64]);
    let t_hread = type_index(&mut types, &[ValType::I32], &[]);
    let t_realloc = type_index(&mut types, &[ValType::I32; 4], &[ValType::I32]);
    let t_status = type_index(&mut types, &[], &[ValType::I32]);
    let t_callback = type_index(&mut types, &[ValType::I32; 3], &[ValType::I32]);
    // fs read-surface core shapes (sync lowers).
    let t_open = type_index(&mut types, &[ValType::I32; 7], &[]);
    let t_stat = type_index(&mut types, &[ValType::I32; 5], &[]);
    let t_rvs = type_index(
        &mut types,
        &[ValType::I32, ValType::I64, ValType::I32],
        &[],
    );
    let t_aopen = type_index(&mut types, &[ValType::I32; 2], &[ValType::I32]);
    // write-via-stream: (self, stream, offset:i64) -> future — the future
    // returns DIRECTLY (1 flat result), as stdio's write-via-stream.
    let t_wvs = type_index(
        &mut types,
        &[ValType::I32, ValType::I32, ValType::I64],
        &[ValType::I32],
    );
    // append-via-stream: (self, stream) -> future.
    let t_avs = type_index(&mut types, &[ValType::I32; 2], &[ValType::I32]);
    // path ops: (self, ptr, len, retptr).
    let t_pathop = type_index(&mut types, &[ValType::I32; 4], &[]);
    let t_ws_new = type_index(&mut types, &[], &[ValType::I32]);
    let t_ws_join = type_index(&mut types, &[ValType::I32; 2], &[]);
    let t_ws_wait = type_index(&mut types, &[ValType::I32; 2], &[ValType::I32]);
    // http client core shapes (#1710 PR B).
    let t_set4 = type_index(&mut types, &[ValType::I32; 4], &[ValType::I32]);
    let t_set5 = type_index(&mut types, &[ValType::I32; 5], &[ValType::I32]);
    let t_consume = type_index(&mut types, &[ValType::I32; 3], &[]);
    // [method]fields.append(self, name ptr/len, value ptr/len, retptr) —
    // `result<_, header-error>` carries a payload, so it lands via retptr.
    let t_append = type_index(&mut types, &[ValType::I32; 6], &[]);

    let mut type_sec = TypeSection::new();
    for (p, r) in &types {
        type_sec.ty().function(p.iter().copied(), r.iter().copied());
    }

    // The import table, POSITION-CHECKED against the I_* constants: the
    // list is the single source of order, and a drifted constant fails
    // the build of every artifact instead of silently calling the wrong
    // host function.
    let cli_out = "wasi:cli/stdout@0.3.0";
    let cli_err = "wasi:cli/stderr@0.3.0";
    let cli_in = "wasi:cli/stdin@0.3.0";
    let fs_types = "wasi:filesystem/types@0.3.0";
    let import_list: &[(u32, &str, &str, u32)] = &[
        (I_EXIT, "wasi:cli/exit@0.3.0", "exit", t_exit),
        (I_OUT_CALL, cli_out, "write-via-stream", t_call),
        (I_OUT_NEW, cli_out, "[stream-new-0]write-via-stream", t_new),
        (I_OUT_WRITE, cli_out, "[stream-write-0]write-via-stream", t_rw),
        (I_OUT_DROP_TX, cli_out, "[stream-drop-writable-0]write-via-stream", t_drop),
        (I_OUT_FUT_READ, cli_out, "[future-read-1]write-via-stream", t_fut_read),
        (I_ERR_CALL, cli_err, "write-via-stream", t_call),
        (I_ERR_NEW, cli_err, "[stream-new-0]write-via-stream", t_new),
        (I_ERR_WRITE, cli_err, "[stream-write-0]write-via-stream", t_rw),
        (I_ERR_DROP_TX, cli_err, "[stream-drop-writable-0]write-via-stream", t_drop),
        (I_ERR_FUT_READ, cli_err, "[future-read-1]write-via-stream", t_fut_read),
        (I_STDIN_OPEN, cli_in, "read-via-stream", t_retptr),
        (I_STDIN_READ, cli_in, "[stream-read-0]read-via-stream", t_rw),
        (I_STDIN_DROP_RX, cli_in, "[stream-drop-readable-0]read-via-stream", t_drop),
        (I_STDIN_DROP_FUT, cli_in, "[future-drop-readable-1]read-via-stream", t_drop),
        (I_CLOCK_NOW, "wasi:clocks/system-clock@0.3.0", "now", t_retptr),
        (I_RANDOM, "wasi:random/random@0.3.0", "get-random-bytes", t_random),
        (I_TASK_RETURN, "[export]wasi:cli/run@0.3.0", "[task-return]run", t_exit),
        (I_FS_PRE, "wasi:filesystem/preopens@0.3.0", "get-directories", t_retptr),
        (I_FS_OPEN, fs_types, "[method]descriptor.open-at", t_open),
        (I_FS_STAT, fs_types, "[method]descriptor.stat-at", t_stat),
        (I_FS_RVS, fs_types, "[method]descriptor.read-via-stream", t_rvs),
        (I_FS_SREAD, fs_types, "[stream-read-0][method]descriptor.read-via-stream", t_rw),
        (I_FS_SDROP, fs_types, "[stream-drop-readable-0][method]descriptor.read-via-stream", t_drop),
        (I_FS_FDROP, fs_types, "[future-drop-readable-1][method]descriptor.read-via-stream", t_drop),
        (I_FS_RESDROP, fs_types, "[resource-drop]descriptor", t_drop),
        (I_FS_AOPEN, fs_types, "[async-lower][method]descriptor.open-at", t_aopen),
        (I_WS_NEW, "$root", "[waitable-set-new]", t_ws_new),
        (I_WS_JOIN, "$root", "[waitable-join]", t_ws_join),
        (I_WS_WAIT, "$root", "[waitable-set-wait]", t_ws_wait),
        (I_SUBTASK_DROP, "$root", "[subtask-drop]", t_drop),
        (I_WS_DROP, "$root", "[waitable-set-drop]", t_drop),
        (I_SUBTASK_CANCEL, "$root", "[subtask-cancel]", t_call),
        (I_FS_WVS, fs_types, "[method]descriptor.write-via-stream", t_wvs),
        (I_FS_AVS, fs_types, "[method]descriptor.append-via-stream", t_avs),
        (I_FS_WNEW, fs_types, "[stream-new-0][method]descriptor.write-via-stream", t_new),
        (I_FS_WWRITE, fs_types, "[stream-write-0][method]descriptor.write-via-stream", t_rw),
        (I_FS_WDROP, fs_types, "[stream-drop-writable-0][method]descriptor.write-via-stream", t_drop),
        (I_FS_WFUT, fs_types, "[future-read-1][method]descriptor.write-via-stream", t_fut_read),
        (I_FS_MKDIR, fs_types, "[method]descriptor.create-directory-at", t_pathop),
        (I_FS_UNLINK, fs_types, "[method]descriptor.unlink-file-at", t_pathop),
        (I_FS_RMDIR, fs_types, "[method]descriptor.remove-directory-at", t_pathop),
    ];
    assert_eq!(import_list.len() as u32, IMPORTS, "IMPORTS count drift");
    let http_types = "wasi:http/types@0.3.0";
    let http_client = "wasi:http/client@0.3.0";
    let http_import_list: &[(u32, &str, &str, u32)] = &[
        (I_HTTP_FIELDS_NEW, http_types, "[constructor]fields", t_ws_new),
        (I_HTTP_REQ_NEW, http_types, "[static]request.new", t_open),
        (I_HTTP_REQ_SNEW, http_types, "[stream-new-0][static]request.new", t_new),
        (
            I_HTTP_REQ_SWRITE,
            http_types,
            "[async-lower][stream-write-0][static]request.new",
            t_rw,
        ),
        (I_HTTP_REQ_SDROPW, http_types, "[stream-drop-writable-0][static]request.new", t_drop),
        (I_HTTP_REQ_FNEW, http_types, "[future-new-1][static]request.new", t_new),
        (
            I_HTTP_REQ_FWRITE,
            http_types,
            "[async-lower][future-write-1][static]request.new",
            t_fut_read,
        ),
        (I_HTTP_REQ_FDROPW, http_types, "[future-drop-writable-1][static]request.new", t_drop),
        (I_HTTP_REQ_SENTDROP, http_types, "[future-drop-readable-2][static]request.new", t_drop),
        (I_HTTP_SET_METHOD, http_types, "[method]request.set-method", t_set4),
        (I_HTTP_SET_SCHEME, http_types, "[method]request.set-scheme", t_set5),
        (I_HTTP_SET_AUTH, http_types, "[method]request.set-authority", t_set4),
        (I_HTTP_SET_PATH, http_types, "[method]request.set-path-with-query", t_set4),
        (I_HTTP_SEND, http_client, "[async-lower]send", t_fut_read),
        (I_HTTP_STATUS, http_types, "[method]response.get-status-code", t_call),
        (I_HTTP_CONSUME, http_types, "[static]response.consume-body", t_consume),
        (I_HTTP_CB_FNEW, http_types, "[future-new-0][static]response.consume-body", t_new),
        (I_HTTP_CB_FWRITE, http_types, "[future-write-0][static]response.consume-body", t_fut_read),
        (I_HTTP_CB_FDROPW, http_types, "[future-drop-writable-0][static]response.consume-body", t_drop),
        (I_HTTP_BODY_READ, http_types, "[stream-read-1][static]response.consume-body", t_rw),
        (I_HTTP_BODY_DROPR, http_types, "[stream-drop-readable-1][static]response.consume-body", t_drop),
        (I_HTTP_TRL_DROPR, http_types, "[future-drop-readable-2][static]response.consume-body", t_drop),
        (I_HTTP_REQ_DROP, http_types, "[resource-drop]request", t_drop),
        (I_HTTP_RESP_DROP, http_types, "[resource-drop]response", t_drop),
        (I_HTTP_FIELDS_DROP, http_types, "[resource-drop]fields", t_drop),
        (I_HTTP_FIELDS_APPEND, http_types, "[method]fields.append", t_append),
    ];
    assert_eq!(
        IMPORTS + http_import_list.len() as u32,
        IMPORTS_HTTP,
        "IMPORTS_HTTP count drift"
    );
    let imports = build_import_section(import_list, wants_http.then_some(http_import_list));

    let mut functions = FunctionSection::new();
    for ti in &func_types {
        functions.function(*ti);
    }
    // `$reserve` shares `exit`'s `(i32) -> ()` shape and `$alloc` shares
    // `cabi_realloc`'s, so the pair adds no type-section entry.
    for ti in
        [t_print, t_print, t_exit, t_fs, t_hread, t_realloc, t_status, t_callback, t_exit, t_realloc]
    {
        functions.function(ti);
    }
    if wants_http {
        functions.function(t_fs); // shim_http (the almide fs_call ABI)
    }

    let mut memories = MemorySection::new();
    memories.memory(MemoryType {
        minimum: old_mem_min + PARK_SPAN / 65536,
        // #1729: carry the heap-cap maximum through, span-shifted.
        maximum: old_mem_max.map(|m| m.max(old_mem_min) + PARK_SPAN / 65536),
        memory64: false,
        shared: false,
        page_size_log2: None,
    });

    let exports = {
        let mut e = wasm_encoder::ExportSection::new();
        e.export("memory", ExportKind::Memory, 0);
        e.export("[async-lift]wasi:cli/run@0.3.0#run", ExportKind::Func, f_run);
        e.export(
            "[callback][async-lift]wasi:cli/run@0.3.0#run",
            ExportKind::Func,
            f_callback,
        );
        e.export("cabi_realloc", ExportKind::Func, f_realloc);
        e
    };

    let mut code = CodeSection::new();
    let mut remap = Remap { shim_base, shift };
    for b in bodies {
        code.function(&reencode_body(&b, &mut remap, I_EXIT)?);
    }
    let g = P3Globals {
        park, f_alloc, g_plen, g_ppos, g_in_rx, g_in_fut, g_out_tx, g_out_fut,
        g_err_tx, g_err_fut, g_pre, g_wset, g_slots, g_slotn, f_reserve,
    };
    let out_port = PrintPort { g_tx: g_out_tx, g_fut: g_out_fut, call_import: I_OUT_CALL, new_import: I_OUT_NEW, write_import: I_OUT_WRITE };
    let err_port = PrintPort { g_tx: g_err_tx, g_fut: g_err_fut, call_import: I_ERR_CALL, new_import: I_ERR_NEW, write_import: I_ERR_WRITE };
    code.function(&shim_print(out_port, park, true));
    code.function(&shim_print(err_port, park, true));
    code.function(&shim_exit());
    let f_fs_self = shim_base + 3;
    let f_http = wants_http.then_some(shim_base + 10);
    code.function(&shim_fs_call(g, &abi, f_fs_self, f_http));
    code.function(&shim_host_read(g_plen, g_ppos));
    code.function(&shim_cabi_realloc(heap_global));
    code.function(&shim_run(main_index + shift, g));
    code.function(&shim_callback());
    code.function(&shim_reserve(
        heap_global,
        f_eprintln,
        I_EXIT,
        (park + MSG_OOM) as u32,
        OOM_MSG.len() - 1,
    ));
    code.function(&shim_realloc_checked(f_reserve, f_realloc));
    if let Some(h) = habi.as_ref() {
        code.function(&shim_http(park, g_plen, g_ppos, f_alloc, h));
    }

    // Elements re-encode through the Remap (#1716): the import shift must
    // move funcref table entries too (#1688's silent class).
    let mut element_sec = wasm_encoder::ElementSection::new();
    for e in elements {
        use wasm_encoder::reencode::Reencode as _;
        remap
            .parse_element(&mut element_sec, e)
            .map_err(|e| anyhow::anyhow!("element reencode: {e:?}"))?;
    }

    data.active(0, &ConstExpr::i32_const((park + MSG) as i32), UNSUPPORTED_MSG.iter().copied());
    data.active(0, &ConstExpr::i32_const((park + MSG_OOM) as i32), OOM_MSG.iter().copied());
    for (off, msg) in [
        (MSG_NOENT, E_NOENT),
        (MSG_ACCES, E_ACCES),
        (MSG_ISDIR, E_ISDIR),
        (MSG_GEN, E_GEN),
        (MSG_NOPRE, E_NOPRE),
    ] {
        data.active(0, &ConstExpr::i32_const((park + off) as i32), msg.iter().copied());
    }
    if wants_http {
        data.active(0, &ConstExpr::i32_const((park + MSG_HTTP) as i32), E_HTTP.iter().copied());
        data.active(0, &ConstExpr::i32_const((park + MSG_CLEN) as i32), E_CLEN.iter().copied());
    }

    let mut m = Module::new();
    m.section(&type_sec)
        .section(&imports)
        .section(&functions)
        .section(&tables)
        .section(&memories)
        .section(&globals)
        .section(&exports)
        .section(&element_sec)
        .section(&code)
        .section(&data);
    let mut core = m.finish();
    wasmparser::validate(&core)?;

    wit_component::embed_component_metadata(
        &mut core,
        &resolve,
        world,
        wit_component::StringEncoding::UTF8,
    )
    .map_err(|e| anyhow::anyhow!("embed: {e}"))?;
    let component = wit_component::ComponentEncoder::default()
        .module(&core)
        .map_err(|e| anyhow::anyhow!("module: {e}"))?
        .encode()
        .map_err(|e| anyhow::anyhow!("encode: {e}"))?;
    Ok(component)
}

/// The import section in declaration order, each entry asserted against the
/// index constant it must land on (an `I_*` drift fails loudly here rather
/// than as a mis-wired call). Extracted from `to_p3` (codopsy cc 22).
fn build_import_section(
    import_list: &[(u32, &str, &str, u32)],
    http_import_list: Option<&[(u32, &str, &str, u32)]>,
) -> ImportSection {
    let mut imports = ImportSection::new();
    for (k, (want, m, n, t)) in import_list.iter().enumerate() {
        assert_eq!(k as u32, *want, "import order drift at {m}#{n}");
        imports.import(m, n, EntityType::Function(*t));
    }
    if let Some(http) = http_import_list {
        for (k, (want, m, n, t)) in http.iter().enumerate() {
            assert_eq!(IMPORTS + k as u32, *want, "http import order drift at {m}#{n}");
            imports.import(m, n, EntityType::Function(*t));
        }
    }
    imports
}
