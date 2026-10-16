//! #3758: the streaming family validates UTF-8 per line READ, not per file.
//! Native's walkers are `BufRead::read_line` loops, so a byte is checked only
//! when a line holding it is read: a range whose lines are valid answers ok
//! however broken the rest of the file is, an empty range reads nothing, and
//! a mid-file invalid line errs AFTER the callbacks for the lines before it
//! ran (C-220). The wasm leg read the whole file as one UTF-8 text first, so
//! any invalid byte anywhere failed the call before a single callback.
//!
//! The matrix: every walker of the family × {invalid byte inside the walked
//! part, outside it, in a range's discarded prefix, a binary file with an
//! empty range}, with callbacks that print each line they see, on both legs
//! one binary drives — byte-identical to one pinned expectation (native's).
use std::path::{Path, PathBuf};
use std::process::Command;

const PROBE: &str = r#"import fs

effect fn step(acc: Int, l: String) -> Result[Int, String] = {
  println("  eff " + l)
  ok(acc + 1)
}

effect fn show(l: String) -> Result[Unit, String] = {
  println("  each_eff " + l)
  ok(())
}

effect fn main() -> Unit = {
  println("fold_lines msi mix")
  let f1: Result[Map[String, Int], String] = fs.fold_lines("mix.txt", map.new(), (m, s) => {
    println("  cb " + s)
    map.set(m, s, 1)
  })
  println("  = ${f1}")
  println("fold_lines i bin")
  let f2: Result[Int, String] = fs.fold_lines("bin.dat", 0, (acc, s) => acc + 1)
  println("  = ${f2}")
  println("fold_lines eff tail")
  let f3: Result[Int, String] = fs.fold_lines("tail.txt", 0, (acc, l) => step(acc, l)!)
  println("  = ${f3}")
  println("for_each_line mix")
  let e1: Result[Unit, String] = fs.for_each_line("mix.txt", (l) => println("  each " + l))
  println("  = ${e1}")
  println("for_each_line eff mix")
  let e2: Result[Unit, String] = fs.for_each_line("mix.txt", (l) => show(l)!)
  println("  = ${e2}")
  println("chunked i w1 mix")
  let c1: Result[List[Int], String] = fs.fold_lines_chunked("mix.txt", 1, 0, (acc, s) => {
    println("  ccb " + s)
    acc + 1
  })
  println("  = ${c1}")
  let c2: Result[List[Int], String] = fs.fold_lines_chunked("mix.txt", 3, 0, (acc, s) => acc + 1)
  let c3: Result[List[List[String]], String] = fs.fold_lines_chunked("mix.txt", 3, [], (acc, s) => acc + [s])
  let c4: Result[List[Map[String, Int]], String] = fs.fold_lines_chunked("mix.txt", 3, map.new(), (m, s) => map.set(m, s, 1))
  let c5: Result[List[Int], String] = fs.fold_lines_chunked("split.txt", 4, 0, (acc, s) => acc + 1)
  let c6: Result[List[List[String]], String] = fs.fold_lines_chunked("bin.dat", 4, [], (acc, s) => acc + [s])
  println("chunked i3=${c2} ls3=${c3} msi3=${c4} split=${c5} bin=${c6}")
  println("range ls mix [0,9)")
  let r0: Result[List[String], String] = fs.fold_lines_range("mix.txt", 0, 9, [], (acc, s) => {
    println("  rcb " + s)
    acc + [s]
  })
  println("  = ${r0}")
  let r1: Result[List[String], String] = fs.fold_lines_range("mix.txt", 0, 2, [], (acc, s) => acc + [s])
  let r2: Result[List[String], String] = fs.fold_lines_range("mix.txt", 0, 5, [], (acc, s) => acc + [s])
  let r3: Result[List[String], String] = fs.fold_lines_range("mix.txt", 9, 11, [], (acc, s) => acc + [s])
  let r4: Result[List[String], String] = fs.fold_lines_range("bin.dat", 0, 0, [], (acc, s) => acc + [s])
  let r5: Result[List[String], String] = fs.fold_lines_range("inner.txt", 4, 9, [], (acc, s) => acc + [s])
  let r6: Result[Int, String] = fs.fold_lines_range("mix.txt", 0, 2, 0, (acc, s) => acc + 1)
  let r7: Result[List[String], String] = fs.fold_lines_range("tail.txt", 0, 2, [], (acc, s) => acc + [s])
  let r8: Result[List[String], String] = fs.fold_lines_range("split.txt", 2, 5, [], (acc, s) => acc + [s])
  let r9: Result[List[String], String] = fs.fold_lines_range("split.txt", 1, 5, [], (acc, s) => acc + [s])
  let ra: Result[Int, String] = fs.fold_lines_range("bin.dat", 0, 0, 0, (acc, s) => acc + 1)
  println("range out=${r1} in=${r2} after=${r3} bin0=${r4} discard_bad=${r5} i_out=${r6}")
  println("range tail=${r7} split_mid=${r8} split_lead=${r9} bin0_i=${ra}")
}
"#;

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

fn probe_dir() -> PathBuf {
    let d = std::env::temp_dir().join(format!("almide-fold-utf8-scope-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("mkdir");
    std::fs::write(d.join("probe.almd"), PROBE).expect("write probe");
    // invalid line in the middle
    std::fs::write(d.join("mix.txt"), b"a\nb\n\xff\xfe\nc\n").expect("write");
    // the inspection fuzzer's sandbox file
    std::fs::write(d.join("bin.dat"), [0u8, 1, 2, 127, 128, 254, 255, 10, 13, 0]).expect("write");
    // invalid byte in the line a range starting at 4 discards
    std::fs::write(d.join("inner.txt"), b"ab\nc\xffd\ne\n").expect("write");
    // invalid final, unterminated line
    std::fs::write(d.join("tail.txt"), b"a\nb\nc\nd\n\xff").expect("write");
    // valid UTF-8 whose 2-byte character a range boundary can split
    std::fs::write(d.join("split.txt"), "\u{e9}\nx\n").expect("write");
    d
}

fn run_leg(dir: &Path, args: &[&str]) -> String {
    let o = Command::new(almide_bin())
        .arg("run")
        .arg("probe.almd")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("spawn almide");
    assert!(o.status.success(), "leg {args:?} failed:\n{}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).expect("utf8")
}

const ERR_MIX: &str = r#"err("fs.fold_lines_range(\"mix.txt\"): stream did not contain valid UTF-8")"#;

fn expected() -> String {
    let u = |call: &str, f: &str| format!(r#"err("{call}(\"{f}\"): stream did not contain valid UTF-8")"#);
    let r = |f: &str| u("fs.fold_lines_range", f);
    [
        "fold_lines msi mix".to_string(),
        "  cb a".into(),
        "  cb b".into(),
        format!("  = {}", u("fs.fold_lines", "mix.txt")),
        "fold_lines i bin".into(),
        format!("  = {}", u("fs.fold_lines", "bin.dat")),
        "fold_lines eff tail".into(),
        "  eff a".into(),
        "  eff b".into(),
        "  eff c".into(),
        "  eff d".into(),
        format!("  = {}", u("fs.fold_lines", "tail.txt")),
        "for_each_line mix".into(),
        "  each a".into(),
        "  each b".into(),
        format!("  = {}", u("fs.for_each_line", "mix.txt")),
        "for_each_line eff mix".into(),
        "  each_eff a".into(),
        "  each_eff b".into(),
        format!("  = {}", u("fs.for_each_line", "mix.txt")),
        "chunked i w1 mix".into(),
        "  ccb a".into(),
        "  ccb b".into(),
        format!("  = {ERR_MIX}"),
        format!(
            "chunked i3={ERR_MIX} ls3={ERR_MIX} msi3={ERR_MIX} split={} bin={}",
            r("split.txt"),
            r("bin.dat")
        ),
        "range ls mix [0,9)".into(),
        "  rcb a".into(),
        "  rcb b".into(),
        format!("  = {ERR_MIX}"),
        format!(
            r#"range out=ok(["a"]) in={ERR_MIX} after=ok([]) bin0=ok([]) discard_bad={} i_out=ok(1)"#,
            r("inner.txt")
        ),
        format!(r#"range tail=ok(["a"]) split_mid={} split_lead=ok(["x"]) bin0_i=ok(0)"#, r("split.txt")),
        String::new(),
    ]
    .join("\n")
}

#[test]
fn utf8_is_validated_per_line_read_on_every_walker_and_leg() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let dir = probe_dir();
    let want = expected();
    for (label, args) in [("native", &[][..]), ("wasm", &["--target", "wasm"][..])] {
        let got = run_leg(&dir, args);
        assert_eq!(got, want, "the {label} leg: UTF-8 must be validated per line read (#3758, C-220)");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
