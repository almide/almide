//! #572: `--trace-map` ships the source-to-generated correspondence with
//! `--target rust` output — `// almd: fn <name> @ line <N>` anchors on
//! every rendered function, and a sidecar `<file>.trace.json` derived by
//! scanning the SHIPPED text (so the map can never disagree with it).
//! The default emission stays byte-identical: zero anchors without the
//! flag (the 974-file emit baselines are untouched).

use std::path::Path;
use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

const PROGRAM: &str = "fn double(n: Int) -> Int = n * 2\n\nfn main() -> Unit = {\n  println(int.to_string(double(21)))\n}\n";

#[test]
fn trace_map_anchors_and_sidecar_agree() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let dir = std::env::temp_dir().join("almide-trace-map");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let src = dir.join("tm.almd");
    std::fs::write(&src, PROGRAM).expect("write");

    // Default emission carries NO anchors — byte-stability with the
    // baselines is the point of the flag.
    let plain = Command::new(almide_bin())
        .args([src.to_str().unwrap(), "--target", "rust"])
        .output()
        .expect("spawn");
    assert!(plain.status.success());
    assert!(
        !String::from_utf8_lossy(&plain.stdout).contains("// almd:"),
        "anchors leaked into the default emission"
    );

    let traced = Command::new(almide_bin())
        .args([src.to_str().unwrap(), "--target", "rust", "--trace-map"])
        .output()
        .expect("spawn");
    assert!(traced.status.success(), "{}", String::from_utf8_lossy(&traced.stderr));
    let code = String::from_utf8_lossy(&traced.stdout).to_string();
    assert!(code.contains("// almd: fn double @ line 1"), "missing the fn anchor");
    assert!(code.contains("// almd: fn main @ line 3"), "missing the main anchor");

    // The sidecar exists, parses, and each row's rust_line points AT (or
    // one past) its own anchor in the shipped text — the agreement claim.
    let map_path = dir.join("tm.trace.json");
    let map: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&map_path).expect("sidecar"))
            .expect("valid json");
    let rows = map.as_array().expect("array");
    assert!(rows.len() >= 2, "expected at least the two program fns, got {}", rows.len());
    let lines: Vec<&str> = code.lines().collect();
    for row in rows {
        let name = row["fn"].as_str().unwrap();
        let rust_line = row["rust_line"].as_u64().unwrap() as usize;
        let anchor = format!("// almd: fn {name} @");
        assert!(
            lines[rust_line - 2].trim_start().starts_with(&anchor),
            "row for {name}: rust_line {rust_line} does not sit under its anchor"
        );
    }
}
