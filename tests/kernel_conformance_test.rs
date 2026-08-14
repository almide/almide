//! Kernel conformance (edit-locality Stage 3, C-280): the backends' stdout
//! for λ_almd-image programs must be EXACTLY the trace the kernel semantics
//! assigns — pinned at Lean compile time in
//! crates/almide-edit-belt/AlmideEditBelt (#guard + eval_sound + ev_det).
//!
//! Two layers: the hand-written family (`spec/wasm_cross/kernel_conformance
//! .almd`, whose wasm leg the wasm_cross harness carries), and the GENERATED
//! 48-program corpus (`proofs/kernel-conformance/`, from `lake exe
//! conformancegen`; drift-gated in CI against Corpus.lean) — run here on
//! native and, when a wasm runtime is present, on wasm too. Expected-output
//! literals/files duplicate the Lean side by construction; that reviewed
//! link is the trusted seam, per docs/contracts/proven-vs-trusted.md.

use std::path::Path;
use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

#[test]
fn kernel_conformance_native_stdout_matches_kernel_trace() {
    let repo_root = env!("CARGO_MANIFEST_DIR");
    let out = Command::new(almide())
        .args(["run", "spec/wasm_cross/kernel_conformance.almd"])
        .current_dir(repo_root)
        .output()
        .expect("run kernel conformance fixture");
    assert!(
        out.status.success(),
        "fixture must run clean, stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The kernel trace of `kAll` (Conformance.lean), one line per `print`.
    let expected = "alpha\nbeta\ngamma\ngot-ok\ngot-err\nreified\nfive-ok\ninside\noutside\n";
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        expected,
        "native stdout diverged from the kernel-semantic trace"
    );
}

/// Compile one corpus program to wasm and run it — wasmtime preferred,
/// Node.js WASI fallback, mirroring tests/wasm_runtime_test.rs. Returns
/// `None` when no wasm runtime exists on this machine.
fn run_wasm_stdout(src: &Path) -> Option<String> {
    let dir = tempfile::tempdir().expect("tempdir");
    let wasm_path = dir.path().join("prog.wasm");
    let out = Command::new(almide())
        .args([
            "build",
            src.to_str().unwrap(),
            "--target",
            "wasm",
            "-o",
            wasm_path.to_str().unwrap(),
        ])
        .output()
        .expect("build wasm");
    assert!(
        out.status.success(),
        "{}: wasm build failed:\n{}",
        src.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    let run = Command::new("wasmtime")
        .arg("--dir=/")
        .arg("-S")
        .arg("inherit-env=y")
        .arg(wasm_path.to_str().unwrap())
        .output();
    let out = match run {
        Ok(o) if o.status.code() != Some(127) => o,
        _ => {
            let js = format!(
                r#"
const {{ readFileSync }} = require('fs');
const {{ WASI }} = require('wasi');
const wasi = new WASI({{ version: 'preview1', args: [], env: {{}} }});
const buf = readFileSync('{}');
const mod = new WebAssembly.Module(buf);
const inst = new WebAssembly.Instance(mod, wasi.getImportObject());
wasi.start(inst);
"#,
                wasm_path.to_str().unwrap().replace('\\', "/")
            );
            let js_path = dir.path().join("run.cjs");
            std::fs::write(&js_path, &js).unwrap();
            match Command::new("node").arg(js_path.to_str().unwrap()).output() {
                Ok(o) => o,
                Err(_) => return None,
            }
        }
    };
    assert!(
        out.status.success(),
        "{}: wasm run failed:\n{}",
        src.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

#[test]
fn kernel_conformance_corpus_matches_kernel_traces() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("proofs/kernel-conformance");
    let mut programs: Vec<_> = std::fs::read_dir(&dir)
        .expect("corpus dir missing — run `lake exe conformancegen --write proofs/kernel-conformance`")
        .filter_map(|e| {
            let p = e.ok()?.path();
            (p.extension()? == "almd").then_some(p)
        })
        .collect();
    programs.sort();
    assert!(
        programs.len() >= 40,
        "corpus unexpectedly small: {} programs",
        programs.len()
    );
    let mut wasm_checked = 0usize;
    let mut wasm_available = true;
    for path in &programs {
        let expected = std::fs::read_to_string(path.with_extension("expected"))
            .expect("expected file");
        let out = Command::new(almide())
            .args(["run", path.to_str().unwrap()])
            .output()
            .expect("run almide");
        assert!(
            out.status.success(),
            "{}: native run failed:\n{}",
            path.display(),
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            expected,
            "{}: native stdout diverged from the kernel trace",
            path.display()
        );
        if wasm_available {
            match run_wasm_stdout(path) {
                Some(got) => {
                    assert_eq!(
                        got.trim_end(),
                        expected.trim_end(),
                        "{}: wasm stdout diverged from the kernel trace",
                        path.display()
                    );
                    wasm_checked += 1;
                }
                None => {
                    wasm_available = false;
                    eprintln!(
                        "kernel_conformance: wasm leg SKIPPED (no wasmtime/node runtime)"
                    );
                }
            }
        }
    }
    eprintln!(
        "kernel_conformance: {} programs native-verified, {} wasm-verified",
        programs.len(),
        wasm_checked
    );
}
