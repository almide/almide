// ── render_wasm.rs: the WASI floor prim calls ──
//
// include!-spliced next to `render_wasm_p2_b.rs` (the 800-line file
// discipline). The `(call $<floor> …)` a `PrimKind::{ArgsGetList, EnvGet,
// ReadTextFile, ReadBytesFile, ReadDir, WriteTextFile, MakeDir, RemoveAll,
// Rename, PathExists, PathFilestat*, ReadLine, ReadNBytes}` op renders to, and
// the #2206 call-head operands the fs floors take.

/// The `$ha $hl` operands of an fs floor call (#2206): the call head's bytes.
/// `extra` is what follows the floor's own args in the `Op::Prim` — empty for the
/// plain prim (the head is the static [`FS_MSG_CALLS`] row `own_call`), else the
/// call-name String a `_as` / `_as_pair` twin passed (its bytes and length).
fn fs_floor_head(extra: &[ValueId], own_call: usize) -> String {
    match extra.first() {
        Some(call) => format!(
            "(i32.add (local.get {c}) (i32.const {LIST_HEADER})) (i32.load (i32.add (local.get {c}) (i32.const {LIST_LEN_OFFSET})))",
            c = local(*call)
        ),
        None => {
            let (addr, len) = crate::render_wasm::fs_msg_call(own_call);
            format!("(i32.const {addr}) (i32.const {len})")
        }
    }
}

/// The `$ha $hl $op1 $op2` operands of a FILE floor call: [`fs_floor_head`] plus the
/// message operands' handles — `[call, first, second]` from a `_as_pair` twin names
/// both explicitly, in message order; otherwise 0/0 means "the path itself" / "none".
fn fs_floor_head_args(extra: &[ValueId], own_call: usize) -> String {
    let (op1, op2) = match extra {
        [_, first, second] => (format!("(local.get {})", local(*first)), format!("(local.get {})", local(*second))),
        _ => ("(i32.const 0)".to_string(), "(i32.const 0)".to_string()),
    };
    format!("{} {op1} {op2}", fs_floor_head(extra, own_call))
}

fn render_op_prim_mem_io_b(kind: &PrimKind, args: &[ValueId]) -> Option<String> {
    let w = |i: usize| format!("(i32.wrap_i64 (local.get {}))", local(args[i]));
    Some(match kind {
                // args_get_list() — the WASI CLI-args floor; builds a fresh owned
                // `List[String]` of argv[1..] in the preamble helper. dst is a heap Ptr
                // (i32 handle, value_reprs_wasm), so the call result sets the local DIRECTLY
                // (no i64 extend) — exactly like a LoadHandle.
                PrimKind::ArgsGetList => "(call $args_get_list (i32.const 1))".to_string(),
                PrimKind::ArgsGetListFull => "(call $args_get_list (i32.const 0))".to_string(),
                // env_get(name) — the WASI environ lookup floor; scans KEY=VALUE entries
                // for `name` + '=' and builds a fresh owned `Option[String]` in the
                // preamble helper. Same heap-Ptr name arg + heap-Ptr dst conventions as
                // ReadTextFile (the i32 handle passes DIRECTLY, no wrap).
                PrimKind::EnvGet => {
                    format!("(call $env_get (local.get {}))", local(args[0]))
                }
                // read_text_file(path) — the WASI file-read floor; opens + reads the file at
                // `path` and builds a fresh owned `Result[String, String]` in the preamble helper.
                // The path arg is a heap Ptr local (i32 handle, like a $list ptr), passed DIRECTLY
                // (no i32.wrap — it is already i32, like `Handle`'s arg). dst is a heap Ptr
                // (value_reprs_wasm), so the call result sets the local directly (no i64 extend).
                // The second operand is the `$validate` flag (#1506): 1 = refuse invalid UTF-8
                // with Err like native read_to_string; 0 = raw bytes like native std::fs::read.
                // Then the #2206 call head and message operands (`fs_floor_head_args`).
                PrimKind::ReadTextFile => {
                    format!(
                        "(call $read_text_file (local.get {}) (i32.const 1) {})",
                        local(args[0]),
                        fs_floor_head_args(&args[1..], FS_MSG_READ_TEXT)
                    )
                }
                PrimKind::ReadBytesFile => {
                    format!(
                        "(call $read_text_file (local.get {}) (i32.const 0) {})",
                        local(args[0]),
                        fs_floor_head_args(&args[1..], FS_MSG_READ_BYTES)
                    )
                }
                // read_dir(path) — the WASI directory-listing floor; path_open(O_DIRECTORY) +
                // fd_readdir, parses the dirent buffer (skipping `.`/`..`), sorts the names, and
                // builds a fresh owned `Result[List[String], String]` in the preamble helper.
                // Same heap-Ptr path arg + heap-Ptr dst conventions as ReadTextFile.
                PrimKind::ReadDir => {
                    format!(
                        "(call $read_dir (local.get {}) {})",
                        local(args[0]),
                        fs_floor_head(&args[1..], FS_MSG_LIST_DIR)
                    )
                }
                // write_text_file(path, content) — the WASI file-WRITE floor; path_open(O_CREAT|
                // O_TRUNC) + fd_write of `content`'s bytes, then builds a fresh owned
                // `Result[Unit, String]` (Ok(()) / Err) in the preamble helper. Both args are heap
                // Ptr locals (i32 handles), passed DIRECTLY (no i32.wrap). dst is a heap Ptr
                // (value_reprs_wasm), so the call result sets the local directly (no i64 extend).
                // Then the #2206 call head and message operands (`fs_floor_head_args`).
                PrimKind::WriteTextFile => {
                    format!(
                        "(call $write_text_file (local.get {}) (local.get {}) {})",
                        local(args[0]),
                        local(args[1]),
                        fs_floor_head_args(&args[2..], FS_MSG_WRITE)
                    )
                }
                // make_dir(path) — the WASI directory-CREATE floor; recursive
                // path_create_directory, then builds a fresh owned `Result[Unit, String]`
                // (Ok(()) / Err) in the preamble helper. The path arg is a heap Ptr local (i32
                // handle), passed DIRECTLY (no i32.wrap). dst is a heap Ptr (value_reprs_wasm), so
                // the call result sets the local directly (no i64 extend) — mirror WriteTextFile.
                PrimKind::MakeDir => {
                    format!(
                        "(call $make_dir (local.get {}) {})",
                        local(args[0]),
                        fs_floor_head(&args[1..], FS_MSG_MKDIR)
                    )
                }
                // rename(src, dst) — the WASI path_rename floor; both path args are heap Ptr
                // locals (i32 handles), passed DIRECTLY. dst is a heap Ptr (the Result block)
                // — mirror WriteTextFile.
                PrimKind::Rename => {
                    format!(
                        "(call $rename (local.get {}) (local.get {}))",
                        local(args[0]),
                        local(args[1])
                    )
                }
                // remove_all(path) — the WASI recursive-remove floor; recursively unlinks files +
                // removes directories under `path`, then builds a fresh owned `Result[Unit, String]`
                // (Ok(()) / Err) in the preamble helper. The path arg is a heap Ptr local (i32
                // handle), passed DIRECTLY (no i32.wrap). dst is a heap Ptr (value_reprs_wasm), so
                // the call result sets the local directly (no i64 extend) — mirror MakeDir.
                PrimKind::RemoveAll => {
                    format!(
                        "(call $remove_all (local.get {}) {})",
                        local(args[0]),
                        fs_floor_head(&args[1..], FS_MSG_REMOVE)
                    )
                }
                // path_exists(path) — the WASI path-stat floor; path_filestat_get on `path`,
                // yielding 1 if it exists (errno 0) else 0. The path arg is a heap Ptr local (i32
                // handle), passed DIRECTLY (no i32.wrap). UNLIKE the heap-result fs prims, dst is a
                // SCALAR Bool (an i64 local), so the i32 0/1 the $path_exists func returns is
                // i64.extend'd into it — exactly the FloatCmp scalar-Bool widening discipline.
                PrimKind::PathExists => {
                    format!("(i64.extend_i32_u (call $path_exists (local.get {})))", local(args[0]))
                }
                // path_filestat(bufaddr, path) — the WASI FULL-stat floor; path_filestat_get on
                // `path` writing the 64-byte filestat at `bufaddr`. The bufaddr arg is an i64
                // scalar local (wrapped to the i32 address); the path arg is a heap Ptr local
                // (i32 handle, passed directly). dst is the SCALAR errno (i64.extend'd) — the
                // PathExists scalar-result discipline.
                PrimKind::PathFilestat => {
                    format!(
                        "(i64.extend_i32_u (call $path_filestat_q (i32.wrap_i64 (local.get {})) (local.get {})))",
                        local(args[0]),
                        local(args[1])
                    )
                }
                // path_filestat_nofollow(bufaddr, path) — the NO-FOLLOW stat twin
                // ($path_filestat_nf, lookupflags 0); identical arg/dst discipline.
                PrimKind::PathFilestatNoFollow => {
                    format!(
                        "(i64.extend_i32_u (call $path_filestat_nf (i32.wrap_i64 (local.get {})) (local.get {})))",
                        local(args[0]),
                        local(args[1])
                    )
                }
                // read_line() — the WASI stdin-line floor; reads fd 0 byte-by-byte until '\n'/EOF
                // and builds a fresh owned canonical `String` (newline excluded, trailing '\r'
                // stripped) in the preamble helper. NO args. dst is a heap Ptr (value_reprs_wasm),
                // so the call result sets the local directly (no i64 extend) — like ArgsGetList.
                PrimKind::ReadLine => "(call $read_line)".to_string(),
                // read_n_bytes(n) — the WASI stdin-N-bytes floor; reads up to n bytes from fd 0 into a
                // fresh owned Bytes block (the byte-buffer layout, built in the preamble helper). The
                // n arg is an Int (i64 local), wrapped to i32 for the byte count; dst is a heap Ptr.
                PrimKind::ReadNBytes => {
                    format!("(call $read_n_bytes (i32.wrap_i64 (local.get {})))", local(args[0]))
                }
                // RAW refcount ops (the self-host drop/copy mechanism) — reuse the proven $rc_dec/
                // $rc_inc on the i32-wrapped handle. dst is None (Unit), so the `match dst` below
                // emits the call as a STATEMENT (no local.set).
                PrimKind::RcDec => format!("(call $rc_dec {})", w(0)),
                PrimKind::RcInc => format!("(call $rc_inc {})", w(0)),
        _ => return None,
    })
}
