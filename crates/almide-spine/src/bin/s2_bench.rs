//! SPIKE S2a harness — gates (d)(e)(f) of ARCHITECTURE.md §6.5.
//!   cargo run --release -p almide-spine --bin s2_bench
//!
//! Ten victim files get a known pair appended BEFORE setup:
//!     fn __s2_base() -> Int = 1
//!     fn __s2_user() -> Int = __s2_base()
//! so expected invalidation counts are derived from construction:
//!   (d) body edit of __s2_base (digit toggle)     → exactly 1 re-check
//!   (e) rename __s2_base ⇄ __s2_basex             → exactly 2 re-checks
//!       (the renamed decl + its one dependent)
//!   (f) blank line prepended to a file            → exactly 0 re-checks

use almide_spine::s2::{check_decl, parse_decls, project_symbols, DeclKey, SProject, SymbolKey, CHECK_EXECUTIONS, SYMBOL_REGISTRY};
use almide_spine::{SourceFile, PARSE_EXECUTIONS};
use salsa::Setter;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::Instant;

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).expect("probe-bin invariant") {
        let p = e.expect("probe-bin invariant").path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x == "almd") {
            out.push(p);
        }
    }
}

fn pair(k: usize) -> String {
    format!("\nfn __s2_base_{k}() -> Int = 1\nfn __s2_user_{k}() -> Int = __s2_base_{k}()\n")
}

fn main() {
    let root = std::env::current_dir().expect("probe-bin invariant");
    let mut files = Vec::new();
    walk(&root.join("spec"), &mut files);
    files.sort();

    let db = almide_spine::SpineDb::default();
    let mut inputs = Vec::new();
    let mut victims = Vec::new();
    for (i, p) in files.iter().enumerate() {
        let rel = p.strip_prefix(&root).expect("probe-bin invariant").to_string_lossy().to_string();
        let mut text = std::fs::read_to_string(p).expect("probe-bin invariant");
        // Ten victims, spread across the corpus, get the known pair.
        if i % (files.len() / 10) == 7 && victims.len() < 10 {
            text.push_str(&pair(victims.len()));
            victims.push(inputs.len());
        }
        inputs.push(SourceFile::new(&db, rel, text));
    }
    let project = SProject::new(&db, inputs.clone());

    // Setup: symbol registry + per-decl keys, frozen for the whole run.
    let symbols = project_symbols(&db, project);
    if SYMBOL_REGISTRY
        .set(symbols.keys().map(|n| (n.clone(), SymbolKey::new(&db, project, n.clone()))).collect())
        .is_err()
    {
        panic!("registry already set");
    }
    let mut keys = Vec::new();
    for f in &inputs {
        let n = parse_decls(&db, *f).len();
        for i in 0..n {
            keys.push(DeclKey::new(&db, project, *f, i));
        }
    }

    // Cold: check everything once.
    PARSE_EXECUTIONS.store(0, Ordering::Relaxed);
    CHECK_EXECUTIONS.store(0, Ordering::Relaxed);
    let t = Instant::now();
    let mut sink = 0u64;
    for k in &keys {
        sink = sink.wrapping_add(check_decl(&db, *k));
    }
    let cold_ms = t.elapsed().as_secs_f64() * 1e3;
    println!(
        "cold: {} files, {} decls, {} symbols, {} checks in {cold_ms:.1} ms",
        inputs.len(), keys.len(), symbols.len(), CHECK_EXECUTIONS.load(Ordering::Relaxed)
    );

    // Report the natural dependency graph so the overapproximation is visible.
    let mut dep_edges = 0usize;
    let mut dependents: BTreeMap<String, usize> = BTreeMap::new();
    for f in &inputs {
        for fp in parse_decls(&db, *f) {
            for d in &fp.deps {
                if symbols.contains_key(d) {
                    dep_edges += 1;
                    *dependents.entry(d.clone()).or_default() += 1;
                }
            }
        }
    }
    println!("dep graph (name-level, overapprox): {dep_edges} edges; __s2_base_0 dependents: {}", dependents.get("__s2_base_0").copied().unwrap_or(0));

    let mut db = db;
    let recheck_all = |db: &almide_spine::SpineDb, keys: &[DeclKey], sink: &mut u64| {
        for k in keys {
            *sink = sink.wrapping_add(check_decl(db, *k));
        }
    };

    // ── (f) span-only edits: prepend a blank line ───────────────────────────
    let mut f_checks = 0usize;
    let mut f_ms = Vec::new();
    for i in 0..20 {
        let victim = inputs[(i * 53) % inputs.len()];
        let new_text = format!("\n{}", victim.text(&db).clone());
        victim.set_text(&mut db).to(new_text);
        CHECK_EXECUTIONS.store(0, Ordering::Relaxed);
        let t = Instant::now();
        recheck_all(&db, &keys, &mut sink);
        f_ms.push(t.elapsed().as_secs_f64() * 1e3);
        f_checks += CHECK_EXECUTIONS.load(Ordering::Relaxed);
    }
    let f_med = { let mut v = f_ms.clone(); v.sort_by(|a, b| a.partial_cmp(b).expect("probe-bin invariant")); v[v.len() / 2] };

    // ── (d) body edits of __s2_base: digit toggle, iface unchanged ──────────
    let mut d_rounds = Vec::new();
    let mut d_ms = Vec::new();
    for i in 0..20 {
        let k = i % victims.len();
        let victim = inputs[victims[k]];
        let old = victim.text(&db).clone();
        let one = format!("__s2_base_{k}() -> Int = 1");
        let two = format!("__s2_base_{k}() -> Int = 2");
        let new_text = if old.contains(&one) { old.replace(&one, &two) } else { old.replace(&two, &one) };
        victim.set_text(&mut db).to(new_text);
        CHECK_EXECUTIONS.store(0, Ordering::Relaxed);
        let t = Instant::now();
        recheck_all(&db, &keys, &mut sink);
        d_ms.push(t.elapsed().as_secs_f64() * 1e3);
        d_rounds.push(CHECK_EXECUTIONS.load(Ordering::Relaxed));
    }
    let d_med = { let mut v = d_ms.clone(); v.sort_by(|a, b| a.partial_cmp(b).expect("probe-bin invariant")); v[v.len() / 2] };
    let d_max = *d_rounds.iter().max().expect("probe-bin invariant");

    // ── (e) interface edits: rename __s2_base ⇄ __s2_basex ──────────────────
    let mut e_rounds = Vec::new();
    for i in 0..10 {
        let k = i % victims.len();
        let victim = inputs[victims[k]];
        let old = victim.text(&db).clone();
        let a = format!("fn __s2_base_{k}(");
        let b = format!("fn __s2_basex_{k}(");
        let new_text = if old.contains(&a) { old.replace(&a, &b) } else { old.replace(&b, &a) };
        victim.set_text(&mut db).to(new_text);
        CHECK_EXECUTIONS.store(0, Ordering::Relaxed);
        recheck_all(&db, &keys, &mut sink);
        e_rounds.push(CHECK_EXECUTIONS.load(Ordering::Relaxed));
    }
    let e_max = *e_rounds.iter().max().expect("probe-bin invariant");
    let e_min = *e_rounds.iter().min().expect("probe-bin invariant");

    println!("---");
    println!("(d) body edit  → re-checks/round: max {d_max} (want 1); warm re-derive {d_med:.3} ms");
    println!("(e) iface edit → re-checks/round: min {e_min} max {e_max} (want exactly 2: decl + 1 dependent)");
    println!("(f) span edit  → re-checks over 20 rounds: {f_checks} (want 0); warm re-derive {f_med:.3} ms");
    println!("---");
    println!("(d) {}", if d_max == 1 { "PASS" } else { "FAIL" });
    println!("(e) {}", if e_min == 2 && e_max == 2 { "PASS" } else { "FAIL" });
    println!("(f) {}", if f_checks == 0 { "PASS" } else { "FAIL" });
    std::hint::black_box(sink);
}
