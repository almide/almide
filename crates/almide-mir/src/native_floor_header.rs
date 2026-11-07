/// The runtime items the native header itself defines for a floor body: the
/// §13 abort (`Error: <msg>` on stderr, exit 1), the v0 prelude's
/// `almide_abort` / `almide_abort_oom` with the render's own stdout (std's
/// line-buffered handle, nothing of ours to flush first).
pub(crate) const FLOOR_HEADER: &str = "fn almide_abort(msg: impl std::fmt::Display) -> ! { eprintln!(\"Error: {}\", msg); std::process::exit(1) }\nfn almide_abort_oom() -> ! { almide_abort(\"out of memory\") }";
