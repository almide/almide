//! SPIKE S2a (ARCHITECTURE.md §6.5): sema-as-queries MECHANICS, measured.
//!
//! The firewall chain, after rust-analyzer's pattern:
//!
//!   SourceFile.text ──▶ parse_decls(file)      (parses; returns fingerprints)
//!                        │ backdates when only spans moved (spans/ids erased)
//!                        ▼
//!                       decl_fp(key)           (per-decl projection)
//!                        │ backdates when THIS decl is untouched
//!                        ▼
//!   project_symbols ──▶ symbol_fp(symbol)      (per-name interface projection)
//!                        │ backdates unless THIS name's interface changed
//!                        ▼
//!                       check_decl(key)        (the stand-in "sema" query)
//!
//! `check_decl` is a stand-in for cost (unit 4 ports the real checker); what
//! S2a measures is INVALIDATION COUNTS — gates (d)(e)(f). Fingerprints are
//! hashes of the decl's JSON with every `id` key removed; spans never reach
//! JSON at all (`#[serde(skip)]` throughout the ported AST), which is the
//! span-independence MoonBit buys with relative locations and rust-analyzer
//! with AstId maps — here it falls out of the serialization boundary.
//! Dependencies are name-level and overapproximate: every string in the body
//! JSON that names a project symbol counts as a dep (declared limitation;
//! symbol keys are fixed at setup, so names introduced by later edits are
//! outside the measured graph).

use crate::{SourceFile, PARSE_EXECUTIONS};
use std::collections::BTreeMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

pub static CHECK_EXECUTIONS: AtomicUsize = AtomicUsize::new(0);

#[salsa::input]
pub struct SProject {
    #[returns(ref)]
    pub files: Vec<SourceFile>,
}

/// Immutable key bundle for one declaration (input-struct-as-key: created
/// once at setup, never mutated, so the handle is stable across revisions).
#[salsa::input]
pub struct DeclKey {
    pub project: SProject,
    pub file: SourceFile,
    pub index: usize,
}

/// Immutable key bundle for one project-level symbol name.
#[salsa::input]
pub struct SymbolKey {
    pub project: SProject,
    #[returns(ref)]
    pub name: String,
}

/// name → SymbolKey handle, fixed at setup. Constant data (the salsa
/// dependency edge comes from calling `symbol_fp`, not from this lookup).
pub static SYMBOL_REGISTRY: OnceLock<BTreeMap<String, SymbolKey>> = OnceLock::new();

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DeclFp {
    pub name: Option<String>,
    pub iface_fp: u64,
    pub body_fp: u64,
    pub deps: Vec<String>,
}

fn hash_str(s: &str) -> u64 {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// Remove every `"id"` key, recursively. Spans are already absent from the
/// JSON; ids are per-file parse counters and shift on any earlier-in-file
/// edit, so they must not reach a fingerprint.
fn strip_ids(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::Object(m) => {
            m.remove("id");
            for x in m.values_mut() {
                strip_ids(x);
            }
        }
        serde_json::Value::Array(a) => {
            for x in a {
                strip_ids(x);
            }
        }
        _ => {}
    }
}

fn collect_strings(v: &serde_json::Value, out: &mut Vec<String>) {
    match v {
        serde_json::Value::String(s) => out.push(s.clone()),
        serde_json::Value::Object(m) => {
            for x in m.values() {
                collect_strings(x, out);
            }
        }
        serde_json::Value::Array(a) => {
            for x in a {
                collect_strings(x, out);
            }
        }
        _ => {}
    }
}

fn fingerprint_decl(decl: &almide_syntax::ast::Decl) -> DeclFp {
    let mut v = serde_json::to_value(decl).expect("AST serializes");
    strip_ids(&mut v);
    // Decl serializes internally tagged and FLAT: {"kind":"fn","name":…,
    // "body":…} — name and body live at the top level (verified via s2_dbg).
    let name = v.get("name").and_then(|n| n.as_str()).map(str::to_string);
    // Interface = the decl JSON minus its body/value; body = that field alone.
    let mut iface = v.clone();
    let mut body = serde_json::Value::Null;
    if let Some(obj) = iface.as_object_mut() {
        for body_field in ["body", "value"] {
            if let Some(b) = obj.remove(body_field) {
                body = b;
                break;
            }
        }
    }
    let mut deps = Vec::new();
    collect_strings(&body, &mut deps);
    deps.sort();
    deps.dedup();
    DeclFp {
        name,
        iface_fp: hash_str(&iface.to_string()),
        body_fp: hash_str(&body.to_string()),
        deps,
    }
}

#[salsa::tracked]
pub fn parse_decls(db: &dyn salsa::Database, file: SourceFile) -> Vec<DeclFp> {
    PARSE_EXECUTIONS.fetch_add(1, Ordering::Relaxed);
    let tokens = almide_syntax::lexer::Lexer::tokenize(file.text(db));
    let mut parser = almide_syntax::parser::Parser::new(tokens).with_file(file.path(db));
    match parser.parse() {
        Ok(prog) => prog.decls.iter().map(fingerprint_decl).collect(),
        Err(_) => Vec::new(),
    }
}

#[salsa::tracked]
pub fn decl_fp(db: &dyn salsa::Database, key: DeclKey) -> Option<DeclFp> {
    parse_decls(db, *key.file(db)).get(*key.index(db)).cloned()
}

#[salsa::tracked]
pub fn project_symbols(db: &dyn salsa::Database, project: SProject) -> BTreeMap<String, u64> {
    let mut out = BTreeMap::new();
    for f in project.files(db) {
        for fp in parse_decls(db, *f) {
            if let Some(n) = &fp.name {
                out.insert(n.clone(), fp.iface_fp);
            }
        }
    }
    out
}

#[salsa::tracked]
pub fn symbol_fp(db: &dyn salsa::Database, sk: SymbolKey) -> Option<u64> {
    project_symbols(db, *sk.project(db)).get(sk.name(db)).copied()
}

/// The stand-in sema query: depends on this decl's own fingerprint plus the
/// INTERFACE fingerprint of every project symbol its body names. Unit 4
/// replaces the fold with the real checker; the dependency shape is the bet
/// being measured.
#[salsa::tracked]
pub fn check_decl(db: &dyn salsa::Database, key: DeclKey) -> u64 {
    CHECK_EXECUTIONS.fetch_add(1, Ordering::Relaxed);
    let Some(fp) = decl_fp(db, key) else { return 0 };
    let registry = SYMBOL_REGISTRY.get().expect("registry initialized");
    let mut acc = fp.body_fp ^ fp.iface_fp;
    for dep in &fp.deps {
        if let Some(sk) = registry.get(dep) {
            if let Some(ifp) = symbol_fp(db, *sk) {
                acc = acc.wrapping_mul(1_000_003).wrapping_add(*ifp);
            } else {
                acc = acc.wrapping_mul(1_000_003).wrapping_add(0xDEAD);
            }
        }
    }
    acc
}
