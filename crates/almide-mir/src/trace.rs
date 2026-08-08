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
//! detail. [`CHANNELS`] is the roster, and `debug_channels_are_documented`
//! asserts every channel used in this crate appears in it.
//!
//! Traces go to stderr because stdout is the compiler's data channel: `almide
//! compile --json` and `--target rust` write their real output there, and a
//! trace line mixed into it would corrupt a machine-read result.

/// Every debug channel this crate reads, with what it reports.
///
/// A channel is either a flag (set it to anything) or takes a value, noted in
/// the description.
#[cfg_attr(not(test), allow(dead_code))] // read only by debug_channels_are_documented
pub(crate) const CHANNELS: &[(&str, &str)] = &[
    (
        "ALMIDE_DBG_ANF",
        "why a lambda lift or statement inline declined",
    ),
    (
        "ALMIDE_DBG_CELLS",
        "the captured / mutated / celled variable sets",
    ),
    (
        "ALMIDE_DBG_NESTED_MATCH",
        "why a nested-match chain was refused",
    ),
    (
        "ALMIDE_DBG_UNLINKED",
        "wasm references with no resolvable definition",
    ),
    (
        "ALMIDE_ABI_PROBE",
        "the lifted-effect-fn ABI decision per function",
    ),
    (
        "ALMIDE_DUMP_IR",
        "the lowered IR of every fn whose name contains the value",
    ),
    ("ALMIDE_DUMP_DROPS", "the computed drop set"),
    (
        "ALMIDE_MG_DEBUG",
        "mutable-global slot assignment and the cross-module name-bridge decisions",
    ),
    (
        "ALMIDE_DBG_ELEM",
        "why a list-literal Block element declined",
    ),
    (
        "DBG_LOWER_FN",
        "the lowered body of the fn named by the value",
    ),
    (
        "DBG_DESUGAR_FN",
        "the desugared body of the fn named by the value",
    ),
    (
        "DBG_DESUGAR_RAW",
        "with DBG_DESUGAR_FN, the raw pre-desugar body too",
    ),
];

/// True when `var` is set to anything.
pub(crate) fn enabled(var: &str) -> bool {
    std::env::var_os(var).is_some()
}

/// True when `var` is set to exactly `want`.
///
/// Used by the per-function channels, where dumping every function would bury
/// the one being investigated.
pub(crate) fn enabled_for(var: &str, want: &str) -> bool {
    std::env::var(var).is_ok_and(|v| v == want)
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

#[cfg(test)]
mod tests {
    /// Every channel the crate reads must be in [`super::CHANNELS`].
    ///
    /// The roster is how someone finds these without grepping, so a channel
    /// added at a trace site and not listed here is invisible — which is how
    /// the previous scattered form behaved by default.
    #[test]
    fn debug_channels_are_documented() {
        let listed: std::collections::HashSet<&str> =
            super::CHANNELS.iter().map(|(v, _)| *v).collect();
        let mut missing = Vec::new();
        for entry in walk_sources(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src")) {
            let text = std::fs::read_to_string(&entry).unwrap_or_default();
            for var in read_vars(&text) {
                if !listed.contains(var.as_str()) {
                    missing.push(format!("{} (in {})", var, entry.display()));
                }
            }
        }
        assert!(
            missing.is_empty(),
            "undocumented debug channels: {missing:?}"
        );
    }

    /// Every `.rs` file under `dir`, recursively.
    fn walk_sources(dir: std::path::PathBuf) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return out;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk_sources(p));
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
        out
    }

    /// The channel names passed to `trace` / `trace_for` / `enabled*` in `text`.
    ///
    /// Deliberately syntactic: it reads the literal that follows the call, which
    /// is what a reader looking for the channel would do.
    fn read_vars(text: &str) -> Vec<String> {
        let mut out = Vec::new();
        for call in ["trace(\"", "trace_for(\"", "enabled(\"", "enabled_for(\""] {
            let mut rest = text;
            while let Some(i) = rest.find(call) {
                rest = &rest[i + call.len()..];
                if let Some(end) = rest.find('"') {
                    out.push(rest[..end].to_string());
                }
            }
        }
        out
    }
}
