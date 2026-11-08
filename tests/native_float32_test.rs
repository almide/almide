//! Float32 must run through the certified renderer, including calls, joins
//! and aggregate slots. Compare untrimmed observations against v0 and wasm.

use std::process::Command;

#[test]
fn float32_values_preserve_cross_target_observations() {
    let cases = [
        ("rounding", r#"
fn step(a: Float32, b: Float32) -> Float32 = (a + b) - a
fn narrow(x: Int) -> Float32 = float32.from(x)
fn main() -> Unit = {
  println(float32.to_string(step(16777216.0, 1.0)))
  println(int.to_string(float32.to_bits(narrow(4611686293305294849))))
  println(int.to_string(float32.to_bits(narrow(-4611686293305294849))))
  println(float32.to_string(float32.from(1.0000000596046448)))
  println(float.to_string(float.from(float32.from_bits(1))))
}
"#),
        ("arithmetic", r#"
fn calc(a: Float32, b: Float32) -> Float32 = (a * b - b) / a
fn order(a: Float32, b: Float32) -> Unit = {
  println(if a < b then "lt" else "nlt")
  println(if a <= b then "le" else "nle")
  println(if a > b then "gt" else "ngt")
  println(if a >= b then "ge" else "nge")
  println(if a == b then "eq" else "ne")
  println(if a != b then "ne" else "eq")
}
fn main() -> Unit = {
  println(float32.to_string(calc(3.0, 2.0)))
  order(1.0, 2.0)
  order(float32.from_bits(2143289345), 2.0)
  order(-0.0, 0.0)
}
"#),
        ("containers", r#"
fn choose(b: Bool, x: Float32) -> Float32 = if b then -x else x + 1.0
fn maybe(b: Bool) -> Float32? = if b then some(-0.0) else none
fn main() -> Unit = {
  let xs: List[Float32] = [choose(true, 0.0), choose(false, 1.5), maybe(true) ?? 7.0, maybe(false) ?? 7.0]
  for x in xs { println(int.to_string(float32.to_bits(x))) }
  println(int.to_string(list.len(xs)))
}
"#),
    ];
    let dir = std::env::temp_dir().join(format!("almide-native-f32-source-{}", std::process::id()));
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
        if name == "containers" {
            assert_eq!(actual.stdout, b"2147483648\n1075838976\n2147483648\n1088421888\n4\n");
        }
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
