//! #4338: every op the guest emitter, the stock-WASI transform or the cert
//! projection names falls in exactly one family of `almide_wasm::host_ops`.
//! The families' disjointness is a compile-time assertion there; this test
//! holds the ops declared OUTSIDE the table to it, so a new op written as a
//! bare number inside another family's range is refused too.
//!
//! The ops are read from their declarations: every `const OP_…: i32 = <n>;`
//! in the three crates' sources, the p1 served set, the abort-site pair and
//! every op the cert projection gives a capability.

use almide_wasm::host_ops::{family_of, FAMILIES};

/// `(file, line, op)` for every `const OP_<NAME>: i32 = <literal>;` under `dir`.
fn literal_op_consts(dir: &std::path::Path, out: &mut Vec<(String, usize, i32)>) {
    for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            literal_op_consts(&path, out);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read source");
        for (n, line) in text.lines().enumerate() {
            let Some(at) = line.find("const OP_") else { continue };
            let rest = &line[at + "const ".len()..];
            let Some((name, value)) = rest.split_once(": i32 =") else { continue };
            if !name.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_') {
                continue;
            }
            if let Ok(op) = value.trim().trim_end_matches(';').trim().parse::<i32>() {
                out.push((path.display().to_string(), n + 1, op));
            }
        }
    }
}

#[test]
fn every_declared_op_falls_in_exactly_one_family() {
    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut consts = Vec::new();
    for c in ["almide-wasm/src", "almide-wasi/src", "almide-wasm-run/src"] {
        literal_op_consts(&crates.join(c), &mut consts);
    }
    assert!(consts.len() > 50, "the source scan went blind: {} literal op constants", consts.len());
    let mut stray: Vec<String> = Vec::new();
    let mut check = |what: String, op: i32| {
        let n = FAMILIES.iter().filter(|(_, r)| r.contains(&op)).count();
        if n != 1 {
            stray.push(format!("{what}: op {op} is in {n} declared families"));
        }
    };
    for (file, line, op) in &consts {
        check(format!("{file}:{line}"), *op);
    }
    for op in crate::wasi::P1_SERVED_OPS {
        check("P1_SERVED_OPS".into(), *op);
    }
    for op in almide_wasm::ABORT_SITE_OPS {
        check("ABORT_SITE_OPS".into(), op);
    }
    for op in 0..1024 {
        if almide_wasm::cert_project::op_caps(op) != &[almide_wasm::cert_project::SENTINEL][..] {
            check("cert_project::op_caps".into(), op);
        }
    }
    assert!(stray.is_empty(), "ops outside the host_ops table (#4338):\n{}", stray.join("\n"));
}

/// The dispatch ranges the embedded host and the transform use are the
/// table's own.
#[test]
fn the_dispatch_ranges_are_the_declared_families() {
    assert_eq!(family_of(crate::net_host::UDP_FIRST), Some("UDP"));
    assert_eq!(family_of(crate::net_host::UDP_LAST), Some("UDP"));
    assert_eq!(family_of(crate::net_host::OP_FIRST), Some("TCP"));
    assert_eq!(family_of(crate::net_host::OP_LAST), Some("UNIX_SHM"));
    assert_eq!(crate::wasi::PROC_OPS, almide_wasm::host_ops::PROC);
    assert_eq!(crate::wasi::PROC_CHILD_OPS, almide_wasm::host_ops::PROC_CHILD);
    assert_eq!(family_of(crate::host_serve::OP_SERVE_RUN), Some("SERVE_RUN"));
}
