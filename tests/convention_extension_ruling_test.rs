//! The #1591 ruling, pinned: convention-method extension at a distance is
//! RATIFIED (a caller module may define a method on a foreign type), and the
//! DUPLICATE case — the same `Type.method` defined in more than one module —
//! is a check-time E012 naming both sites.
//!
//! The duplicate rule exists because the duplicate had no defined winner:
//! measured on 2026-08-31, native answered the defining module's body and
//! wasm the caller's (#1726, I-divergence). Refusing at check removes the
//! divergence without either backend mirroring a precedence.
//!
//! Same-named types in DIFFERENT modules (`moda.Box` vs `modb.Box`) are
//! distinct identities: each may carry its own `tag` — pinned here so the
//! conflict detector can never regress into flagging them.

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

/// Write `files` under a temp package dir and run `almide check` on the first.
fn check_project(name: &str, files: &[(&str, &str)]) -> (bool, String) {
    let dir = std::env::temp_dir().join(format!("almd_conv_ruling_{name}_{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("almide.toml"),
        "[package]\nname = \"ruling\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    for (rel, src) in files {
        std::fs::write(dir.join(rel), src).unwrap();
    }
    let entry = dir.join(files[0].0);
    let out = Command::new(almide_bin())
        .args(["check", entry.to_str().unwrap()])
        .output()
        .expect("failed to spawn almide");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::remove_dir_all(&dir).ok();
    (out.status.success(), text)
}

const BOXLIB: &str = "type Box = { n: Int }\n\nfn Box.get(self) -> Int = self.n\n\nfn Box.tag(self) -> String = \"lib\"\n";

#[test]
fn extension_at_a_distance_checks_clean() {
    let (ok, text) = check_project(
        "extend",
        &[
            (
                "src/main.almd",
                "import self.boxlib\n\nfn Box.double(self) -> Int = self.n * 2\n\neffect fn main() -> Unit = println(int.to_string(boxlib.Box { n: 21 }.double()))\n",
            ),
            ("src/boxlib.almd", BOXLIB),
        ],
    );
    assert!(ok, "ratified extension must check clean:\n{text}");
}

#[test]
fn duplicate_across_modules_is_e012_with_both_sites() {
    let (ok, text) = check_project(
        "dup",
        &[
            (
                "src/main.almd",
                "import self.boxlib\n\nfn Box.tag(self) -> String = \"caller\"\n\neffect fn main() -> Unit = println(boxlib.Box { n: 1 }.tag())\n",
            ),
            ("src/boxlib.almd", BOXLIB),
        ],
    );
    assert!(!ok, "the duplicate must be refused:\n{text}");
    assert!(text.contains("E012"), "must be the duplicate-definition family:\n{text}");
    assert!(
        text.contains("more than one module"),
        "must name the rule:\n{text}"
    );
    assert!(
        text.contains("Box.tag") && text.contains("boxlib.Box.tag"),
        "must name both sites:\n{text}"
    );
    // Exactly ONE conflict error — not one per validation pass.
    let count = text.matches("more than one module").count();
    assert_eq!(count, 1, "the conflict must be reported once, got {count}:\n{text}");
}

#[test]
fn same_bare_name_in_two_modules_stays_two_method_sets() {
    let (_, text) = check_project(
        "twins",
        &[
            (
                "src/main.almd",
                "import self.moda\nimport self.modb\n\neffect fn main() -> Unit = {\n  println(moda.Box { n: 1 }.tag())\n  println(modb.Box { s: \"x\" }.tag())\n}\n",
            ),
            ("src/moda.almd", "type Box = { n: Int }\n\nfn Box.tag(self) -> String = \"A\"\n"),
            ("src/modb.almd", "type Box = { s: String }\n\nfn Box.tag(self) -> String = \"B\"\n"),
        ],
    );
    assert!(
        !text.contains("more than one module"),
        "distinct same-named types must not be flagged as duplicates:\n{text}"
    );
    // #1728: the E005 receiver-typing wart on this shape is fixed — the
    // method's registered self type is the canonical `moda.Box`, so the
    // twins CHECK clean outright.
    assert!(
        !text.contains("E005"),
        "twin methods must type against their own module's receiver:\n{text}"
    );
}

/// #1728 end-to-end: each twin dispatches to ITS OWN module's method — the
/// qualified emit key + the origin-qualified symbol map keep `moda.Box.tag`
/// and `modb.Box.tag` distinct all the way to the generated symbols.
#[test]
fn twins_dispatch_to_their_own_module() {
    let dir = std::env::temp_dir().join(format!("almd_conv_ruling_twinrun_{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("almide.toml"),
        "[package]\nname = \"ruling\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/main.almd"),
        "import self.moda\nimport self.modb\n\neffect fn main() -> Unit = {\n  println(moda.Box { n: 1 }.tag())\n  println(modb.Box { s: \"x\" }.tag())\n}\n",
    )
    .unwrap();
    std::fs::write(dir.join("src/moda.almd"), "type Box = { n: Int }\n\nfn Box.tag(self) -> String = \"A\"\n").unwrap();
    std::fs::write(dir.join("src/modb.almd"), "type Box = { s: String }\n\nfn Box.tag(self) -> String = \"B\"\n").unwrap();
    let out = Command::new(almide_bin())
        .args(["run", dir.join("src/main.almd").to_str().unwrap()])
        .output()
        .expect("failed to spawn almide");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    std::fs::remove_dir_all(&dir).ok();
    assert!(out.status.success(), "twins project must run:\n{stdout}{stderr}");
    assert_eq!(stdout, "A\nB\n", "each twin must dispatch to its own module's method:\n{stderr}");
}
