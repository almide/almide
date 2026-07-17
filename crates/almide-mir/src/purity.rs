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
//! - **Effectful (the `effect fn` keyword)**: `env`, `fs`, `http`, `io`, `net`,
//!   `process`, `random`, `zlib`. Each declares ≥1 `effect fn`; the drift gate
//!   (`proofs/check-stdlib-purity-registry.sh`) asserts no PURE module ever does.
//! - **Impure-plain (host reach WITHOUT the `effect` keyword — the keyword
//!   UNDER-approximates)**: `datetime` (`now`/`monotonic_ns` read the wall clock),
//!   `args` (`raw` = `env.args()`, reads process args), `mem` (`save`/`restore`
//!   the allocator arena), `testing` (`assert_*` print/abort). These have zero
//!   `effect fn` yet reach the host, so the keyword alone is NOT a safety proof —
//!   they are walled WHOLESALE here and the gate records the justification.
//!
//! Module-granularity (not per-function) is sound BECAUSE the impure-plain modules
//! are walled in full: every function in an admitted module is genuinely pure, so
//! `is_pure(module)` needs no per-function table. (Per-function admission — e.g.
//! the pure `datetime.add_days` inside a walled module — is a later refinement.)
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
    "base64", "base64_encode", "bool", "bytes", "bytes_append_multi", "bytes_array", "bytes_core", "bytes_cursor", "bytes_f16", "bytes_lenprefix", "bytes_mutate", "bytes_rawptr", "bytes_skip", "bytes_split", "bytes_string", "bytes_typed", "codec_decode", "codec_encode", "datetime_arith", "datetime_calendar", "datetime_format", "datetime_parse_iso", "env_os", "env_temp_dir", "error", "error_chain", "error_message", "fan_map", "float", "float32", "float32_convert", "float_bits", "float_checked", "float_convert", "float_core", "float_extra", "float_parse", "float_round", "float_saturating", "float_to_string", "float_to_string_compound", "hex", "hex_encode", "html", "http_response", "http_url_decode", "int", "int16", "int16_convert", "int32", "int32_convert", "int8", "int8_convert", "int_bitcount", "int_bits", "int_checked", "int_hex", "int_rotate", "int_scalar", "int_sized", "int_to_float", "int_to_string", "int_wrap", "json", "json_as_array", "json_ctor", "json_get", "json_get_typed", "json_parse", "json_path", "json_scalar", "json_string", "list", "list_anyall", "list_chunk", "list_concat", "list_dedup", "list_enumerate", "list_filter", "list_filter_rc", "list_filter_str", "list_filtermap", "list_find", "list_find_int_str", "list_flatmap", "list_flatten", "list_fold", "list_fold_float", "list_fold_hrec", "list_fold_ols", "list_foldf", "list_get", "list_get_or", "list_get_or_str", "list_get_str", "list_hshare", "list_intersperse", "list_is_empty", "list_len", "list_make", "list_map", "list_map_s2h", "list_map_str", "list_modify", "list_pairs", "list_partition", "list_pop", "list_reduce", "list_reverse", "list_reverse_str", "list_scan", "list_search", "list_sort", "list_sort_by_keys", "list_sort_float", "list_sortby", "list_sortby_float", "list_str", "list_sum", "list_take_drop", "list_takedrop_str", "list_to_string", "list_to_string_b", "list_to_string_f", "list_to_string_ll", "list_to_string_lo", "list_to_string_lr", "list_to_string_lsi", "list_to_string_s", "list_unique", "list_uniqueby", "list_whilep", "list_zipwith", "map", "map_core", "map_fold_hacc", "map_hobj", "map_hval", "map_if", "map_ivh", "map_mlo", "map_msv", "map_skv", "map_str", "map_to_string", "math", "math_atan", "math_exp", "math_float", "math_fpow", "math_int", "math_lgamma", "math_log", "math_tanh", "math_trig", "matrix", "matrix_activations", "matrix_arith", "matrix_core", "matrix_ext", "matrix_fused", "matrix_shape", "option", "option_collect", "option_collect_map", "option_map", "option_pred", "option_to_string", "option_to_string_b", "option_to_string_f", "option_to_string_lf", "option_to_string_li", "option_to_string_ls", "option_to_string_msi", "option_to_string_nested", "option_to_string_oi", "option_to_string_s", "option_unwrap_or_str", "path", "regex", "regex_engine", "result", "result_collect", "result_core", "result_map", "result_to_string", "set", "set_core", "set_str", "set_to_string", "set_to_string_s", "string", "string_capitalize", "string_chars", "string_class", "string_cmp", "string_codepoint", "string_concat", "string_eq", "string_first_last", "string_from_bytes_self", "string_from_codepoint", "string_is_digit", "string_is_empty", "string_join", "string_len", "string_lines", "string_mutate", "string_pad", "string_quote", "string_repeat", "string_replace", "string_reverse", "string_rle", "string_search", "string_slice", "string_slice2", "string_split", "string_take_drop", "string_to_bytes", "string_to_int", "string_to_lower", "string_to_upper", "string_trim", "testing_assert", "uint16", "uint16_convert", "uint32", "uint32_convert", "uint64", "uint64_convert", "uint8", "uint8_convert", "value", "value_core", "value_utils",
];

/// Is `module` a provably-pure stdlib data module (reaches no host capability)?
/// `func` is accepted for a future per-function refinement but unused today —
/// admission is module-granular because the impure-plain modules are walled in
/// full (see the module doc).
pub fn is_pure(module: &str, func: &str) -> bool {
    PURE_MODULES.contains(&module) || is_pure_fn_in_impure_module(module, func)
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
            "assert_gt" | "assert_lt" | "assert_approx" | "assert_contains" | "assert_some" | "assert_ok"
        ),
        // The WASM-target env CONSTANTS: v0's emit_wasm folds os() to "wasi" and
        // temp_dir() to "/tmp" (calls_env.rs) — on this target they reach NO host
        // capability (the native target's real host reads live in the runtime
        // intrinsics, which the v1 wasm lowering never emits). The effectful env
        // fns (args, vars, …) stay walled/admitted-with-caps as before.
        "env" => matches!(func, "os" | "temp_dir"),
        // Pure data codecs on the blanket-impure http module (the network fns stay
        // walled): url_decode is a percent-decoder (stdlib/http_url_decode.almd).
        "http" => matches!(
            func,
            "url_decode" | "response" | "json" | "redirect" | "with_headers" | "status"
                | "body" | "set_header" | "get_header"
        ),
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
    }

    #[test]
    fn effectful_and_impure_plain_modules_are_walled() {
        // Effectful (the `effect fn` keyword).
        for m in ["fs", "http", "net", "io", "env", "process", "random", "zlib"] {
            assert!(!is_pure(m, "anything"), "{m} must be walled (effectful)");
        }
        // Impure-plain (host reach without the keyword).
        for m in ["datetime", "args", "mem", "testing"] {
            assert!(!is_pure(m, "now"), "{m} must be walled (impure-plain)");
        }
    }

    #[test]
    fn pure_modules_list_is_sorted_and_unique() {
        // Auditability + a cheap guard against accidental duplicates.
        let mut sorted = PURE_MODULES.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.as_slice(), PURE_MODULES, "PURE_MODULES must be sorted + unique");
    }
}
