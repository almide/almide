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
//! `is_pure(module)` needs no per-function table. A function of any other module
//! is admitted one at a time from its declarations (`is_pure_fn_in_impure_module`,
//! #4827): no host category, and no serving body reaching a declared host op.
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
    "bytes_mutate",
    "bytes_split",
    "bytes_string",
    "codec_decode",
    "codec_encode",
    "datetime_arith",
    "datetime_calendar",
    "datetime_format",
    "datetime_parse_iso",
    "deque",
    "env_os",
    "fan_any",
    "fan_map",
    "ffi_bridge",
    "float",
    "float32",
    "float64",
    "float64_convert",
    "float_bits",
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
    "string_search",
    "string_slice",
    "string_slice2",
    "string_take_drop",
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
/// A module in `PURE_MODULES` is admitted whole; any other module's `func` is
/// admitted when its declarations derive it host-free (see the module doc).
pub fn is_pure(module: &str, func: &str) -> bool {
    PURE_MODULES.contains(&module)
        || is_pure_fn_in_impure_module(module, func)
        // A runtime-backed type's declared repr (#4204), derived from its row.
        || almide_lang::stdlib_info::is_runtime_backed_repr(module, func)
        // Its declared equality (#4749), derived from the same row.
        || almide_lang::stdlib_info::is_runtime_backed_eq(module, func)
}

/// A host-free function of a module that is not admitted whole (#4827),
/// derived from the declarations rather than listed: something serves
/// `module.func` (its declaration in the module's source or a self-host
/// registry body), its declaration charges no host category (the `effect`
/// keyword and `@category`, read by `effect::stdlib_call_effect`), and no
/// body serving it reaches a host op the floor declares (`@prim_op(host)`),
/// directly or through what it calls. A new fn that reaches the network or a
/// file is therefore never admitted by its module's name or its prefix. A
/// row of the host-op matrix (`host_ops::HOST_OPS`) is a host call by that
/// declaration, whatever its module's row says.
fn is_pure_fn_in_impure_module(module: &str, func: &str) -> bool {
    crate::host_ops::host_op(module, func).is_none()
        && !almide_ir::host_reach::serving_bodies(module, func).is_empty()
        && almide_ir::effect::stdlib_call_effect(module, func).is_empty()
        && !almide_ir::host_reach::stdlib_fn_reaches_host(module, func)
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

    /// #4827: a function of a module not admitted whole is admitted from its
    /// declarations — never by its module or a name prefix. A served data
    /// accessor is admitted; a fn whose serving body reads the clock or the
    /// entropy source is not; a name nothing serves (a future
    /// `__http_response_*` helper) is not.
    #[test]
    fn per_function_admission_is_derived_from_declarations() {
        assert!(is_pure("datetime", "add_days"));
        assert!(is_pure("http", "status"));
        assert!(!is_pure("datetime", "now"));
        assert!(!is_pure("random", "int"));
        assert!(!is_pure("http", "__http_response_reaches_net"));
    }

    /// #4827: holding a handle is the capability, not purity. The generic
    /// handle fns dispatch to the caller's conformer (`h.__self_read`), so
    /// they stay refused although they declare `@category()`.
    #[test]
    fn the_generic_handle_fns_are_not_pure() {
        for f in ["read", "read_exact", "read_to_end", "write", "close"] {
            assert!(!is_pure("io", f), "io.{f} reads or writes a handle");
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
