//! Convention methods across a module boundary (#1087, #1089).
//!
//! Convention/protocol methods live in tables keyed by a BARE type name, while
//! type identity is the qualified `mod.Type`. Everything that started from a
//! checked expression's type therefore missed across an `import`, and the two
//! halves failed differently:
//!
//!   A. Two modules declaring the same bare type name, one deriving Codec,
//!      collided — reported as a field mismatch attributed to the module that
//!      declares no Codec at all. Plain same-named types always coexisted, so
//!      the derive is the trigger.
//!   B. `p.encode()`, `"${v}"` with a custom repr, `x.repr()`, `json.encode(p)`
//!      and a `[T: Codec]` bound all failed from another module, while
//!      `mod.T.method(x)` — the one spelling that throws the module segment
//!      away — worked. The repr case failed SILENTLY, rendering the variant
//!      name with no diagnostic anywhere.
//!
//! These run the whole pipeline (`almide run`), not just `check`: the first
//! attempt at the fix type-checked and then died in IR verify, because the
//! checker needs the key that HOLDS the signature while lowering needs the name
//! the DEFINITION carries. A `check`-only test would have passed on it.

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

/// A package with `almide.toml`, the given `src/*.almd` modules, and `main.almd`.
fn scratch(name: &str, modules: &[(&str, &str)], main: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-issue1087-{}", name));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).expect("mkdir scratch");
    std::fs::write(
        dir.join("almide.toml"),
        "[package]\nname = \"convpkg\"\nversion = \"0.1.0\"\n",
    )
    .expect("write toml");
    for (file, body) in modules {
        std::fs::write(dir.join("src").join(file), body).expect("write module");
    }
    std::fs::write(dir.join("main.almd"), main).expect("write main");
    dir
}

/// stdout+stderr of `almide run main.almd` inside `dir`.
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

const LIB: &str = concat!(
    "type Color = Red | Blue\n",
    "fn Color.repr(c: Color) -> String = match c { Red => \"red\", Blue => \"blue\" }\n",
    "type P: Codec = { x: Int }\n",
);

#[test]
fn same_bare_type_name_in_two_modules_one_deriving_codec() {
    if !tools_available() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    let dir = scratch(
        "collision",
        &[
            ("domain.almd", "type Span = { name: String, n: Int }\n"),
            (
                "wire.almd",
                concat!(
                    "import json\n",
                    "import self.domain\n",
                    "type Span: Codec = { name: String, kind: Int }\n",
                    "fn to_wire(s: domain.Span) -> Span = Span { name: s.name, kind: s.n }\n",
                    "fn show(s: domain.Span) -> String = json.stringify(Span.encode(to_wire(s)))\n",
                ),
            ),
        ],
        concat!(
            "import self.domain\n",
            "import self.wire\n",
            "effect fn main() -> Unit = println(wire.show(domain.Span { name: \"op\", n: 3 }))\n",
        ),
    );
    let out = run_in(&dir);
    assert!(
        out.contains(r#"{"name":"op","kind":3}"#),
        "same-named types in two modules must stay distinct, got:\n{out}"
    );
}

#[test]
fn derived_encode_resolves_in_method_form_across_import() {
    if !tools_available() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    let dir = scratch(
        "encode",
        &[("lib.almd", LIB)],
        concat!(
            "import json\n",
            "import self.lib\n",
            "effect fn main() -> Unit = println(json.stringify(lib.P { x: 1 }.encode()))\n",
        ),
    );
    assert!(run_in(&dir).contains(r#"{"x":1}"#));
}

/// The silent one: with no explicit repr found, this rendered `Red`.
#[test]
fn custom_repr_is_honoured_by_interpolation_across_import() {
    if !tools_available() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    let dir = scratch(
        "repr-interp",
        &[("lib.almd", LIB)],
        "import self.lib\neffect fn main() -> Unit = println(\"${lib.Red}\")\n",
    );
    let out = run_in(&dir);
    assert!(out.contains("red"), "custom repr ignored across import:\n{out}");
}

#[test]
fn custom_repr_resolves_in_ufcs_form_across_import() {
    if !tools_available() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    let dir = scratch(
        "repr-ufcs",
        &[("lib.almd", LIB)],
        "import self.lib\neffect fn main() -> Unit = println(lib.Blue.repr())\n",
    );
    assert!(run_in(&dir).contains("blue"));
}

#[test]
fn json_encode_convenience_resolves_across_import() {
    if !tools_available() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    let dir = scratch(
        "json-encode",
        &[(
            "lib.almd",
            "import json\ntype P: Codec = { x: Int }\nfn show(p: P) -> String = json.encode(p)\n",
        )],
        "import self.lib\neffect fn main() -> Unit = println(lib.show(lib.P { x: 1 }))\n",
    );
    assert!(run_in(&dir).contains(r#"{"x":1}"#));
}

/// Monomorphization named the specialized method from the QUALIFIED type, so
/// the call flattened to `lib_P_encode` against `almide_rt_lib_P_encode`.
#[test]
fn codec_protocol_bound_accepts_a_type_from_another_module() {
    if !tools_available() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    let dir = scratch(
        "codec-bound",
        &[("lib.almd", LIB)],
        concat!(
            "import json\n",
            "import self.lib\n",
            "fn to_json[T: Codec](v: T) -> String = json.stringify(v.encode())\n",
            "effect fn main() -> Unit = println(to_json(lib.P { x: 1 }))\n",
        ),
    );
    assert!(run_in(&dir).contains(r#"{"x":1}"#));
}
