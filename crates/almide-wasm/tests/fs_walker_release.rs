//! The I/O walkers release what they allocate per element (#3137).
//!
//! The alloc ledger (alloc_ledger.rs) cannot judge these: every corpus
//! fixture that imports `fs` is a host-variant `~` row in
//! `alloc-baseline.txt`, since its heap depends on the files the host
//! serves. This gate gives each walker a DETERMINISTIC input — files it
//! writes itself into a fresh directory — and asserts the live-heap
//! instrument (`allocs − frees − region_reclaimed`, live_at_exit.rs) reads
//! 0 when `main` returns, together with the program's stdout.
//!
//! Before #3137 the line walkers (`fs.fold_lines`, `fs.for_each_line` and
//! the fallible `__fallible_fold_lines` / `__fallible_for_each_line`, both
//! the inline and the closure routes) left one block per line live, the
//! prefetch fans (`fan.map` / `fan.any` over `fs.read_text`) one Result shell
//! per element, and the size probe under `fs.fold_lines_chunked` /
//! `fs.file_size` its 8-byte scratch.
//!
//! The negative control for a use-after-free: callbacks that KEEP the line
//! (`acc + [line]`, a fallible `ok(acc + [line])`) return it in a list that
//! is printed after the walk. Releasing a block the callback kept would
//! print reused bytes, or trap, instead of the lines.

mod harness;
use harness::run_wasm;

struct Dir(std::path::PathBuf);

impl Dir {
    fn new(tag: &str) -> Self {
        let p = std::env::temp_dir().join(format!("almide_fs_walker_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).expect("temp dir");
        std::fs::write(p.join("in.txt"), "a\nbb\nccc\nd\ne\nf\ng\nh\n").expect("in.txt");
        std::fs::write(p.join("bad.txt"), "a\nzz\nc\n").expect("bad.txt");
        std::fs::write(p.join("a.txt"), "x").expect("a.txt");
        std::fs::write(p.join("b.txt"), "yy").expect("b.txt");
        std::fs::write(p.join("c.txt"), "zzz").expect("c.txt");
        Dir(p)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Emit `src` (with `{D}` replaced by the input directory) under the
/// counting switch, run it, and return (stdout, live blocks at exit).
fn live_at_exit(tag: &str, src: &str) -> (String, i64) {
    let dir = Dir::new(tag);
    let src = src.replace("{D}", &dir.0.to_string_lossy());
    let ir = almide_spine::s5::lower_to_ir("probe.almd", &src).expect("front");
    let _armed = almide_wasm::alloc_count::CountGuard::set();
    let bytes = almide_wasm::emit_program(&ir).expect("emit");
    let r = run_wasm(&bytes).expect("run");
    assert_eq!(r.exit, 0, "stderr: {}", r.stderr);
    let c = r.alloc_count.expect("the armed module exports the counters");
    (r.stdout, c.live())
}

fn assert_clean(tag: &str, src: &str, want: &str) {
    let (out, live) = live_at_exit(tag, src);
    assert_eq!(out, want, "{tag}: stdout");
    assert_eq!(live, 0, "{tag}: blocks live at exit (stdout {out:?})");
}

#[test]
fn fold_lines_releases_each_line() {
    assert_clean(
        "fold_int",
        r#"import fs
effect fn main() -> Unit = {
  let n = fs.fold_lines("{D}/in.txt", 0, (acc, line) => acc + string.len(line))!
  println(int.to_string(n))
}
"#,
        "11\n",
    );
}

/// The negative control: the callback keeps every line, and a heap
/// accumulator is replaced (concat), returned as-is, or failed with.
#[test]
fn fold_lines_keeps_what_the_callback_keeps() {
    assert_clean(
        "fold_keep",
        r#"import fs
effect fn main() -> Unit = {
  let xs = fs.fold_lines("{D}/in.txt", [], (acc, line) => acc + [line])!
  println(list.join(xs, ","))
  let s = fs.fold_lines("{D}/in.txt", "", (acc, line) => acc + line)!
  println(s)
  let k = fs.fold_lines("{D}/in.txt", "keep", (acc, line) => acc)!
  println(k)
  match fs.fold_lines("{D}/absent.txt", ["x"], (acc, line) => acc + [line]) {
    ok(_) => println("absent ok"),
    err(_) => println("absent err"),
  }
}
"#,
        "a,bb,ccc,d,e,f,g,h\nabbcccdefgh\nkeep\nabsent err\n",
    );
}

#[test]
fn for_each_line_releases_each_line() {
    assert_clean(
        "each",
        r#"import fs
effect fn main() -> Unit = {
  fs.for_each_line("{D}/in.txt", (line) => println(line))!
}
"#,
        "a\nbb\nccc\nd\ne\nf\ng\nh\n",
    );
}

/// The fallible walkers, both callback shapes: the canonical `step(a, l)!`
/// inlines, a block body is called as a closure (#1806). Ok and err paths.
#[test]
fn fallible_walkers_release_each_line_and_carrier() {
    assert_clean(
        "fallible",
        r#"import fs
fn add_row(acc: Int, line: String) -> Int! = if line == "zz" then err("bad row: " + line) else ok(acc + string.len(line))
fn keep(acc: List[String], line: String) -> List[String]! = if line == "zz" then err("bad") else ok(acc + [line])
fn note(l: String) -> Unit! = if l == "zz" then err("bad line: " + l) else ok(())
fn show(r: Result[List[String], String]) -> String = match r {
  ok(xs) => list.join(xs, ","),
  err(e) => "err " + e,
}
fn show_unit(r: Result[Unit, String]) -> String = match r {
  ok(_) => "ok",
  err(e) => "err " + e,
}
effect fn main() -> Unit = {
  let a = fs.fold_lines("{D}/in.txt", 0, (acc, line) => add_row(acc, line)!)!
  let b = fs.fold_lines("{D}/in.txt", 0, (acc, line) => {
    let l = string.trim(line)
    add_row(acc, l)!
  })!
  println("${a} ${b}")
  println(show(fs.fold_lines("{D}/in.txt", [], (acc, line) => keep(acc, line)!)))
  println(show(fs.fold_lines("{D}/in.txt", [], (acc, line) => {
    let l = string.trim(line)
    keep(acc, l)!
  })))
  println(show(fs.fold_lines("{D}/bad.txt", [], (acc, line) => keep(acc, line)!)))
  println(show(fs.fold_lines("{D}/bad.txt", [], (acc, line) => {
    let l = string.trim(line)
    keep(acc, l)!
  })))
  println(show_unit(fs.for_each_line("{D}/in.txt", (line) => note(line)!)))
  println(show_unit(fs.for_each_line("{D}/bad.txt", (line) => note(line)!)))
  println(show_unit(fs.for_each_line("{D}/bad.txt", (line) => {
    let l = string.trim(line)
    note(l)!
  })))
}
"#,
        "11 11\na,bb,ccc,d,e,f,g,h\na,bb,ccc,d,e,f,g,h\nerr bad\nerr bad\nok\nerr bad line: zz\nerr bad line: zz\n",
    );
}

#[test]
fn fold_lines_chunked_releases_the_size_probe() {
    assert_clean(
        "chunked",
        r#"import fs
effect fn main() -> Unit = {
  let parts = fs.fold_lines_chunked("{D}/in.txt", 3, [], (acc, line) => acc + [line])!
  println(int.to_string(list.len(parts)))
  println(int.to_string(fs.file_size("{D}/in.txt")!))
}
"#,
        "3\n19\n",
    );
}

/// The prefetch fans: an ok's shell is released once its payload moved, a
/// losing err is released, and an err result releases what was collected.
#[test]
fn prefetch_fans_release_each_result_shell() {
    assert_clean(
        "fan",
        r#"import fs
import fan
effect fn main() -> Unit = {
  let xs = fan.map(["{D}/a.txt", "{D}/b.txt", "{D}/c.txt"], (p) => fs.read_text(p)!)!
  println(list.join(xs, ","))
  let y = fan.any(["{D}/no1.txt", "{D}/no2.txt", "{D}/b.txt", "{D}/c.txt"], (p) => fs.read_text(p)!)!
  println(y)
  match fan.map(["{D}/a.txt", "{D}/no.txt", "{D}/c.txt"], (p) => fs.read_text(p)!) {
    ok(_) => println("map ok"),
    err(_) => println("map err"),
  }
  match fan.any(["{D}/no1.txt", "{D}/no2.txt"], (p) => fs.read_text(p)!) {
    ok(_) => println("any ok"),
    err(e) => println(e),
  }
}
"#,
        "x,yy,zzz\nyy\nmap err\nfan.any: all candidates failed\n",
    );
}
