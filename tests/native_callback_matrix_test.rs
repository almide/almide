//! Callback ABI regressions: exact bytes on certified native, v0, wasm and IR.
use std::process::Command;

#[test]
fn callback_values_preserve_all_execution_routes() {
    let cases = [
        ("map_compound_entries", r#"
fn main() -> Unit = {
 let pairs = [("b", [1, 2]), ("a", []), ("b", [7])]
 let m = map.from_list(pairs)
 println("${m}")
 println("${pairs}")
 let nested = map.from_list([("left", ["x": "y"]), ("right", [:])])
 println("${nested}")
 let optional = map.from_list([("x", [some(1), none]), ("y", [])])
 println("${optional}")
 let ss = set.from_list(["b", "a", "b"])
 println(set.fold(ss, "seed", (acc, s) => acc + s))
}
"#, "[\"b\": [7], \"a\": []]\n[(\"b\", [1, 2]), (\"a\", []), (\"b\", [7])]\n[\"left\": [\"x\": \"y\"], \"right\": [:]]\n[\"x\": [some(1), none], \"y\": []]\nseedba\n"),
        ("map_forwarded_updates", r#"
fn main() -> Unit = {
 let m = ["b": 1, "a": 2]
 println("${map.update(m, "a", (v) => v + 3)}")
 println("${map.update(m, "missing", (v) => 1 / (v - v))}")
 println("${map.upsert(m, "z", 7, (v) => 1 / (v - v))}")
 println("${map.upsert(m, "b", 9, (v) => v + 4)}")
 println("${map.map(m, (v) => v + 10)}")
 println("${m}")
 let short = ["a": 1, "b": 0]
 println(bool.to_string(map.any(short, ((k, v)) => if k == "a" then true else 1 / v > 0)))
 println(bool.to_string(map.all(short, ((k, v)) => if k == "a" then false else 1 / v > 0)))
}
"#, "[\"b\": 1, \"a\": 5]\n[\"b\": 1, \"a\": 2]\n[\"b\": 1, \"a\": 2, \"z\": 7]\n[\"b\": 5, \"a\": 2]\n[\"b\": 11, \"a\": 12]\n[\"b\": 1, \"a\": 2]\ntrue\nfalse\n"),
        ("map_scalar_family", r#"
fn main() -> Unit = {
 let m = [3: 30, 1: 10, 2: 20]
 let alias = m
 let limit = 15
 println("${map.find(m, ((k, v)) => k < 3 and v > limit)}")
 println("${map.find(m, ((k, v)) => v < 0)}")
 println("${map.filter(m, ((k, v)) => k != 1)}")
 println(int.to_string(map.fold(m, 5, (acc, (k, v)) => acc + k + v)))
 println(bool.to_string(map.any(m, ((k, v)) => v > 20)))
 println(bool.to_string(map.all(m, ((k, v)) => v >= 10)))
 println(int.to_string(map.count(m, ((k, v)) => v >= 20)))
 println("${alias}")
}
"#, "some((2, 20))\nnone\n[3: 30, 2: 20]\n71\ntrue\ntrue\n2\n[3: 30, 1: 10, 2: 20]\n"),
        ("map_empty_and_short_circuit", r#"
fn main() -> Unit = {
 let empty: Map[String, Int] = map.new()
 println("${map.filter(empty, ((k, v)) => v > 0)}")
 println(int.to_string(map.fold(empty, 7, (acc, (k, v)) => acc / (v - v))))
 println(bool.to_string(map.any(empty, ((k, v)) => v > 0)))
 println(bool.to_string(map.all(empty, ((k, v)) => v > 0)))
 println(int.to_string(map.count(empty, ((k, v)) => v > 0)))
 println("${map.find(empty, ((k, v)) => v > 0)}")
 let m = [1: 1, 2: 0]
 println(bool.to_string(map.any(m, ((k, v)) => if k == 1 then true else 1 / v > 0)))
 println(bool.to_string(map.all(m, ((k, v)) => if k == 1 then false else 1 / v > 0)))
 println("${map.find(m, ((k, v)) => if k == 1 then true else 1 / v > 0)}")
}
"#, "[:]\n7\nfalse\ntrue\n0\nnone\ntrue\nfalse\nsome((1, 1))\n"),
        ("map_heap_values", r#"
fn main() -> Unit = {
 let m = ["b": [2, 3], "a": [1]]
 let alias = m
 let seed = [9]
 println("${map.fold(m, seed, (acc, (k, vs)) => acc + vs)}")
 println("${map.filter(m, ((k, vs)) => list.len(vs) > 1)}")
 println("${map.find(m, ((k, vs)) => k == "a")}")
 println("${seed}")
 println("${alias}")
}
"#, "[9, 2, 3, 1]\n[\"b\": [2, 3]]\nsome((\"a\", [1]))\n[9]\n[\"b\": [2, 3], \"a\": [1]]\n"),
        ("map_float_bits", r#"
fn main() -> Unit = {
 let m = ["x": 2.5, "y": -0.0, "z": 1.0]
 assert_eq(float.to_bits(map.fold(m, 1.0, (acc, (k, v)) => -0.0)), float.to_bits(-0.0))
 println("${map.find(m, ((k, v)) => float.to_bits(v) == float.to_bits(-0.0))}")
 println(float.to_string(map.fold(m, 0.0, (acc, (k, v)) => acc + v)))
 println("${map.filter(m, ((k, v)) => v > 0.0)}")
 println("${m}")
}
"#, "some((\"y\", -0))\n3.5\n[\"x\": 2.5, \"z\": 1]\n[\"x\": 2.5, \"y\": -0, \"z\": 1]\n"),
        ("map_tuple_accumulator", r#"
fn main() -> Unit = {
 let m = ["b": 2, "c": 3, "a": 1]
 let seed = (0, "")
 let result = map.fold(m, seed, ((n, text), (k, v)) => (n + v, text + k))
 println("${result}")
 println("${seed}")
 println("${m}")
}
"#, "(6, \"bca\")\n(0, \"\")\n[\"b\": 2, \"c\": 3, \"a\": 1]\n"),
    ];
    let dir = std::env::temp_dir().join(format!("almide-native-callbacks-{}", std::process::id()));
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
    assert!(failures.is_empty(), "callback cases failed: {failures:?}; artifacts {}", dir.display());
    std::fs::remove_dir_all(dir).unwrap();
}
