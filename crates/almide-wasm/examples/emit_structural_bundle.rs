//! Print the structural leg's CERTIFICATE BUNDLE for one program (#2759) —
//! every witness its build produced, in the format `almide-verify bundle`
//! reads (crates/almide-verify/src/bundle.rs).
//!
//! Usage: emit_structural_bundle <program.almd> [--artifact <file.wasm>]
//!        emit_structural_bundle <program.almd> --only <property> <function>
//!
//! The program goes through the PRODUCT route (`almide::wasm_route`, the
//! function `almide build --target wasm` calls, build form) with the
//! witness sinks armed. The bundle carries, for the pass that shipped:
//!
//! - `ownership`, one per frame (witness.rs), and an `uncertified` record
//!   for every declined frame;
//! - `call-modes`, the program's call-mode witness (witness_modes.rs);
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
//! `--only` prints one witness's bytes instead (the gate.sh feeder). Exit 2
//! when the program does not build or the witness is absent.

use std::io::Write;

fn fail(msg: &str) -> ! {
    let _ = writeln!(std::io::stderr(), "{msg}");
    std::process::exit(2);
}

struct Record {
    property: &'static str,
    function: String,
    bytes: String,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (rel, only, artifact) = match args.as_slice() {
        [rel] => (rel.clone(), None, None),
        [rel, flag, p, f] if flag == "--only" => (rel.clone(), Some((p.clone(), f.clone())), None),
        [rel, flag, a] if flag == "--artifact" => {
            // Absolute before the chdir below, so the bundle names the file
            // the caller meant.
            let a = std::path::absolute(a).unwrap_or_else(|e| fail(&format!("{a}: {e}")));
            (rel.clone(), None, Some(a))
        }
        _ => fail("usage: emit_structural_bundle <program.almd> [--artifact <file.wasm> | --only <property> <function>]"),
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    std::env::set_current_dir(&root).unwrap_or_else(|e| fail(&format!("cd {}: {e}", root.display())));
    let text = std::fs::read_to_string(&rel).unwrap_or_else(|e| fail(&format!("read {rel}: {e}")));
    let (records, uncertified, shipped) = produce(&rel, &text);
    if let Some((p, f)) = only {
        match records.iter().find(|r| r.property == p && r.function == f) {
            Some(r) => {
                let _ = std::io::stdout().write_all(r.bytes.as_bytes());
            }
            None => fail(&format!("no {p} witness for `{f}` in {rel}")),
        }
        return;
    }
    let mut out = match &artifact {
        None => format!("almide-certificate-bundle 1\nproducer almide-wasm emit_structural_bundle\nsource {rel}\n"),
        Some(path) => {
            let file = std::fs::read(path).unwrap_or_else(|e| fail(&format!("read {}: {e}", path.display())));
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
            format!(
                "almide-certificate-bundle 2\nproducer almide-wasm emit_structural_bundle\nsource {rel}\nartifact sha256 {hex} {}\n",
                path.display()
            )
        }
    };
    for r in &records {
        out.push_str(&format!("witness {} {} {}\n{}\n", r.property, r.bytes.len(), r.function, r.bytes));
    }
    for (f, reason) in &uncertified {
        out.push_str(&format!("uncertified {f} {reason}\n"));
    }
    let _ = std::io::stdout().write_all(out.as_bytes());
}

/// Build `rel` through the product route and collect every witness, with
/// the stock-WASI bytes the witnesses describe.
fn produce(rel: &str, text: &str) -> (Vec<Record>, Vec<(String, String)>, Vec<u8>) {
    use almide::wasm_route::{render_wasm_routed, ModuleSource, RouteOptions};
    almide_wasm::witness::start_collecting();
    let opts = RouteOptions { library: true, ..RouteOptions::default() };
    let routed = render_wasm_routed(rel, text, ModuleSource::Disk { dep_paths: &[] }, opts)
        .unwrap_or_else(|e| fail(&format!("{rel} does not build on the structural leg: {e:?}")));
    let (pass, frames) = almide_wasm::witness::take_shipped().unwrap_or_else(|| fail("no shipped pass recorded"));
    let modes = almide_wasm::witness::take_modes();
    let decls = almide_wasm::witness::decls::take_for(&routed.bytes).unwrap_or_else(|| fail("no declaration table for the shipped module"));
    let shipped = routed.stock_wasi().unwrap_or_else(|e| fail(&format!("to_wasi: {e}")));

    let mut records = Vec::new();
    let mut uncertified = Vec::new();
    for (name, cert) in frames {
        match cert.strip_prefix(almide_wasm::witness::DECLINE_PREFIX) {
            Some(reason) => uncertified.push((name, reason.trim().to_string())),
            None if cert.starts_with('!') => uncertified.push((name, cert.trim().to_string())),
            None => records.push(Record { property: "ownership", function: name, bytes: cert }),
        }
    }
    if let Some((_, w)) = modes.into_iter().find(|(p, _)| *p == pass) {
        records.push(Record { property: "call-modes", function: "(program)".into(), bytes: w });
    }
    // `to_wasi` keeps every defined function's order and shifts it by the
    // imports it adds, so a declared function keeps its name in the
    // stock-WASI bytes; any other function is named by index.
    let named = almide_wasm::cert_project::decl_names(&decls).unwrap_or_else(|e| fail(&format!("decls: {e}")));
    let imports = |b: &[u8]| almide_wasm::cert_project::function_imports(b).unwrap_or_else(|e| fail(&format!("imports: {e}")));
    let shift = imports(&shipped) - imports(&routed.bytes);
    let label = |i: u32| match i.checked_sub(shift).and_then(|m| named.get(&m)) {
        Some(n) if i >= imports(&shipped) => format!("locals:{n}"),
        _ => format!("locals:func[{i}]"),
    };
    for (function, bytes) in almide_wasm::cert_project::names(&shipped, &label).unwrap_or_else(|e| fail(&format!("names: {e}"))) {
        records.push(Record { property: "names", function, bytes });
    }
    let (flat, graph) = almide_wasm::cert_project::caps(&decls).unwrap_or_else(|e| fail(&format!("caps: {e}")));
    for (function, bytes) in flat {
        records.push(Record { property: "caps", function, bytes });
    }
    records.push(Record { property: "caps-transitive", function: "(program)".into(), bytes: graph });
    (records, uncertified, shipped)
}
