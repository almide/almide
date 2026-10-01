//! #2755 / #3137 — the I/O WALKERS in the structural witness. The fs line
//! walkers inline a literal callback per line like `list.fold`, and each
//! line is a block the walk allocates and releases around the activation
//! (`i` … `d`); a heap accumulator is `list.fold`'s loop-carried owner, or —
//! in a fallible fold, which keeps it across an err — a frame owner the ok
//! rebinds. The prefetch fans run no body: each awaited read's Result carrier
//! is born in the frame and moved out or released. Every such frame must
//! certify, and the portable checker must accept it; a compound fallible body
//! (a closure) still declines.

const PROGRAM: &str = r#"import fs
import fan

fn add_row(acc: Int, line: String) -> Int! = if line == "zz" then err("bad") else ok(acc + string.len(line))

fn keep(acc: List[String], line: String) -> List[String]! = if line == "zz" then err("bad") else ok(acc + [line])

fn note(l: String) -> Unit! = if l == "zz" then err("bad") else ok(())

effect fn total(p: String) -> Int = fs.fold_lines(p, 0, (acc, line) => acc + string.len(line))!

effect fn joined(p: String) -> String = fs.fold_lines(p, "", (acc, line) => acc + line)!

effect fn rows(p: String) -> List[String] = fs.fold_lines(p, [], (acc, line) => acc + [line])!

effect fn each(p: String) -> Unit = fs.for_each_line(p, (line) => println(line))!

effect fn checked(p: String) -> Int = fs.fold_lines(p, 0, (acc, line) => add_row(acc, line)!)!

effect fn kept(p: String) -> List[String] = fs.fold_lines(p, [], (acc, line) => keep(acc, line)!)!

effect fn noted(p: String) -> Unit = fs.for_each_line(p, (line) => note(line)!)!

effect fn compound(p: String) -> Int = fs.fold_lines(p, 0, (acc, line) => {
  let l = string.trim(line)
  add_row(acc, l)!
})!

effect fn chunked(p: String) -> List[Int] = fs.fold_lines_chunked(p, 2, 0, (acc, line) => acc + 1)!

effect fn ranged(p: String) -> List[String] = fs.fold_lines_range(p, 0, 100, [], (acc, line) => acc + [line])!

effect fn texts(ps: List[String]) -> Result[List[String], String] = fan.map(ps, (p) => fs.read_text(p)!)

effect fn first(ps: List[String]) -> Result[String, String] = fan.any(ps, (p) => fs.read_text(p)!)

effect fn main() -> Unit = {
  let p = "in.txt"
  println("${total(p)!} ${joined(p)!} ${rows(p)!} ${checked(p)!} ${kept(p)!} ${compound(p)!}")
  each(p)!
  noted(p)!
  println("${chunked(p)!} ${ranged(p)!} ${texts([p])!} ${first([p])!}")
}
"#;

fn witnesses() -> std::collections::BTreeMap<String, String> {
    let ir = almide_spine::s5::lower_to_ir("walkers.almd", PROGRAM).expect("front");
    almide_wasm::witness::start_collecting();
    let _ = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    almide_wasm::witness::take().into_iter().collect()
}

fn accepted(cert: &str) -> bool {
    almide_verify::check(almide_verify::Property::Ownership, cert.as_bytes())
}

#[test]
fn io_walkers_witness_each_line_and_carrier() {
    // ONE test: the witness sink is process-global.
    let w = witnesses();
    for name in [
        "total", "joined", "rows", "each", "checked", "kept", "noted", "chunked", "ranged", "texts", "first",
    ] {
        let got = w.get(name).unwrap_or_else(|| panic!("{name} must be witnessed"));
        assert!(!got.starts_with('!'), "{name}: {got:?}");
        assert!(accepted(got), "{name}: the portable checker must accept {got:?}");
    }
    // A compound fallible body is a closure called per line: not recorded.
    assert_eq!(w["compound"], "!decline:call-arg:Lambda:fs.__fallible_fold_lines:propagating\n");
}
