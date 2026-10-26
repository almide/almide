//! The affine-shape census (#4236, stage 1): what fraction of real functions
//! lower to a body with no `Dup`.
//!
//!   affine_census <file.almd | dir> ...
//!
//! Each file is lowered alone (`program_affine_shapes(src, &[])`), so a file
//! that imports a user module is a whole-program wall and is counted as such.
//! Walled functions are reported, never classified. Output on stdout:
//!   - one `AFFINE|PARAM|LOCAL|BOTH <file> <fn> param=<n> local=<n>` row per
//!     lowered function (`--rows`),
//!   - the totals, and the share of affine-shaped functions among the lowered.

use std::path::{Path, PathBuf};

fn collect(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        let Ok(rd) = std::fs::read_dir(path) else { return };
        let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
        entries.sort();
        for p in entries {
            collect(&p, out);
        }
    } else if path.extension().is_some_and(|e| e == "almd") {
        out.push(path.to_path_buf());
    }
}

#[derive(Default)]
struct Totals {
    files: usize,
    program_walls: usize,
    walled_fns: usize,
    affine: usize,
    param_only: usize,
    local_only: usize,
    both: usize,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rows = args.iter().any(|a| a == "--rows");
    let mut files = Vec::new();
    for a in args.iter().filter(|a| !a.starts_with("--")) {
        collect(Path::new(a), &mut files);
    }
    let mut t = Totals::default();
    for file in &files {
        t.files += 1;
        let Ok(src) = std::fs::read_to_string(file) else { continue };
        let run = std::panic::catch_unwind(|| almide_mir::pipeline::program_affine_shapes(&src, &[]));
        let Ok(Ok((shapes, walled))) = run else {
            t.program_walls += 1;
            continue;
        };
        t.walled_fns += walled.len();
        for (name, s) in shapes {
            let kind = match (s.param_dups.is_empty(), s.local_dups.is_empty()) {
                (true, true) => { t.affine += 1; "AFFINE" }
                (false, true) => { t.param_only += 1; "PARAM" }
                (true, false) => { t.local_only += 1; "LOCAL" }
                (false, false) => { t.both += 1; "BOTH" }
            };
            if rows {
                println!("{kind} {} {name} param={} local={}", file.display(), s.param_dups.len(), s.local_dups.len());
            }
        }
    }
    let lowered = t.affine + t.param_only + t.local_only + t.both;
    let pct = |n: usize| if lowered == 0 { 0.0 } else { 100.0 * n as f64 / lowered as f64 };
    println!("files            {}", t.files);
    println!("program walls    {}", t.program_walls);
    println!("walled fns       {}", t.walled_fns);
    println!("lowered fns      {lowered}");
    println!("affine-shaped    {} ({:.1}%)", t.affine, pct(t.affine));
    println!("param dups only  {} ({:.1}%)", t.param_only, pct(t.param_only));
    println!("local dups only  {} ({:.1}%)", t.local_only, pct(t.local_only));
    println!("both             {} ({:.1}%)", t.both, pct(t.both));
}
