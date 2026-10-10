//! Native string hot paths copy only what they keep.
//!
//! A line-parsing loop (`station;-12.3` → `split_once` → `strip_prefix` →
//! `split_once` → `map.upsert`) made six to seven heap allocations per line
//! on the native leg where the handwritten Rust makes one: every
//! `split_once` / `strip_prefix` half was an owned `String`, the upsert
//! closure was boxed into a fresh `Rc` per call, and the key was copied on
//! every call though it is inserted once. These emit-shape tests pin the
//! three rewrites (SliceBinders, the `_fn` twin routing in RustLowering,
//! StrMapKey) and the place each one must decline; `the_parse_loop_prints_
//! the_same_on_both_legs` runs the shape on native and wasm.
//!
//! Skips cleanly when the `almide` binary is unavailable.

use std::path::Path;
use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

fn tool_available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

fn with_program<T>(source: &str, tag: &str, f: impl FnOnce(&Path) -> T) -> T {
    let dir = std::env::temp_dir().join(format!("almide-str-slices-{}-{}", tag, std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("prog.almd");
    std::fs::write(&src, source).unwrap();
    let out = f(&src);
    std::fs::remove_dir_all(&dir).ok();
    out
}

fn emitted(source: &str, tag: &str) -> String {
    with_program(source, tag, |src| {
        let output = Command::new(almide_bin())
            .args([src.to_str().unwrap(), "--target", "rust"])
            .output()
            .expect("failed to spawn almide");
        assert!(output.status.success(), "--target rust emit failed:\n{}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8_lossy(&output.stdout).to_string()
    })
}

/// The body of `pub fn <name>` in the emitted Rust, up to the next blank line.
fn fn_body(rust: &str, name: &str) -> String {
    let head = format!("pub fn {name}(");
    let start = rust.find(&head).unwrap_or_else(|| panic!("no `{head}` in the emitted Rust"));
    let rest = &rust[start..];
    let end = rest.find("\n\n").unwrap_or(rest.len());
    rest[..end].to_string()
}

const PARSE: &str = r#"
type Stats = { min: Int, max: Int, count: Int }

fn parse_int(s: String) -> Int = match int.parse(s) { ok(v) => v, err(_) => 0 }

fn parse_tenths(s: String) -> Int =
  match string.strip_prefix(s, "-") {
    some(rest) => 0 - parse_tenths(rest),
    none => match string.split_once(s, ".") {
      some((whole, frac)) => parse_int(whole) * 10 + parse_int(frac),
      none => parse_int(s) * 10,
    },
  }

fn step(acc: Map[String, Stats], line: String) -> Map[String, Stats] =
  match string.split_once(line, ";") {
    some((station, temp)) => {
      let t = parse_tenths(temp)
      map.upsert(acc, station, Stats { min: t, max: t, count: 1 }, (s) => Stats {
        min: if t < s.min then t else s.min,
        max: if t > s.max then t else s.max,
        count: s.count + 1,
      })
    },
    none => acc,
  }

fn lens(acc: Map[String, Int], line: String) -> Map[String, Int] =
  match string.split_once(line, ";") {
    some((k, v)) => map.set(acc, k, string.len(v)),
    none => acc,
  }

fn tagged(line: String) -> String =
  match string.split_once(line, ";") {
    some((k, v)) => if k == "x" then v else k,
    none => line,
  }

fn suffix_len(s: String) -> Int =
  match string.strip_suffix(s, ".txt") {
    some(stem) => string.len(stem),
    none => 0 - 1,
  }

effect fn main() -> Unit = {
  let lines = ["Oslo;-3.5", "Lima;12.0", "Oslo;7.25", "Rome;0.5", "Lima;-0.1", "bad"]
  let stats = lines |> list.fold(map.new(), (acc, l) => step(acc, l))
  let lengths = lines |> list.fold(map.new(), (acc, l) => lens(acc, l))
  for name in ["Lima", "Oslo", "Rome"] {
    let s = map.get(stats, name) ?? Stats { min: 0, max: 0, count: 0 }
    println("${name} ${s.min} ${s.max} ${s.count} ${map.get_or(lengths, name, 0)}")
  }
  println("${tagged("x;y")} ${tagged("a;b")} ${tagged("plain")} ${suffix_len("notes.txt")} ${suffix_len("notes")}")
}
"#;

const EXPECTED: &str = "Lima -1 120 2 4\nOslo -35 95 2 4\nRome 5 5 1 3\ny a plain 5 -1\n";

#[test]
fn read_only_substring_binders_are_slices_of_the_subject() {
    if !tool_available() { eprintln!("skipping: almide binary not available"); return; }
    let rust = emitted(PARSE, "slices");
    let parse = fn_body(&rust, "parse_tenths");
    assert!(parse.contains("almide_rt_string_strip_prefix_ref("), "strip_prefix binder only read → slice twin:\n{parse}");
    assert!(parse.contains("almide_rt_string_split_once_ref("), "split_once binders only read → slice twin:\n{parse}");
    assert!(!parse.contains(".to_string()"), "no binder is consumed, so none is copied:\n{parse}");
    let suffix = fn_body(&rust, "suffix_len");
    assert!(suffix.contains("almide_rt_string_strip_suffix_ref("), "strip_suffix binder only read → slice twin:\n{suffix}");
}

#[test]
fn a_consumed_binder_is_copied_where_it_is_consumed() {
    if !tool_available() { eprintln!("skipping: almide binary not available"); return; }
    let rust = emitted(PARSE, "consumed");
    let step = fn_body(&rust, "step");
    assert!(step.contains("almide_rt_string_split_once_ref("), "a binder handed to a consumer still lets the match slice:\n{step}");
    let lens = fn_body(&rust, "lens");
    assert!(lens.contains("almide_rt_string_split_once_ref("), "a key consumed by map.set slices too:\n{lens}");
}

#[test]
fn a_binder_used_any_other_way_keeps_the_owned_twin() {
    if !tool_available() { eprintln!("skipping: almide binary not available"); return; }
    let rust = emitted(PARSE, "declines");
    let tagged = fn_body(&rust, "tagged");
    // `k == "x"` compares the binder: the `&str` spelling is not proved for
    // a comparison, so the match keeps the owned twin.
    assert!(tagged.contains("almide_rt_string_split_once("), "a compared binder keeps the owned twin:\n{tagged}");
    assert!(!tagged.contains("split_once_ref"), "a compared binder keeps the owned twin:\n{tagged}");
}

#[test]
fn upsert_with_a_closure_literal_takes_it_unboxed_and_the_key_borrowed() {
    if !tool_available() { eprintln!("skipping: almide binary not available"); return; }
    let rust = emitted(PARSE, "upsert");
    let step = fn_body(&rust, "step");
    assert!(step.contains("almide_rt_map_upsert_str_fn(acc, &*station,"), "upsert: static closure + borrowed key:\n{step}");
    assert!(!step.contains("Rc::new("), "the upsert closure is not boxed per call:\n{step}");
    let lens = fn_body(&rust, "lens");
    assert!(lens.contains("almide_rt_map_set_str(acc, &*k,"), "map.set with a borrowed String key copies it on insert only:\n{lens}");
    assert!(!lens.contains("k.to_string()"), "the key is not copied before the lookup:\n{lens}");
}

#[test]
fn an_indexed_read_is_copied_once() {
    if !tool_available() { eprintln!("skipping: almide binary not available"); return; }
    let src = r#"
effect fn main() -> Unit = {
  let names = ["a", "bb", "ccc"]
  var out: List[String] = []
  for i in 0..<6 {
    let name = names[i % 3]
    out = out + [name + "!"]
  }
  println(string.join(out, ","))
}
"#;
    let rust = emitted(src, "index");
    let main = fn_body(&rust, "__almide_main");
    assert!(main.contains("almide_index!("), "the read renders through the index macro:\n{main}");
    assert!(!main.contains(")).clone()") && !main.contains("i64)).clone()"), "`almide_index!` already copies the element; no second clone:\n{main}");
}

const INDEX_READS: &str = r#"
effect fn main() -> Unit = {
  let names = ["a", "bb", "ccc"]
  for i in 0..<5 {
    let name = names[i % 3]
    let next = names[(i + 1) % 3]
    println("${name}>${next}")
  }
}
"#;

#[test]
fn a_let_of_an_arithmetic_index_only_printed_borrows_into_the_list() {
    if !tool_available() { eprintln!("skipping: almide binary not available"); return; }
    let rust = emitted(INDEX_READS, "index_ref");
    let main = fn_body(&rust, "__almide_main");
    assert!(main.contains("let name: _ = almide_index_ref!(names, almide_mod!(i, 3i64));"), "`names[i % 3]` read only by an interpolation borrows:\n{main}");
    assert!(main.contains("almide_index_ref!(names, almide_mod!((i).wrapping_add(1i64), 3i64))"), "a compound arithmetic index borrows too:\n{main}");
    assert!(!main.contains("almide_index!(names"), "no element is copied:\n{main}");
    with_program(INDEX_READS, "index_ref_run", |src| {
        let expected = "a>bb\nbb>ccc\nccc>a\na>bb\nbb>ccc\n";
        assert_eq!(run(src, false), expected, "native");
        assert_eq!(run(src, true), expected, "wasm");
    });
}

/// Run on wasm, or natively through the codegen these rewrites live in: the
/// default native `run` renders the v1 MIR first where it lowers, so the
/// codegen leg is pinned with the retired-flag override.
fn run(src: &Path, wasm: bool) -> String {
    let mut cmd = Command::new(almide_bin());
    cmd.arg("run").arg(src);
    if wasm {
        cmd.args(["--target", "wasm"]);
    } else {
        cmd.arg("--no-verified").env("ALMIDE_NO_VERIFIED_OK", "1");
    }
    let output = cmd.output().expect("failed to spawn almide");
    assert!(output.status.success(), "run failed (wasm={wasm}):\n{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn the_parse_loop_prints_the_same_on_both_legs() {
    if !tool_available() { eprintln!("skipping: almide binary not available"); return; }
    with_program(PARSE, "run", |src| {
        assert_eq!(run(src, false), EXPECTED, "native");
        assert_eq!(run(src, true), EXPECTED, "wasm");
    });
}
