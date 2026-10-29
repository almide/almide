//! Concurrent `almide compile <stdlib module> --json` runs each read the whole
//! module. The bundled source is staged at one temp path per module; a plain
//! write truncated that file under a concurrent run's read, which then parsed
//! an empty module and reported an interface with zero functions — the
//! domain-edge matrix (`tools/domain_edge_matrix.py`) then counted the public
//! `value.int` / `value.index` as self-host helpers and its forged-signature
//! test failed under load.

use std::process::Command;

#[test]
fn concurrent_stdlib_interfaces_all_list_the_whole_module() {
    let runs: Vec<_> = (0..24)
        .map(|_| {
            std::thread::spawn(|| {
                let out = Command::new(env!("CARGO_BIN_EXE_almide"))
                    .args(["compile", "value", "--json"])
                    .output()
                    .expect("run almide compile value --json");
                assert!(out.status.success(), "compile value --json failed:\n{}", String::from_utf8_lossy(&out.stderr));
                let iface: serde_json::Value = serde_json::from_slice(&out.stdout).expect("interface JSON parses");
                iface["functions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(|f| f["name"].as_str().map(str::to_string))
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    for r in runs {
        let names = r.join().unwrap();
        assert!(
            names.iter().any(|n| n == "index") && names.iter().any(|n| n == "int"),
            "a concurrent run read a partial module: {} functions",
            names.len()
        );
    }
}
