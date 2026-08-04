//! A DEPENDENCY package's submodules (#1094).
//!
//! Everything here works when the package is compiled directly; it broke only
//! once the same package was reached through `[dependencies]`, where every
//! module carries the package prefix (`collidelib.wire`) and the origin is
//! spelled with a version segment (`collidelib_v0_wire`). Two independent
//! failures came out of that:
//!
//!   1. A qualified sibling reference (`domain.Span` inside `wire.almd`)
//!      resolved to the REFERENCING file's own same-named type, silently — a
//!      structurally compatible type still type-checks.
//!   2. A derived Codec method never linked. The definition emitted
//!      `almide_rt_collidelib_v0_wire_collidelib_wire_Span_encode` (module
//!      doubled, because the origin spelling does not prefix-match the IR
//!      name) while every call site guessed a different shape. This one did
//!      NOT need a name collision — it hit any dependency submodule with a
//!      `Codec` type, which is why the plain case is pinned here too.

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

fn tools_available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

/// An app package that depends on `dep` by path. `dep_modules` become
/// `dep/src/*.almd`; the app gets `main.almd`.
fn scratch(name: &str, dep_modules: &[(&str, &str)], main: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue1094-{}", name));
    let _ = std::fs::remove_dir_all(&root);
    let dep = root.join("dep");
    let app = root.join("app");
    std::fs::create_dir_all(dep.join("src")).expect("mkdir dep");
    std::fs::create_dir_all(&app).expect("mkdir app");
    std::fs::write(
        dep.join("almide.toml"),
        "[package]\nname = \"deplib\"\nversion = \"0.1.0\"\n",
    )
    .expect("write dep toml");
    for (file, body) in dep_modules {
        std::fs::write(dep.join("src").join(file), body).expect("write dep module");
    }
    std::fs::write(
        app.join("almide.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\ndeplib = { path = \"../dep\" }\n",
    )
    .expect("write app toml");
    std::fs::write(app.join("main.almd"), main).expect("write main");
    app
}

fn run_in(dir: &Path) -> String {
    let output = Command::new(almide_bin())
        .args(["run", "main.almd"])
        .current_dir(dir)
        .output()
        .expect("failed to spawn almide");
    let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    combined
}

const MAIN: &str = "import deplib\neffect fn main() -> Unit = println(deplib.run())\n";

/// No collision at all — this is the case that proved the gap is about the
/// dependency prefix, not about two same-named types.
#[test]
fn dep_submodule_derived_codec_links() {
    if !tools_available() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    let dir = scratch(
        "plain-codec",
        &[
            (
                "inner.almd",
                concat!(
                    "import json\n",
                    "type Pigment: Codec = { r: Int, g: Int }\n",
                    "fn show(p: Pigment) -> String = json.encode(p)\n",
                ),
            ),
            (
                "mod.almd",
                "import self.inner\nfn run() -> String = inner.show(inner.Pigment { r: 1, g: 2 })\n",
            ),
        ],
        MAIN,
    );
    let out = run_in(&dir);
    assert!(out.contains(r#"{"r":1,"g":2}"#), "dep submodule Codec did not link:\n{out}");
}

/// A qualified sibling reference must name the SIBLING's type, even when the
/// referencing file declares one with the same bare name.
#[test]
fn dep_submodule_qualified_sibling_type_wins_over_local_same_name() {
    if !tools_available() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    let dir = scratch(
        "collision",
        &[
            (
                "domain.almd",
                "type Span = { name: String, n: Int }\nfn make(name: String, n: Int) -> Span = Span { name: name, n: n }\n",
            ),
            (
                "wire.almd",
                concat!(
                    "import json\n",
                    "import self.domain\n",
                    "type Span: Codec = { name: String, kind: Int }\n",
                    "fn to_wire(s: domain.Span) -> Span = Span { name: s.name, kind: s.n }\n",
                    "fn show(s: domain.Span) -> String = json.encode(to_wire(s))\n",
                ),
            ),
            (
                "mod.almd",
                "import self.domain\nimport self.wire\nfn run() -> String = wire.show(domain.make(\"op\", 3))\n",
            ),
        ],
        MAIN,
    );
    let out = run_in(&dir);
    assert!(
        out.contains(r#"{"name":"op","kind":3}"#),
        "qualified sibling type resolved to the local same-named type:\n{out}"
    );
}
