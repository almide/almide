//! The compiler's debug-trace channels.
//!
//! Lowering and rendering carry a dozen opt-in trace points — why a lambda
//! declined to lift, why a nested-match chain was refused, what the ABI probe
//! decided. Each used to be written out as its own
//! `if std::env::var("...").is_ok() { eprintln!(...) }`, which meant the set of
//! channels existed only as a grep, and the prefix convention (`[lift]`,
//! `[cells]`, …) was maintained by hand at each site.
//!
//! The variable NAMES are unchanged — they are what someone debugging types on
//! the command line, so they are a compatibility surface, not an implementation
//! detail. The roster is `almide_base::env::SWITCHES` (#2205 — one registry for
//! the whole tree; this crate's own `CHANNELS` list was folded into it), and
//! `scripts/check-env-switches.sh` refuses a channel read here that it lacks.
//!
//! Traces go to stderr because stdout is the compiler's data channel: `almide
//! compile --json` and `--target rust` write their real output there, and a
//! trace line mixed into it would corrupt a machine-read result.

/// True when the channel `var` is on (`almide_base::env::flag` — the one
/// boolean semantics every switch has, #2205).
pub(crate) fn enabled(var: &str) -> bool {
    almide_base::env::flag(var)
}

/// True when `var` is set to exactly `want`.
///
/// Used by the per-function channels, where dumping every function would bury
/// the one being investigated.
pub(crate) fn enabled_for(var: &str, want: &str) -> bool {
    almide_base::env::var(var).is_some_and(|v| v == want)
}

/// Print one trace line if `var` is set.
///
/// The message is a closure so a disabled channel costs one env lookup and
/// never formats — several of these interpolate a `{:#?}` of a whole function
/// body.
pub(crate) fn trace(var: &str, line: impl FnOnce() -> String) {
    if enabled(var) {
        eprintln!("{}", line());
    }
}

/// Print one trace line if `var` is set to exactly `want`.
pub(crate) fn trace_for(var: &str, want: &str, line: impl FnOnce() -> String) {
    if enabled_for(var, want) {
        eprintln!("{}", line());
    }
}
