/// The runtime items the native header itself defines for a floor body: the
/// §13 abort (`Error: <msg>` on stderr, exit 1), the v0 prelude's
/// `almide_abort` / `almide_abort_oom`, shared host lifecycle state, and the
/// render's stdout (std's line-buffered handle). Ordinary exits stop live
/// children before flushing; a contained abort unwinds to its handler.
pub(crate) const FLOOR_IO_HEADER: &str = include_str!("native_io_header.rs");
pub(crate) fn floor_header() -> &'static str {
    static HEADER: OnceLock<String> = OnceLock::new();
    HEADER.get_or_init(|| [
    "#[allow(unused_imports)] use std::io::{Read as _, Write as _};\n",
    almide_codegen::HOST_LIFECYCLE_PRELUDE,
    "use std::process::exit as almide_host_raw_exit;\nstruct AlmideHostExitGuard;\nimpl Drop for AlmideHostExitGuard { fn drop(&mut self) { almide_at_exit_run(); } }\nfn almide_host_exit(code: i32) -> ! { almide_at_exit_run(); almide_stdout_flush(); almide_host_raw_exit(code) }\n",
    "fn almide_abort(msg: impl std::fmt::Display) -> ! { almide_contained_unwind(|| msg.to_string()); eprintln!(\"Error: {}\", msg); std::process::exit(1) }\nfn almide_abort_oom() -> ! { eprintln!(\"Error: out of memory\"); std::process::exit(1) }\n",
    include_str!("native_io_header.rs")
    ].concat())
}
