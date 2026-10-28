//! #4099: an operator written on a type parameter (`fn f[T: Numeric](a: T,
//! b: T) -> T = a + b`) lowered to a `BinOp`, and instantiating it at `T =
//! BigInt` kept the `BinOp` — monomorphization only re-dispatched primitive
//! widths. The operator on a bound conforming type must be that type's
//! requirement, exactly as the same operator on a concrete `BigInt` is
//! (R-bigint-04), and both targets must print the same values.

use std::process::Command;

fn almide_bin() -> String {
    env!("CARGO_BIN_EXE_almide").to_string()
}

const PROGRAM: &str = r#"import bigint

fn add2[T: Numeric](a: T, b: T) -> T = a + b
fn sub2[T: Numeric](a: T, b: T) -> T = a - b
fn mul2[T: Numeric](a: T, b: T) -> T = a * b
fn div2[T: Numeric](a: T, b: T) -> T = a / b
fn rem2[T: Numeric](a: T, b: T) -> T = a % b
fn neg1[T: Numeric](a: T) -> T = -a
fn below[T: Integer](a: T, b: T) -> Bool = a < b
fn at_least[T: Integer](a: T, b: T) -> Bool = a >= b

fn big(s: String) -> BigInt = match bigint.parse(s) {
  ok(x) => x,
  err(e) => panic(e),
}

effect fn main() -> Unit = {
  let a = big("123456789012345678901234567890")
  let b = big("-98765432109876543210")
  println(bigint.to_string(add2(a, b)))
  println(bigint.to_string(sub2(a, b)))
  println(bigint.to_string(mul2(a, b)))
  println(bigint.to_string(div2(a, b)))
  println(bigint.to_string(rem2(a, b)))
  println(bigint.to_string(neg1(b)))
  println("${below(b, a)} ${below(a, b)} ${at_least(a, a)} ${at_least(b, a)}")
  println("${add2(3, 4)} ${neg1(5)} ${below(1, 2)}")
}
"#;

const EXPECTED: &str = "123456788913580246791358024680\n\
123456789111111111011111111100\n\
-12193263113702179522496570642237463801111263526900\n\
-1249999988\n\
60185185207253086410\n\
98765432109876543210\n\
true false true false\n\
7 -5 true\n";

fn run(dir: &std::path::Path, target: Option<&str>) -> String {
    let src = dir.join("generic_bigint.almd");
    std::fs::write(&src, PROGRAM).expect("write program");
    let mut cmd = Command::new(almide_bin());
    cmd.arg("run").arg(&src);
    if let Some(t) = target {
        cmd.args(["--target", t]);
    }
    let out = cmd.output().expect("spawn almide run");
    assert!(
        out.status.success(),
        "almide run ({target:?}) failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn an_operator_on_a_type_parameter_bound_to_bigint_calls_the_requirement_on_both_targets() {
    let dir = std::env::temp_dir().join(format!("almide-4099-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let native = run(&dir, None);
    let wasm = run(&dir, Some("wasm"));
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(native, EXPECTED, "native output");
    assert_eq!(wasm, native, "wasm and native disagree");
}
