//! Stream fusion's eligibility judgment: may a pipeline run element by element
//! instead of stage by stage?
//!
//! Both backends fuse. Native rewrites chains into an `IterChain`
//! (almide-codegen `pass_stream_fusion`); the structural wasm leg fuses
//! `map*/filter* → fold` in its emitter (almide-wasm `list_fuse`). Staged
//! evaluation runs every element through one stage before the next stage
//! starts, so a fused pipeline is only the same program when reordering the
//! stages' work cannot be seen. Each backend used to decide that with its own
//! module lists and its own abort rule, and the wasm one admitted pipelines
//! native refused: a callback calling a printing user-module fn interleaved its
//! output, and two stages that could each abort reported the other abort
//! (#2953). The vocabulary and the sequencing rule live HERE; each backend only
//! walks its own IR to fill in a [`Stage`] per callback.

/// Stdlib modules whose calls are effects, or read the outside world.
pub const EFFECT_MODULES: &[&str] = &[
    "args", "datetime", "env", "fan", "fs", "http", "io", "mem", "net",
    "process", "random", "sse", "testing", "time", "zlib",
];

/// Stdlib modules whose every fn is TOTAL: pure arithmetic / conversion that
/// never aborts. (`list`/`string`/`map` have aborting members —
/// `list.chunk(xs, 0)` — so a call into them counts as a possible abort.)
pub const TOTAL_MODULES: &[&str] = &["int", "float", "math", "bool"];

/// A call into stdlib `module` observes nothing and changes nothing outside
/// its result. A user module is never answered here: its fns are judged from
/// their bodies, or refused.
pub fn stdlib_module_is_pure(module: &str) -> bool {
    almide_lang::stdlib_info::is_stdlib_module(module) && !EFFECT_MODULES.contains(&module)
}

/// Every fn of stdlib `module` returns without aborting.
pub fn stdlib_module_is_total(module: &str) -> bool {
    TOTAL_MODULES.contains(&module)
}

/// One stage of a pipeline, as its backend judged it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stage {
    /// The stage's callbacks observe nothing (no effect, no write to a
    /// captured var).
    pub pure: bool,
    /// The stage cannot abort.
    pub total: bool,
    /// The stage can stop the pipeline early (`take`, `find`, `any`, `all`).
    pub short_circuits: bool,
}

/// May stages that run in this order be fused? Every stage must be pure; at
/// most ONE may abort (fused, a later stage's abort on an early element would
/// fire before an earlier stage's abort on a later element); and every stage
/// before a short-circuit must be total (the unfused program runs the earlier
/// stage over EVERY element before the short-circuit stops anything).
pub fn stages_mergeable(stages: &[Stage]) -> bool {
    if !stages.iter().all(|s| s.pure) {
        return false;
    }
    if stages.iter().filter(|s| !s.total).count() > 1 {
        return false;
    }
    let mut seen_partial = false;
    for s in stages {
        if s.short_circuits && seen_partial {
            return false;
        }
        if !s.total {
            seen_partial = true;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    const PURE_TOTAL: Stage = Stage { pure: true, total: true, short_circuits: false };
    const PURE_PARTIAL: Stage = Stage { pure: true, total: false, short_circuits: false };

    #[test]
    fn two_stages_that_can_abort_do_not_fuse() {
        assert!(stages_mergeable(&[PURE_PARTIAL, PURE_TOTAL, PURE_TOTAL]));
        assert!(!stages_mergeable(&[PURE_PARTIAL, PURE_PARTIAL, PURE_TOTAL]));
    }

    #[test]
    fn an_impure_stage_does_not_fuse() {
        assert!(!stages_mergeable(&[Stage { pure: false, ..PURE_TOTAL }, PURE_TOTAL]));
    }

    #[test]
    fn a_short_circuit_after_a_partial_stage_does_not_fuse() {
        let short = Stage { short_circuits: true, ..PURE_TOTAL };
        assert!(stages_mergeable(&[short, PURE_PARTIAL]));
        assert!(!stages_mergeable(&[PURE_PARTIAL, short]));
    }

    #[test]
    fn user_modules_and_effect_modules_are_not_pure_stdlib() {
        assert!(stdlib_module_is_pure("list"));
        assert!(!stdlib_module_is_pure("fs"));
        assert!(!stdlib_module_is_pure("util"));
    }
}
