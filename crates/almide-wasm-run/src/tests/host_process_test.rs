//! The embedded `almide:process/spawn` (#2589): the frame reader, the
//! answers' shapes, the `[permissions] proc` bound, and the canonical-ABI
//! import driven by a hand-built guest.

use super::{call, cells, dispatch, link_spawn_import, set_allowlist, ProcTable};

/// The allowlist is process-wide; these tests set it one at a time.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The `start` children these tests make.
static TABLE: ProcTable = std::sync::Mutex::new(Vec::new());

fn frame(items: &[&str]) -> String {
    items.iter().map(|s| format!("{}\n{s}", s.chars().count())).collect()
}

#[test]
fn cells_round_trip_char_lengths_and_refuse_a_short_cell() {
    assert_eq!(cells(&frame(&["a", "", "héllo\nx"])).unwrap(), vec!["a", "", "héllo\nx"]);
    assert_eq!(cells("").unwrap(), Vec::<String>::new());
    assert!(cells("5\nabc").is_err());
    assert!(cells("x\nabc").is_err());
}

#[test]
fn exec_status_answers_code_then_the_two_texts() {
    let _g = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    set_allowlist(None);
    let got = call(83, "echo", &frame(&["hi"]), &|| {}, &TABLE).expect("echo runs");
    assert_eq!(got, "0\n3\nhi\n");
    let missing = call(83, "almide-no-such-binary-2589", "", &|| {}, &TABLE).unwrap_err();
    assert!(missing.starts_with("process.exec_status(\"almide-no-such-binary-2589\"): "), "{missing}");
}

#[test]
fn the_allowlist_refuses_a_command_outside_it_by_name() {
    let _g = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    set_allowlist(Some(vec!["echo".to_string()]));
    assert!(call(80, "echo", &frame(&["ok"]), &|| {}, &TABLE).is_ok());
    let refused = call(80, "cargo", &frame(&["--version"]), &|| {}, &TABLE).unwrap_err();
    assert_eq!(refused, "process.exec(\"cargo\"): `cargo` is not in [permissions] proc");
    // exec_in names the command its frame carries, not the directory.
    let refused_in = call(81, "/", &frame(&["cargo"]), &|| {}, &TABLE).unwrap_err();
    assert_eq!(refused_in, "process.exec_in(\"cargo\"): `cargo` is not in [permissions] proc");
    set_allowlist(None);
}

/// #4125: kill is bounded as the `kill` command — refused with the
/// refused-spawn text, lifted by listing it.
#[test]
fn the_allowlist_bounds_signalling_as_the_kill_command() {
    let _g = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    let me = std::process::id().to_string();
    set_allowlist(Some(vec!["echo".to_string()]));
    assert_eq!(
        call(87, &me, "0", &|| {}, &TABLE).unwrap_err(),
        "process.kill(\"kill\"): `kill` is not in [permissions] proc"
    );
    set_allowlist(Some(vec!["echo".to_string(), "kill".to_string()]));
    assert_eq!(call(87, &me, "0", &|| {}, &TABLE).unwrap(), "");
    set_allowlist(None);
    assert_eq!(
        call(87, "-1", "0", &|| {}, &TABLE).unwrap_err(),
        "process.kill(-1, 0): pid must be in 1..=2147483647"
    );
}

/// C-418 (#4126): a refused command is named with its control characters
/// escaped, so it cannot forge a line of the error text.
#[test]
fn a_refused_command_is_named_escaped() {
    let _g = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    set_allowlist(Some(vec!["echo".to_string()]));
    let refused = call(80, "x\ny\u{1b}", &frame(&[]), &|| {}, &TABLE).unwrap_err();
    assert_eq!(
        refused,
        "process.exec(\"x\\ny\\u{1b}\"): `x\\ny\\u{1b}` is not in [permissions] proc"
    );
    set_allowlist(None);
}

#[test]
fn the_fs_call_form_packs_status_and_length() {
    let _g = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    set_allowlist(None);
    let (ret, buf) = dispatch(89, "", b"", &|| {}, &TABLE);
    assert_eq!(ret >> 32, 0);
    assert_eq!(String::from_utf8(buf).unwrap(), std::process::id().to_string());
    let (ret, buf) = dispatch(87, "x", b"9", &|| {}, &TABLE);
    assert_eq!(ret >> 32, 1);
    assert_eq!(String::from_utf8(buf).unwrap(), "malformed process operand `x`");
}

/// A guest that imports `almide:process/spawn.call` in its canonical form,
/// exports `memory` and a bump `cabi_realloc`, and calls `exec` on `echo`.
fn guest() -> Vec<u8> {
    use wasm_encoder::{
        CodeSection, ConstExpr, DataSection, EntityType, ExportKind, ExportSection, Function, FunctionSection,
        GlobalSection, GlobalType, ImportSection, MemorySection, MemoryType, Module, TypeSection, ValType,
    };
    let i32t = ValType::I32;
    let mut types = TypeSection::new();
    types.ty().function([i32t; 6], []); // 0: the import
    types.ty().function([i32t; 4], [i32t]); // 1: cabi_realloc
    types.ty().function([], []); // 2: run
    let mut imports = ImportSection::new();
    imports.import("almide:process/spawn", "call", EntityType::Function(0));
    let mut funcs = FunctionSection::new();
    funcs.function(1);
    funcs.function(2);
    let mut mem = MemorySection::new();
    mem.memory(MemoryType { minimum: 1, maximum: None, memory64: false, shared: false, page_size_log2: None });
    let mut globals = GlobalSection::new();
    globals.global(GlobalType { val_type: i32t, mutable: true, shared: false }, &ConstExpr::i32_const(1024));
    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("cabi_realloc", ExportKind::Func, 1);
    exports.export("run", ExportKind::Func, 2);
    let mut code = CodeSection::new();
    let mut realloc = Function::new([]);
    realloc.instructions().global_get(0).global_get(0).local_get(3).i32_add().global_set(0).end();
    code.function(&realloc);
    let mut run = Function::new([]);
    // op 0 = exec; a = "echo" at 0, b = frame ["hi"] at 16; retptr 64.
    run.instructions()
        .i32_const(0)
        .i32_const(0)
        .i32_const(4)
        .i32_const(16)
        .i32_const(4)
        .i32_const(64)
        .call(0)
        .end();
    code.function(&run);
    let mut data = DataSection::new();
    data.active(0, &ConstExpr::i32_const(0), b"echo".iter().copied());
    data.active(0, &ConstExpr::i32_const(16), b"2\nhi".iter().copied());
    let mut m = Module::new();
    m.section(&types).section(&imports).section(&funcs).section(&mem).section(&globals);
    m.section(&exports).section(&code).section(&data);
    m.finish()
}

#[test]
fn the_canonical_import_lands_the_answer_through_cabi_realloc() {
    let _g = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    set_allowlist(None);
    let engine = wasmtime::Engine::default();
    let module = wasmtime::Module::new(&engine, guest()).expect("guest validates");
    let mut linker = wasmtime::Linker::new(&engine);
    link_spawn_import(&mut linker).expect("link");
    let mut store = wasmtime::Store::new(&engine, ());
    let inst = linker.instantiate(&mut store, &module).expect("instantiate");
    inst.get_typed_func::<(), ()>(&mut store, "run").expect("run export").call(&mut store, ()).expect("run");
    let mem = inst.get_memory(&mut store, "memory").expect("memory");
    let mut cell = [0u8; 12];
    mem.read(&store, 64, &mut cell).expect("result cell");
    assert_eq!(cell[0], 0, "ok discriminant");
    let word = |at: usize| u32::from_le_bytes([cell[at], cell[at + 1], cell[at + 2], cell[at + 3]]) as usize;
    let (ptr, len) = (word(4), word(8));
    let mut text = vec![0u8; len];
    mem.read(&store, ptr, &mut text).expect("answer bytes");
    assert_eq!(String::from_utf8(text).expect("utf-8"), "hi\n");
}

#[test]
fn a_linker_without_the_import_refuses_the_guest_at_instantiation() {
    let engine = wasmtime::Engine::default();
    let module = wasmtime::Module::new(&engine, guest()).expect("guest validates");
    let linker = wasmtime::Linker::<()>::new(&engine);
    let mut store = wasmtime::Store::new(&engine, ());
    let Err(e) = linker.instantiate(&mut store, &module) else { panic!("a stock linker has no almide:process/spawn") };
    assert!(format!("{e:#}").contains("almide:process/spawn"), "{e:#}");
}

/// The WIT is the interface's one written form: its `enum op` order IS the
/// host op numbering (case index = op − 80, the `start` child's cases
/// continuing at op 140), and `call` keeps the shape the
/// canonical import above lowers. A reordered case or a changed signature
/// fails here before it can disagree with the emitter or the host.
#[test]
fn the_wit_names_the_ops_in_host_op_order_and_the_call_shape() {
    let mut resolve = wit_parser::Resolve::default();
    let pkg = resolve
        .push_str("spawn.wit", include_str!("../../wit/process/spawn.wit"))
        .expect("spawn.wit parses");
    assert_eq!(resolve.packages[pkg].name.to_string(), "almide:process");
    let iface = resolve.packages[pkg].interfaces["spawn"];
    let op = resolve.interfaces[iface].types["op"];
    let wit_parser::TypeDefKind::Enum(e) = &resolve.types[op].kind else { panic!("op is an enum") };
    let cases: Vec<&str> = e.cases.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        cases,
        [
            "exec", "exec-in", "exec-with-stdin", "exec-status", "output", "run", "spawn", "kill", "is-alive", "pid",
            "run-with", "start", "poll", "wait", "stop", "read-new", "child-pid",
        ]
    );
    let first_block = super::OP_LAST - super::OP_FIRST + 1;
    assert_eq!(first_block + super::CHILD_LAST - super::CHILD_FIRST + 1, cases.len() as i32);
    // Every case maps to its host op and back (the forwarder's arithmetic).
    for (case, _) in cases.iter().enumerate() {
        let op = crate::wasi::proc_op_of_case(case as i32);
        let want = if (case as i32) < first_block { super::OP_FIRST + case as i32 } else { super::CHILD_FIRST + case as i32 - first_block };
        assert_eq!(op, want, "case {case}");
        assert!(super::is_op(op), "op {op}");
        assert_eq!(crate::wasi::proc_case(op), case as i32);
    }
    let call = &resolve.interfaces[iface].functions["call"];
    let params: Vec<&str> = call.params.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(params, ["op", "a", "b"]);
}

/// A raw `almide.*` module that runs `exec_status("sh", ["-c", "printf hi"])`
/// over `fs_call` op 83 and prints the answer — the shape the emitter writes.
fn raw_exec_status_module() -> Vec<u8> {
    use wasm_encoder::{
        CodeSection, ConstExpr, DataSection, EntityType, ExportKind, ExportSection, Function, FunctionSection,
        GlobalSection, GlobalType, ImportSection, MemorySection, MemoryType, Module, TypeSection, ValType,
    };
    let i32t = ValType::I32;
    let mut types = TypeSection::new();
    types.ty().function([i32t, i32t], []); // 0: println / eprintln
    types.ty().function([i32t], []); // 1: exit / host_read
    types.ty().function([i32t; 5], [ValType::I64]); // 2: fs_call
    types.ty().function([], []); // 3: main
    let mut imports = ImportSection::new();
    for (name, ty) in [("println", 0), ("eprintln", 0), ("exit", 1), ("fs_call", 2), ("host_read", 1)] {
        imports.import("almide", name, EntityType::Function(ty));
    }
    let mut funcs = FunctionSection::new();
    funcs.function(3);
    let mut mem = MemorySection::new();
    mem.memory(MemoryType { minimum: 1, maximum: None, memory64: false, shared: false, page_size_log2: None });
    let mut globals = GlobalSection::new();
    globals.global(GlobalType { val_type: i32t, mutable: true, shared: false }, &ConstExpr::i32_const(4096));
    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("main", ExportKind::Func, 5);
    exports.export("__heap", ExportKind::Global, 0);
    let frame = frame(&["-c", "printf hi"]);
    let mut main = Function::new([(1, i32t)]);
    main.instructions()
        .i32_const(83)
        .i32_const(0)
        .i32_const(2)
        .i32_const(16)
        .i32_const(frame.len() as i32)
        .call(3)
        .i32_wrap_i64()
        .local_set(0)
        .i32_const(512)
        .call(4)
        .i32_const(512)
        .local_get(0)
        .call(0)
        .end();
    let mut code = CodeSection::new();
    code.function(&main);
    let mut data = DataSection::new();
    data.active(0, &ConstExpr::i32_const(0), b"sh".iter().copied());
    data.active(0, &ConstExpr::i32_const(16), frame.into_bytes());
    let mut m = Module::new();
    m.section(&types).section(&imports).section(&funcs).section(&mem).section(&globals);
    m.section(&exports).section(&code).section(&data);
    m.finish()
}

/// The stock artifact end to end: `to_wasi` ships the private import (and
/// only because op 83 is in the op set), a host that implements it plus two
/// preview-1 calls runs the artifact to the right output, and a host without
/// it refuses the module before `_start`.
#[test]
fn the_p1_artifact_forwards_process_ops_to_the_private_import() {
    let _g = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    set_allowlist(None);
    let raw = raw_exec_status_module();
    let p1 = crate::wasi::to_wasi(&raw, &[83]).expect("to_wasi");
    let imports: Vec<(String, String)> = wasmparser::Parser::new(0)
        .parse_all(&p1)
        .filter_map(|p| match p {
            Ok(wasmparser::Payload::ImportSection(r)) => Some(r),
            _ => None,
        })
        .flat_map(|r| r.into_imports().flatten().map(|i| (i.module.to_string(), i.name.to_string())).collect::<Vec<_>>())
        .collect();
    assert!(imports.contains(&("almide:process/spawn".into(), "call".into())), "{imports:?}");
    // The selection: an op set without a process op ships no such import.
    let quiet = crate::wasi::to_wasi(&raw, &[30]).expect("to_wasi");
    assert!(!String::from_utf8_lossy(&quiet).contains("almide:process/spawn"));

    let engine = wasmtime::Engine::default();
    let module = wasmtime::Module::new(&engine, &p1).expect("artifact validates");
    let mut linker = wasmtime::Linker::<Vec<u8>>::new(&engine);
    link_spawn_import(&mut linker).expect("link");
    linker
        .func_wrap(
            "wasi_snapshot_preview1",
            "fd_write",
            |mut caller: wasmtime::Caller<'_, Vec<u8>>, _fd: i32, iovs: i32, n: i32, nwritten: i32| -> i32 {
                let mem = caller.get_export("memory").and_then(|e| e.into_memory()).expect("memory");
                let mut total = 0u32;
                for k in 0..n as u32 {
                    let mut iov = [0u8; 8];
                    mem.read(&caller, iovs as usize + 8 * k as usize, &mut iov).expect("iov");
                    let (ptr, len) = (u32::from_le_bytes([iov[0], iov[1], iov[2], iov[3]]), u32::from_le_bytes([iov[4], iov[5], iov[6], iov[7]]));
                    let mut buf = vec![0u8; len as usize];
                    mem.read(&caller, ptr as usize, &mut buf).expect("bytes");
                    caller.data_mut().extend_from_slice(&buf);
                    total += len;
                }
                mem.write(&mut caller, nwritten as usize, &total.to_le_bytes()).expect("nwritten");
                0
            },
        )
        .expect("fd_write");
    linker
        .func_wrap("wasi_snapshot_preview1", "proc_exit", |code: i32| -> wasmtime::Result<()> {
            Err(wasmtime::Error::msg(format!("proc_exit({code})")))
        })
        .expect("proc_exit");
    let mut store = wasmtime::Store::new(&engine, Vec::new());
    let inst = linker.instantiate(&mut store, &module).expect("a host with the import instantiates it");
    inst.get_typed_func::<(), ()>(&mut store, "_start").expect("_start").call(&mut store, ()).expect("runs");
    assert_eq!(String::from_utf8(store.data().clone()).expect("utf-8"), "0\n2\nhi\n");

    // A stock host: the two preview-1 calls, no almide:process/spawn.
    let mut stock = wasmtime::Linker::<()>::new(&engine);
    stock.func_wrap("wasi_snapshot_preview1", "fd_write", |_: i32, _: i32, _: i32, _: i32| -> i32 { 0 }).expect("fd_write");
    stock.func_wrap("wasi_snapshot_preview1", "proc_exit", |_: i32| {}).expect("proc_exit");
    let Err(e) = stock.instantiate(&mut wasmtime::Store::new(&engine, ()), &module) else { panic!("refused") };
    assert!(format!("{e:#}").contains("almide:process/spawn"), "{e:#}");
}

/// The `start` child (#4308) over ops 140..=145: native's child core on the
/// run's table — the handle, poll while running, the incremental reads, the
/// outcome, stop, and an unknown handle.
#[test]
fn the_start_child_answers_through_its_handle() {
    let _g = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    set_allowlist(None);
    let t: ProcTable = std::sync::Mutex::new(Vec::new());
    let script = "printf a; printf x >&2; exit 3";
    let h = call(140, "sh", &frame(&["0", "", "", "-c", script]), &|| {}, &t).expect("start");
    let st = call(142, &h, "", &|| {}, &t).expect("wait");
    assert_eq!(st, "3\n1\nax");
    assert_eq!(call(141, &h, "", &|| {}, &t).as_deref(), Ok("1"));
    assert_eq!(call(144, &h, "1", &|| {}, &t).as_deref(), Ok("a"));
    assert_eq!(call(144, &h, "1", &|| {}, &t).as_deref(), Ok(""));
    assert_eq!(call(144, &h, "2", &|| {}, &t).as_deref(), Ok("x"));
    assert!(call(145, &h, "", &|| {}, &t).expect("pid").parse::<i64>().expect("decimal") > 0);

    let s = call(140, "sleep", &frame(&["0", "", "", "30"]), &|| {}, &t).expect("start sleep");
    assert_eq!(call(141, &s, "", &|| {}, &t).as_deref(), Ok("0"));
    let stopped = call(143, &s, "200", &|| {}, &t).expect("stop");
    assert!(stopped.starts_with("-1\n"), "{stopped}");
    assert_eq!(call(143, &s, "200", &|| {}, &t).as_deref(), Ok(stopped.as_str()));

    let late = call(140, "sleep", &frame(&["100", "", "", "30"]), &|| {}, &t).expect("start");
    assert_eq!(call(142, &late, "", &|| {}, &t).unwrap_err(), "exec timed out after 100ms");

    assert_eq!(call(142, "99", "", &|| {}, &t).unwrap_err(), "invalid ProcessChild handle: 99");
    let missing = call(140, "almide-no-such-binary-4308", &frame(&["0", "", ""]), &|| {}, &t).unwrap_err();
    assert!(missing.starts_with("process.start(\"almide-no-such-binary-4308\"): "), "{missing}");
}

/// The run's end stops a child neither waited on nor stopped (C-437's rule).
#[test]
fn the_run_end_stops_an_unwaited_child() {
    let _g = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    set_allowlist(None);
    let t: ProcTable = std::sync::Mutex::new(Vec::new());
    let h = call(140, "sleep", &frame(&["0", "", "", "30"]), &|| {}, &t).expect("start");
    let pid: i32 = call(145, &h, "", &|| {}, &t).expect("pid").parse().expect("decimal");
    super::stop_unwaited(&t);
    assert_eq!(call(141, &h, "", &|| {}, &t).as_deref(), Ok("1"));
    let gone = std::process::Command::new("sh").arg("-c").arg(format!("kill -0 {pid} 2>/dev/null")).status().expect("sh");
    assert!(!gone.success(), "the child outlived the run");
}
