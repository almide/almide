//! Where a removed stdlib spelling went — derived from the dialect ledger.
//!
//! An epoch that removes stdlib functions lists, in its `breaks`
//! (`proofs/dialect-epochs.toml`), one entry per removed spelling in one of
//! two fixed shapes:
//!
//! ```text
//! `matrix.gelu(…)` → `import nnlib` + `nnlib.gelu(…)` (package almide/nnlib: `almide add almide/nnlib@v0.1.0`)
//! `mem.save()` → delete the call …
//! ```
//!
//! The diagnostic a call to a removed spelling gets (E002, or the
//! module-not-found error for a module that left whole) reads its repair from
//! here, so the ledger is the single place a removal is written: there is no
//! second, hand-kept table to drift from it. A `breaks` entry in neither shape
//! is ordinary prose and is not a removal.

use std::sync::OnceLock;

const LEDGER: &str = include_str!("../../../proofs/dialect-epochs.toml");

/// What became of one removed spelling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fate {
    /// Moved into a package: `import <import>`, then call `<to>`.
    Moved { to: String, import: String, package: String, add: String },
    /// Deleted outright; `repair` is the ledger's "delete the call …" text.
    Deleted { repair: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removal {
    /// `module.fn`, as a program spelled the call.
    pub from: String,
    /// The epoch whose `breaks` removed it.
    pub epoch: u32,
    pub fate: Fate,
}

fn between<'a>(s: &'a str, open: &str, close: &str) -> Option<(&'a str, &'a str)> {
    let start = s.find(open)? + open.len();
    let len = s[start..].find(close)?;
    Some((&s[start..start + len], &s[start + len + close.len()..]))
}

fn callee(call: &str) -> Option<&str> {
    let name = call.split('(').next()?.trim();
    let (m, f) = name.split_once('.')?;
    let ident = |x: &str| !x.is_empty() && x.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    (ident(m) && ident(f)).then_some(name)
}

/// Parse one `breaks` entry; `None` for an entry that is not a removal.
pub fn parse_break(epoch: u32, text: &str) -> Option<Removal> {
    let (old, rest) = between(text, "`", "`")?;
    if !text.starts_with('`') {
        return None;
    }
    let from = callee(old)?.to_string();
    let rest = rest.trim_start().strip_prefix('→')?.trim_start();
    let fate = if rest.starts_with("delete the call") {
        Fate::Deleted { repair: rest.to_string() }
    } else {
        let (import_line, rest) = between(rest, "`", "`")?;
        let import = import_line.strip_prefix("import ")?.trim().to_string();
        let (new_call, rest) = between(rest.trim_start().strip_prefix('+')?, "`", "`")?;
        let to = callee(new_call)?.to_string();
        let (package, rest) = between(rest, "(package ", ":")?;
        let (add, _) = between(rest, "`", "`")?;
        if !add.starts_with("almide add ") {
            return None;
        }
        Fate::Moved { to, import, package: package.trim().to_string(), add: add.to_string() }
    };
    Some(Removal { from, epoch, fate })
}

fn table() -> &'static [Removal] {
    static TABLE: OnceLock<Vec<Removal>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let ledger: toml::Value = toml::from_str(LEDGER).expect("proofs/dialect-epochs.toml parses");
        let mut out = Vec::new();
        for e in ledger.get("epoch").and_then(|v| v.as_array()).into_iter().flatten() {
            let n = e.get("n").and_then(|v| v.as_integer()).unwrap_or(0) as u32;
            for b in e.get("breaks").and_then(|v| v.as_array()).into_iter().flatten() {
                if let Some(r) = b.as_str().and_then(|s| parse_break(n, s)) {
                    out.push(r);
                }
            }
        }
        out
    })
}

/// Every removal the ledger records, in ledger order.
pub fn all() -> &'static [Removal] {
    table()
}

/// The removal of `module.fn`, if the ledger records one.
pub fn lookup(name: &str) -> Option<&'static Removal> {
    table().iter().find(|r| r.from == name)
}

/// The hint for a call to a removed spelling.
pub fn hint(r: &Removal) -> String {
    match &r.fate {
        Fate::Moved { to, import, package, add } => format!(
            "`{}` left the stdlib at dialect epoch {} for package {}: run `{}`, add `import {}`, then call `{}(…)`",
            r.from, r.epoch, package, add, import, to
        ),
        Fate::Deleted { repair } => format!("`{}` was removed at dialect epoch {}: {}", r.from, r.epoch, repair),
    }
}

/// A module every spelling of which the ledger deleted (`mem`): importing it
/// is the error, and the hint says to delete the import and the calls.
pub fn removed_module_hint(module: &str) -> Option<String> {
    // `bytes` lost only `heap_save`/`heap_restore`: the module itself stays.
    if !crate::stdlib::module_functions_current(module).is_empty() {
        return None;
    }
    let prefix = format!("{module}.");
    let rows: Vec<&Removal> = table().iter().filter(|r| r.from.starts_with(&prefix)).collect();
    let first = rows.first()?;
    let Fate::Deleted { repair } = &first.fate else { return None };
    if !rows.iter().all(|r| matches!(r.fate, Fate::Deleted { .. })) {
        return None;
    }
    let names: Vec<String> = rows.iter().map(|r| format!("`{}`", r.from)).collect();
    // The reason is what follows the repair's first `: `.
    let why = repair.split_once(": ").map(|(_, why)| why).unwrap_or("");
    Some(format!(
        "the stdlib `{module}` module was removed at dialect epoch {} ({}): delete the import and the calls{}",
        first.epoch,
        names.join(", "),
        if why.is_empty() { String::new() } else { format!(" — {why}") },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_moved_break_parses() {
        let r = parse_break(
            14,
            "`matrix.gelu(m)` → `import nnlib` + `nnlib.gelu(m)` (package almide/nnlib: `almide add almide/nnlib@v0.1.0`)",
        )
        .unwrap();
        assert_eq!(r.from, "matrix.gelu");
        assert_eq!(
            r.fate,
            Fate::Moved {
                to: "nnlib.gelu".into(),
                import: "nnlib".into(),
                package: "almide/nnlib".into(),
                add: "almide add almide/nnlib@v0.1.0".into(),
            }
        );
    }

    #[test]
    fn a_deleted_break_parses_and_prose_does_not() {
        let r = parse_break(14, "`mem.save()` → delete the call: a no-op").unwrap();
        assert_eq!(r.fate, Fate::Deleted { repair: "delete the call: a no-op".into() });
        assert!(parse_break(3, "a type qualified by a module the file does not import (`lex.LexError`)").is_none());
        assert!(parse_break(3, "`args.flag` is an `effect fn` — mark the caller").is_none());
    }

    /// Epoch 14 removed 33 stdlib spellings, and every one of them parses as
    /// a removal, so the diagnostic cannot silently lose a row to a typo in
    /// the ledger. Other breaks may share the epoch (prose, not removals):
    /// every break that NAMES one of the removed modules' functions in the
    /// removal shape must parse.
    #[test]
    fn every_epoch_14_removal_parses() {
        let ledger: toml::Value = toml::from_str(LEDGER).unwrap();
        let e14 = ledger["epoch"].as_array().unwrap().iter().find(|e| e["n"].as_integer() == Some(14)).unwrap();
        let breaks = e14["breaks"].as_array().unwrap();
        for b in breaks.iter().filter_map(|b| b.as_str()) {
            let shaped = b.contains("` → `import ") || b.contains("` → delete the call");
            assert!(!shaped || parse_break(14, b).is_some(), "a removal-shaped break that does not parse: {b}");
        }
        assert_eq!(all().iter().filter(|r| r.epoch == 14).count(), 33);
        assert!(lookup("matrix.gelu").is_some());
        assert!(lookup("matrix.mul").is_none());
        assert!(removed_module_hint("mem").is_some());
        assert!(removed_module_hint("matrix").is_none());
        assert!(removed_module_hint("bytes").is_none());
    }
}
