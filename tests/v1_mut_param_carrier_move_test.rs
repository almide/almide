//! A `mut` parameter fn that returns a Result hands its buffer back inside
//! the Result, and the call site takes it out again. Where the Result is
//! bound once in its loop round and nothing names it on a later path, the
//! native render moves the buffer out by value instead of writing an empty
//! one into a carrier about to be dropped (#4640). The program holds the
//! shapes that decide it: the call in a loop on the ok and the err path, a
//! String error, a Result read twice after the call (which must not be
//! consumed), and the call under each arm of a bound branch.
use std::process::Command;

const PROGRAM: &str = r##"type Fail =
  | Plain
  | Why(String)

fn chk(x: Int, lim: Int) -> Result[Int, Fail] =
  if x > lim then err(Why("big " + int.to_string(x))) else if x < 0 then err(Plain) else ok(x)

fn add(mut acc: List[Int], x: Int, lim: Int) -> Result[Int, Fail] = {
  let v = chk(x, lim)!
  list.push(acc, v)
  ok(list.len(acc))
}

fn add_s(mut acc: List[String], x: Int, lim: Int) -> Result[Int, String] = {
  if x > lim then err("over " + int.to_string(x)) else {
    list.push(acc, "s" + int.to_string(x))
    ok(list.len(acc))
  }
}

fn show(r: Result[Int, Fail]) -> String = match r {
  ok(n) => "ok " + int.to_string(n),
  err(Why(m)) => "why " + m,
  err(Plain) => "plain",
}

fn twice(lim: Int) -> String = {
  var xs: List[Int] = [1]
  let r = add(xs, 5, lim)
  let a = show(r)
  let b = match r {
    ok(n) => n,
    err(_) => -1,
  }
  a + " / " + int.to_string(b) + " / " + int.to_string(list.len(xs))
}

fn arms(flag: Bool, lim: Int) -> String = {
  var xs: List[Int] = []
  let n = if flag then match add(xs, 3, lim) {
    ok(n) => n,
    err(_) => 100,
  } else match add(xs, -3, lim) {
    ok(n) => n,
    err(e) => string.len(show(err(e))),
  }
  int.to_string(n) + ":" + int.to_string(list.len(xs))
}

effect fn main() -> Unit = {
  var total = 0
  var errs = 0
  var xs: List[Int] = []
  for k in 0..<12 {
    match add(xs, k, 8) {
      ok(n) => {
        total = total + n
      },
      err(e) => {
        errs = errs + 1
        println(show(err(e)))
      },
    }
  }
  println(int.to_string(total) + " " + int.to_string(errs) + " " + int.to_string(list.len(xs)))
  var ss: List[String] = []
  for k in 0..<6 {
    match add_s(ss, k, 3) {
      ok(n) => println("len " + int.to_string(n)),
      err(e) => println(e),
    }
  }
  println(list.join(ss, ","))
  println(twice(9))
  println(twice(2))
  println(arms(true, 9))
  println(arms(true, 1))
  println(arms(false, 9))
}
"##;
const EXPECTED: &str = "why big 9\nwhy big 10\nwhy big 11\n45 3 9\nlen 1\nlen 2\nlen 3\nlen 4\nover 4\nover 5\ns0,s1,s2,s3\nok 2 / 2 / 2\nwhy big 5 / -1 / 1\n1:1\n100:0\n5:0\n";

/// Both legs print `EXPECTED`; the native one is the certified render, held
/// to the ownership certificate with no fallback to the standard codegen.
fn both_legs_print(program: &str, expected: &str) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("prog.almd");
    std::fs::write(&file, program).expect("source");
    for extra in [&[][..], &["--target", "wasm"][..]] {
        let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).args(extra)
            .env("ALMIDE_CERTIFY_OWNERSHIP", "fail").env("ALMIDE_VERBOSE", "1")
            .current_dir(dir.path()).output().expect("almide run");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(0), "{extra:?}: {stderr}");
        assert!(!stderr.contains("walled"), "{extra:?} fell back to the standard codegen: {stderr}");
        assert_eq!(String::from_utf8_lossy(&out.stdout), expected, "{extra:?}");
    }
}

#[test]
fn the_buffer_comes_back_on_every_path_on_both_legs() {
    both_legs_print(PROGRAM, EXPECTED);
}
