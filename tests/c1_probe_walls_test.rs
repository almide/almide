//! PROBE ONLY (c1 probe branch): print every spec/wasm_cross fixture's v1 native wall reason.
use std::path::Path;
#[test]
fn probe() {
    std::thread::Builder::new().stack_size(512 << 20).spawn(run).unwrap().join().unwrap();
}
fn run() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files: Vec<_> = std::fs::read_dir(root.join("spec/wasm_cross")).unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "almd")).collect();
    files.sort();
    let mut out = String::new();
    let mut ok = 0;
    for p in &files {
        let rel = p.strip_prefix(root).unwrap().display().to_string();
        let src = std::fs::read_to_string(p).unwrap();
        match almide_mir::pipeline::try_render_rust_source(&src) {
            Ok(_) => { ok += 1; out.push_str(&format!("OK\t{rel}\n")); }
            Err(e) => { let e = e.to_string().replace('\n', " | "); out.push_str(&format!("WALL\t{rel}\t{e}\n")); }
        }
    }
    eprintln!("rendered {ok} of {}", files.len());
    std::fs::write(root.join("c1-walls.tsv"), &out).unwrap();
}
