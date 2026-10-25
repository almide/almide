//! Stdlib purity registry — the UNTRUSTED emission-side predicate that decides
//! whether a `<module>.<func>` stdlib call may be lowered into the value-semantics
//! subset (PCC §capability).
//!
//! # Why this exists (the capability-soundness crux)
//! The proven capability checker (`proofs/CapabilityBound.v`) accepts iff
//! `used ⊆ allowed`, where the UNTRUSTED emitter ([`crate::certificate::cap_witness`])
//! derives `used` ONLY from `Op::Call`'s typed [`crate::RtFn`]. An `Op::CallFn`
//! to a name absent from the program map (every real stdlib function) contributes
//! ZERO capabilities. So lowering a stdlib call as `Op::CallFn` is sound **iff the
//! callee reaches no host capability** — i.e. is PURE. An EFFECTFUL stdlib call
//! lowered this way would silently omit its real capability from `used`, and the
//! checker would still accept: **accept-but-unsafe**. The whole correctness burden
//! therefore reduces to one predicate decided BEFORE lowering: is this call pure?
//!
//! This registry is the answer, and it lives entirely on the untrusted emission
//! side — the proven checker is byte-unchanged (still a pure `used ⊆ allowed`
//! subset test, no stdlib enumeration in the proof).
//!
//! # The classification (module-granular, audited)
//! A module is admitted as PURE iff EVERY function it declares is a pure data
//! transformation — no host capability, no clock, no I/O, no randomness, no global
//! state. The non-pure modules fall in two classes, both WALLED:
//!
//! - **Effectful (the `effect fn` keyword)**: `args`, `env`, `fs`, `http`, `io`,
//!   `net`, `process`, `random`, `zlib`. Each declares ≥1 `effect fn`; the drift gate
//!   (`proofs/check-stdlib-purity-registry.sh`) asserts no PURE module ever does.
//! - **Impure-plain (host reach WITHOUT the `effect` keyword — the keyword
//!   UNDER-approximates)**: `datetime` (`now`/`monotonic_ns` read the wall clock),
//!   `mem` (`save`/`restore` the allocator arena), `testing` (`assert_*`
//!   print/abort). (`args` was here until its readers became `effect fn`s,
//!   dialect epoch 13.) These have zero
//!   `effect fn` yet reach the host, so the keyword alone is NOT a safety proof —
//!   they are walled WHOLESALE here and the gate records the justification.
//!
//! Module-granularity (not per-function) is sound BECAUSE the impure-plain modules
//! are walled in full: every function in an admitted module is genuinely pure, so
//! `is_pure(module)` needs no per-function table. (Per-function admission — e.g.
//! the pure `datetime.add_days` inside a walled module — is a later refinement.)
//!
//! EFFECTFUL calls are admitted only through [`crate::host_ops`] (#2739, #3302):
//! one table, audited as a matrix over every capability module's intrinsics.
//! Each row is certified as an ordinary call and `cap_witness` counts its
//! capabilities at the call site, so `used` stays complete whether or not a
//! self-host body is in the program map.
//!
//! HIGHER-ORDER calls are walled SEPARATELY in lowering (a pure module like `list`
//! still has `list.map`, whose closure argument invokes user code with unmodelled
//! capabilities) — see `lower::is_higher_order`. Purity here is necessary, not
//! sufficient; both gates must pass.

/// Stdlib modules whose every function is a pure data transformation (reaches no
/// host capability). A `<module>.<func>` call into one of these — when also
/// first-order — lowers into the value-semantics subset with an empty (and
/// therefore complete) capability witness. Kept SORTED for auditability; the
/// drift gate re-derives the effectful set from `stdlib/*.almd` and fails if any
/// name here ever declares an `effect fn`.
pub const PURE_MODULES: &[&str] = &[
    "base64",
    "base64_encode",
    "bigint",
    "bool",
    "bytes",
    "bytes_append_multi",
    "bytes_array",
    "bytes_core",
    "bytes_ct",
    "bytes_cursor",
    "bytes_f16",
    "bytes_lenprefix",
    "bytes_mutate",
    "bytes_rawptr",
    "bytes_skip",
    "bytes_split",
    "bytes_string",
    "bytes_typed",
    "codec_decode",
    "codec_encode",
    "datetime_arith",
    "datetime_calendar",
    "datetime_format",
    "datetime_parse_iso",
    "deque",
    "env_os",
    "error",
    "error_chain",
    "error_message",
    "fan_any",
    "fan_map",
    "ffi_bridge",
    "float",
    "float32",
    "float32_convert",
    "float64",
    "float64_convert",
    "float_bits",
    "float_checked",
    "float_convert",
    "float_core",
    "float_extra",
    "float_fmod",
    "float_parse",
    "float_round",
    "float_saturating",
    "float_to_string",
    "hash",
    "hash_impl",
    "heap",
    "hex",
    "hex_encode",
    "html",
    "http_request",
    "http_response",
    "int",
    "int16",
    "int16_convert",
    "int32",
    "int32_convert",
    "int64",
    "int64_convert",
    "int8",
    "int8_convert",
    "int_bitcount",
    "int_bits",
    "int_bounds",
    "int_checked",
    "int_hex",
    "int_rotate",
    "int_scalar",
    "int_sized",
    "int_to_float",
    "int_to_string",
    "int_wrap",
    "json",
    "json_ctor",
    "json_get_typed",
    "json_parse",
    "json_path",
    "list",
    "list_anyall",
    "list_chunk",
    "list_concat",
    "list_dedup",
    "list_enumerate",
    "list_filter",
    "list_filter_rc",
    "list_filter_str",
    "list_filtermap",
    "list_find",
    "list_find_int_str",
    "list_flatmap",
    "list_flatten",
    "list_fold",
    "list_fold_float",
    "list_fold_hrec",
    "list_fold_hsca",
    "list_fold_ols",
    "list_foldf",
    "list_get",
    "list_get_or",
    "list_get_or_str",
    "list_get_str",
    "list_heapelem",
    "list_hshare",
    "list_intersperse",
    "list_is_empty",
    "list_len",
    "list_make",
    "list_map",
    "list_map_s2h",
    "list_map_str",
    "list_modify",
    "list_ord_compound",
    "list_pairs",
    "list_partition",
    "list_pop",
    "list_reduce",
    "list_reverse",
    "list_reverse_str",
    "list_scan",
    "list_search",
    "list_sort",
    "list_sort_by_keys",
    "list_sort_by_str_key",
    "list_sort_float",
    "list_sortby",
    "list_sortby_float",
    "list_str",
    "list_sum",
    "list_take_drop",
    "list_takedrop_str",
    "list_to_string",
    "list_to_string_b",
    "list_to_string_f",
    "list_to_string_ll",
    "list_to_string_lo",
    "list_to_string_lob",
    "list_to_string_lr",
    "list_to_string_lsi",
    "list_to_string_s",
    "list_unique",
    "list_uniqueby",
    "list_whilep",
    "list_zipwith",
    "map",
    "map_core",
    "map_fold_hacc",
    "map_hobj",
    "map_hval",
    "map_if",
    "map_ivh",
    "map_mlo",
    "map_msv",
    "map_skv",
    "map_str",
    "map_to_string",
    "map_typechange",
    "map_vkey",
    "math",
    "math_atan",
    "math_exp",
    "math_float",
    "math_fpow",
    "math_int",
    "math_lgamma",
    "math_log",
    "math_tanh",
    "math_trig",
    "matrix",
    "matrix_arith",
    "matrix_core",
    "matrix_ext",
    "matrix_fused",
    "matrix_shape",
    "option",
    "option_collect",
    "option_collect_map",
    "option_map",
    "option_pred",
    "option_to_string",
    "option_to_string_b",
    "option_to_string_f",
    "option_to_string_lf",
    "option_to_string_li",
    "option_to_string_ls",
    "option_to_string_msi",
    "option_to_string_nested",
    "option_to_string_oi",
    "option_to_string_s",
    "option_unwrap_or_str",
    "path",
    "regex",
    "regex_engine",
    "result",
    "result_collect",
    "result_core",
    "result_map",
    "result_to_string",
    "secret",
    "set",
    "set_core",
    "set_str",
    "set_to_string",
    "set_to_string_s",
    "string",
    "string_byte_slice",
    "string_capitalize",
    "string_chars",
    "string_class",
    "string_cmp",
    "string_codepoint",
    "string_concat",
    "string_eq",
    "string_err_text",
    "string_first_last",
    "string_from_bytes_self",
    "string_from_codepoint",
    "string_is_digit",
    "string_is_empty",
    "string_join",
    "string_len",
    "string_lines",
    "string_mutate",
    "string_pad",
    "string_quote",
    "string_repeat",
    "string_reverse",
    "string_rle",
    "string_search",
    "string_slice",
    "string_slice2",
    "string_take_drop",
    "string_to_bytes",
    "string_to_int",
    "string_to_lower",
    "string_to_upper",
    "string_trim",
    "testing_assert",
    "uint16",
    "uint16_convert",
    "uint32",
    "uint32_convert",
    "uint64",
    "uint64_convert",
    "uint8",
    "uint8_convert",
    "url",
    "value",
    "value_core",
    "value_utils",
    "zlib_inflate",
];

/// Is `module` a provably-pure stdlib data module (reaches no host capability)?
/// `func` is accepted for a future per-function refinement but unused today —
/// admission is module-granular because the impure-plain modules are walled in
/// full (see the module doc).
pub fn is_pure(module: &str, func: &str) -> bool {
    PURE_MODULES.contains(&module)
        || is_pure_fn_in_impure_module(module, func)
        // A runtime-backed type's declared repr (#4204), derived from its row.
        || almide_lang::stdlib_info::is_runtime_backed_repr(module, func)
}

/// Whether bundled `module` declares a public (non-`local`) fn named `func`,
/// read from its `.almd` source — the declaration is the one place the
/// module's surface is written down.
fn declares_public_fn(module: &str, func: &str) -> bool {
    let Some(src) = almide_lang::stdlib_info::bundled_source(module) else {
        return false;
    };
    src.lines().any(|line| {
        let decl = line.strip_prefix("effect ").unwrap_or(line);
        decl.strip_prefix("fn ").is_some_and(|rest| {
            rest.strip_prefix(func).is_some_and(|tail| tail.starts_with(['(', '[']))
        })
    })
}

/// A genuinely-PURE function living inside an otherwise impure-plain module. `datetime` is walled
/// as a whole because `now`/`monotonic_ns` read the wall clock, but its calendar arithmetic over a
/// plain i64 Unix-seconds timestamp reaches NO host capability — admitting it is the refinement the
/// module-level under-approximation anticipated. SOUND: every fn listed is scalar arithmetic /
/// comparison (its self-host body in datetime_arith.almd uses no prim host op, no Stdout); the
/// effectful `now`/`monotonic_ns` are deliberately NOT listed, so they stay caps-walled.
fn is_pure_fn_in_impure_module(module: &str, func: &str) -> bool {
    match module {
        "datetime" => matches!(
            func,
            "add_days"
                | "add_hours"
                | "add_minutes"
                | "add_seconds"
                | "diff_seconds"
                | "from_unix"
                | "to_unix"
                | "hour"
                | "minute"
                | "second"
                | "is_before"
                | "is_after"
                | "year"
                | "month"
                | "day"
                | "weekday"
                | "from_parts"
                | "to_iso"
                | "format"
                | "parse_iso"
        ),
        // The self-hosted assert family: PURE-OR-HALT (a failed assert aborts via
        // `prim.die` — a halt, not an effect; stdlib/testing_assert.almd). The module
        // stays impure-plain (`testing.now`-class fns remain walled).
        "testing" => matches!(
            func,
            "assert_gt"
                | "assert_snapshot"
                | "assert_lt"
                | "assert_approx"
                | "assert_contains"
                | "assert_some"
                | "assert_ok"
                | "assert_none"
                | "assert_err"
                | "assert_ok_sc"
                | "assert_err_sc"
        ),
        // The WASM-target env CONSTANTS: v0's emit_wasm folds os() to "wasi" and
        // temp_dir() to "/tmp" (calls_env.rs) — on this target they reach NO host
        // capability (the native target's real host reads live in the runtime
        // intrinsics, which the v1 wasm lowering never emits). The effectful env
        // fns (args, vars, …) stay walled/admitted-with-caps as before.
        // `temp_dir` left this list when its self-host started honoring the
        // host's `$TMPDIR` claim (the Go/Python WASI convention) — an env
        // read is a capability, admitted the same way env.get is.
        "env" => matches!(func, "os"),
        // The whole zlib surface is a DETERMINISTIC data transform of its
        // input bytes (#1700): the self-host bodies (stdlib/zlib_inflate.almd
        // / zlib_deflate.almd) reach no host capability, and the `effect fn`
        // spelling is the Result carrier, not an effect. The bounded decoders
        // (`inflate_max` / `decompress_max` / `gunzip_max`, #4020) are the same
        // transforms with a caller-chosen output bound. Admission is read from
        // the module's own declarations (stdlib/zlib.almd) rather than from a
        // list of its names here — a list is a second copy of the surface, and
        // the bounded decoders were missing from it. A name zlib does not
        // declare stays walled, and a future zlib fn that reaches a real
        // capability is a host op, which `host_ops::classify` answers before
        // it asks purity.
        "zlib" => declares_public_fn("zlib", func),
        // `ffi` (R-ffi-01): `address` / `to_int` build an address as a value,
        // and `read` / `write` copy through one inside the program's own
        // memory (stdlib/ffi_bridge.almd over bytes_rawptr.almd: prim loads and stores, no host op).
        // The `effect fn` spelling of read/write marks memory outside Almide's
        // values (R-ffi-02), not a host capability; `--profile critical`
        // refuses the module wholesale (#4008).
        "ffi" => true,
        // Pure data on the blanket-impure http module (the network fns stay
        // walled): the response and request reps (stdlib/http_response.almd,
        // stdlib/http_request.almd — List[String] data), the HttpMessage
        // accessors over them and their per-type requirements (R-http-12),
        // and the router layer (stdlib/http.almd, #2588). The router's
        // construction and dispatch are pure; a HANDLER's own capabilities
        // are counted transitively through the closure it holds, exactly
        // like a list.map callback. `serve` (the listener) stays walled.
        "http" => {
            func.starts_with("__http_request_")
                || func.starts_with("__http_response_")
                || matches!(
                    func,
                    "new_request"
                        | "__new_request"
                        | "method"
                        | "path"
                        | "query_params"
                        | "param"
                        | "query"
                        | "queries"
                        | "route"
                        | "router"
                        | "mount"
                        | "wrap"
                        | "recover"
                        | "logger"
                        | "__with_params"
                        | "__with_path"
                        | "__flat"
                        | "__pairs"
                        | "response"
                        | "json"
                        | "redirect"
                        | "with_headers"
                        | "__with_headers"
                        | "set_status"
                        | "status"
                        | "header"
                        | "header_values"
                        | "headers"
                        | "body"
                        | "body_bytes"
                        | "set_header"
                        | "set_body"
                        | "set_body_bytes"
                )
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pure_modules_admit_data_transforms() {
        assert!(is_pure("string", "trim"));
        assert!(is_pure("list", "len"));
        assert!(is_pure("json", "parse"));
        assert!(is_pure("path", "join"));
        // zlib is admitted by its own declarations: every declared fn, the
        // bounded decoders included, and no undeclared name.
        assert!(is_pure("zlib", "inflate"));
        assert!(is_pure("zlib", "gunzip_max"));
        assert!(!is_pure("zlib", "inflat"));
    }

    #[test]
    fn effectful_and_impure_plain_modules_are_walled() {
        // Effectful (the `effect fn` keyword).
        for m in [
            "args", "fs", "http", "net", "io", "env", "process", "random", "zlib",
        ] {
            assert!(!is_pure(m, "anything"), "{m} must be walled (effectful)");
        }
        // Impure-plain (host reach without the keyword).
        for m in ["datetime", "testing"] {
            assert!(!is_pure(m, "now"), "{m} must be walled (impure-plain)");
        }
    }

    #[test]
    fn pure_modules_list_is_sorted_and_unique() {
        // Auditability + a cheap guard against accidental duplicates.
        let mut sorted = PURE_MODULES.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.as_slice(),
            PURE_MODULES,
            "PURE_MODULES must be sorted + unique"
        );
    }
}
