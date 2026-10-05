// `include!`d part of wasi_p3.rs (codopsy max-lines split, mechanical text move —
// shares the parent module's imports and items; nothing here is pub beyond the parent).

/// `host_ops` is the emitted module's op set (the build path's audit
/// input): the http block ships when it reaches ops 43..=50, and each env
/// service import when it names op 26 / 29 / 36.
pub fn to_p3(bytes: &[u8], host_ops: &[i32]) -> anyhow::Result<Vec<u8>> {
    to_p3_shaped(bytes, host_ops, false)
}

/// The stock serve export (#2659, C-375): the same transform, shaped as a
/// `wasi:http/handler@0.3.0` component (`wasi_p3_serve.rs`) — the world
/// imports no filesystem, `main` runs once per request from the handler.
pub fn to_p3_service(bytes: &[u8], host_ops: &[i32]) -> anyhow::Result<Vec<u8>> {
    to_p3_shaped(bytes, host_ops, true)
}

fn to_p3_shaped(bytes: &[u8], host_ops: &[i32], service: bool) -> anyhow::Result<Vec<u8>> {
    let wants_http = host_ops.iter().any(|op| (43..=50).contains(op));
    let http_types = wants_http || service;
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
    let world_name = if service {
        "p3-service"
    } else if wants_http {
        "p3-command-http"
    } else {
        "p3-command"
    };
    let world = resolve
        .select_world(&[pkg], Some(world_name))
        .map_err(|e| anyhow::anyhow!("world: {e}"))?;
    let abi = fs_abi(&resolve)?;
    let habi = if http_types { Some(http_abi(&resolve)?) } else { None };
    let sabi = if service { Some(serve_abi(&resolve)?) } else { None };
    // The stat result's WIT-derived footprint must fit its park slot.
    assert!(STATRET + abi.stat_size <= MSG2, "STATRET reaches the messages");

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
        foreign_imports: _,
    } = parsed;
    let main_index = main_index.ok_or_else(|| anyhow::anyhow!("no main export"))?;
    let heap_global = heap_global.ok_or_else(|| anyhow::anyhow!("no __heap export"))?;
    let n_funcs = func_types.len() as u32;
    let heap_init = parsed_globals[heap_global as usize]
        .1
        .ok_or_else(|| anyhow::anyhow!("__heap init not i32"))? as u32 as u64;
    let park: u64 = heap_init;
    let env_base = if service {
        IMPORTS_SERVE
    } else if wants_http {
        IMPORTS_HTTP
    } else {
        IMPORTS
    };
    let (env, n_env) = EnvImports::plan(host_ops, env_base, wants_http);
    let t = P3Types::new(&mut types);
    let import_list = base_import_list(&t);
    let http_import_list = http_import_list(&t);
    let serve_list = match &sabi {
        Some(a) => {
            let t_bp = type_index(&mut types, &[], &[]);
            let t_tr = type_index(&mut types, &a.tr_params, &[]);
            serve_import_list(&t, t_bp, t_tr)
        }
        None => Vec::new(),
    };
    let env_import_list = env.import_list(t.retptr, t.wait);
    let mut blocks: Vec<&[(u32, &str, &str, u32)]> = vec![&import_list];
    if http_types {
        blocks.push(&http_import_list);
    }
    if service {
        blocks.push(&serve_list);
    }
    blocks.push(&env_import_list);
    // The fs service (#3140): its page sits right past the park, and its
    // imports the artifact does not already carry follow the env block.
    let has = |m: &str, n: &str| blocks.iter().flat_map(|b| b.iter()).any(|(_, bm, bn, _)| *bm == m && *bn == n);
    let fs = P3Fs::plan(host_ops, (park + PARK_SPAN) as u32, &resolve, &abi, &has)?;
    if service && fs.is_some() {
        anyhow::bail!("the serve export reaches the filesystem, which its world does not import (check_service refuses it)");
    }
    let fs_span: u64 = fs.as_ref().map_or(0, |_| FS_PAGE);
    let n_imports = n_env + fs.as_ref().map_or(0, |f| f.splice.fresh_imports());
    let shift = n_imports - 5;
    let shim_base = n_imports + n_funcs;
    // Shim order mirrors the almide.* import order (println, eprintln,
    // exit, fs_call, host_read), then cabi_realloc, run, callback, the
    // #2119 reservation pair, `$await`, and the optional http and env
    // shims.
    let f_realloc = shim_base + 5;
    let f_run = shim_base + 6;
    let f_callback = shim_base + 7;
    let f_reserve = shim_base + 8;
    let f_alloc = shim_base + 9;
    let f_eprintln = shim_base + 1;
    let f_await = shim_base + 10;
    // The env.set overlay log (#3223): its own page past the fs page, its
    // length global past the environment cache.
    let ovl = P3Overlay::plan(env, park + PARK_SPAN + fs_span, global_count + 13, f_eprintln);
    let span = PARK_SPAN + fs_span + ovl.map_or(0, |_| crate::wasi::env_overlay::OVERLAY_BYTES);
    let f_http = wants_http.then_some(shim_base + 11);
    let f_env = env.any().then_some(shim_base + 11 + u32::from(wants_http));
    // The http error helpers (ADR-0023 step 2) follow the env service,
    // which an http program always ships (its limits are read from it).
    let http_fns = f_env.filter(|_| wants_http).map(|fe| HttpErrFns {
        quote: fe + 1,
        err: fe + 2,
        check: fe + 3,
        num: fe + 4,
    });
    // The fs service's functions come last, after every optional shim.
    let serve_first = shim_base + 11 + u32::from(wants_http) + u32::from(env.any()) + 4 * u32::from(http_fns.is_some());
    let serve_fns = service.then_some(ServeFns { op: serve_first, handle: serve_first + 1, cell: serve_first + 2 });
    let fs_first = serve_first + 3 * u32::from(service);

    // Globals: originals, then plen, ppos, the stream state: stdout/stderr
    // writable ends + completion futures, stdin readable + its future.
    let (g_plen, g_ppos) = (global_count, global_count + 1);
    let (g_out_tx, g_out_fut) = (global_count + 2, global_count + 3);
    let (g_err_tx, g_err_fut) = (global_count + 4, global_count + 5);
    let (g_in_rx, g_in_fut) = (global_count + 6, global_count + 7);
    let g_wset = global_count + 8; // the ONE waitable set (lazy, -1)
    let g_slots = global_count + 9; // fan slot-table base (0 = unallocated)
    let g_slotn = global_count + 10; // slot high-water mark
    let (g_env, g_envn) = (global_count + 11, global_count + 12); // cached environment list
    let fs_first_global = global_count + 13 + u32::from(ovl.is_some()); // the fs service's own globals
    let mut globals = GlobalSection::new();
    for (idx, (gt, i32v, i64v, f64v)) in parsed_globals.iter().enumerate() {
        let init = if idx as u32 == heap_global {
            ConstExpr::i32_const((heap_init + span) as i32)
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
    for _ in 0..7 {
        globals.global(mutable_i32, &ConstExpr::i32_const(-1)); // stream state + g_wset
    }
    globals.global(mutable_i32, &ConstExpr::i32_const(0)); // g_slots
    globals.global(mutable_i32, &ConstExpr::i32_const(0)); // g_slotn
    globals.global(mutable_i32, &ConstExpr::i32_const(-1)); // g_env (unfetched)
    globals.global(mutable_i32, &ConstExpr::i32_const(0)); // g_envn
    P3Overlay::emit_global(ovl, &mut globals);
    if let Some(f) = &fs {
        f.splice.emit_globals(&mut globals);
    }
    // The service's globals, where the fs service's would start (the
    // export carries no fs service).
    let serve_globals = service.then(|| {
        ServeGlobals::emit(&mut globals);
        ServeGlobals::at(fs_first_global)
    });

    let fs_types = fs.as_ref().map(|f| f.splice.register_types(&mut types));
    let mut type_sec = TypeSection::new();
    for (p, r) in &types {
        type_sec.ty().function(p.iter().copied(), r.iter().copied());
    }
    let mut imports = build_import_section(&blocks);
    assert_eq!(
        blocks.iter().map(|b| b.len() as u32).sum::<u32>(),
        n_env,
        "import count drift"
    );
    let f_fs_self = shim_base + 3;
    let mut next_import = n_env;
    let fs_targets = P3FsTargets { blocks: &blocks, shims: (f_fs_self, f_await, f_alloc, f_reserve) };
    let fs_import_at = fs
        .as_ref()
        .zip(fs_types.as_ref())
        .map(|(f, ft)| p3_fs_imports(f, &mut imports, ft, &mut next_import, &fs_targets));
    assert_eq!(next_import, n_imports, "fs service import count drift");

    let mut functions = FunctionSection::new();
    for ti in &func_types {
        functions.function(*ti);
    }
    // `$reserve` shares `exit`'s `(i32) -> ()` shape and `$alloc` shares
    // `cabi_realloc`'s, so the pair adds no type-section entry.
    // `$await` shares `future.read`'s `(i32, i32) -> i32` shape.
    for ti in [
        t.print, t.print, t.exit, t.fs, t.hread, t.realloc, t.status, t.callback, t.exit, t.realloc,
        t.fut_read,
    ] {
        functions.function(ti);
    }
    // shim_http, then shim_env — both on the almide fs_call ABI.
    for _ in 0..(u32::from(wants_http) + u32::from(env.any())) {
        functions.function(t.fs);
    }
    if http_fns.is_some() {
        for ti in [t.quote, t.herr, t.hcheck, t.hnum] {
            functions.function(ti);
        }
    }
    // The service's op shim, its export and the lossy cell writer.
    if service {
        for ti in [t.fs, t.call, t.rw] {
            functions.function(ti);
        }
    }
    // The fs service's shipped functions, last; its dispatcher is what
    // shim_fs_call forwards the fs ops to.
    let svc = fs.as_ref().zip(fs_types.as_ref()).map(|(f, ft)| FsService {
        f: f.splice.declare(&mut functions, ft, fs_first),
        ops: f.forwarded.clone(),
    });

    let mut memories = MemorySection::new();
    memories.memory(MemoryType {
        minimum: old_mem_min + span / 65536,
        // #1729: carry the heap-cap maximum through, span-shifted.
        maximum: old_mem_max.map(|m| m.max(old_mem_min) + span / 65536),
        memory64: false,
        shared: false,
        page_size_log2: None,
    });

    let exports = {
        let mut e = wasm_encoder::ExportSection::new();
        e.export("memory", ExportKind::Memory, 0);
        let (lift, entry) = match serve_fns {
            Some(s) => ("wasi:http/handler@0.3.0#handle", s.handle),
            None => ("wasi:cli/run@0.3.0#run", f_run),
        };
        e.export(&format!("[async-lift]{lift}"), ExportKind::Func, entry);
        e.export(&format!("[callback][async-lift]{lift}"), ExportKind::Func, f_callback);
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
        g_err_tx, g_err_fut, g_wset, g_slots, g_slotn, f_reserve, f_await, g_env, g_envn,
    };
    let out_port = PrintPort { g_tx: g_out_tx, g_fut: g_out_fut, call_import: I_OUT_CALL, new_import: I_OUT_NEW, write_import: I_OUT_WRITE };
    let err_port = PrintPort { g_tx: g_err_tx, g_fut: g_err_fut, call_import: I_ERR_CALL, new_import: I_ERR_NEW, write_import: I_ERR_WRITE };
    code.function(&shim_print(out_port, park, f_await, true));
    code.function(&shim_print(err_port, park, f_await, true));
    code.function(&shim_exit());
    let env_ops = env.ops();
    code.function(&shim_fs_call(g, &abi, (f_fs_self, f_http, serve_fns.map(|s| s.op)), f_env.map(|fe| (fe, env_ops.as_slice())), svc.as_ref()));
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
    code.function(&shim_await(park));
    let texts = habi.as_ref().filter(|_| wants_http).map(|h| HttpErrTexts::new(park, h));
    push_optional_shims(&mut code, g, habi.as_ref().zip(texts.as_ref()).zip(http_fns), (env, ovl), f_env);
    let serve_texts = sabi.as_ref().map(|a| ServeTexts::new(park, a));
    if let (Some(a), Some(st), Some(sg), Some(sf)) = (&sabi, &serve_texts, serve_globals, serve_fns) {
        code.function(&shim_serve_op(g, sg));
        code.function(&shim_serve_handle(g, sg, sf, a, st, main_index + shift));
        code.function(&shim_serve_cell());
    }
    let fs_to = fs_import_at.map(|import_at| SpliceTargets {
        import_at,
        g_plen,
        g_ppos,
        heap: heap_global,
        first_global: fs_first_global,
        first_func: fs_first,
    });
    p3_fs_emit(fs.as_ref().zip(fs_types.as_ref()).zip(fs_to.as_ref()), &mut code, &mut data, (g_env, g_envn))?;

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
    if wants_http {
        data.active(0, &ConstExpr::i32_const((park + MSG_HTTP) as i32), E_HTTP.iter().copied());
        data.active(0, &ConstExpr::i32_const((park + MSG_CLEN) as i32), E_CLEN.iter().copied());
    }
    if let Some(t) = &texts {
        assert!(t.base + t.blob.len() as u64 <= park + SERVE_TEXT, "the http texts reach the service statics");
        data.active(0, &ConstExpr::i32_const(t.base as i32), t.blob.iter().copied());
    }
    if let Some(t) = &serve_texts {
        data.active(0, &ConstExpr::i32_const(t.base as i32), t.blob.iter().copied());
    }
    P3Overlay::emit_data(ovl, &mut data, park);

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
    // The p1 build's last pass (#3136): drop the shims, helper slots,
    // imports and globals nothing the exports reach names, before the
    // component encode reads the core module's imports.
    let mut core = crate::wasi::prune(&m.finish())?;
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
    validate_baseline(&component)?;
    Ok(component)
}

/// The optional shims after `$await`, in function-index order: the http
/// client (when the op set reaches 43..=50), the env service, then the
/// http error helpers.
fn push_optional_shims(
    code: &mut CodeSection,
    g: P3Globals,
    http: Option<((&HttpAbi, &HttpErrTexts), HttpErrFns)>,
    (env, ovl): (EnvImports, Option<P3Overlay>),
    f_env: Option<u32>,
) {
    if let Some(((h, t), fns)) = http {
        code.function(&shim_http(g, h, t, fns));
    }
    if env.any() {
        code.function(&shim_env(g, env, ovl));
    }
    if let (Some(((_, t), fns)), Some(fe)) = (http, f_env) {
        code.function(&shim_http_quote());
        code.function(&shim_http_err(g, fns.quote));
        code.function(&shim_http_hdr_check(t));
        code.function(&shim_http_env_num(g, fe));
    }
}

/// ADR-0023 step 1: the component must validate with the 🚝
/// `component-model-more-async-builtins` feature OFF — the synchronous
/// stream/future builtins it gates are what made every p3 artifact need
/// `wasmtime run -W component-model-more-async-builtins`. A shim change
/// that reintroduces one fails the build here, before any runtime sees it.
pub fn validate_baseline(component: &[u8]) -> anyhow::Result<()> {
    let mut features = wasmparser::WasmFeatures::default();
    features.remove(wasmparser::WasmFeatures::CM_MORE_ASYNC_BUILTINS);
    wasmparser::Validator::new_with_features(features)
        .validate_all(component)
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!("the p3 component needs a builtin outside the WASI 0.3 baseline: {e}"))
}

/// The import section in declaration order — the base table, then the
/// optional http and env blocks — each entry asserted against the index
/// it must land on (an `I_*` drift fails loudly here rather than as a
/// mis-wired call). Extracted from `to_p3` (codopsy cc 22).
fn build_import_section(blocks: &[&[(u32, &str, &str, u32)]]) -> ImportSection {
    let mut imports = ImportSection::new();
    for (k, (want, m, n, t)) in blocks.iter().flat_map(|b| b.iter()).enumerate() {
        assert_eq!(k as u32, *want, "import order drift at {m}#{n}");
        imports.import(m, n, EntityType::Function(*t));
    }
    imports
}

/// The canonical-ABI core types every p3 module declares (import shapes and
/// the shim signatures), by role — one place, so `to_p3` reads them by name.
#[derive(Clone, Copy)]
struct P3Types {
    exit: u32,
    call: u32,
    new: u32,
    rw: u32,
    fut_read: u32,
    drop: u32,
    retptr: u32,
    random: u32,
    print: u32,
    fs: u32,
    hread: u32,
    realloc: u32,
    status: u32,
    callback: u32,
    open: u32,
    stat: u32,
    rvs: u32,
    aopen: u32,
    wvs: u32,
    avs: u32,
    pathop: u32,
    ws_new: u32,
    ws_join: u32,
    ws_wait: u32,
    set4: u32,
    set5: u32,
    consume: u32,
    append: u32,
    wait: u32,
    opt_set: u32,
    quote: u32,
    herr: u32,
    hcheck: u32,
    hnum: u32,
}

impl P3Types {
    fn new(types: &mut Vec<(Vec<ValType>, Vec<ValType>)>) -> Self {
        // Canonical-ABI core types.
        let t_exit = type_index(types, &[ValType::I32], &[]);
        let t_call = type_index(types, &[ValType::I32], &[ValType::I32]);
        let t_new = type_index(types, &[], &[ValType::I64]);
        let t_rw = type_index(
            types,
            &[ValType::I32, ValType::I32, ValType::I32],
            &[ValType::I32],
        );
        let t_fut_read = type_index(types, &[ValType::I32, ValType::I32], &[ValType::I32]);
        let t_drop = type_index(types, &[ValType::I32], &[]);
        let t_retptr = type_index(types, &[ValType::I32], &[]);
        let t_random = type_index(types, &[ValType::I64, ValType::I32], &[]);
        // Shim types (the almide.* signatures) + realloc + run + callback.
        let t_print = type_index(types, &[ValType::I32, ValType::I32], &[]);
        let t_fs = type_index(types, &[ValType::I32; 5], &[ValType::I64]);
        let t_hread = type_index(types, &[ValType::I32], &[]);
        let t_realloc = type_index(types, &[ValType::I32; 4], &[ValType::I32]);
        let t_status = type_index(types, &[], &[ValType::I32]);
        let t_callback = type_index(types, &[ValType::I32; 3], &[ValType::I32]);
        // fs read-surface core shapes (sync lowers).
        let t_open = type_index(types, &[ValType::I32; 7], &[]);
        let t_stat = type_index(types, &[ValType::I32; 5], &[]);
        let t_rvs = type_index(
            types,
            &[ValType::I32, ValType::I64, ValType::I32],
            &[],
        );
        let t_aopen = type_index(types, &[ValType::I32; 2], &[ValType::I32]);
        // write-via-stream: (self, stream, offset:i64) -> future — the future
        // returns DIRECTLY (1 flat result), as stdio's write-via-stream.
        let t_wvs = type_index(
            types,
            &[ValType::I32, ValType::I32, ValType::I64],
            &[ValType::I32],
        );
        // append-via-stream: (self, stream) -> future.
        let t_avs = type_index(types, &[ValType::I32; 2], &[ValType::I32]);
        // path ops: (self, ptr, len, retptr).
        let t_pathop = type_index(types, &[ValType::I32; 4], &[]);
        let t_ws_new = type_index(types, &[], &[ValType::I32]);
        let t_ws_join = type_index(types, &[ValType::I32; 2], &[]);
        let t_ws_wait = type_index(types, &[ValType::I32; 2], &[ValType::I32]);
        // http client core shapes (#1710 PR B).
        let t_set4 = type_index(types, &[ValType::I32; 4], &[ValType::I32]);
        let t_set5 = type_index(types, &[ValType::I32; 5], &[ValType::I32]);
        let t_consume = type_index(types, &[ValType::I32; 3], &[]);
        // [method]fields.append(self, name ptr/len, value ptr/len, retptr) —
        // `result<_, header-error>` carries a payload, so it lands via retptr.
        let t_append = type_index(types, &[ValType::I32; 6], &[]);
        // monotonic-clock.wait-for(duration) — a sync lower of the async func.
        let t_wait = type_index(types, &[ValType::I64], &[]);
        // request-options setters: (self, option<duration> as (disc, u64), retptr).
        let t_opt_set = type_index(types, &[ValType::I32, ValType::I32, ValType::I64, ValType::I32], &[]);
        // The http error helpers: $http_quote, $http_err, $http_hdr_check, $http_env_num.
        let t_quote = type_index(types, &[ValType::I32; 3], &[ValType::I32]);
        let t_herr = type_index(types, &[ValType::I32; 10], &[ValType::I64]);
        let t_hcheck = type_index(types, &[ValType::I32; 4], &[ValType::I32]);
        let t_hnum = type_index(types, &[ValType::I32, ValType::I32, ValType::I64], &[ValType::I64]);
        P3Types { exit: t_exit, call: t_call, new: t_new, rw: t_rw, fut_read: t_fut_read, drop: t_drop, retptr: t_retptr, random: t_random, print: t_print, fs: t_fs, hread: t_hread, realloc: t_realloc, status: t_status, callback: t_callback, open: t_open, stat: t_stat, rvs: t_rvs, aopen: t_aopen, wvs: t_wvs, avs: t_avs, pathop: t_pathop, ws_new: t_ws_new, ws_join: t_ws_join, ws_wait: t_ws_wait, set4: t_set4, set5: t_set5, consume: t_consume, append: t_append, wait: t_wait, opt_set: t_opt_set, quote: t_quote, herr: t_herr, hcheck: t_hcheck, hnum: t_hnum }
    }
}

/// The import table, POSITION-CHECKED against the I_* constants: the
/// list is the single source of order, and a drifted constant fails
/// the build of every artifact instead of silently calling the wrong
/// host function.
fn base_import_list(t: &P3Types) -> Vec<(u32, &'static str, &'static str, u32)> {
    let cli_out = "wasi:cli/stdout@0.3.0";
    let cli_err = "wasi:cli/stderr@0.3.0";
    let cli_in = "wasi:cli/stdin@0.3.0";
    let fs_types = "wasi:filesystem/types@0.3.0";
    let list = vec![
        (I_EXIT, "wasi:cli/exit@0.3.0", "exit", t.exit),
        (I_OUT_CALL, cli_out, "write-via-stream", t.call),
        (I_OUT_NEW, cli_out, "[stream-new-0]write-via-stream", t.new),
        (I_OUT_WRITE, cli_out, "[async-lower][stream-write-0]write-via-stream", t.rw),
        (I_OUT_DROP_TX, cli_out, "[stream-drop-writable-0]write-via-stream", t.drop),
        (I_OUT_FUT_READ, cli_out, "[async-lower][future-read-1]write-via-stream", t.fut_read),
        (I_ERR_CALL, cli_err, "write-via-stream", t.call),
        (I_ERR_NEW, cli_err, "[stream-new-0]write-via-stream", t.new),
        (I_ERR_WRITE, cli_err, "[async-lower][stream-write-0]write-via-stream", t.rw),
        (I_ERR_DROP_TX, cli_err, "[stream-drop-writable-0]write-via-stream", t.drop),
        (I_ERR_FUT_READ, cli_err, "[async-lower][future-read-1]write-via-stream", t.fut_read),
        (I_STDIN_OPEN, cli_in, "read-via-stream", t.retptr),
        (I_STDIN_READ, cli_in, "[async-lower][stream-read-0]read-via-stream", t.rw),
        (I_STDIN_DROP_RX, cli_in, "[stream-drop-readable-0]read-via-stream", t.drop),
        (I_STDIN_DROP_FUT, cli_in, "[future-drop-readable-1]read-via-stream", t.drop),
        (I_CLOCK_NOW, "wasi:clocks/system-clock@0.3.0", "now", t.retptr),
        (I_RANDOM, "wasi:random/random@0.3.0", "get-random-bytes", t.random),
        (I_TASK_RETURN, "[export]wasi:cli/run@0.3.0", "[task-return]run", t.exit),
        (I_FS_PRE, "wasi:filesystem/preopens@0.3.0", "get-directories", t.retptr),
        (I_FS_OPEN, fs_types, "[method]descriptor.open-at", t.open),
        (I_FS_STAT, fs_types, "[method]descriptor.stat-at", t.stat),
        (I_FS_RVS, fs_types, "[method]descriptor.read-via-stream", t.rvs),
        (I_FS_SREAD, fs_types, "[async-lower][stream-read-0][method]descriptor.read-via-stream", t.rw),
        (I_FS_SDROP, fs_types, "[stream-drop-readable-0][method]descriptor.read-via-stream", t.drop),
        (I_FS_FDROP, fs_types, "[future-drop-readable-1][method]descriptor.read-via-stream", t.drop),
        (I_FS_RESDROP, fs_types, "[resource-drop]descriptor", t.drop),
        (I_FS_AOPEN, fs_types, "[async-lower][method]descriptor.open-at", t.aopen),
        (I_WS_NEW, "$root", "[waitable-set-new]", t.ws_new),
        (I_WS_JOIN, "$root", "[waitable-join]", t.ws_join),
        (I_WS_WAIT, "$root", "[waitable-set-wait]", t.ws_wait),
        (I_SUBTASK_DROP, "$root", "[subtask-drop]", t.drop),
        (I_WS_DROP, "$root", "[waitable-set-drop]", t.drop),
        (I_SUBTASK_CANCEL, "$root", "[subtask-cancel]", t.call),
        (I_FS_WVS, fs_types, "[method]descriptor.write-via-stream", t.wvs),
        (I_FS_AVS, fs_types, "[method]descriptor.append-via-stream", t.avs),
        (I_FS_WNEW, fs_types, "[stream-new-0][method]descriptor.write-via-stream", t.new),
        (I_FS_WWRITE, fs_types, "[async-lower][stream-write-0][method]descriptor.write-via-stream", t.rw),
        (I_FS_WDROP, fs_types, "[stream-drop-writable-0][method]descriptor.write-via-stream", t.drop),
        (I_FS_WFUT, fs_types, "[async-lower][future-read-1][method]descriptor.write-via-stream", t.fut_read),
        (I_FS_MKDIR, fs_types, "[method]descriptor.create-directory-at", t.pathop),
        (I_FS_UNLINK, fs_types, "[method]descriptor.unlink-file-at", t.pathop),
        (I_FS_RMDIR, fs_types, "[method]descriptor.remove-directory-at", t.pathop),
        (I_MONO_NOW, "wasi:clocks/monotonic-clock@0.3.0", "now", t.new),
    ];
    assert_eq!(list.len() as u32, IMPORTS, "IMPORTS count drift");
    list
}

/// The http client block (#1710 PR B), appended after the base table.
fn http_import_list(t: &P3Types) -> Vec<(u32, &'static str, &'static str, u32)> {
    let http_types = "wasi:http/types@0.3.0";
    let http_client = "wasi:http/client@0.3.0";
    let list = vec![
        (I_HTTP_FIELDS_NEW, http_types, "[constructor]fields", t.ws_new),
        (I_HTTP_REQ_NEW, http_types, "[static]request.new", t.open),
        (I_HTTP_REQ_SNEW, http_types, "[stream-new-0][static]request.new", t.new),
        (
            I_HTTP_REQ_SWRITE,
            http_types,
            "[async-lower][stream-write-0][static]request.new",
            t.rw,
        ),
        (I_HTTP_REQ_SDROPW, http_types, "[stream-drop-writable-0][static]request.new", t.drop),
        (I_HTTP_REQ_FNEW, http_types, "[future-new-1][static]request.new", t.new),
        (
            I_HTTP_REQ_FWRITE,
            http_types,
            "[async-lower][future-write-1][static]request.new",
            t.fut_read,
        ),
        (I_HTTP_REQ_FDROPW, http_types, "[future-drop-writable-1][static]request.new", t.drop),
        (I_HTTP_REQ_SENTDROP, http_types, "[future-drop-readable-2][static]request.new", t.drop),
        (I_HTTP_SET_METHOD, http_types, "[method]request.set-method", t.set4),
        (I_HTTP_SET_SCHEME, http_types, "[method]request.set-scheme", t.set5),
        (I_HTTP_SET_AUTH, http_types, "[method]request.set-authority", t.set4),
        (I_HTTP_SET_PATH, http_types, "[method]request.set-path-with-query", t.set4),
        (I_HTTP_SEND, http_client, "[async-lower]send", t.fut_read),
        (I_HTTP_STATUS, http_types, "[method]response.get-status-code", t.call),
        (I_HTTP_CONSUME, http_types, "[static]response.consume-body", t.consume),
        (I_HTTP_CB_FNEW, http_types, "[future-new-0][static]response.consume-body", t.new),
        (I_HTTP_CB_FWRITE, http_types, "[async-lower][future-write-0][static]response.consume-body", t.fut_read),
        (I_HTTP_CB_FDROPW, http_types, "[future-drop-writable-0][static]response.consume-body", t.drop),
        (I_HTTP_BODY_READ, http_types, "[async-lower][stream-read-1][static]response.consume-body", t.rw),
        (I_HTTP_BODY_DROPR, http_types, "[stream-drop-readable-1][static]response.consume-body", t.drop),
        (I_HTTP_TRL_DROPR, http_types, "[future-drop-readable-2][static]response.consume-body", t.drop),
        (I_HTTP_REQ_DROP, http_types, "[resource-drop]request", t.drop),
        (I_HTTP_RESP_DROP, http_types, "[resource-drop]response", t.drop),
        (I_HTTP_FIELDS_DROP, http_types, "[resource-drop]fields", t.drop),
        (I_HTTP_FIELDS_APPEND, http_types, "[method]fields.append", t.append),
        (I_HTTP_OPT_NEW, http_types, "[constructor]request-options", t.ws_new),
        (I_HTTP_OPT_CONNECT, http_types, "[method]request-options.set-connect-timeout", t.opt_set),
        (I_HTTP_OPT_FIRST, http_types, "[method]request-options.set-first-byte-timeout", t.opt_set),
        (I_HTTP_OPT_BETWEEN, http_types, "[method]request-options.set-between-bytes-timeout", t.opt_set),
    ];
    assert_eq!(IMPORTS + list.len() as u32, IMPORTS_HTTP, "IMPORTS_HTTP count drift");
    list
}
