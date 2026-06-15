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
    "base64", "bytes", "error", "float", "float32", "hex", "html", "int", "int16",
    "int32", "int8", "int_bitcount", "int_bits", "int_hex", "int_scalar", "int_sized", "int_to_string", "int_wrap", "json", "list", "list_dedup", "list_fold", "list_get", "list_get_or", "list_intersperse", "list_is_empty",
    "list_len", "list_make", "list_map", "list_modify", "list_reverse", "list_search", "list_sort", "list_sum", "list_take_drop", "list_unique", "map", "math", "math_int", "matrix", "option", "option_pred", "path", "regex", "result",
    "set", "string", "string_codepoint", "string_from_codepoint", "string_is_digit", "string_is_empty",
    "string_len", "string_pad", "string_repeat", "string_replace", "string_reverse", "string_search", "string_slice", "string_take_drop", "string_to_bytes", "string_trim",
    "uint16", "uint32",
    "uint64", "uint8", "value",
];

/// Is `module` a provably-pure stdlib data module (reaches no host capability)?
/// `func` is accepted for a future per-function refinement but unused today —
/// admission is module-granular because the impure-plain modules are walled in
/// full (see the module doc).
pub fn is_pure(module: &str, _func: &str) -> bool {
    PURE_MODULES.contains(&module)
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
