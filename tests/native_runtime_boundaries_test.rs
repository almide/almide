//! Runtime boundaries preserve structural equality and thread-local cache behavior.
use std::process::Command;

#[test]
fn runtime_boundaries_preserve_all_execution_routes() {
    let cases = [
        ("borrowed_list_repeats", r#"
fn main() -> Unit = {
 let m: Map[String, Int?] = ["a": some(7)]
 let found = list.get([m, m, m], 2) ?? [:]
 println("${map.len(found)}:${map.len(m)}")
 let r: Result[Int?, String] = err("kept")
 let hit = list.get([r, r], 1) ?? ok(none)
 let show = (x: Result[Int?, String]) => match x { ok(n) => "ok", err(e) => e }
 println(show(hit))
 println(show(r))
 let s = "text"
 println(list.get([s, s], 0) ?? "missing")
 println(s)
}
"#, "1:1\nkept\nkept\ntext\ntext\n"),
        ("empty_concat", r#"
type Row = { xs: List[String] }
fn strings(xs: List[String]) -> List[String] = [] + xs + []
fn lists(xs: List[List[Int]]) -> List[List[Int]] = [] + xs + []
fn main() -> Unit = {
 let xs = ["a", "日"]
 let ss = strings(xs)
 println("${ss}")
 println("${xs}")
 let ls = lists([[1], [], [2, 3]])
 println("${ls}")
 let rows = [Row { xs: ["a", "b"] }, Row { xs: ["c"] }]
 let flattened = list.flat_map(rows, (r) => r.xs)
 println("${flattened}")
 for r in rows { println("${r.xs}") }
}
"#, "[\"a\", \"日\"]\n[\"a\", \"日\"]\n[[1], [], [2, 3]]\n[\"a\", \"b\", \"c\"]\n[\"a\", \"b\"]\n[\"c\"]\n"),
        ("bytes_equality", r#"
fn main() -> Unit = {
 let a = bytes.from_list([0, 127, 255])
 let b = bytes.from_list([0, 127, 255])
 println("${a == b}")
 println("${a == bytes.from_list([0, 127])}")
 println("${a == bytes.from_list([0, 127, 254])}
${bytes.from_list([]) == bytes.from_list([])}")
 println("${bytes.to_list(a)}")
 println("${bytes.to_list(b)}")
}
"#, "true\nfalse\nfalse\ntrue\n[0, 127, 255]\n[0, 127, 255]\n"),
        ("nested_list_equality", r#"
fn main() -> Unit = {
 let a = [[1, 2], [], [3]]
 println("${a == [[1, 2], [], [3]]}")
 println("${a == [[1, 2], [3], []]}")
 println("${a == [[1, 2]]}")
 println("${[["a", "日"], []] == [["a", "日"], []]}")
 println("${[["a"]] == [["b"]]}")
 let zeros = [[-0.0]]
 println("${zeros == [[0.0]]}")
 let nan = 0.0 / 0.0
 println("${[[nan]] == [[nan]]}")
 println("${a == [[1, 2], [], [3]]}")
}
"#, "true\nfalse\nfalse\ntrue\nfalse\ntrue\nfalse\ntrue\n"),
        ("tagged_list_equality", r#"
fn main() -> Unit = {
 let ints: List[Int?] = [none, some(0), some(3)]
 println("${ints == [none, some(0), some(3)]}")
 println("${ints == [some(0), none, some(3)]}")
 println("${ints == [none]}")
 let strings: List[String?] = [none, some(""), some("日")]
 println("${strings == [none, some(""), some("日")]}")
 println("${strings == [some(""), none, some("日")]}")
 let results: List[Result[Int, String]] = [ok(0), err(""), err("bad")]
 println("${results == [ok(0), err(""), err("bad")]}")
 println("${results == [err(""), ok(0), err("bad")]}")
 println("${results == [ok(0), err(""), err("other")]}")
 println("${ints == [none, some(0), some(3)]}")
}
"#, "true\nfalse\nfalse\ntrue\nfalse\ntrue\nfalse\nfalse\ntrue\n"),
        ("regex_cache", r#"
import regex
fn reason(p: String) -> String = match regex.compile(p) { ok(_) => "OK", err(e) => e }
fn show(o: List[String?]?) -> String = match o {
 some(xs) => "[" + list.join(list.map(xs, (x) => x ?? "MISSING"), "|") + "]",
 none => "NONE",
}
fn main() -> Unit = {
 for _ in 0..<3 {
  println(show(regex.captures(regex.new("(x)?(日)"), "日")))
  println(regex.find(regex.new(".."), "日本語") ?? "NONE")
 }
 println(show(regex.captures(regex.new("(a*)*"), "")))
 println(show(regex.captures(regex.new("z+"), "日")))
 println(reason("["))
}
"#, "[日|MISSING|日]\n日本\n[日|MISSING|日]\n日本\n[日|MISSING|日]\n日本\n[|]\nNONE\nunclosed character class\n"),
    ];
    let dir = std::env::temp_dir().join(format!("almide-native-runtime-boundaries-{}", std::process::id()));
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
    assert!(failures.is_empty(), "runtime boundary cases failed: {failures:?}; artifacts {}", dir.display());
    std::fs::remove_dir_all(dir).unwrap();
}
