//! A `mut` buffer param on the native render is lent `&mut` by its caller
//! (crate almide-mir `native_inout`): the callee pushes into the caller's own
//! buffer, as the standard codegen does, instead of returning it in a tuple
//! the call site writes back. A shape outside the convention keeps the
//! move-mode tuple, and every program prints what the other legs print.

fn render(src: &str) -> Result<String, String> {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_source(&src).map_err(|e| e.to_string()))
        .unwrap()
        .join()
        .unwrap()
}

/// The rendered header line of the fn named `almd_<name>`.
fn header_of(out: &str, name: &str) -> String {
    let head = format!("fn almd_{name}(");
    out.lines()
        .find(|l| l.trim_start().starts_with(&head))
        .unwrap_or_else(|| panic!("{name} is rendered:\n{out}"))
        .to_string()
}

fn run_native(src: &str) -> String {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("p.almd");
    std::fs::write(&file, src).expect("source");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("run")
        .arg(&file)
        .env("ALMIDE_CERTIFY_OWNERSHIP", "fail")
        .output()
        .expect("almide run");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

const PUSH: &str = r#"
type Fail =
  | Plain
  | Why(String)

fn chk(x: Int, lim: Int) -> Result[Int, Fail] =
  if x > lim then err(Why("big " + int.to_string(x))) else ok(x)

fn add(mut acc: List[Int], x: Int, lim: Int) -> Result[Int, Fail] = {
  let v = chk(x, lim)!
  list.push(acc, v)
  ok(list.len(acc))
}

fn add_arg(mut acc: List[Int], x: Int, lim: Int) -> Result[Int, Fail] = {
  list.push(acc, chk(x, lim)!)
  ok(list.len(acc))
}

effect fn main() -> Unit = {
  var total = 0
  var errs = 0
  var xs: List[Int] = []
  for k in 0..<20 {
    match add(xs, k, 12) {
      ok(n) => {
        total = total + n
      },
      err(_) => {
        errs = errs + 1
      },
    }
    match add_arg(xs, k, 15) {
      ok(n) => {
        total = total + n
      },
      err(_) => {
        errs = errs + 1
      },
    }
  }
  println(int.to_string(total) + " " + int.to_string(errs) + " " + int.to_string(list.len(xs)))
}
"#;

#[test]
fn a_mut_list_param_is_lent_and_pushed_in_place() {
    let out = render(PUSH).expect("renders");
    for name in ["add", "add_arg"] {
        let header = header_of(&out, name);
        assert!(header.contains("&mut Vec<i64>"), "{name} borrows the caller's buffer: {header}");
    }
    assert!(out.contains("almd_add(&mut "), "the caller lends its own list:\n{out}");
    assert_eq!(run_native(PUSH), "435 11 29\n");
}

/// The body of the rendered fn named `almd_<name>`, up to the next fn.
fn body_of(out: &str, name: &str) -> String {
    let head = format!("fn almd_{name}(");
    let mut lines = out.lines().skip_while(|l| !l.trim_start().starts_with(&head));
    let first = lines.next().unwrap_or_else(|| panic!("{name} is rendered:\n{out}"));
    std::iter::once(first).chain(lines.take_while(|l| !l.starts_with("fn "))).collect::<Vec<_>>().join("\n")
}

#[test]
fn a_dying_result_gives_up_its_err_by_value_and_its_consumer_frees_it_in_each_arm() {
    let out = render(PUSH).expect("renders");
    for name in ["add", "add_arg"] {
        let body = body_of(&out, name);
        // The err payload leaves the carrier with it: no `Default` written back
        // through `&mut` into a carrier about to be dropped.
        assert!(!body.contains("std::mem::take(v)"), "{name} moves the err out by value:\n{body}");
        assert!(body.contains("{ Err(v) => v, Ok(_) => unreachable!() }"), "{name}:\n{body}");
    }
    // main's two consumers each free their Result in both arms of its match,
    // not at the end of the round past the other call.
    let main = out.lines().skip_while(|l| !l.starts_with("fn main(")).collect::<Vec<_>>().join("\n");
    assert_eq!(main.matches("drop(v").count(), 4, "{main}");
    assert_eq!(run_native(PUSH), "435 11 29\n");
}

const RESET: &str = r#"
fn note(mut log: List[Int], v: Int) -> Unit = {
  if v % 5 == 0 then list.push(log, v * 2) else list.push(log, v)
}

fn step(mut log: List[Int], v: Int) -> Result[Int, String] = {
  if v < 0 then err("neg") else {
    note(log, v)
    if list.len(log) > 9 then log = [] else ()
    ok(list.len(log))
  }
}

effect fn main() -> Unit = {
  var log: List[Int] = []
  var acc = 0
  var bad = 0
  for i in 0..<40 {
    let v = if i % 10 == 9 then -1 else i
    match step(log, v) {
      ok(n) => {
        acc = acc + n
      },
      err(_) => {
        bad = bad + 1
      },
    }
  }
  println(int.to_string(acc) + " " + int.to_string(bad) + " " + int.to_string(list.len(log)))
}
"#;

#[test]
fn a_lent_buffer_rebound_in_one_arm_and_lent_on_reaches_the_caller() {
    let out = render(RESET).expect("renders");
    assert!(header_of(&out, "step").contains("&mut Vec<i64>"), "{out}");
    assert!(out.contains("almd_note(&mut *"), "the lent buffer is lent on:\n{out}");
    assert!(out.contains("*v0 = "), "the rebind writes the caller's buffer:\n{out}");
    assert_eq!(run_native(RESET), "156 4 6\n");
}

const SNAPSHOT: &str = r#"
fn put(mut s: String, x: Int) -> Int = {
  string.push(s, if x % 2 == 0 then "a" else "bc")
  string.len(s)
}

effect fn main() -> Unit = {
  var s = "x"
  let before = s
  let n = put(s, 1) + put(s, 2)
  println(int.to_string(n) + " " + s + " " + before)
}
"#;

#[test]
fn a_branch_operand_beside_the_place_keeps_the_move_mode_convention() {
    // The operand-order lowering would bind the place before the branch it
    // lifts out of the argument list, so the write would land in that copy.
    let out = render(SNAPSHOT).expect("renders");
    assert!(!header_of(&out, "put").contains("&mut"), "{out}");
    assert_eq!(run_native(SNAPSHOT), "7 xbca x\n");
}
