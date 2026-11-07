//! Render a list of programs on the verified native renderer, in one process
//! (scripts/check-native-v1-perf-shapes.sh, #4006).
//!
//! Usage: render_corpus < paths — one `.almd` path per line on stdin; prints
//! `<path>\tOK` or `<path>\tWALL` per program. The shapes gate builds this
//! example with coverage instrumentation and reads which render paths the
//! perf gate's corpus reaches; the verdicts let it confirm the corpus it
//! meant to run actually rendered.
//! `--reasons` appends the full refusal reason to WALL rows, for planning
//! coverage work without grouping distinct unsupported shapes together.

use std::io::{BufRead, Write};

fn main() {
    let reasons = std::env::args().any(|arg| arg == "--reasons");
    std::thread::Builder::new()
        .stack_size(512 << 20)
        .spawn(move || {
            for line in std::io::stdin().lock().lines() {
                let path = line.expect("stdin");
                let path = path.trim();
                if path.is_empty() {
                    continue;
                }
                let src = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
                let verdict = match almide_mir::pipeline::try_render_rust_source(&src) {
                    Ok(_) => "OK".to_string(),
                    Err(e) if reasons => format!("WALL\t{}", e.to_string().replace(['\t', '\n', '\r'], " ")),
                    Err(_) => "WALL".to_string(),
                };
                writeln!(std::io::stdout(), "{path}\t{verdict}").expect("write stdout");
            }
        })
        .expect("spawn")
        .join()
        .unwrap_or_else(|e| std::panic::resume_unwind(e));
}
