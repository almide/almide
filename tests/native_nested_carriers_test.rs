//! Nested carriers must preserve distinct absent states and remain usable
//! through aliases. Compare exact expected bytes and all four execution routes.

use std::process::Command;

#[test]
fn nested_carriers_preserve_cross_target_observations() {
    let cases = [
        ("nested_option", r#"
fn make(x: Int) -> (Int?)? = if x == 0 then none else if x == 1 then some(none) else some(some(x))
fn read(x: (Int?)?) -> Int = match x { none => -1, some(y) => y ?? -2 }
fn main() -> Unit = {
  println(int.to_string(read(make(0))))
  println(int.to_string(read(make(1))))
  let x = make(42)
  println(int.to_string(read(x)))
  println(int.to_string(read(x)))
}
"#),
        ("option_list", r#"
fn read(xs: List[Int?]) -> Unit = for x in xs { println(int.to_string(x ?? -1)) }
fn main() -> Unit = {
  let xs: List[Int?] = [some(7), none, some(9)]
  read(xs)
  read(xs)
}
"#),
        ("result_option", r#"
fn make(x: Int) -> Result[Int?, String] = if x == 0 then err("empty") else if x == 1 then ok(none) else ok(some(x))
fn read(x: Result[Int?, String]) -> String = match x { err(e) => e, ok(y) => int.to_string(y ?? -2) }
fn main() -> Unit = {
  println(read(make(0)))
  println(read(make(1)))
  let x = make(42)
  println(read(x))
  println(read(x))
}
"#),
    ];
    let dir = std::env::temp_dir().join(format!("almide-native-carriers-source-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, source) in cases {
        let input = dir.join(format!("{name}.almd"));
        let rs = dir.join(format!("{name}.rs"));
        let binary = dir.join(name);
        std::fs::write(&input, source).unwrap();
        let (rust, certs) = almide_mir::pipeline::try_render_rust_source_certified(source)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        for prop in ["ownership", "native-calls", "bounds"] {
            assert!(certs.iter().any(|(p, _, _)| p == prop), "{name}: missing {prop}");
        }
        std::fs::write(&rs, rust).unwrap();
        let compiled = Command::new("rustc").args(["--edition=2021", "-C", "opt-level=1"])
            .arg(&rs).arg("-o").arg(&binary).output().unwrap();
        assert!(compiled.status.success(), "{name}: {}", String::from_utf8_lossy(&compiled.stderr));
        let actual = Command::new(&binary).output().unwrap();
        assert!(actual.status.success(), "{name}: {}", String::from_utf8_lossy(&actual.stderr));
        assert!(actual.stderr.is_empty(), "{name}");
        let expected: &[u8] = match name {
            "nested_option" => b"-1\n-2\n42\n42\n",
            "option_list" => b"7\n-1\n9\n7\n-1\n9\n",
            "result_option" => b"empty\n-2\n42\n42\n",
            _ => unreachable!(),
        };
        assert_eq!(actual.stdout, expected, "{name}");
        let ir = almide::wasm_leg::lower_to_ir(input.to_str().unwrap(), source).unwrap();
        let reference = almide_interp::Interpreter::new(&ir).run_main();
        assert!(matches!(reference.status, almide_interp::RunStatus::Ok), "{name}: {:?}", reference.status);
        assert_eq!((actual.stdout.as_slice(), actual.stderr.as_slice(), actual.status.code()),
            (reference.stdout.as_bytes(), reference.stderr.as_bytes(), Some(reference.exit_code())), "{name}: interp");
        let oracle = dir.join(format!("{name}-v0"));
        let built = Command::new(env!("CARGO_BIN_EXE_almide"))
            .args(["build", "--no-verified"]).arg(&input).arg("-o").arg(&oracle)
            .env("ALMIDE_NO_VERIFIED_OK", "1").output().unwrap();
        assert!(built.status.success(), "{name}: {}", String::from_utf8_lossy(&built.stderr));
        let v0 = Command::new(oracle).output().unwrap();
        let wasm = Command::new(env!("CARGO_BIN_EXE_almide"))
            .arg("run").arg(&input).args(["--target", "wasm"]).output().unwrap();
        for (leg, other) in [("v0", v0), ("wasm", wasm)] {
            assert_eq!((&actual.stdout, &actual.stderr, actual.status.code()),
                (&other.stdout, &other.stderr, other.status.code()), "{name}: {leg}");
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}
