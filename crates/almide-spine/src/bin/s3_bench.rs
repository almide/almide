//! Gate (g) harness — unit 4 stage 1 (ARCHITECTURE.md §6.5):
//! warm full-loop (front end + REAL check) vs the batch equivalent.
//!   cargo run --release -p almide-spine --bin s3_bench
//!
//! Batch model: what per-file compiles pay today (`almide check` / the test
//! runner) — every invocation re-runs the whole front end + check for its
//! file, stdlib inference included. One batch round = that, over every
//! qualifying file, on a FRESH database (no memoization). Warm model: one
//! persistent database; each round edits one file and re-derives everything.
//!
//! Purity contract: only stdlib-only-import files qualify (s3.rs); the rest
//! are counted and reported as excluded, never silently dropped.

use almide_spine::s3::{check_file_json_v3 as check_file_json, stdlib_only, FILE_CHECK_EXECUTIONS};
use almide_spine::{SourceFile, SpineDb};
use salsa::Setter;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::Instant;

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x == "almd") {
            out.push(p);
        }
    }
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn main() {
    let root = std::env::current_dir().unwrap();
    let mut files = Vec::new();
    walk(&root.join("spec"), &mut files);
    files.sort();

    // Prefilter: parseable + stdlib-only imports (purity contract).
    let mut sources: Vec<(String, String)> = Vec::new();
    let mut excluded = 0usize;
    for p in &files {
        let rel = p.strip_prefix(&root).unwrap().to_string_lossy().to_string();
        let text = std::fs::read_to_string(p).unwrap();
        let tokens = almide::lexer::Lexer::tokenize(&text);
        let mut parser = almide::parser::Parser::new(tokens).with_file(&rel);
        match parser.parse() {
            Ok(prog) if stdlib_only(&prog) => sources.push((rel, text)),
            _ => excluded += 1,
        }
    }
    let total_lines: usize = sources.iter().map(|(_, t)| t.lines().count()).sum();
    println!("corpus: {} qualifying files ({} lines), {} excluded (non-stdlib imports or parse failure)",
        sources.len(), total_lines, excluded);

    // ── batch: fresh db per round = per-file compiles with no reuse ─────────
    let mut batch_ms = Vec::new();
    let mut total_diags = 0usize;
    for _ in 0..3 {
        let db = SpineDb::default();
        let inputs: Vec<SourceFile> = sources.iter()
            .map(|(p, t)| SourceFile::new(&db, p.clone(), t.clone())).collect();
        let t = Instant::now();
        total_diags = 0;
        for f in &inputs {
            let out = check_file_json(&db, *f);
            total_diags += out.diags.len() + out.module_diags.len();
        }
        batch_ms.push(t.elapsed().as_secs_f64() * 1e3);
    }
    let batch = median(batch_ms);
    println!("batch full check (fresh per round): {batch:.0} ms; {total_diags} diagnostics");

    // ── warm: persistent db, one-file edits ─────────────────────────────────
    let mut db = SpineDb::default();
    let inputs: Vec<SourceFile> = sources.iter()
        .map(|(p, t)| SourceFile::new(&db, p.clone(), t.clone())).collect();
    for f in &inputs {
        std::hint::black_box(check_file_json(&db, *f));
    }
    // Memo sanity: a no-edit sweep must run zero checks.
    FILE_CHECK_EXECUTIONS.store(0, Ordering::Relaxed);
    for f in &inputs {
        std::hint::black_box(check_file_json(&db, *f));
    }
    let noedit = FILE_CHECK_EXECUTIONS.load(Ordering::Relaxed);

    let mut warm_ms = Vec::new();
    let mut max_execs = 0usize;
    for i in 0..30 {
        let idx = (i * 41) % inputs.len();
        let victim = inputs[idx];
        let new_text = format!("{}\n// tick {i}\n", victim.text(&db).clone());
        victim.set_text(&mut db).to(new_text);
        FILE_CHECK_EXECUTIONS.store(0, Ordering::Relaxed);
        let t = Instant::now();
        for f in &inputs {
            std::hint::black_box(check_file_json(&db, *f));
        }
        warm_ms.push(t.elapsed().as_secs_f64() * 1e3);
        max_execs = max_execs.max(FILE_CHECK_EXECUTIONS.load(Ordering::Relaxed));
    }
    let warm = median(warm_ms);
    let speedup = batch / warm;

    println!("warm re-derive after 1-file edit: {warm:.2} ms (median/30); checks/round max {max_execs}; no-edit sweep {noedit}");
    println!("---");
    println!("(g) warm >= 10x batch: {} ({speedup:.0}x)", if speedup >= 10.0 { "PASS" } else { "FAIL" });
    println!("(a3) only edited file re-checks: {}", if max_execs == 1 && noedit == 0 { "PASS" } else { "FAIL" });
}
