//! Print the structural leg's CERTIFICATE BUNDLE for one program (#2759) —
//! every witness its build produced, in the format `almide-verify bundle`
//! reads (crates/almide-verify/src/bundle.rs).
//!
//! Usage: emit_structural_bundle <program.almd> [--artifact <file.wasm> [--anchors <file>]]
//!        emit_structural_bundle <program.almd> --only <property> <function>
//!
//! The program goes through the PRODUCT route (`almide::wasm_route`, the
//! function `almide build --target wasm` calls, build form) with the
//! witness sinks armed. The bundle carries, for the pass that shipped:
//!
//! - `ownership`, one per frame (witness.rs), and an `uncertified` record
//!   for every declined frame;
//! - `call-modes`, the program's call-mode witness (witness_modes.rs);
//! - `bounds`, the program's bounds witness (witness_bounds.rs, #4005): every
//!   `xs[i]` / `b[i]` read and store with the check before it, so the checks
//!   #3345 elided are re-derived from the checks that were emitted;
//! - `names`, over the stock-WASI bytes the build writes (`to_wasi` of the
//!   module): `(module:<space>)` per shared index space, `locals:<fn>` per
//!   function (cert_project.rs);
//! - `caps` per source-declared function and one `caps-transitive` call
//!   graph `(program)`, over the structural module (cert_project.rs).
//!
//! `--artifact <file.wasm>` (#2760) names the file `almide build --target
//! wasm` wrote for the same program. The producer refuses unless those bytes
//! are exactly the stock-WASI bytes its own route produced — the witnesses
//! then describe that file — and writes a version-2 bundle whose `artifact`
//! record carries the file's SHA-256 (the `sha2` crate; almide-verify
//! recomputes it with its own implementation and rejects a mismatch).
//!
//! `--anchors <file>` (#4011 (d)) also writes where in the shipped bytes
//! each bounds and init event is (witness_bundle.rs `records_with_anchors`),
//! the input `almide-anchor-check` judges against the artifact. Without
//! `--artifact`, the stock-WASI bytes the route produced are written beside
//! it as `<file>.wasm` (the anchor drill, scripts/witness-reinject.sh).
//!
//! `--only` prints one witness's bytes instead (the gate.sh feeder). Exit 2
//! when the program does not build or the witness is absent.

use std::io::Write;

fn fail(msg: &str) -> ! {
    let _ = writeln!(std::io::stderr(), "{msg}");
    std::process::exit(2);
}

use almide_wasm::witness::bundle::Record;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Absolute before the chdir below, so the paths name the files the
    // caller meant.
    let abs = |a: &String| std::path::absolute(a).unwrap_or_else(|e| fail(&format!("{a}: {e}")));
    let (rel, only, artifact, anchors) = match args.as_slice() {
        [rel] => (rel.clone(), None, None, None),
        [rel, flag, p, f] if flag == "--only" => (rel.clone(), Some((p.clone(), f.clone())), None, None),
        [rel, flag, a] if flag == "--artifact" => (rel.clone(), None, Some(abs(a)), None),
        [rel, flag, n] if flag == "--anchors" => (rel.clone(), None, None, Some(abs(n))),
        [rel, flag, a, flag2, n] if flag == "--artifact" && flag2 == "--anchors" => {
            (rel.clone(), None, Some(abs(a)), Some(abs(n)))
        }
        _ => fail(
            "usage: emit_structural_bundle <program.almd> [--artifact <file.wasm> [--anchors <file>] | --only <property> <function>]",
        ),
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    std::env::set_current_dir(&root).unwrap_or_else(|e| fail(&format!("cd {}: {e}", root.display())));
    let text = std::fs::read_to_string(&rel).unwrap_or_else(|e| fail(&format!("read {rel}: {e}")));
    let (records, uncertified, shipped, anchor_file) = produce(&rel, &text);
    if let Some(path) = &anchors {
        std::fs::write(path, &anchor_file).unwrap_or_else(|e| fail(&format!("write {}: {e}", path.display())));
        if artifact.is_none() {
            let w = path.with_extension("anchors.wasm");
            std::fs::write(&w, &shipped).unwrap_or_else(|e| fail(&format!("write {}: {e}", w.display())));
        }
    }
    if let Some((p, f)) = only {
        match records.iter().find(|r| r.property == p && r.function == f) {
            Some(r) => {
                let _ = std::io::stdout().write_all(r.bytes.as_bytes());
            }
            None => fail(&format!("no {p} witness for `{f}` in {rel}")),
        }
        return;
    }
    let pin = artifact.map(|path| {
        let file = std::fs::read(&path).unwrap_or_else(|e| fail(&format!("read {}: {e}", path.display())));
        if file != shipped {
            fail(&format!(
                "{} ({} bytes) is not the stock-WASI module the route produced for {rel} ({} bytes) — the witnesses would describe other bytes",
                path.display(),
                file.len(),
                shipped.len()
            ));
        }
        use sha2::Digest;
        let hex: String = sha2::Sha256::digest(&file).iter().map(|b| format!("{b:02x}")).collect();
        (hex, path.display().to_string())
    });
    let out = almide_wasm::witness::bundle::render(
        "almide-wasm emit_structural_bundle",
        &rel,
        pin.as_ref().map(|(h, p)| (h.as_str(), p.as_str())),
        &records,
        &uncertified,
    );
    let _ = std::io::stdout().write_all(out.as_bytes());
}

/// Build `rel` through the product route and collect every witness, with
/// the stock-WASI bytes the witnesses describe (witness_bundle.rs).
fn produce(rel: &str, text: &str) -> (Vec<Record>, Vec<(String, String)>, Vec<u8>, String) {
    use almide::wasm_route::{render_wasm_routed, ModuleSource, RouteOptions};
    almide_wasm::witness::start_collecting();
    let opts = RouteOptions { library: true, ..RouteOptions::default() };
    let routed = render_wasm_routed(rel, text, ModuleSource::Disk { dep_paths: &[] }, opts)
        .unwrap_or_else(|e| fail(&format!("{rel} does not build on the structural leg: {e:?}")));
    let shipped = routed.stock_wasi().unwrap_or_else(|e| fail(&format!("to_wasi: {e}")));
    // The same transform with its function map: where each emitted body
    // ships (the anchors' locator).
    let (again, fmap) = almide_wasm_run::wasi::to_wasi_mapped(&routed.bytes, &routed.host_ops)
        .unwrap_or_else(|e| fail(&format!("to_wasi_mapped: {e}")));
    if again != shipped {
        fail("to_wasi_mapped and to_wasi disagree on the shipped bytes");
    }
    let ((records, uncertified), anchor_file) =
        almide_wasm::witness::bundle::records_with_anchors(&routed.bytes, &shipped, None, &fmap)
            .unwrap_or_else(|e| fail(&e));
    (records, uncertified, shipped, anchor_file)
}
