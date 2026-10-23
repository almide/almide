//! #4006 step 3: how much of the corpus the verified native renderer covers.
//!
//! `almide build --target rust` tries the v1 trust-spine render
//! (`almide_mir::pipeline::try_render_rust_source`, the render whose every
//! function passed `verify_ownership`) and falls back to the standard
//! codegen on a wall. A program that walls gets no ownership certificate on
//! the native leg. This test measures the share of spec/wasm_cross that
//! renders, and holds it: proofs/native-verified-floor.txt lists every
//! fixture that renders today.
//!
//! - A listed fixture that walls fails: the native certificate's coverage
//!   went down.
//! - A fixture that renders but is not listed fails too: grow the floor with
//!   ALMIDE_UPDATE_NATIVE_VERIFIED_FLOOR=1, so the gain cannot slip back
//!   unnoticed.
//!
//! The walls print as a histogram by reason: the to-do list for #4006.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const FLOOR: &str = "proofs/native-verified-floor.txt";

fn reason_class(e: &str) -> String {
    // Fold names out of the reason so the histogram counts kinds of wall.
    let mut out = String::new();
    let mut in_tick = false;
    for c in e.chars() {
        match c {
            '`' => {
                in_tick = !in_tick;
                if in_tick {
                    out.push('_');
                }
            }
            _ if in_tick => {}
            _ => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(90).collect()
}

#[test]
fn the_native_verified_floor_holds() {
    // The compiler recurses deeply in a debug build; the CLI runs on a big
    // stack too.
    std::thread::Builder::new()
        .stack_size(512 << 20)
        .spawn(check)
        .expect("spawn")
        .join()
        .unwrap_or_else(|e| std::panic::resume_unwind(e));
}

fn check() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files: Vec<_> = std::fs::read_dir(root.join("spec/wasm_cross"))
        .expect("spec/wasm_cross")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "almd"))
        .collect();
    files.sort();
    let mut rendered = BTreeSet::new();
    let mut walls: BTreeMap<String, usize> = BTreeMap::new();
    for p in &files {
        let rel = p.strip_prefix(root).expect("under the root").display().to_string();
        let src = std::fs::read_to_string(p).expect("read");
        match almide_mir::pipeline::try_render_rust_source(&src) {
            Ok(_) => {
                rendered.insert(rel);
            }
            Err(e) => *walls.entry(reason_class(&e.to_string())).or_default() += 1,
        }
    }
    let total = files.len();
    let mut hist: Vec<_> = walls.into_iter().collect();
    hist.sort_by_key(|e| std::cmp::Reverse(e.1));
    eprintln!("native verified render: {} of {total} spec/wasm_cross fixtures", rendered.len());
    for (reason, n) in hist.iter().take(15) {
        eprintln!("  {n:4}  {reason}");
    }

    let floor_path = root.join(FLOOR);
    if almide_base::env::flag("ALMIDE_UPDATE_NATIVE_VERIFIED_FLOOR") {
        let mut out = String::from(
            "# spec/wasm_cross fixtures the verified native renderer (v1 trust spine,\n\
             # almide_mir::pipeline::try_render_rust_source) renders, #4006. GROW-ONLY; re-anchor with\n\
             # ALMIDE_UPDATE_NATIVE_VERIFIED_FLOOR=1 cargo test --test native_verified_floor_test\n",
        );
        out.push_str(&format!("# rendered: {} of {total}\n", rendered.len()));
        for r in &rendered {
            out.push_str(r);
            out.push('\n');
        }
        std::fs::write(&floor_path, out).expect("write the floor");
        return;
    }
    let floor: BTreeSet<String> = std::fs::read_to_string(&floor_path)
        .expect("proofs/native-verified-floor.txt")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(str::to_string)
        .collect();
    let lost: Vec<_> = floor.difference(&rendered).collect();
    let gained: Vec<_> = rendered.difference(&floor).collect();
    assert!(
        lost.is_empty(),
        "{} fixture(s) on the native verified floor now wall — the native certificate's coverage went down: {lost:?}",
        lost.len()
    );
    assert!(
        gained.is_empty(),
        "{} fixture(s) render natively but are not on the floor — grow it: \
         ALMIDE_UPDATE_NATIVE_VERIFIED_FLOOR=1 cargo test --test native_verified_floor_test: {gained:?}",
        gained.len()
    );
}
