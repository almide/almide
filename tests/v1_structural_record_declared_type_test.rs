//! A record literal inside a tuple, bound before anything gives it a type:
//! `let t = (s, { name: s, n: i })`, later returned as `(String, Raw)` or under
//! `some`. The checker leaves the literal's type structural there, and the
//! native render generated a struct for the structural spelling beside the
//! declared record's own, so rustc refused the function under its declared
//! signature (E0308) — after the render, where no other route is taken.
//! A structural record type is the declared record it matches; the native
//! render now spells it so and generates one type.
use std::process::Command;

/// The literal inside a pair: under `some` in a branch arm, returned whole,
/// with its fields written out of declaration order, matching a generic
/// declared record, read back by destructuring, displayed, compared, and in a
/// loop.
const SHAPE: &str = r##"type Raw = { name: String, n: Int }
type Box[T] = { item: T, tag: String }

fn pick(s: String, i: Int) -> Option[(String, Raw)] =
  if i < 0 then none
  else if string.len(s) > i then {
    let t = (s, { name: s, n: i })
    some(t)
  }
  else none

fn pair(s: String, i: Int) -> (String, Raw) = {
  let t = (s, { name: s + "!", n: i })
  t
}

fn swapped(s: String, i: Int) -> (Int, Raw) = {
  let t = (i, { n: i + 1, name: s })
  t
}

fn boxed(s: String) -> (Int, Box[String]) = {
  let t = (string.len(s), { item: s, tag: "t" })
  t
}

fn main() -> Unit = {
  match pick("hello", 2) {
    some((s, r)) => println("${s} ${r.name} ${int.to_string(r.n)}"),
    none => println("none"),
  }
  match pick("hello", 9) {
    some((s, r)) => println(s + r.name),
    none => println("none"),
  }
  let (s, r) = pair("c", 3)
  println("${s} ${r.name} ${int.to_string(r.n)}")
  println("${r}")
  let (i, w) = swapped("w", 4)
  println("${int.to_string(i)} ${w.name} ${int.to_string(w.n)}")
  println("${w == { name: "w", n: 5 }}")
  let (k, b) = boxed("box")
  println("${int.to_string(k)} ${b.item} ${b.tag}")
  let loose = ("p", { name: "p", n: 9 })
  println("${loose}")
  var total = 0
  for j in 0..<200000 {
    total = total + (match pick("abcdef", j % 9) {
      some((x, y)) => string.len(x) + y.n,
      none => 1,
    })
  }
  println(int.to_string(total))
}
"##;

const SHAPE_STDOUT: &str = "hello hello 2\nnone\nc c! 3\nRaw { name: \"c!\", n: 3 }\n4 w 5\ntrue\n3 box t\n(\"p\", Raw { name: \"p\", n: 9 })\n1200001\n";

/// Records the rule must leave alone: the declared record's field names with
/// other field types, and a shape no declared record has, each keeping a
/// generated type of its own; and a literal the checker already typed.
const CONTROL: &str = r##"type Raw = { name: String, n: Int }

fn other(i: Int) -> (Int, { name: Int, n: Int }) = {
  let t = (i, { name: i, n: i + 1 })
  t
}

fn free(s: String) -> (String, { label: String, k: Int }) = {
  let t = (s, { label: s, k: string.len(s) })
  t
}

fn direct(s: String, i: Int) -> Option[Raw] = {
  let r = { name: s, n: i }
  if i < 0 then none else some(r)
}

fn main() -> Unit = {
  let (j, o) = other(5)
  println("${int.to_string(j)} ${int.to_string(o.name)} ${int.to_string(o.n)}")
  println("${o}")
  let (s, f) = free("xyz")
  println("${s} ${f.label} ${int.to_string(f.k)}")
  println("${f}")
  match direct("d", 1) {
    some(r) => println("${r}"),
    none => println("none"),
  }
}
"##;

const CONTROL_STDOUT: &str = "5 5 6\n{ n: 6, name: 5 }\nxyz xyz 3\n{ k: 3, label: \"xyz\" }\nRaw { name: \"d\", n: 1 }\n";

/// The doc line the render writes above a struct generated for a structural
/// record spelling.
const STRUCTURAL_STRUCT: &str = "/// record `Record {";

fn render(src: &str) -> String {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_source(&src).map_err(|e| format!("{e:?}")))
        .expect("spawn")
        .join()
        .expect("render thread")
        .unwrap_or_else(|e| panic!("walls: {e}"))
}

fn runs_unwalled(name: &str, src: &str, stdout: &str) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join(format!("{name}.almd"));
    std::fs::write(&file, src).expect("source");
    for extra in [&[][..], &["--target", "wasm"][..]] {
        let run = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).args(extra)
            .env("ALMIDE_CERTIFY_OWNERSHIP", "fail").env("ALMIDE_VERBOSE", "1")
            .current_dir(dir.path()).output().expect("almide run");
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(run.status.success(), "{name} {extra:?}: {stderr}");
        assert!(!stderr.contains("walled"), "{name} {extra:?} fell back: {stderr}");
        assert_eq!(String::from_utf8_lossy(&run.stdout), stdout, "{name} {extra:?}");
    }
}

#[test]
fn a_record_literal_inside_a_pair_is_the_declared_record_it_matches() {
    let rust = render(SHAPE);
    assert!(!rust.contains(STRUCTURAL_STRUCT), "a structural spelling kept a struct of its own");
    runs_unwalled("structural_record_declared", SHAPE, SHAPE_STDOUT);
}

#[test]
fn a_record_no_declared_type_matches_keeps_its_structural_type() {
    let rust = render(CONTROL);
    assert_eq!(rust.matches(STRUCTURAL_STRUCT).count(), 2, "one struct per unmatched structural record");
    runs_unwalled("structural_record_control", CONTROL, CONTROL_STDOUT);
}
