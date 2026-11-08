//! Null's inlined MIR constructor must keep its Value type across returns,
//! branches and aggregates, with the same observable bytes as the v0 oracle.

use std::process::Command;

#[test]
fn null_values_preserve_native_observations() {
    let cases = [
        ("direct", "fn main() -> Unit = println(json.stringify(value.null()))", "null\n"),
        ("all_null", "fn main() -> Unit = { let xs = [value.null(), value.null()]\nfor x in xs { println(json.stringify(x)) } }", "null\nnull\n"),
        ("return", "fn fresh() -> Value = value.null()\nfn main() -> Unit = { let x = fresh()\nprintln(json.stringify(x))\nprintln(json.stringify(x)) }", "null\nnull\n"),
        ("branch", "fn pick(b: Bool) -> Value = if b then value.null() else value.int(7)\nfn main() -> Unit = { println(json.stringify(pick(true)))\nprintln(json.stringify(pick(false))) }", "null\n7\n"),
        ("list", "fn main() -> Unit = { let xs = [value.null(), value.int(7), value.null()]\nfor x in xs { println(json.stringify(x)) }\nprintln(int.to_string(list.len(xs))) }", "null\n7\nnull\n3\n"),
    ];
    let dir = std::env::temp_dir().join(format!("almide-native-null-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, body, expected) in cases {
        let source = format!("import json\n{body}\n");
        let (rust, certs) = almide_mir::pipeline::try_render_rust_source_certified(&source)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(certs.iter().any(|(p, f, _)| p == "ownership" && f == "main"));
        assert!(certs.iter().any(|(p, _, _)| p == "native-calls"));
        assert!(certs.iter().any(|(p, f, _)| p == "bounds" && f == "main"));
        let rs = dir.join(format!("{name}.rs"));
        let binary = dir.join(name);
        std::fs::write(&rs, rust).unwrap();
        let compiled = Command::new("rustc").args(["--edition=2021", "-C", "opt-level=1"])
            .arg(&rs).arg("-o").arg(&binary).output().unwrap();
        assert!(compiled.status.success(), "{name}: {}", String::from_utf8_lossy(&compiled.stderr));
        let actual = Command::new(binary).output().unwrap();
        assert!(actual.status.success(), "{name}");
        assert_eq!(actual.stdout, expected.as_bytes(), "{name}");
        assert!(actual.stderr.is_empty(), "{name}");
        let input = dir.join(format!("{name}.almd"));
        let oracle = dir.join(format!("{name}-v0"));
        std::fs::write(&input, source).unwrap();
        let build = Command::new(env!("CARGO_BIN_EXE_almide"))
            .args(["build", "--no-verified"]).arg(&input).arg("-o").arg(&oracle)
            .env("ALMIDE_NO_VERIFIED_OK", "1").output().unwrap();
        assert!(build.status.success(), "{name}: {}", String::from_utf8_lossy(&build.stderr));
        let other = Command::new(oracle).output().unwrap();
        assert_eq!((actual.stdout, actual.stderr, actual.status.code()),
            (other.stdout, other.stderr, other.status.code()), "{name}: v1/v0 bytes and exit");
    }
    std::fs::remove_dir_all(dir).unwrap();
}
