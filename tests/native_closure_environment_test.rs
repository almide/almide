//! Closure environments retain source types, ownership and observable behavior.
use std::process::Command;

#[test]
fn closure_environments_preserve_all_execution_routes() {
    let cases = [
        ("write_only_capture", r#"
effect fn main() -> Unit = {
 var values: Map[String, Int] = [:]
 let put = (k: String, n: Int) => { values[k] = n }
 put("a", 2)
 put("b", 3)
 put("a", 4)
 println("${values["a"] ?? 0}:${values["b"] ?? 0}")
}
"#, "4:3\n"),
        ("shared_scalar_cell", r#"
type Counter = { add: (Int) -> Int, get: () -> Int }
effect fn make() -> Counter = {
 var n = 1
 { add: (x) => { n = n + x; n }, get: () => n }
}
effect fn main() -> Unit = {
 let c = make()!
 let alias = c
 println(int.to_string(c.add(2)))
 println(int.to_string(alias.get()))
 println(int.to_string(alias.add(4)))
 println(int.to_string(c.get()))
}
"#, "3\n3\n7\n7\n"),
        ("shared_heap_cells", r#"
type Buffer = { add: (Int) -> Int, read: () -> String }
effect fn make() -> Buffer = {
 var xs = [1]
 var text = "a"
 { add: (x) => { list.push(xs, x); text = text + "!"; list.len(xs) },
   read: () => text + ":${xs}" }
}
effect fn main() -> Unit = {
 let b = make()!
 let alias = b
 println(int.to_string(b.add(2)))
 println(alias.read())
 println(int.to_string(alias.add(3)))
 println(b.read())
}
"#, "2\na!:[1, 2]\n3\na!!:[1, 2, 3]\n"),
        ("escaping_heap_captures", r#"
fn make(s: String, xs: List[Int]) -> (Int) -> String = (n) => s + ":${xs}:${n}"
fn main() -> Unit = {
 var xs = [2, 4]
 let f = make("cap" + "ture", xs)
 list.push(xs, 9)
 println(f(1))
 println(f(2))
 println("${xs}")
}
"#, "capture:[2, 4]:1\ncapture:[2, 4]:2\n[2, 4, 9]\n"),
        ("nested_heap_closures", r#"
fn make(s: String) -> (Int) -> String = {
 let first = (n: Int) => s + int.to_string(n)
 (n: Int) => first(n) + "!"
}
fn main() -> Unit = {
 let f = make("n")
 println(f(3))
 println(f(4))
}
"#, "n3!\nn4!\n"),
        ("stored_heap_closures", r#"
type Handler = { f: (Int) -> String, name: String }
fn make(s: String) -> Handler = { f: (n) => s + int.to_string(n), name: s }
fn call(h: Handler, n: Int) -> String = h.f(n)
fn choose(left: Bool, s: String) -> (Int) -> String = if left then (n) => s + int.to_string(n) else (n) => int.to_string(n) + s
fn main() -> Unit = {
 let a = make("a")
 let b = { ...a, name: "b" }
 println(call(a, 1))
 println(call(b, 2))
 println(a.name + b.name)
 let fs = [choose(true, "x"), choose(false, "y")]
 for f in fs { println(f(3)) }
}
"#, "a1\na2\nab\nx3\n3y\n"),
        ("heap_capture_callbacks", r#"
fn make(s: String) -> (Int) -> String = (n) => s + int.to_string(n)
fn main() -> Unit = {
 let prefix = "p"
 println("${list.map([1, 2], (n) => prefix + int.to_string(n))}")
 let callback = make("q")
 println("${list.map([3, 4], callback)}")
 let wanted = "b"
 let m = ["a": 1, "b": 2]
 println("${map.filter(m, ((k, v)) => k == wanted)}")
 println(wanted)
}
"#, "[\"p1\", \"p2\"]\n[\"q3\", \"q4\"]\n[\"b\": 2]\nb\n"),
        ("compound_heap_captures", r#"
type Data = { title: String, values: List[Int] }
fn make(d: Data, m: Map[String, Int], b: Bytes) -> () -> String = () => d.title + ":${d.values}:${map.get(m, "x") ?? 0}:${bytes.len(b)}"
fn main() -> Unit = {
 let data = Data { title: "r", values: [2, 4] }
 let m = ["x": 7]
 let b = bytes.from_list([1, 2, 3])
 let f = make(data, m, b)
 println(f())
 println(f())
 println(data.title)
 println("${m}")
 println(int.to_string(bytes.len(b)))
}
"#, "r:[2, 4]:7:3\nr:[2, 4]:7:3\nr\n[\"x\": 7]\n3\n"),
        ("distinct_callable_results", r#"
fn text(n: Int) -> String = "v${n}"
fn values(n: Int) -> List[Int] = [n, n + 1]
fn invoke(f: (Int) -> String, g: (Int) -> List[Int]) -> String = f(2) + ":${g(4)}"
fn make(x: Float) -> () -> Float = () => x
fn main() -> Unit = {
 println(invoke(text, values))
 let f = make(-0.0)
 assert_eq(float.to_bits(f()), float.to_bits(-0.0))
 println("bits preserved")
}
"#, "v2:[4, 5]\nbits preserved\n"),
    ];
    let dir = std::env::temp_dir().join(format!("almide-native-closures-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut failures = Vec::new();
    for (name, source, expected) in cases {
        let result = std::panic::catch_unwind(|| {
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
            let compile = Command::new("rustc").args(["--edition=2021", "-C", "opt-level=1"])
                .arg(&rs).arg("-o").arg(&binary).output().unwrap();
            assert!(compile.status.success(), "{name}: {}", String::from_utf8_lossy(&compile.stderr));
            let actual = Command::new(&binary).output().unwrap();
            assert!(actual.status.success(), "{name}: native execution");
            assert_eq!(actual.stdout, expected.as_bytes(), "{name}: expected bytes");
            assert!(actual.stderr.is_empty(), "{name}: unexpected stderr");
            let ir = almide::wasm_leg::lower_to_ir(input.to_str().unwrap(), source).unwrap();
            let reference = almide_interp::Interpreter::new(&ir).run_main();
            assert!(matches!(reference.status, almide_interp::RunStatus::Ok), "{name}: {:?}", reference.status);
            assert_eq!((&actual.stdout[..], &actual.stderr[..], actual.status.code()),
                (reference.stdout.as_bytes(), reference.stderr.as_bytes(), Some(reference.exit_code())), "{name}: interp");
            let oracle = dir.join(format!("{name}-v0"));
            let built = Command::new(env!("CARGO_BIN_EXE_almide"))
                .args(["build", "--no-verified"]).arg(&input).arg("-o").arg(&oracle)
                .env("ALMIDE_NO_VERIFIED_OK", "1").output().unwrap();
            assert!(built.status.success(), "{name}: {}", String::from_utf8_lossy(&built.stderr));
            let v0 = Command::new(oracle).output().unwrap();
            assert_eq!((&actual.stdout, &actual.stderr, actual.status.code()),
                (&v0.stdout, &v0.stderr, v0.status.code()), "{name}: v0");
            let wasm = almide::wasm_route::route_wasm(input.to_str().unwrap(), source,
                almide::wasm_route::ModuleSource::Disk { dep_paths: &[] }, None,
                Default::default(), &mut |_| {}).unwrap_or_else(|e| panic!("{name}: wasm route {e:?}"));
            let wasm = almide_wasm_run::run_wasm(&wasm.bytes).unwrap();
            assert_eq!((&actual.stdout, &actual.stderr, actual.status.code()),
                (&wasm.stdout_bytes, &wasm.stderr_bytes, Some(wasm.exit)), "{name}: wasm");
        });
        if result.is_err() { failures.push(name); }
    }
    assert!(failures.is_empty(), "closure cases failed: {failures:?}; artifacts {}", dir.display());
    std::fs::remove_dir_all(dir).unwrap();
}
