//! Novel-item generators for the decision-site ledger's c2 condition
//! (proofs/decision-sites.toml): "an unknown item is refused, and a test adds
//! a NOVEL item and sees it refused".
//!
//! A test that hard-codes a known item proves the site knows THAT item. These
//! generators hand a test something no list in the compiler can have named:
//! a fresh identifier, a fresh module, a fresh switch name, a fresh runtime
//! symbol, a fresh fs op number — or, where the decision reads a registry of
//! declarations, EVERY declared item enumerated at run time (so an item added
//! after the test was written is covered without editing the test).
//!
//! The Almide judge (`tools/almide-gates decision-sites`) counts a `novel`
//! test only when its file calls into this module (`novel::`).
#![allow(dead_code)]

use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A value no two calls in any run share: the process id, the clock and a
/// counter, mixed.
fn salt() -> u64 {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
    let mut x = t ^ (u64::from(std::process::id()) << 32) ^ n.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x
}

/// A fresh lower-case identifier: `<prefix>_<12 hex>`. Valid as an Almide fn,
/// variable or module name.
pub fn ident(prefix: &str) -> String {
    format!("{prefix}_{:012x}", salt() & 0xffff_ffff_ffff)
}

/// A fresh module name (also a fresh file stem: `<name>.almd`).
pub fn module() -> String {
    ident("novelmod")
}

/// A fresh native runtime symbol, spelled the way the runtime's are.
pub fn runtime_symbol() -> String {
    format!("almide_rt_{}", ident("novel"))
}

/// A fresh switch name: `ALMIDE_NOVEL_<HEX>`. Built at run time — the env
/// switch gate refuses an unregistered `ALMIDE_*` literal in the tree.
pub fn switch_name() -> String {
    format!("{}NOVEL_{:012X}", "ALMIDE_", salt() & 0xffff_ffff_ffff)
}

/// A fresh path segment for filesystem-scope probes.
pub fn path_segment() -> String {
    ident("novelseg")
}

/// An fs op number no module was built with: far outside the op table.
pub fn fs_op() -> i32 {
    1_000_000 + (salt() % 1_000_000) as i32
}

/// Every public stdlib `module.fn` the stdlib sources declare with `attr`
/// (for example `@memory_unsafe`), with its parameter types, read from
/// `stdlib/*.almd` at run time.
pub fn stdlib_fns_declared(attr: &str) -> Vec<(String, String, Vec<String>)> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib");
    let mut out = Vec::new();
    let mut files: Vec<_> = std::fs::read_dir(&root).expect("stdlib/").flatten().map(|e| e.path()).collect();
    files.sort();
    for p in files {
        if p.extension().is_none_or(|x| x != "almd") {
            continue;
        }
        let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        // `string_case.almd` is a part of module `string`.
        let module = stem.split('_').next().unwrap_or(&stem).to_string();
        let text = std::fs::read_to_string(&p).unwrap_or_default();
        let mut armed = false;
        for line in text.lines() {
            let t = line.trim_start();
            if t.starts_with(attr) {
                armed = true;
                continue;
            }
            if armed && t.starts_with('@') {
                continue;
            }
            if armed {
                let local = t.starts_with("local ");
                let rest = t.strip_prefix("pub ").unwrap_or(t);
                let rest = rest.strip_prefix("effect ").unwrap_or(rest);
                if let Some(sig) = rest.strip_prefix("fn ").filter(|_| !local) {
                    let name: String = sig.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                    let params: Vec<String> = sig
                        .split_once('(')
                        .and_then(|(_, r)| r.split_once(')'))
                        .map(|(ps, _)| {
                            ps.split(',')
                                .filter(|p| !p.trim().is_empty())
                                .map(|p| p.split_once(':').map(|(_, ty)| ty.trim().to_string()).unwrap_or_default())
                                .collect()
                        })
                        .unwrap_or_default();
                    if !name.is_empty() && !name.starts_with("__") {
                        out.push((module.clone(), name, params));
                    }
                }
                armed = false;
            }
        }
    }
    out
}
