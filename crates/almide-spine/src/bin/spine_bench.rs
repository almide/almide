//! SPIKE S1 measurement harness. Run with:
//!   cargo run --release -p almide-spine --bin spine_bench
//! from the workspace root. Prints the (a)/(b)/(c) verdicts of
//! ARCHITECTURE.md §6.5 over the real spec/ corpus.

use almide_spine::{project_digest, Project, SourceFile, SpineDb, PARSE_EXECUTIONS};
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

fn batch_parse_all(sources: &[(String, String)]) -> u64 {
    let mut acc = 0u64;
    for (path, text) in sources {
        let tokens = almide_syntax::lexer::Lexer::tokenize(text);
        let mut parser = almide_syntax::parser::Parser::new(tokens).with_file(path);
        acc = acc.wrapping_add(match parser.parse() {
            Ok(p) => p.decls.len() as u64,
            Err(_) => 1,
        });
    }
    acc
}

fn main() {
    let root = std::env::current_dir().unwrap();
    let mut files = Vec::new();
    walk(&root.join("spec"), &mut files);
    files.sort();
    let sources: Vec<(String, String)> = files
        .iter()
        .map(|p| {
            (
                p.strip_prefix(&root).unwrap().to_string_lossy().to_string(),
                std::fs::read_to_string(p).unwrap(),
            )
        })
        .collect();
    let total_lines: usize = sources.iter().map(|(_, t)| t.lines().count()).sum();
    println!("corpus: {} files, {} lines", sources.len(), total_lines);

    const ROUNDS: usize = 30;

    // ── baseline: batch front-end (what `almide check`'s front end does every
    // invocation — re-lex + re-parse the world) ─────────────────────────────
    let mut batch_ms = Vec::new();
    let mut sink = 0u64;
    for _ in 0..ROUNDS {
        let t = Instant::now();
        sink = sink.wrapping_add(batch_parse_all(&sources));
        batch_ms.push(t.elapsed().as_secs_f64() * 1e3);
    }
    let batch = median(batch_ms);
    println!("batch front-end re-parse: {batch:.2} ms (median of {ROUNDS})");

    // ── salsa: cold ─────────────────────────────────────────────────────────
    let mut db = SpineDb::default();
    let inputs: Vec<SourceFile> = sources
        .iter()
        .map(|(p, t)| SourceFile::new(&db, p.clone(), t.clone()))
        .collect();
    let project = Project::new(&db, inputs.clone());
    PARSE_EXECUTIONS.store(0, Ordering::Relaxed);
    let t = Instant::now();
    sink = sink.wrapping_add(project_digest(&db, project));
    let cold = t.elapsed().as_secs_f64() * 1e3;
    let cold_execs = PARSE_EXECUTIONS.load(Ordering::Relaxed);
    let overhead = (cold / batch - 1.0) * 100.0;
    println!("salsa cold: {cold:.2} ms ({cold_execs} parses), overhead vs batch: {overhead:+.1}%");

    // ── salsa: warm — edit one file per round, re-derive the project ────────
    let mut warm_ms = Vec::new();
    let mut per_round_execs = Vec::new();
    for i in 0..ROUNDS {
        let victim = inputs[(i * 37) % inputs.len()];
        let new_text = format!("{}\n// tick {i}\n", sources[(i * 37) % inputs.len()].1);
        victim.set_text(&mut db).to(new_text);
        PARSE_EXECUTIONS.store(0, Ordering::Relaxed);
        let t = Instant::now();
        sink = sink.wrapping_add(project_digest(&db, project));
        warm_ms.push(t.elapsed().as_secs_f64() * 1e3);
        per_round_execs.push(PARSE_EXECUTIONS.load(Ordering::Relaxed));
    }
    let warm = median(warm_ms);
    let max_execs = *per_round_execs.iter().max().unwrap();
    println!("salsa warm re-derive after 1-file edit: {warm:.3} ms (median of {ROUNDS})");
    println!("parses per warm round: max {max_execs} (claim (a) requires 1)");

    // ── sanity: a memo hit round (no edit) must run 0 parses ────────────────
    PARSE_EXECUTIONS.store(0, Ordering::Relaxed);
    sink = sink.wrapping_add(project_digest(&db, project));
    let hit_execs = PARSE_EXECUTIONS.load(Ordering::Relaxed);

    let speedup = batch / warm;
    println!("---");
    println!("(a) only edited file re-parses: {} (max {max_execs}/round, {hit_execs} on no-edit)",
        if max_execs == 1 && hit_execs == 0 { "PASS" } else { "FAIL" });
    println!("(b) cold overhead < 20%: {} ({overhead:+.1}%)", if overhead < 20.0 { "PASS" } else { "FAIL" });
    println!("(c) warm >= 10x batch: {} ({speedup:.0}x)", if speedup >= 10.0 { "PASS" } else { "FAIL" });
    std::hint::black_box(sink);
}
