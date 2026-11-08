//! Value representation matrix: scalar joins, lists, bytes and nested carriers. Compare exact expected bytes and all four execution routes.

use std::process::Command;

#[test]
fn value_representations_preserve_cross_target_observations() {
    let cases = [
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
    ];
    let dir =
        std::env::temp_dir().join(format!("almide-native-value-matrix-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, source) in cases {
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
            "nested_results" => b"4\nbad\n4\nbad\nouter\n",
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
        let wasm = Command::new(env!("CARGO_BIN_EXE_almide"))
            .arg("run")
            .arg(&input)
            .args(["--target", "wasm"])
            .output()
            .unwrap();
        for (leg, other) in [("v0", v0), ("wasm", wasm)] {
            assert_eq!(
                (&actual.stdout, &actual.stderr, actual.status.code()),
                (&other.stdout, &other.stderr, other.status.code()),
                "{name}: {leg}"
            );
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}
