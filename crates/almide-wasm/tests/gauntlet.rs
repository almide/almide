//! The DDD gauntlet gate (spec/gauntlet/): every cell is a program shape
//! a real ports-and-adapters exercise produced on the incumbent
//! (2026-08-22..26), replayed here as a SECOND AXIS beside the corpus
//! burn-up — written against the language, not harvested from the
//! tracker (spec/gauntlet/README.md §1).
//!
//! Each cell's verdict is pinned in golden/gauntlet-manifest.txt:
//!   run    exit=<n> sha256=<hex>   — compiles, runs, byte-pinned stdout
//!   wall   <reason>                — the emitter's honest refusal
//!   reject <first error line>     — the front refuses at source level
//!
//! ANY drift — regression or improvement — is red until the manifest is
//! regenerated deliberately:
//!   ALMIDE_UPDATE_GAUNTLET=1 cargo test --release -p almide-wasm --test gauntlet
//! A silent wrong value is unrepresentable in this scheme: a run cell's
//! stdout hash is exact, and `RUN_FLOOR` makes demoting a running cell
//! back to a wall a two-place edit, like the burn-up's SUPPORTED_FLOOR.

mod harness;
use harness::run_wasm;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Grow-only: the number of `run` rows may never drop below this.
const RUN_FLOOR: usize = 18;

fn gauntlet_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/gauntlet").canonicalize().expect("spec/gauntlet exists")
}

fn manifest_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/gauntlet-manifest.txt")
}

/// Every cell entry file, sorted: single-file cells, then dir cells
/// (almide.toml + src/main.almd), then the layered packages.
fn cells(root: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let mut singles: Vec<_> = std::fs::read_dir(root.join("cells"))
        .expect("cells/")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "almd"))
        .collect();
    singles.sort();
    for p in singles {
        out.push((format!("cells/{}", p.file_name().unwrap().to_string_lossy()), p));
    }
    for sub in ["cells", "pkg"] {
        let mut dirs: Vec<_> = std::fs::read_dir(root.join(sub))
            .expect("gauntlet subdir")
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.join("almide.toml").is_file())
            .collect();
        dirs.sort();
        for d in dirs {
            out.push((
                format!("{sub}/{}", d.file_name().unwrap().to_string_lossy()),
                d.join("src/main.almd"),
            ));
        }
    }
    out
}

/// One cell's verdict line (without the trailing cell name).
fn verdict(entry: &Path) -> String {
    let text = std::fs::read_to_string(entry).expect("cell source");
    let path = entry.to_string_lossy();
    let ir = match almide_spine::s5::lower_to_ir(&path, &text) {
        Ok(ir) => ir,
        Err(e) => {
            let first = e.lines().next().unwrap_or("").trim();
            return format!("reject\t{first}");
        }
    };
    match almide_wasm::emit_program(&ir) {
        Ok(bytes) => {
            let run = run_wasm(&bytes).expect("engine runs the module");
            use sha2::Digest;
            let mut h = String::new();
            for b in sha2::Sha256::digest(run.stdout.as_bytes()) {
                let _ = write!(h, "{b:02x}");
            }
            format!("run\texit={} sha256={h}", run.exit)
        }
        Err(almide_wasm::EmitError::Unsupported(r)) => format!("wall\t{r}"),
    }
}

#[test]
fn gauntlet_matches_manifest() {
    let root = gauntlet_root();
    let mut rows = String::new();
    let mut run_count = 0usize;
    for (name, entry) in cells(&root) {
        let v = verdict(&entry);
        if v.starts_with("run\t") {
            run_count += 1;
        }
        rows.push_str(&format!("{v}\t{name}\n"));
    }
    let mp = manifest_path();
    if std::env::var("ALMIDE_UPDATE_GAUNTLET").is_ok() {
        std::fs::write(&mp, &rows).expect("write manifest");
    }
    let want = std::fs::read_to_string(&mp)
        .expect("golden/gauntlet-manifest.txt exists — generate with ALMIDE_UPDATE_GAUNTLET=1");
    if rows != want {
        let diff: Vec<String> = want
            .lines()
            .zip(rows.lines())
            .filter(|(w, g)| w != g)
            .map(|(w, g)| format!("  manifest: {w}\n  actual:   {g}"))
            .collect();
        panic!(
            "gauntlet drift ({} row(s)) — every change, regression OR improvement, is ratified by \
             regenerating: ALMIDE_UPDATE_GAUNTLET=1 cargo test --release -p almide-wasm --test gauntlet\n{}",
            diff.len().max(want.lines().count().abs_diff(rows.lines().count())),
            diff.join("\n")
        );
    }
    assert!(
        run_count >= RUN_FLOOR,
        "run rows fell to {run_count} < RUN_FLOOR {RUN_FLOOR} — a running cell was demoted"
    );
}
