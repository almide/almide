//! Render a list of programs on the verified native renderer, in one process
//! (scripts/check-native-v1-perf-shapes.sh, #4006).
//!
//! Usage: render_corpus < paths — one `.almd` path per line on stdin; prints
//! `<path>\tOK` or `<path>\tWALL` per program. The shapes gate builds this
//! example with coverage instrumentation and reads which render paths the
//! perf gate's corpus reaches; the verdicts let it confirm the corpus it
//! meant to run actually rendered.

use std::io::{BufRead, Write};

fn main() {
    std::thread::Builder::new()
        .stack_size(512 << 20)
        .spawn(|| {
            for line in std::io::stdin().lock().lines() {
                let path = line.expect("stdin");
                let path = path.trim();
                if path.is_empty() {
                    continue;
                }
                let src = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
                let verdict = match almide_mir::pipeline::try_render_rust_source(&src) {
                    Ok(r) => {
                        if r.contains("HSLOT") {
                            eprintln!("HSLOTDBG {path} snap={} aliasdrop={} rc={}", r.matches("_rc: std::rc::Rc<").count(), r.matches("a heap slot's snapshot").count(), r.matches("RefCell<std::rc::Rc<").count());
                        }
                        "OK"
                    }
                    Err(_) => "WALL",
                };
                writeln!(std::io::stdout(), "{path}\t{verdict}").expect("write stdout");
            }
        })
        .expect("spawn")
        .join()
        .unwrap_or_else(|e| std::panic::resume_unwind(e));
}
