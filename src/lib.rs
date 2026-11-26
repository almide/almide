#![recursion_limit = "512"]

//! Re-export map: `almide::module::*` paths resolve to the actual crates.
//! No stub files — just module aliases.

// ── Primitives (almide-base) ──
pub use almide_base::intern;
pub use almide_base::diagnostic;

// ── Diagnostic rendering (CLI-layer, moved out of almide-base) ──
pub mod diagnostic_render;

// ── Language (almide-lang) ──
pub use almide_lang::ast;
pub use almide_lang::lexer;
pub use almide_lang::parser;
pub use almide_lang::stdlib_info;
pub use almide_lang::time_units;

// ── IR (almide-ir) ──
pub use almide_ir as ir;

// ── Codegen (almide-codegen) ──
pub use almide_codegen as codegen;

// ── Frontend (almide-frontend) ──
pub use almide_frontend::check;
pub use almide_frontend::canonicalize;
pub use almide_frontend::lower;
pub use almide_frontend::ir_link;
pub use almide_frontend::import_table;
pub use almide_frontend::stdlib;
pub use almide_frontend::media_owners;
pub use almide_frontend::lint_shebang;

// ── Optimizer (almide-optimize) ──
pub use almide_optimize::optimize;
pub use almide_optimize::mono;

// The stock-WASI transform of a structural module (#2554): what a library
// consumer applies to `wasm_route`'s structural bytes before a p1 runtime
// loads them (`RoutedWasm::stock_wasi` does it for you).
pub use almide_wasi as wasi;

// Reference interpreter (the third cross-target judge).
pub use almide_interp as interp;

// ── Tools (almide-tools) ──
pub use almide_tools::fmt;
pub use almide_tools::regroup;
pub use almide_tools::interface;
pub use almide_tools::almdi;

// ── Types (composite: almide-lang types + frontend TypeEnv + TypeMap) ──
pub mod types {
    pub use almide_lang::types::*;
    pub use almide_frontend::types::{TypeEnv, TypeMap};
}

// ── CLI-only modules (remain in main crate) ──
pub mod project;
pub mod cargo_cfg;
pub mod wasm_leg;
pub mod wasm_route;
pub mod serve_export;
pub mod project_fetch;
pub mod native_grant;
// The native build gate over the linked verifier (#4828, W2).
pub mod certificate_gate_native;
pub mod media_grant;
pub mod trust;
pub mod git_source;
pub mod package_tree;
pub mod fetch;
pub mod resolve;
pub mod source_overlay;
pub mod effect_value_gate;
pub mod certificate_gate;

// ── Wall marker: the machine-readable line of a wasm wall's stderr ──
//
// When the verified wasm renderer refuses a program (an honest wall, #782),
// the HUMAN diagnostic is free to evolve — #931 already reworked it once,
// from `wall: {e:?}` to headline + caret + note — but the nightly fuzzer's
// honest-wall classifier string-matched the OLD form and silently rotted:
// every wall became a WasmBuildFailure finding and the night went red on
// subset debt. This constant is the contract now: a walled
// `almide build/run --target wasm` always emits exactly one stderr line
// `wall: <reason>` (whitespace-flattened, after the human diagnostic), and
// xtarget-fuzz's ladder classifies on this same constant through its
// `almide` path-dep. tests/wall_shape_rendering_test.rs pins the emission —
// a diagnostic rework that drops the line fails the test, not the nightly.
pub const WASM_WALL_MARKER: &str = "wall: ";

// ── CLI output routing ──
//
// A CLI's whole job is printing to the user, so println!/eprintln!/print!/
// eprint! call sites are architecturally legitimate here (not debug
// leftovers) — but a lint that counts one issue per call site still turns
// every one of them into a separate hit, and this binary has ~300 of them
// scattered across two dozen command functions. Routing all output through
// these four named functions collapses that surface to just the four
// functions below, and makes a future `--quiet` flag or output-capturing
// test trivial to add (swap the bodies, nothing else changes).
//
// They are also THE output sink of decision row front.diagnostic-echo
// (#4819): each writes `almide_base::echo::visible` of its text, so a hidden
// character no caller escaped is escaped here, and a new output path is
// escaped by construction. tests/echo_sink_test.rs refuses a print macro or a
// raw stdout / stderr handle anywhere else in this binary's sources; the one
// raw channel is `relay_program_output`, below.
pub fn out(s: &str) { println!("{}", sink_text(s)); }
pub fn out_no_nl(s: &str) { print!("{}", sink_text(s)); }
pub fn err(s: &str) { eprintln!("{}", sink_text(s)); }
pub fn err_no_nl(s: &str) { eprint!("{}", sink_text(s)); }

/// What the sink writes for `s`: `s` itself when it holds no hidden
/// character; else, when `s` is one JSON document (`--emit-ast`, `--json`
/// rows, an interface), each hidden character as its JSON escape `\uXXXX`,
/// so the document still parses to what it said; else each as `\u{XXXX}`.
fn sink_text(s: &str) -> std::borrow::Cow<'_, str> {
    let escaped = almide_base::echo::visible(s);
    if matches!(escaped, std::borrow::Cow::Borrowed(_)) {
        return escaped;
    }
    if serde_json::from_str::<serde_json::Value>(s).is_ok() {
        return almide_base::echo::json_visible(s);
    }
    escaped
}

/// One JSON document as a line on stdout (`--json`, MCP), every hidden
/// character inside its strings written as its JSON `\uXXXX` escape
/// (`echo::json_visible`), so the document still parses to what it said. An
/// error writing it (the client closed the pipe) is returned, not a panic.
pub fn out_json_line(json: &str) -> std::io::Result<()> {
    use std::io::Write as _;
    let mut o = std::io::stdout().lock();
    writeln!(o, "{}", almide_base::echo::json_visible(json))?;
    o.flush()
}

/// One LSP base-protocol frame on stdout: the header and the JSON body, the
/// body passed through `echo::json_visible` before its length is taken.
pub fn out_lsp_frame(json: &str) -> std::io::Result<()> {
    use std::io::Write as _;
    let body = almide_base::echo::json_visible(json);
    let mut o = std::io::stdout().lock();
    write!(o, "Content-Length: {}\r\n\r\n{}", body.len(), body)?;
    o.flush()
}

/// Flush the sink's stdout (a prompt written with [`out_no_nl`]).
pub fn flush_out() {
    use std::io::Write as _;
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
}

/// Is the sink's stderr a terminal (the renderer colours only then)?
pub fn stderr_is_terminal() -> bool {
    use std::io::IsTerminal as _;
    std::io::stderr().is_terminal()
}

/// The byte-exact relay of what a USER PROGRAM wrote — `almide run --target
/// wasm` replaying the embedded host's captured stdout and stderr. That is
/// the program's output, which every target writes unchanged (the
/// cross-target contracts compare it byte for byte), not text the compiler
/// echoes; it is the one write that does not pass `echo::visible`.
pub fn relay_program_output(stdout: &str, stderr: &str) {
    use std::io::Write as _;
    print!("{stdout}");
    eprint!("{stderr}");
    let _ = std::io::stdout().flush();
}
