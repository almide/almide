//! Native host I/O preserves bytes, flush points and fan output timelines.
use std::io::Write;
use std::process::{Command, Output, Stdio};

const FAN_IO: &str = r#"
import io
fn burn(n: Int) -> Int = {
 var x = 0
 for i in 0..<n { x = (x * 31 + i) % 1000003 }
 x
}
effect fn emit(n: Int) -> Result[Int, String] = {
 let work = burn(if n == 0 then 2000000 else 0)
 io.print("out ${n}/${work}:")
 io.write(io.stdout()!, bytes.from_list([255, n, 10]))!
 io.eprint("err ${n}:")
 io.write(io.stderr()!, bytes.from_list([254, n, 10]))!
 ok(n)
}
effect fn main() -> Unit = {
 let (a, b) = fan {
  emit(0)
  emit(1)
 }
 println("done ${a} ${b}")
}
"#;

const FAN_TRAP_IO: &str = r#"
import io
fn burn(n: Int) -> Int = {
 var x = 0
 for i in 0..<n { x = (x * 31 + i) % 1000003 }
 x
}
effect fn first() -> Result[Int, String] = {
 let work = burn(2000000)
 io.print("first ${work}:")
 io.write(io.stdout()!, bytes.from_list([255, 10]))!
 io.write(io.stderr()!, bytes.from_list([254, 10]))!
 ok(1)
}
effect fn crash() -> Result[Int, String] = {
 io.print("second")
 io.eprint("second-error\n")
 let z = int.parse("0") ?? 1
 ok(7 / z)
}
effect fn late() -> Result[Int, String] = {
 io.print("MUST NOT APPEAR")
 io.eprint("MUST NOT APPEAR")
 ok(3)
}
effect fn main() -> Unit = {
 let (a, b, c) = fan {
  first()
  crash()
  late()
 }
 println("unreachable ${a} ${b} ${c}")
}
"#;

fn run(binary: &std::path::Path, input: &[u8]) -> Output {
    let mut child = Command::new(binary).stdin(Stdio::piped()).stdout(Stdio::piped())
        .stderr(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

fn check(name: &str, source: &str, input: &[u8], stdout: &[u8], stderr: &[u8], exit: i32, threaded: bool) {
    let dir = std::env::temp_dir().join(format!("almide-native-io-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source_path = dir.join("program.almd");
    std::fs::write(&source_path, source).unwrap();
    let (rust, certificates) = almide_mir::pipeline::try_render_rust_source_certified(source)
        .unwrap_or_else(|e| panic!("{name}: {e}"));
    for property in ["ownership", "native-calls", "bounds"] {
        assert!(certificates.iter().any(|(p, _, _)| p == property), "{name}: missing {property}");
    }
    let expected_caps = if name == "input" {
        vec![almide_mir::Capability::Stdout.id(), almide_mir::Capability::Stdin.id()]
    } else { vec![almide_mir::Capability::Stdout.id()] };
    assert_eq!(almide_mir::pipeline::native_capability_reach(source).unwrap(),
        expected_caps.into_iter().collect(), "{name}: native capability reach");
    for (property, function, witness) in &certificates {
        let prop = almide_verify::Property::from_name(property).expect("known property");
        assert!(almide_verify::check(prop, witness.as_bytes()), "{name}: {property} {function}");
    }
    if threaded {
        assert!(rust.contains("let __almide_fan_h0 = almide_fan_spawn"), "{name}: real fan threads required");
    }
    let rs = dir.join("program.rs");
    std::fs::write(&rs, rust).unwrap();
    let native = dir.join("native");
    let built = Command::new("rustc").args(["--edition=2021", "-C", "opt-level=1"])
        .arg(&rs).arg("-o").arg(&native).output().unwrap();
    assert!(built.status.success(), "{name}: {}", String::from_utf8_lossy(&built.stderr));
    let actual = run(&native, input);
    assert_eq!((&actual.stdout[..], &actual.stderr[..], actual.status.code()), (stdout, stderr, Some(exit)), "{name}: native");
    let oracle = dir.join("v0");
    let built = Command::new(env!("CARGO_BIN_EXE_almide")).args(["build", "--no-verified"])
        .arg(&source_path).arg("-o").arg(&oracle).env("ALMIDE_NO_VERIFIED_OK", "1").output().unwrap();
    assert!(built.status.success(), "{name}: {}", String::from_utf8_lossy(&built.stderr));
    let other = run(&oracle, input);
    assert_eq!((&other.stdout[..], &other.stderr[..], other.status.code()), (stdout, stderr, Some(exit)), "{name}: v0");
    let wasm = almide::wasm_route::route_wasm(source_path.to_str().unwrap(), source,
        almide::wasm_route::ModuleSource::Disk { dep_paths: &[] }, None, Default::default(), &mut |_| {})
        .unwrap_or_else(|e| panic!("{name}: {e:?}"));
    let other = almide_wasm_run::run_wasm_with(&wasm.bytes, input).unwrap();
    assert_eq!((&other.stdout_bytes[..], &other.stderr_bytes[..], other.exit), (stdout, stderr, exit), "{name}: wasm");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn native_raw_output_and_order() {
    let mut expected = b"begin\n".to_vec();
    expected.extend(0..=255u8);
    expected.extend([255, 0, 128, 137, 80, 78, 71, 255, 254, 10, 65, 255, 10]);
    expected.extend(b"end\n");
    check("raw", include_str!("../spec/wasm_cross/io_write_raw_bytes.almd"), b"", &expected, b"", 0, false);
    check("order", include_str!("../spec/wasm_cross/io_write_ordering.almd"), b"",
        b"line-1\nBYTES\nline-2\nRAW\nline-3\nline-4\n", b"", 0, false);
    check("match", include_str!("../spec/wasm_cross/loop_effect_match_stmt.almd"), b"",
        b"ok 0ok 2ok 4acc=6 acc2=50", b"", 0, false);
}

#[test]
fn native_fan_raw_output_joins_the_timeline() {
    check("fan", FAN_IO, b"", b"out 0/235197:\xff\0\nout 1/0:\xff\x01\ndone 0 1\n",
        b"err 0:\xfe\0\nerr 1:\xfe\x01\n", 0, true);
    check("trap", FAN_TRAP_IO, b"", b"first 235197:\xff\nsecond", b"\xfe\nsecond-error\nError: division by zero\n", 1, true);
}

#[test]
fn native_input_preserves_raw_bytes_and_text_decoding() {
    check("input", r#"
import io
effect fn main() -> Unit = {
 io.print("prompt>")
 let line = io.read_line() ?? "missing"
 println(line)
 let raw = io.read(io.stdin()!, 3)!
 println("${bytes.to_list(raw)}")
 let rest = io.read_all()
 println(rest)
 println("${io.read_line() == none}")
}
"#, b"a\xff\r\n\0\x80\xffrest\xfe", "prompt>a�\n[0, 128, 255]\nrest�\ntrue\n".as_bytes(), b"", 0, false);
}
