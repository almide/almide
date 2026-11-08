//! Declared runtime routes must preserve callback ordering and alias reuse.
//! Execute certified v1 Rust directly, then compare all reference routes.

use std::process::Command;

#[test]
fn runtime_routes_preserve_cross_target_observations() {
    let cases = [
        (
            "record_edges",
            r#"
type Item = { label: String, n: Int }
fn size(xs: List[Item]) -> Unit = println(int.to_string(list.len(xs)))
fn main() -> Unit = {
  let xs = [Item { label: "a", n: 1 }, Item { label: "b", n: 2 }]
  let absent = list.filter(xs, (x) => x.n < 0)
  let all = list.filter(xs, (x) => x.n >= 0)
  let empty: List[Item] = []
  let still_empty = list.filter(empty, (x) => x.n >= 0)
  size(absent)
  size(all)
  size(still_empty)
  size(xs)
}
"#,
        ),
        (
            "variant_predicates",
            r#"
type Item = | Named(String, Int) | Empty
fn main() -> Unit = {
  let xs = [Named("a", 1), Empty, Named("b", 2)]
  let any = list.any(xs, (x) => match x { Named(s, n) => n > 1, Empty => false })
  let all = list.all(xs, (x) => match x { Named(s, n) => n > 1, Empty => false })
  println(if any then "any" else "none")
  println(if all then "all" else "some")
}
"#,
        ),
        (
            "variant_filter",
            r#"
type Item = | Named(String, Int) | Empty
fn keep(s: String, n: Int) -> Bool = { println(s) n >= 2 }
fn label(x: Item) -> String = match x { Named(s, n) => s, Empty => "empty" }
fn show(xs: List[Item]) -> Unit = for x in xs { println(label(x)) }
fn main() -> Unit = {
  let xs = [Named("a", 1), Named("b", 2), Empty, Named("c", 3)]
  let kept = list.filter(xs, (x) => match x { Named(s, n) => keep(s, n), Empty => false })
  show(kept)
  show(xs)
  show(kept)
}
"#,
        ),
        (
            "record_filter",
            r#"
type Item = { label: String, n: Int }
fn keep(x: Item, minimum: Int) -> Bool = { println(x.label) x.n >= minimum }
fn show(xs: List[Item]) -> Unit = for x in xs { println(x.label) }
fn main() -> Unit = {
  let xs = [Item { label: "a", n: 1 }, Item { label: "b", n: 2 }, Item { label: "c", n: 3 }]
  let minimum = 2
  let kept = list.filter(xs, (x) => keep(x, minimum))
  show(kept)
  show(xs)
  show(kept)
}
"#,
        ),
        (
            "nested_list_filter",
            r#"
fn show(xs: List[List[Int]]) -> Unit = for x in xs { println(int.to_string(list.len(x))) }
fn main() -> Unit = {
  let xs: List[List[Int]] = [[1], [2, 3], []]
  let kept = list.filter(xs, (x) => list.len(x) > 1)
  show(kept)
  show(xs)
  show(kept)
}
"#,
        ),
    ];
    let dir = std::env::temp_dir().join(format!(
        "almide-native-runtime-routes-{}",
        std::process::id()
    ));
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
        if name == "variant_filter" {
            assert!(
                rust.contains("almide_rt_list_filter_static"),
                "{name}: missing runtime call"
            );
            let functions = certs.iter().filter(|(p, _, _)| p == "ownership").count();
            let calls = &certs
                .iter()
                .find(|(p, _, _)| p == "native-calls")
                .unwrap()
                .2;
            assert!(
                calls.split('#').count() > functions,
                "{name}: missing runtime witness block"
            );
        } else if name == "variant_predicates" {
            assert!(rust.contains("almide_rt_list_any_static"));
            assert!(rust.contains("almide_rt_list_all_static"));
        } else {
            assert!(
                rust.contains(".truncate("),
                "{name}: missing list commitment"
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
            "record_edges" => b"0\n2\n0\n2\n",
            "variant_predicates" => b"any\nsome\n",
            "variant_filter" => b"a\nb\nc\nb\nc\na\nb\nempty\nc\nb\nc\n",
            "record_filter" => b"a\nb\nc\nb\nc\na\nb\nc\nb\nc\n",
            "nested_list_filter" => b"2\n1\n2\n0\n2\n",
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
