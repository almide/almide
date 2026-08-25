//! #1557 leg 3: a NESTED package import (`import self.application.place_order`
//! — the domain/application/infra directory layout) must resolve on the v1
//! wasm leg. The import-audit gate matched only the FIRST path segment after
//! `self` while the CLI resolver registers the module under its LAST (binding)
//! segment, so every nested path walled "package sibling not resolved" even
//! though the module was already delivered. Pinned as a compiler test because
//! the spec corpus's integration harness only expresses FLAT sibling libs.

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

#[test]
fn nested_package_import_runs_on_wasm() {
    let dir = std::env::temp_dir().join(format!("almd_nestpkg_{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src/application")).unwrap();
    std::fs::write(
        dir.join("almide.toml"),
        "[package]\nname = \"nestpkg\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/application/place_order.almd"),
        "pub fn total(a: Int, b: Int) -> Int = a + b\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/main.almd"),
        "import self.application.place_order\nimport io\n\n\
         effect fn main() -> Unit = {\n  \
           io.print(int.to_string(place_order.total(20, 22)))\n}\n",
    )
    .unwrap();

    for target in ["--target=wasm", "--target=rust"] {
        let out = Command::new(almide_bin())
            .args(["run", dir.join("src/main.almd").to_str().unwrap(), target])
            .output()
            .expect("failed to spawn almide run");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("42"),
            "nested package import must run on {target}: stdout={stdout} stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    std::fs::remove_dir_all(&dir).ok();
}
