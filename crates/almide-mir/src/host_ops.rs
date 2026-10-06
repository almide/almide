//! Host operations — every stdlib call that reaches the host, certified as an
//! ORDINARY call whose capability is counted at the call site (#2739 ruling A,
//! #3302).
//!
//! # What the certificate covers
//! The ownership certificate covers how Almide hands a host op its arguments
//! and takes its result, not what the host does with them. A host op is
//! therefore lowered exactly like any other `Op::CallFn`: every heap argument
//! is BORROWED (a `CallArg::Handle` — live-checked, refcount unchanged, the
//! caller keeps and later drops its own reference) and a heap result is a
//! FRESH OWNED value the caller binds, moves out or drops. That is the
//! ordinary-call contract `proofs/CallModes.v` already assumes for a dotted
//! callee (`pipeline::program_witnesses`); no new mode exists for host ops.
//!
//! # One table, one matrix
//! [`HOST_OPS`] is the only place an effectful stdlib call is admitted into the
//! lowering (`lower::calls::is_admitted_effectful_pure_module_call` reads
//! nothing else). The runtime surface is audited as a MATRIX: every
//! `@intrinsic` declared by a capability-bearing module ([`CAP_MODULES`]) is
//! exactly one of
//!   - a host op (a row here, its capabilities counted),
//!   - a pure function (`crate::purity` admits it: no host reach), or
//!   - walled ([`WALLED`], with the reason it is not lowered at all).
//! `tests/host_ops_contract.rs` derives the intrinsic list from
//! `stdlib/<module>.almd` and fails on an unclassified cell, so a new
//! capability-bearing intrinsic cannot ride in as capability-free — the
//! #3302 class (`env.millis` / `datetime.now` read the clock while counting
//! nothing on the native classifier). The same test holds every row to its
//! stdlib `@intrinsic` and its `runtime/rs/src` signature (no `&mut`, no owned
//! heap parameter outside [`HostOp::by_value`], no returned reference), and
//! the corpus classifier refuses any lowered dotted call into a capability
//! module that resolves to neither a row nor a pure function.
//!
//! # Capabilities
//! [`crate::certificate::cap_witness`] counts a row's capabilities at the
//! `CallFn` (a dotted name is otherwise capability-free). The ids follow the
//! structural wasm leg (`almide_wasm::cert_project::op_caps`): clock reads and
//! the sleep reach `Clock`; argv and environment reads/writes `CliArgs` (the
//! manifest's `Env`); entropy `Entropy`; stdout `Stdout`; stdin `Stdin`; the
//! filesystem `FsRead` / `FsWrite`; the http client `Net`. A function that
//! reaches a capability it does not declare stays caps-unverified.

use crate::Capability;

/// One admitted host op.
#[derive(Clone, Copy, Debug)]
pub struct HostOp {
    /// The stdlib module (`env`, `http`).
    pub module: &'static str,
    /// The function the program calls (`set`, `get`, `start`).
    pub func: &'static str,
    /// The stdlib function whose `@intrinsic` names [`Self::symbol`]: the op
    /// itself, or the private function a self-hosted wrapper calls
    /// (`http.start` → `__call_start`).
    pub intrinsic_fn: &'static str,
    /// The native runtime symbol (`runtime/rs/src/*.rs`).
    pub symbol: &'static str,
    /// The capabilities one call reaches.
    pub caps: &'static [Capability],
    /// Runtime parameters (0-based) taken BY VALUE: the native call site hands
    /// the runtime a copy (a generic accumulator, a list it shuffles in place),
    /// so the Almide caller's reference is still borrowed, never moved.
    pub by_value: &'static [usize],
    /// The lowering routes this call to a typed twin
    /// (`random.choice_str`, `fs.fold_lines_msi`, see [`ROUTED_SUFFIXES`]).
    pub routed: bool,
    /// The stdlib declaration returns a `Result`: the call's block is a
    /// materialized Result a `match` / `!` dispatches on
    /// (`lower::is_self_host_materialized_result_fn`). Checked against the
    /// declaration by the contract test.
    pub returns_result: bool,
}

const fn op(module: &'static str, func: &'static str, symbol: &'static str, caps: &'static [Capability]) -> HostOp {
    HostOp { module, func, intrinsic_fn: func, symbol, caps, by_value: &[], routed: false, returns_result: false }
}

const fn wrapper(
    module: &'static str,
    func: &'static str,
    intrinsic_fn: &'static str,
    symbol: &'static str,
    caps: &'static [Capability],
) -> HostOp {
    HostOp { module, func, intrinsic_fn, symbol, caps, by_value: &[], routed: false, returns_result: false }
}

impl HostOp {
    const fn routed(self) -> Self {
        HostOp { routed: true, ..self }
    }
    const fn by_value(self, by_value: &'static [usize]) -> Self {
        HostOp { by_value, ..self }
    }
    const fn result(self) -> Self {
        HostOp { returns_result: true, ..self }
    }
}

use Capability::{CliArgs, Clock, Entropy, FsRead, FsWrite, Net, Stdin, Stdout};

const ENV: &[Capability] = &[CliArgs];
const CLOCK: &[Capability] = &[Clock];
const ENTROPY: &[Capability] = &[Entropy];
const READ: &[Capability] = &[FsRead];
const WRITE: &[Capability] = &[FsWrite];
const READ_WRITE: &[Capability] = &[FsRead, FsWrite];
const WRITE_ENTROPY: &[Capability] = &[FsWrite, Entropy];
const OUT: &[Capability] = &[Stdout];
const IN: &[Capability] = &[Stdin];
const NET: &[Capability] = &[Net];

/// Every admitted host op. Adding one is adding a row; the contract gate then
/// checks it against the stdlib declaration and the runtime signature.
pub const HOST_OPS: &[HostOp] = &[
    // ── env: argv, the environment, the clock ──
    op("env", "args", "almide_rt_env_args", ENV),
    op("env", "get", "almide_rt_env_get", ENV),
    op("env", "set", "almide_rt_env_set", ENV),
    // `env.cwd` reads PWD and `temp_dir` honours $TMPDIR: environment reads.
    op("env", "cwd", "almide_rt_env_cwd", ENV).result(),
    op("env", "temp_dir", "almide_rt_env_temp_dir", ENV),
    op("env", "unix_timestamp", "almide_rt_env_unix_timestamp", CLOCK),
    op("env", "millis", "almide_rt_env_millis", CLOCK),
    op("env", "sleep_ms", "almide_rt_env_sleep_ms", CLOCK),
    // ── process: argv[0]-inclusive arguments (every other process fn is walled) ──
    op("process", "args", "almide_rt_process_args", ENV),
    // ── datetime: the two clock reads (the rest of the module is pure) ──
    op("datetime", "now", "almide_rt_datetime_now", CLOCK),
    op("datetime", "monotonic_ns", "almide_rt_datetime_monotonic_ns", CLOCK),
    // ── random ──
    op("random", "int", "almide_rt_random_int", ENTROPY),
    op("random", "float", "almide_rt_random_float", ENTROPY),
    op("random", "choice", "almide_rt_random_choice", ENTROPY).routed(),
    op("random", "shuffle", "almide_rt_random_shuffle", ENTROPY).routed().by_value(&[0]),
    // ── io ──
    op("io", "print", "almide_rt_io_print", OUT),
    op("io", "write", "almide_rt_io_write", OUT),
    op("io", "write_bytes", "almide_rt_io_write_bytes", OUT),
    op("io", "read_line", "almide_rt_io_read_line", IN),
    op("io", "read_line_opt", "almide_rt_io_read_line_opt", IN),
    op("io", "read_all", "almide_rt_io_read_all", IN),
    op("io", "read_byte", "almide_rt_io_read_byte", IN),
    op("io", "read_n_bytes", "almide_rt_io_read_n_bytes", IN),
    // ── fs: reads ──
    op("fs", "read_text", "almide_rt_fs_read_text", READ).result(),
    op("fs", "read_bytes", "almide_rt_fs_read_bytes", READ).result(),
    op("fs", "read_bytes_raw", "almide_rt_fs_read_bytes_raw", READ).result(),
    op("fs", "read_lines", "almide_rt_fs_read_lines", READ).result(),
    op("fs", "read_text_if_exists", "almide_rt_fs_read_text_if_exists", READ).result(),
    op("fs", "read_bytes_if_exists", "almide_rt_fs_read_bytes_if_exists", READ).result(),
    op("fs", "read_lines_if_exists", "almide_rt_fs_read_lines_if_exists", READ).result(),
    op("fs", "read_bytes_raw_if_exists", "almide_rt_fs_read_bytes_raw_if_exists", READ).result(),
    op("fs", "list_dir", "almide_rt_fs_list_dir", READ).result(),
    op("fs", "exists", "almide_rt_fs_exists", READ),
    op("fs", "stat", "almide_rt_fs_stat", READ).result(),
    op("fs", "file_size", "almide_rt_fs_file_size", READ).result(),
    op("fs", "modified_at", "almide_rt_fs_modified_at", READ).result(),
    op("fs", "is_dir", "almide_rt_fs_is_dir", READ),
    op("fs", "is_file", "almide_rt_fs_is_file", READ),
    op("fs", "is_symlink", "almide_rt_fs_is_symlink", READ),
    op("fs", "walk", "almide_rt_fs_walk", READ).result(),
    op("fs", "glob", "almide_rt_fs_glob", READ).result(),
    // The streaming walkers: the accumulator is handed over by value, the
    // callback is a closure the walker invokes and drops.
    op("fs", "fold_lines", "almide_rt_fs_fold_lines", READ).routed().by_value(&[1]).result(),
    op("fs", "fold_lines_chunked", "almide_rt_fs_fold_lines_chunked", READ).routed().by_value(&[2]).result(),
    op("fs", "fold_lines_range", "almide_rt_fs_fold_lines_range", READ).routed().by_value(&[3]).result(),
    op("fs", "for_each_line", "almide_rt_fs_for_each_line", READ).result(),
    op("fs", "__fallible_fold_lines", "almide_rt_fs_fold_lines_effect", READ).routed().by_value(&[1]).result(),
    op("fs", "__fallible_for_each_line", "almide_rt_fs_for_each_line_effect", READ).result(),
    // `fs.temp_dir` is env.temp_dir's other spelling (C-189): an environment read.
    op("fs", "temp_dir", "almide_rt_fs_temp_dir", ENV),
    // ── fs: writes ──
    op("fs", "write", "almide_rt_fs_write", WRITE).result(),
    op("fs", "write_bytes", "almide_rt_fs_write_bytes", WRITE).result(),
    op("fs", "write_bytes_raw", "almide_rt_fs_write_bytes_raw", WRITE).result(),
    op("fs", "mkdir_p", "almide_rt_fs_mkdir_p", WRITE).result(),
    op("fs", "remove_all", "almide_rt_fs_remove_all", WRITE).result(),
    op("fs", "rename", "almide_rt_fs_rename", WRITE).result(),
    // Read-then-write compositions, and the unique-name creators (entropy suffix).
    op("fs", "copy", "almide_rt_fs_copy", READ_WRITE).result(),
    op("fs", "append", "almide_rt_fs_append", READ_WRITE).result(),
    op("fs", "remove", "almide_rt_fs_remove", READ_WRITE).result(),
    op("fs", "create_temp_dir", "almide_rt_fs_create_temp_dir", WRITE_ENTROPY).result(),
    op("fs", "create_temp_file", "almide_rt_fs_create_temp_file", WRITE_ENTROPY).result(),
    // ── http: the String client, one row per verb ──
    op("http", "get", "almide_http_get", NET).result(),
    op("http", "post", "almide_http_post", NET).result(),
    op("http", "put", "almide_http_put", NET).result(),
    op("http", "patch", "almide_http_patch", NET).result(),
    op("http", "delete", "almide_http_delete", NET).result(),
    op("http", "request", "almide_http_request", NET).result(),
    // The result-shape variants: (status, body) and the raw body bytes.
    op("http", "get_status", "almide_http_get_status", NET).result(),
    op("http", "request_status", "almide_http_request_status", NET).result(),
    op("http", "get_bytes", "almide_http_get_bytes", NET).result(),
    op("http", "request_bytes", "almide_http_request_bytes", NET).result(),
    // The whole-response twins (#1791).
    op("http", "get_response", "almide_http_get_response", NET).result(),
    op("http", "post_response", "almide_http_post_response", NET).result(),
    op("http", "put_response", "almide_http_put_response", NET).result(),
    op("http", "patch_response", "almide_http_patch_response", NET).result(),
    op("http", "delete_response", "almide_http_delete_response", NET).result(),
    op("http", "request_response", "almide_http_request_response", NET).result(),
    // The call handle (#2631): start, the four verbs over it, and the
    // limited streaming twin built on it.
    wrapper("http", "start", "__call_start", "almide_rt_http___call_start", NET).result(),
    op("http", "poll", "almide_http_call_poll", NET),
    op("http", "read_new", "almide_http_call_read_new", NET),
    op("http", "wait", "almide_http_call_wait", NET).result(),
    op("http", "cancel", "almide_http_call_cancel", NET),
    wrapper(
        "http",
        "request_stream_with_limits",
        "__request_stream_limited",
        "almide_rt_http___request_stream_limited",
        NET,
    )
    .result(),
];

/// The modules whose surface reaches the host. Each one's `@intrinsic`s form
/// the audited matrix; a stdlib module outside this list must be wholly pure
/// (`crate::purity::PURE_MODULES` — the contract test checks the partition).
pub const CAP_MODULES: &[&str] =
    &["args", "datetime", "env", "fs", "http", "io", "mem", "net", "process", "random", "testing", "zlib"];

/// The capability-module intrinsics that are NEITHER a host op NOR pure: they
/// are refused by the lowering outright, so they never reach a certificate.
/// `"*"` covers the rest of a module (a row or a pure admission still wins).
pub const WALLED: &[(&str, &str, &str)] = &[
    ("net", "*", "sockets, unix sockets and shared memory: native-only, no WASI floor (the native-FFI class)"),
    ("process", "*", "child processes, signals, stdin_lines and the exit / sleep / pid / env family: native-only; `exit` lowers as the ProcExit prim"),
    ("http", "serve", "the listener: native-only, and its handler is a closure the host invokes"),
    ("http", "request_stream", "a callback the host invokes per chunk; its limited twin is the admitted form"),
    ("http", "openai_streaming_call", "a callback the host invokes per delta; not admitted"),
    ("http", "anthropic_streaming_call", "a callback the host invokes per delta; not admitted"),
    ("http", "__call_start", "reached only through its wrapper row `http.start`"),
    ("http", "__request_stream_limited", "reached only through its wrapper row `http.request_stream_with_limits`"),
    ("http", "__openai_streaming_limited", "the limited streaming twins: not admitted"),
    ("http", "__anthropic_streaming_limited", "the limited streaming twins: not admitted"),
    ("testing", "assert_throws", "needs an unwinding catch the MIR rung has no form for"),
]
;

/// The typed-twin suffixes a ROUTED row's call name can carry
/// (`lower::list_heap_call_name`): random's element twins and the fold walkers'
/// accumulator twins. Every row can also carry the `_x` refusal twin.
pub const ROUTED_SUFFIXES: &[&str] = &["_str", "_pair", "_msi", "_ls", "_i", "_s"];

/// The row for `<module>.<func>`, if it is an admitted host op.
pub fn host_op(module: &str, func: &str) -> Option<&'static HostOp> {
    HOST_OPS.iter().find(|o| o.module == module && o.func == func)
}

/// The row an EMITTED `CallFn` name resolves to: the row itself, a routed
/// row's typed twin, or any row's `_x` refusal twin.
pub fn host_op_for_call_name(name: &str) -> Option<&'static HostOp> {
    let (module, func) = name.split_once('.')?;
    if let Some(row) = host_op(module, func) {
        return Some(row);
    }
    HOST_OPS.iter().filter(|o| o.module == module).find(|o| {
        let Some(rest) = func.strip_prefix(o.func) else { return false };
        rest == "_x" || (o.routed && ROUTED_SUFFIXES.contains(&rest))
    })
}

/// The capabilities a `CallFn` to the dotted `name` reaches as a host op.
pub fn host_op_capabilities(name: &str) -> &'static [Capability] {
    host_op_for_call_name(name).map_or(&[], |o| o.caps)
}

/// One matrix cell: how a capability-module function reaches the lowering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell {
    /// A row of [`HOST_OPS`]: admitted, capabilities counted.
    HostOp,
    /// Admitted by `crate::purity`: reaches no host capability.
    Pure,
    /// Refused by the lowering ([`WALLED`]).
    Walled,
    /// None of the above — a hole the contract gate reports.
    Unclassified,
}

/// The matrix cell of `<module>.<func>`.
pub fn classify(module: &str, func: &str) -> Cell {
    if host_op(module, func).is_some() {
        Cell::HostOp
    } else if crate::purity::is_pure(module, func) {
        Cell::Pure
    } else if WALLED.iter().any(|(m, f, _)| *m == module && (*f == func || *f == "*")) {
        Cell::Walled
    } else {
        Cell::Unclassified
    }
}

/// Is the emitted dotted `name` a call into a capability module that the
/// certificate does NOT account for — neither a host op (capabilities
/// counted) nor a pure function? The corpus classifier refuses any lowered
/// function holding one; it is the accept-but-unsafe case.
pub fn is_uncounted_host_call(name: &str) -> bool {
    let Some((module, func)) = name.split_once('.') else { return false };
    if !CAP_MODULES.contains(&module) || host_op_for_call_name(name).is_some() {
        return false;
    }
    let base = func.strip_suffix("_x").unwrap_or(func);
    !crate::purity::is_pure(module, base)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_unique_never_pure_and_never_walled() {
        for (i, a) in HOST_OPS.iter().enumerate() {
            assert!(!crate::purity::is_pure(a.module, a.func), "{}.{} is a host op and pure", a.module, a.func);
            assert!(CAP_MODULES.contains(&a.module), "{}.{}: module not in CAP_MODULES", a.module, a.func);
            assert!(!a.caps.is_empty(), "{}.{} reaches no capability", a.module, a.func);
            for b in &HOST_OPS[i + 1..] {
                assert!(!(a.module == b.module && a.func == b.func), "{}.{} listed twice", a.module, a.func);
            }
        }
    }

    /// The clock readers of #3302 count Clock; the structural leg's mapping holds.
    #[test]
    fn the_clock_readers_count_clock() {
        for name in ["env.millis", "env.unix_timestamp", "datetime.now", "datetime.monotonic_ns", "env.sleep_ms"] {
            assert_eq!(host_op_capabilities(name), &[Capability::Clock], "{name}");
        }
        assert_eq!(host_op_capabilities("env.set"), &[Capability::CliArgs]);
        assert_eq!(host_op_capabilities("http.get"), &[Capability::Net]);
        assert_eq!(host_op_capabilities("fs.copy"), &[Capability::FsRead, Capability::FsWrite]);
    }

    #[test]
    fn routed_twins_resolve_and_nothing_else_does() {
        assert_eq!(host_op_for_call_name("random.choice_str").map(|o| o.func), Some("choice"));
        assert_eq!(host_op_for_call_name("fs.fold_lines_msi").map(|o| o.func), Some("fold_lines"));
        assert_eq!(host_op_for_call_name("fs.__fallible_fold_lines_x").map(|o| o.func), Some("__fallible_fold_lines"));
        assert_eq!(host_op_for_call_name("fs.read_bytes_raw").map(|o| o.func), Some("read_bytes_raw"));
        assert_eq!(host_op_for_call_name("env.millis_x").map(|o| o.func), Some("millis"));
        // An unrouted row takes no typed suffix, and a pure fn never resolves.
        assert!(host_op_for_call_name("env.millis_str").is_none());
        assert!(host_op_for_call_name("http.get_header").is_none());
        assert!(host_op_for_call_name("list.map").is_none());
    }

    #[test]
    fn an_uncounted_host_call_is_reported() {
        // A walled or unknown capability-module call, emitted anyway, is uncounted.
        assert!(is_uncounted_host_call("process.exec"));
        assert!(is_uncounted_host_call("net.tcp_connect"));
        assert!(is_uncounted_host_call("env.some_future_reader"));
        // Host ops, pure fns and non-capability modules are accounted for.
        assert!(!is_uncounted_host_call("env.millis"));
        assert!(!is_uncounted_host_call("datetime.add_days"));
        assert!(!is_uncounted_host_call("http.get_header"));
        assert!(!is_uncounted_host_call("list.map"));
    }

    /// The call site counts the op's capability: an effect fn that reaches `Net`
    /// without declaring it is not caps-verified.
    #[test]
    fn the_capability_witness_counts_a_host_call() {
        let call = |name: &str| crate::Op::CallFn { dst: None, name: name.into(), args: Vec::new(), result: None };
        let f = crate::MirFunction {
            name: "f".into(),
            ops: vec![call("http.get"), call("env.set"), call("datetime.now"), call("list.len")],
            declared_caps: vec![Capability::Stdout, Capability::CliArgs],
            ..Default::default()
        };
        let w = crate::certificate::cap_witness(&f);
        assert_eq!(w.used, vec![Capability::Net, Capability::CliArgs, Capability::Clock]);
        assert!(!w.used.iter().all(|c| w.allowed.contains(c)), "Net/Clock undeclared: the bound must not hold");
    }
}
