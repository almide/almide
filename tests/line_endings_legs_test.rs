//! #2813: a line ending is `\n` or `\r\n`; a BARE `\r` is content — native's
//! `str::lines` / `BufRead::read_line` + pop-`\n`-then-`\r` rule, which the
//! structural leg followed and the incumbent did not. The incumbent's self-host
//! walkers stripped a `\r` from the final line whether or not a `\n` ended it
//! (`string.lines`, and through it `fs.read_lines*`; the `fs.fold_lines` /
//! `for_each_line` / `_range` / `_chunked` walk), the interp's fallible
//! fold_lines carrier did the same, and the incumbent `io.read_line` floor cut
//! ONE trailing `\r` where native's `trim_end_matches('\r')` cuts them all.
//!
//! The matrix: every line-splitting surface × inputs with the CR at the end, in
//! the middle and doubled, on each leg one binary drives — native and the
//! wasm leg — all byte-identical to one pinned expectation (native is
//! `str::lines`, so the pin is its output). A new line-aware surface belongs in PROBE_ALL or
//! PROBE_PARTITIONED, never in a point test.
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::io::Write;

const PROBE_ALL: &str = r#"import fs

fn lens(ls: List[String]) -> String =
  ls |> list.map((l) => "[" + int.to_string(string.len(l)) + "]") |> list.join("")

effect fn step(acc: Int, l: String) -> Result[Int, String] = ok(acc * 100 + string.len(l) + 1)

effect fn show_line(l: String) -> Result[Unit, String] = {
  println("    each " + int.to_string(string.len(l)))
  ok(())
}

effect fn probe(name: String, s: String) -> Unit = {
  let p = "probe.txt"
  fs.write(p, s)!
  println(name)
  println("  lines     " + lens(string.lines(s)))
  println("  split     " + lens(string.split(s, "\n")))
  println("  read      " + lens(fs.read_lines(p)!))
  println("  read_ie   " + lens(fs.read_lines_if_exists(p)! ?? []))
  println("  fold_s    " + fs.fold_lines(p, "", (acc, l) => acc + "[" + int.to_string(string.len(l)) + "]")!)
  println("  fold_i    " + int.to_string(fs.fold_lines(p, 0, (acc, l) => acc * 100 + string.len(l) + 1)!))
  println("  fold_ls   " + lens(fs.fold_lines(p, [], (acc, l) => acc + [l])!))
  println("  fold_eff  " + int.to_string(fs.fold_lines(p, 0, (acc, l) => step(acc, l)!)!))
  fs.for_each_line(p, (l) => show_line(l)!)!
  println("  trim_end  " + int.to_string(string.len(string.trim_end(s))))
}

effect fn main() -> Unit = {
  probe("cr-end", "abc\r")!
  probe("cr-cr-end", "abc\r\r")!
  probe("lf-then-cr-end", "a\nbc\r")!
  probe("crlf-end", "abc\r\n")!
  probe("lf-end", "abc\n")!
  probe("empty", "")!
  probe("lone-cr", "\r")!
  probe("crlf-cr", "\r\n\r")!
  probe("cr-mid", "a\rb\r\n\rc\r")!
}
"#;

const EXPECTED_ALL: &str = r#"cr-end
  lines     [4]
  split     [4]
  read      [4]
  read_ie   [4]
  fold_s    [4]
  fold_i    5
  fold_ls   [4]
  fold_eff  5
    each 4
  trim_end  3
cr-cr-end
  lines     [5]
  split     [5]
  read      [5]
  read_ie   [5]
  fold_s    [5]
  fold_i    6
  fold_ls   [5]
  fold_eff  6
    each 5
  trim_end  3
lf-then-cr-end
  lines     [1][3]
  split     [1][3]
  read      [1][3]
  read_ie   [1][3]
  fold_s    [1][3]
  fold_i    204
  fold_ls   [1][3]
  fold_eff  204
    each 1
    each 3
  trim_end  4
crlf-end
  lines     [3]
  split     [4][0]
  read      [3]
  read_ie   [3]
  fold_s    [3]
  fold_i    4
  fold_ls   [3]
  fold_eff  4
    each 3
  trim_end  3
lf-end
  lines     [3]
  split     [3][0]
  read      [3]
  read_ie   [3]
  fold_s    [3]
  fold_i    4
  fold_ls   [3]
  fold_eff  4
    each 3
  trim_end  3
empty
  lines     
  split     [0]
  read      
  read_ie   
  fold_s    
  fold_i    0
  fold_ls   
  fold_eff  0
  trim_end  0
lone-cr
  lines     [1]
  split     [1]
  read      [1]
  read_ie   [1]
  fold_s    [1]
  fold_i    2
  fold_ls   [1]
  fold_eff  2
    each 1
  trim_end  0
crlf-cr
  lines     [0][1]
  split     [1][1]
  read      [0][1]
  read_ie   [0][1]
  fold_s    [0][1]
  fold_i    102
  fold_ls   [0][1]
  fold_eff  102
    each 0
    each 1
  trim_end  0
cr-mid
  lines     [3][3]
  split     [4][3]
  read      [3][3]
  read_ie   [3][3]
  fold_s    [3][3]
  fold_i    404
  fold_ls   [3][3]
  fold_eff  404
    each 3
    each 3
  trim_end  7
"#;

const PROBE_PARTITIONED: &str = r#"import fs

effect fn probe(name: String, s: String) -> Unit = {
  let p = "probe.txt"
  fs.write(p, s)!
  let r = fs.fold_lines_range(p, 0, 1000, [], (acc, l) => acc + [int.to_string(string.len(l))])!
  let c = fs.fold_lines_chunked(p, 2, 0, (acc, l) => acc * 100 + string.len(l) + 1)!
  println(name + " range=" + list.join(r, ",") + " chunked=" + (c |> list.map((x) => int.to_string(x)) |> list.join(",")))
}

effect fn main() -> Unit = {
  probe("cr-end", "abc\r")!
  probe("cr-cr-end", "abc\r\r")!
  probe("lf-then-cr-end", "a\nbc\r")!
  probe("crlf-end", "abc\r\n")!
  probe("lf-end", "abc\n")!
  probe("empty", "")!
  probe("lone-cr", "\r")!
  probe("crlf-cr", "\r\n\r")!
  probe("cr-mid", "a\rb\r\n\rc\r")!
}
"#;

const EXPECTED_PARTITIONED: &str = r#"cr-end range=4 chunked=5,0
cr-cr-end range=5 chunked=6,0
lf-then-cr-end range=1,3 chunked=204,0
crlf-end range=3 chunked=4,0
lf-end range=3 chunked=4,0
empty range= chunked=
lone-cr range=1 chunked=2
crlf-cr range=0,1 chunked=1,2
cr-mid range=3,3 chunked=4,4
"#;

const PROBE_STDIN: &str = r#"import io

effect fn main() -> Unit = {
  let a = io.read_line()
  let b = io.read_line_opt() ?? "<none>"
  println("${string.len(a)} ${string.len(b)}")
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

fn probe_dir(tag: &str, src: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("almide-line-endings-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("mkdir");
    std::fs::write(d.join("probe.almd"), src).expect("write");
    d
}

/// One leg: (label, extra args, env).
type Leg = (&'static str, &'static [&'static str], &'static [(&'static str, &'static str)]);

/// The legs one binary drives.
const LEGS: [Leg; 2] = [("native", &[], &[]), ("wasm", &["--target", "wasm"], &[])];

fn run_leg(dir: &Path, args: &[&str], env: &[(&str, &str)], stdin: &[u8]) -> String {
    let mut c = Command::new(almide_bin());
    c.arg("run").arg("probe.almd").args(args).current_dir(dir);
    for (k, v) in env {
        c.env(k, v);
    }
    c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = c.spawn().expect("spawn almide");
    child.stdin.take().unwrap().write_all(stdin).expect("stdin");
    let o = child.wait_with_output().expect("wait almide");
    assert!(o.status.success(), "leg {args:?} {env:?} failed:\n{}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).expect("utf8")
}

fn every_leg(tag: &str, src: &str, stdin: &[u8], expected: &str) {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let dir = probe_dir(tag, src);
    for (label, args, env) in LEGS {
        let got = run_leg(&dir, args, env, stdin);
        assert_eq!(got, expected, "{tag}: the {label} leg diverges from str::lines' rule");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bare_cr_is_content_on_every_line_surface_and_leg() {
    every_leg("all", PROBE_ALL, b"", EXPECTED_ALL);
}

#[test]
fn bare_cr_is_content_in_the_partitioned_fold_walkers() {
    every_leg("partitioned", PROBE_PARTITIONED, b"", EXPECTED_PARTITIONED);
}

/// `io.read_line` / `read_line_opt` cut the `\n` and then EVERY trailing
/// `\r` (native `trim_end_matches`): "ab\r\r" reads as "ab", not "ab\r".
#[test]
fn stdin_line_readers_cut_every_trailing_cr() {
    every_leg("stdin", PROBE_STDIN, b"ab\r\r\ncd\r\r", "2 2\n");
    every_leg("stdin-bare", PROBE_STDIN, b"x\r\r\r", "1 6\n");
}
