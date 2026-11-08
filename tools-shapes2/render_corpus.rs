//! shapes2 parity probe: `<path>\tOK\t<hash of the rendered source>` or `<path>\tWALL\t<reason>`.
use std::hash::{Hash, Hasher};
use std::io::{BufRead, Write};

fn main() {
    std::thread::Builder::new()
        .stack_size(512 << 20)
        .spawn(move || {
            for line in std::io::stdin().lock().lines() {
                let path = line.expect("stdin");
                let path = path.trim();
                if path.is_empty() {
                    continue;
                }
                let Ok(src) = std::fs::read_to_string(path) else { continue };
                let verdict = match std::panic::catch_unwind(|| almide_mir::pipeline::try_render_rust_source(&src)) {
                    Ok(Ok(out)) => {
                        let mut h = std::collections::hash_map::DefaultHasher::new();
                        format!("{out:?}").hash(&mut h);
                        format!("OK\t{:016x}", h.finish())
                    }
                    Ok(Err(e)) => format!("WALL\t{}", e.to_string().replace(['\t', '\n', '\r'], " ")),
                    Err(_) => "PANIC".to_string(),
                };
                writeln!(std::io::stdout(), "{path}\t{verdict}").expect("write stdout");
            }
        })
        .expect("spawn")
        .join()
        .unwrap_or_else(|e| std::panic::resume_unwind(e));
}
