//! Value representation matrix: scalar joins, lists, bytes and nested carriers. Compare exact expected bytes and all four execution routes.

use std::process::Command;

#[test]
fn value_representations_preserve_cross_target_observations() {
    let cases = [
        (
            "stored_scalar_closures",
            r#"
type Handler = { f: (Int) -> Int, name: String }
type TextHandler = { f: (String) -> String }
fn bracket(s: String) -> String = "[" + s + "]"
fn make(k: Int) -> Handler = { f: (n) => n + k, name: "captured" }
fn call(h: Handler, n: Int) -> Int = h.f(n)
fn main() -> Unit = {
 let a = make(3)
 let b = { ...a, name: "copy" }
 println(int.to_string(call(a, 4)))
 println(int.to_string(call(b, 5)))
 println(a.name)
 println(b.name)
 let fs = [(n: Int) => n + 1, (n: Int) => n * 2]
 for f in fs { println(int.to_string(f(4))) }
 let text = TextHandler { f: bracket }
 println(text.f("x"))
}
"#,
        ),
        (
            "runtime_tuple_results",
            r#"
fn indexed(xs: List[String]) -> List[(Int, String)] = list.enumerate(xs)
fn main() -> Unit = {
 let xs = ["a", "b"]
 for (i, x) in indexed(xs) { println(int.to_string(i) + ":" + x) }
 println(list.join(xs, ","))
 for (i, x) in list.enumerate([7, 9]) { println(int.to_string(i + x)) }
 for (i, x) in list.enumerate([[1, 2], []]) { println(int.to_string(i) + ":" + int.to_string(list.len(x))) }
 for (a, b) in list.zip(["x", "y", "z"], [4, 5]) { println(a + int.to_string(b)) }
 let halves = list.partition([1, -2, 3], (x) => x > 0)
 for x in halves.0 { println(int.to_string(x)) }
 for x in halves.1 { println(int.to_string(x)) }
 println(int.to_string(list.len(indexed([]))))
}
"#,
        ),
        (
            "codec_null_defaults",
            r#"
type Setting: Codec = { n: Int = 7 }
fn show(v: Value) -> Unit = {
 let s = Setting.decode(v) ?? Setting { n: -1 }
 println(int.to_string(s.n))
}
fn main() -> Unit = {
 show(value.object([]))
 show(value.object([("n", value.null())]))
 show(value.object([("n", value.int(9))]))
 show(value.object([("n", value.bool(false))]))
}
"#,
        ),
        (
            "value_object_pairs",
            r#"
import json
fn pairs(v: Value) -> Result[List[(String, Value)], String] = value.as_object(v)
fn main() -> Unit = {
 let v = value.object([("a", value.int(1)), ("b", value.string("text")), ("a", value.int(3))])
 for (k, x) in pairs(v) ?? [] { println(k) println(json.stringify(x)) }
 for (k, x) in pairs(v) ?? [] { println(k) println(json.stringify(x)) }
 println(int.to_string(list.len(pairs(value.object([])) ?? [])))
 println(match pairs(value.null()) { ok(_) => "wrong", err(e) => e })
 println(match pairs(value.array([])) { ok(_) => "wrong", err(e) => e })
}
"#,
        ),
        (
            "set_heap_elements",
            r#"
fn ints(s: Set[List[Int]], k: List[Int]) -> Bool = set.contains(s, k)
fn main() -> Unit = {
 let s = set.from_list([[1, 2], [3], [1, 2]])
 println(int.to_string(set.len(s)))
 println(if ints(s, [1, 2]) then "hit" else "miss")
 println(if ints(s, [2, 1]) then "hit" else "miss")
 let kept = set.filter(s, (xs) => list.len(xs) > 1)
 for xs in set.to_list(kept) { for x in xs { println(int.to_string(x)) } }
 println(int.to_string(set.fold(s, 0, (acc, xs) => acc + list.len(xs))))
 let ss = set.from_list([["a", "b"], [], ["a", "b"]])
 println(int.to_string(set.len(ss)))
 println(if set.contains(ss, []) then "empty" else "missing")
 for xs in set.to_list(ss) { println(list.join(xs, ":")) }
 let bs = set.from_list([bytes.from_list([1, 2]), bytes.from_list([1, 2])])
 println(int.to_string(set.len(bs)))
 println(if set.contains(bs, bytes.from_list([1, 2])) then "bytes" else "missing")
 let nested = set.from_list([set.from_list(["b", "a"]), set.from_list(["a", "b"])])
 println(int.to_string(set.len(nested)))
 for inner in set.to_list(nested) { println(list.join(set.to_list(inner), ":")) }
}
"#,
        ),
        (
            "compound_strings",
            r#"
fn show(o: String?, r: Result[String, String]) -> Unit = {
 println("${o}")
 println("${r}")
}
fn main() -> Unit = {
 show(some("a\\\"\n\r\t日本語"), ok("a\\\"\n\r\t日本語"))
 show(none, err(""))
 let nested: Result[String?, String] = ok(some("z"))
 println("${nested}")
}
"#,
        ),
        (
            "nested_results",
            r#"
fn show(r: Result[Result[Int, String], String]) -> String = match r {
 ok(x) => match x { ok(n) => int.to_string(n), err(e) => e }, err(e) => e,
}
fn main() -> Unit = {
 let xs: List[Result[Int, String]] = [ok(4), err("bad")]
 for x in xs { println(show(ok(x))) }
 for x in xs { println(show(ok(x))) }
 println(show(err("outer")))
 for x in list.slice(xs, 0, 2) { println(show(ok(x))) }
 let options: List[Int?] = [some(7), none]
 for x in list.slice(options, 0, 2) { println(int.to_string(x ?? -1)) }
 let texts: List[Result[String, String]] = [ok("yes"), err("error")]
 for x in list.slice(texts, 0, 2) { println(match x { ok(s) => s, err(e) => e }) }
}
"#,
        ),
        (
            "nested_strings",
            r#"
fn pick(r: Result[String?, String]) -> String = match r {
 ok(some(s)) => s, ok(none) => "none", err(e) => e,
}
fn maybe(b: Bool) -> String? = if b then some("yes") else none
fn main() -> Unit = {
 println(pick(ok(some("inner"))))
 println(pick(ok(none)))
 println(pick(err("error")))
 println(maybe(true) ?? "empty")
 println(maybe(false) ?? "empty")
}
"#,
        ),
        (
            "typed_empty",
            r#"
type Item = { name: String, count: Int }
fn strings(b: Bool) -> List[String] = if b then ["a", "b"] else []
fn records(b: Bool) -> List[Item] = if b then [] else [Item { name: "r", count: 2 }]
fn main() -> Unit = {
 for x in strings(true) { println(x) }
 println(int.to_string(list.len(strings(false))))
 println(int.to_string(list.len(records(true))))
 for x in records(false) { println(x.name) println(int.to_string(x.count)) }
}
"#,
        ),
        (
            "record_updates",
            r#"
type Item = { name: String, count: Int }
fn main() -> Unit = {
 var r = Item { name: "a", count: 1 }
 let saved = r
 for i in 0..<3 { r.count = r.count + i }
 r.name = "b"
 println(r.name) println(int.to_string(r.count))
 println(saved.name) println(int.to_string(saved.count))
}
"#,
        ),
        (
            "float_joins",
            r#"
fn choose(b: Bool, x: Float) -> Float = if b then -0.0 else math.sqrt(x)
fn accum(n: Int) -> Float = { var x = 0.0 for _ in 0..<n { x = x + 0.5 } x }
fn main() -> Unit = {
 println(if float.to_bits(choose(true, 9.0)) == (0 - 9223372036854775807 - 1) then "negative-zero" else "bad")
 println(int.to_string(int.from(choose(false, 9.0))))
 println(int.to_string(int.from(accum(3))))
}
"#,
        ),
        (
            "scalar_filter",
            r#"
fn show(xs: List[Int]) -> Unit = for x in xs { println(int.to_string(x)) }
fn main() -> Unit = {
 let xs = [1, 2, 3]
 let keep = list.filter(xs, (x) => x % 2 == 1)
 show(keep)
 show(xs)
}
"#,
        ),
        (
            "string_filter",
            r#"
fn show(xs: List[String]) -> Unit = for x in xs { println(x) }
fn main() -> Unit = {
 let xs = ["a", "", "bb"]
 let keep = list.filter(xs, (x) => string.len(x) > 0)
 show(keep)
 show(xs)
}
"#,
        ),
        (
            "byte_words",
            r#"
fn size(b: Bytes) -> Int = bytes.len(b)
fn main() -> Unit = {
 var b = bytes.zeros(8)
 bytes.put_u32_le(b, 0, 0x11223344)
 bytes.put_u16_be(b, 4, 0x5566)
 println(int.to_string(size(b)))
 println(int.to_string(bytes.get_u32_le(b, 0) ?? -1))
 println(int.to_string(bytes.get_u16_be(b, 4) ?? -1))
 println(int.to_string(bytes.get_u32_le(b, 8) ?? -1))
}
"#,
        ),
        (
            "nested_aliases",
            r#"
fn wrap(x: Int?) -> Result[Int?, String] = ok(x)
fn unwrap(x: Result[Int?, String]) -> Int = match x { ok(y) => y ?? -1, err(e) => -2 }
fn main() -> Unit = {
 let xs: List[Int?] = [some(7), none]
 for x in xs { println(int.to_string(unwrap(wrap(x)))) }
 for x in xs { println(int.to_string(unwrap(wrap(x)))) }
 println(int.to_string(unwrap(err("unused error payload"))))
}
"#,
        ),
        (
            "result_callbacks",
            r#"
fn show(r: Result[List[Int], String]) -> Unit = match r {
 ok(xs) => for x in xs { println(int.to_string(x)) }, err(e) => println(e),
}
fn main() -> Unit = {
 let source: Result[List[Int], String] = ok([2, 3])
 show(result.map(source, (xs) => xs + [4]))
 show(source)
 let bad: Result[List[Int], String] = err("bad")
 show(result.map_err(bad, (e) => e + "!"))
 show(bad)
 let restored = result.unwrap_or_else(bad, (e) => [string.len(e)])
 for n in restored { println(int.to_string(n)) }
}
"#,
        ),
        (
            "computed_container_args",
            r#"
fn wrap(xs: List[String]) -> Result[List[String], String] = ok(xs)
fn read(r: Result[List[String], String]) -> Unit = match r {
 ok(xs) => for x in xs { println(x) }, err(e) => println(e),
}
fn main() -> Unit = {
 read(wrap(["a", "b"] + ["c"]))
 read(wrap([]))
 read(err("end"))
}
"#,
        ),
        (
            "option_callbacks",
            r#"
fn show(o: List[Int]?) -> Unit = match o {
 some(xs) => for x in xs { println(int.to_string(x)) }, none => println("none"),
}
fn main() -> Unit = {
 let source: List[Int]? = some([2, 3])
 show(option.filter(source, (xs) => list.len(xs) == 2))
 show(option.filter(source, (xs) => list.len(xs) == 0))
 show(source)
 let absent: List[Int]? = none
 show(option.or_else(absent, () => some([8])))
 let restored = option.unwrap_or_else(absent, () => [9])
 for x in restored { println(int.to_string(x)) }
 show(option.flatten(some(source)))
 let inner_empty: (List[Int]?)? = some(none)
 let outer_empty: (List[Int]?)? = none
 show(option.flatten(inner_empty))
 show(option.flatten(outer_empty))
}
"#,
        ),
        (
            "value_equality",
            r#"
fn report(a: Value, b: Value) -> Unit = println(if a == b then "equal" else "different")
fn main() -> Unit = {
 let leaves = [value.null(), value.bool(true), value.int(7), value.float(1.5), value.string("text")]
 let tree = value.object([("items", value.array(leaves))])
 report(tree, value.object([("items", value.array(leaves))]))
 report(tree, value.object([("items", value.array([]))]))
 report(value.object([("a", value.int(1)), ("b", value.int(2))]),
        value.object([("b", value.int(2)), ("a", value.int(1))]))
 report(value.int(1), value.float(1.0))
 report(value.float(-0.0), value.float(0.0))
 report(value.string("あ"), value.string("あ"))
 report(tree, tree)
 for item in leaves { report(item, item) }
}
"#,
        ),
        (
            "heap_accumulator_folds",
            r#"
fn main() -> Unit = {
 let words = ["a", "b"]
 println(list.fold(words, "seed", (a, x) => a + x))
 let numbers = [2, 3]
 let initial = [1]
 let collected = list.fold(numbers, initial, (a, x) => a + [x])
 for n in collected { println(int.to_string(n)) }
 for n in initial { println(int.to_string(n)) }
 let optional: List[String]? = some([])
 let joined = list.fold(words, optional, (a, x) => some((a ?? []) + [x]))
 for word in (joined ?? []) { println(word) }
 let values = [value.int(2), value.int(3)]
 let last = list.fold(values, value.null(), (a, x) => x)
 println(int.to_string(value.as_int(last) ?? -1))
 let initial_map: Map[String, Int] = map.new()
 let counts = list.fold(words, initial_map, (a, x) => map.set(a, x, string.len(x)))
 println(int.to_string(map.get(counts, "a") ?? -1))
 println(int.to_string(map.len(initial_map)))
 for word in words { println(word) }
}
"#,
        ),
        (
            "set_int_callbacks",
            r#"
fn show(s: Set[Int]) -> Unit = for n in set.to_list(s) { println(int.to_string(n)) }
fn main() -> Unit = {
 let source = set.from_list([3, 1, 3, 2])
 let minimum = 2
 show(set.filter(source, (n) => n >= minimum))
 show(set.map(source, (n) => n % 2))
 println(int.to_string(set.fold(source, 0, (a, n) => a * 10 + n)))
 println(if set.any(source, (n) => n == 1) then "any" else "none")
 println(if set.all(source, (n) => n > 0) then "all" else "some")
 let empty: Set[Int] = set.new()
 println(if set.any(empty, (n) => n > 0) then "bad" else "empty")
 println(if set.all(empty, (n) => n > 0) then "all" else "bad")
 show(source)
}
"#,
        ),
        (
            "set_string_callbacks",
            r#"
fn show(s: Set[String]) -> Unit = for text in set.to_list(s) { println(text) }
fn main() -> Unit = {
 let source = set.from_list(["b", "aa", "b", ""])
 show(set.filter(source, (text) => string.len(text) > 0))
 println(int.to_string(set.fold(source, 0, (n, text) => n + string.len(text))))
 println(if set.any(source, (text) => text == "") then "empty" else "none")
 println(if set.all(source, (text) => string.len(text) > 0) then "all" else "some")
 show(source)
}
"#,
        ),
    ];
    let dir =
        std::env::temp_dir().join(format!("almide-native-value-matrix-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut failures = Vec::new();
    for (name, source) in cases {
        // Complete the matrix even when one case fails, so a batch exposes
        // every independent representation gap in one compiler build.
        let result = std::panic::catch_unwind(|| {
            let input = dir.join(format!("{name}.almd"));
            let rs = dir.join(format!("{name}.rs"));
            let binary = dir.join(name);
            std::fs::write(&input, source).unwrap();
            let (rust, certs) = almide_mir::pipeline::try_render_rust_source_certified(source)
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            for prop in ["ownership", "native-calls", "bounds"] {
                assert!(
                    certs.iter().any(|(p, _, _)| p == prop),
                    "{name}: missing {prop}"
                );
            }
            if matches!(name, "set_int_callbacks" | "set_string_callbacks") {
                assert!(
                    rust.contains("almide_rt_set_filter_static"),
                    "{name}: runtime boundary"
                );
                let owners: Vec<_> = certs.iter().filter(|(p, _, _)| p == "ownership").collect();
                assert!(
                    !owners.iter().any(|(_, f, _)| f.starts_with("set_filter")
                        || f.starts_with("__set_str_filter")),
                    "{name}: unreachable helper certified"
                );
                let calls = &certs
                    .iter()
                    .find(|(p, _, _)| p == "native-calls")
                    .unwrap()
                    .2;
                assert!(
                    calls.split('#').count() > owners.len(),
                    "{name}: missing runtime call witness"
                );
            }
            std::fs::write(&rs, rust).unwrap();
            let compiled = Command::new("rustc")
                .args(["--edition=2021", "-C", "opt-level=1"])
                .arg(&rs)
                .arg("-o")
                .arg(&binary)
                .output()
                .unwrap();
            assert!(
                compiled.status.success(),
                "{name}: {}",
                String::from_utf8_lossy(&compiled.stderr)
            );
            let actual = Command::new(&binary).output().unwrap();
            assert!(
                actual.status.success(),
                "{name}: {}",
                String::from_utf8_lossy(&actual.stderr)
            );
            assert!(actual.stderr.is_empty(), "{name}");
            let expected: &[u8] = match name {
            "stored_scalar_closures" => b"7\n8\ncaptured\ncopy\n5\n8\n[x]\n",
            "compound_strings" => "some(\"a\\\\\\\"\\n\\r\\t日本語\")\nok(\"a\\\\\\\"\\n\\r\\t日本語\")\nnone\nerr(\"\")\nok(some(\"z\"))\n".as_bytes(),
            "set_heap_elements" => b"2\nhit\nmiss\n1\n2\n3\n2\nempty\na:b\n\n1\nbytes\n1\nb:a\n",
            "value_object_pairs" => b"a\n3\nb\n\"text\"\na\n3\nb\n\"text\"\n0\nexpected Object, received Null\nexpected Object, received Array\n",
            "codec_null_defaults" => b"7\n7\n9\n-1\n",
            "runtime_tuple_results" => b"0:a\n1:b\na,b\n7\n10\n0:2\n1:0\nx4\ny5\n1\n3\n-2\n0\n",
            "nested_results" => b"4\nbad\n4\nbad\nouter\n4\nbad\n7\n-1\nyes\nerror\n",
            "nested_strings" => b"inner\nnone\nerror\nyes\nempty\n",
            "typed_empty" => b"a\nb\n0\n0\nr\n2\n",
            "record_updates" => b"b\n4\na\n1\n",
            "float_joins" => b"negative-zero\n3\n1\n",
            "scalar_filter" => b"1\n3\n1\n2\n3\n",
            "string_filter" => b"a\nbb\na\n\nbb\n",
            "byte_words" => b"8\n287454020\n21862\n-1\n",
            "nested_aliases" => b"7\n-1\n7\n-1\n-2\n",
            "result_callbacks" => b"2\n3\n4\n2\n3\nbad!\nbad\n3\n",
            "computed_container_args" => b"a\nb\nc\nend\n",
            "option_callbacks" => b"2\n3\nnone\n2\n3\n8\n9\n2\n3\nnone\nnone\n",
            "value_equality" => b"equal\ndifferent\ndifferent\ndifferent\nequal\nequal\nequal\nequal\nequal\nequal\nequal\nequal\n",
            "heap_accumulator_folds" => b"seedab\n1\n2\n3\n1\na\nb\n3\n1\n0\na\nb\n",
            "set_int_callbacks" => b"3\n2\n1\n0\n312\nany\nall\nempty\nall\n3\n1\n2\n",
            "set_string_callbacks" => b"b\naa\n3\nempty\nsome\nb\naa\n\n",
            _ => unreachable!(),
        };
            assert_eq!(actual.stdout, expected, "{name}");
            let ir = almide::wasm_leg::lower_to_ir(input.to_str().unwrap(), source).unwrap();
            let reference = almide_interp::Interpreter::new(&ir).run_main();
            assert!(
                matches!(reference.status, almide_interp::RunStatus::Ok),
                "{name}: {:?}",
                reference.status
            );
            assert_eq!(
                (
                    actual.stdout.as_slice(),
                    actual.stderr.as_slice(),
                    actual.status.code()
                ),
                (
                    reference.stdout.as_bytes(),
                    reference.stderr.as_bytes(),
                    Some(reference.exit_code())
                ),
                "{name}: interp"
            );
            let oracle = dir.join(format!("{name}-v0"));
            let built = Command::new(env!("CARGO_BIN_EXE_almide"))
                .args(["build", "--no-verified"])
                .arg(&input)
                .arg("-o")
                .arg(&oracle)
                .env("ALMIDE_NO_VERIFIED_OK", "1")
                .output()
                .unwrap();
            assert!(
                built.status.success(),
                "{name}: {}",
                String::from_utf8_lossy(&built.stderr)
            );
            let v0 = Command::new(oracle).output().unwrap();
            // Execute the routed module directly so compiler warnings are not
            // mistaken for the program's stderr (Result interpolation warns).
            let wasm = almide::wasm_route::route_wasm(
                input.to_str().unwrap(),
                source,
                almide::wasm_route::ModuleSource::Disk { dep_paths: &[] },
                None,
                Default::default(),
                &mut |_| {},
            )
            .unwrap_or_else(|e| panic!("{name}: wasm route {e:?}"));
            let wasm = almide_wasm_run::run_wasm(&wasm.bytes).unwrap();
            assert_eq!(
                (&actual.stdout, &actual.stderr, actual.status.code()),
                (&wasm.stdout_bytes, &wasm.stderr_bytes, Some(wasm.exit)),
                "{name}: wasm"
            );
            for (leg, other) in [("v0", v0)] {
                assert_eq!(
                    (&actual.stdout, &actual.stderr, actual.status.code()),
                    (&other.stdout, &other.stderr, other.status.code()),
                    "{name}: {leg}"
                );
            }
        });
        if result.is_err() {
            failures.push(name);
        }
    }
    assert!(
        failures.is_empty(),
        "failed value cases: {failures:?}; artifacts: {}",
        dir.display()
    );
    std::fs::remove_dir_all(dir).unwrap();
}
