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
    // The fallible join is declared unset: no `Ok(0)` built and dropped per call.
    assert!(!body_of(&out, "step").contains("= Ok(0);"), "{out}");
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

const LITERAL: &str = r#"
type W = { id: Int, name: String }

fn show(ws: List[W]) -> String = ws |> list.map((w) => "${w.id}${w.name}") |> list.join(",")

fn shout(mut s: String) -> Unit = {
  s = string.to_upper(string.trim(s)) + "!"
}

fn tag(mut s: String, n: Int) -> Unit = {
  s = s + int.to_string(n)
}

effect fn main() -> Unit = {
  // The sort links a bundled module, whose own preparation ran the in-out
  // selection again and left the registry empty for this render.
  let ws = [W { id: 2, name: "b" }, W { id: 1, name: "a" }]
  println(show(list.sort_by(ws, (w) => w.id)))
  var s = "  hi  "
  shout(s)
  println("string [${s}]")
  var t = "x"
  for i in 0..<3 {
    tag(t, i)
  }
  println("t=${t} len=${string.len(t)}")
}
"#;

#[test]
fn a_var_bound_to_a_string_literal_is_lent_and_read_back_after_the_call() {
    // The render reads the lowered fns' own in-out params: a lent literal is
    // rebound by the call, never printed as the literal it started at.
    let out = render(LITERAL).expect("renders");
    for name in ["shout", "tag"] {
        assert!(header_of(&out, name).contains("&mut AlmideString"), "{name} borrows the caller's string:\n{out}");
    }
    assert_eq!(run_native(LITERAL), "1a,2b\nstring [HI!]\nt=x012 len=4\n");
}

const REBIND: &str = r#"
fn adds(mut s: Set[Int], x: Int) -> Unit = { s = set.insert(s, x) }

fn puts(mut m: Map[String, Int], k: String, v: Int) -> Unit = { m = map.set(m, k, v) }

fn grows(mut xs: List[Int], x: Int) -> Unit = { xs = xs + [x] }

fn tags(mut s: String, n: Int) -> Unit = { s = s + int.to_string(n) }

fn maybe(mut s: Set[Int], x: Int) -> Unit = {
  if x % 3 == 0 then { s = set.insert(s, x) } else { s = set.remove(s, x - 1) }
}

effect fn main() -> Unit = {
  var s: Set[Int] = set.new()
  var m: Map[String, Int] = map.new()
  var xs: List[Int] = []
  var t = "t"
  for i in 0..<6 {
    adds(s, i)
    puts(m, int.to_string(i % 4), i)
    grows(xs, i * i)
    tags(t, i)
    maybe(s, i + 10)
  }
  println("${set.len(s)} ${set.contains(s, 12)} ${map.get(m, "1") ?? 0} ${list.len(xs)} ${list.get(xs, 5) ?? 0} ${t}")
}
"#;

#[test]
fn a_rebound_in_out_param_moves_its_old_value_into_the_op() {
    // `p = op(p, ..)` through `&mut`: the old value leaves the caller's place
    // (`std::mem::take`) into the op's by-value form and the rebind writes the
    // result back — never a whole copy per rebind (#4582 on this render).
    let out = render(REBIND).expect("renders");
    for name in ["adds", "puts", "grows", "tags", "maybe"] {
        let body = body_of(&out, name);
        assert!(header_of(&out, name).contains("&mut "), "{name} borrows the caller's buffer:\n{out}");
        assert!(!body.contains(".clone()"), "{name} copies nothing:\n{body}");
    }
    for name in ["adds", "puts", "maybe"] {
        let body = body_of(&out, name);
        assert!(body.contains("(std::mem::take(v0)"), "{name} takes the old value out of the place:\n{body}");
    }
    assert!(body_of(&out, "adds").contains("almide_rt_set_insert_owned(std::mem::take(v0)"), "{out}");
    assert_eq!(run_native(REBIND), "7 false 5 6 25 t012345\n");
}

const RECORD: &str = r#"
type Budget = { left: Int, depth: Int, exhausted: Bool, memo: Map[Int, String] }

fn spend(mut b: Budget) -> Bool =
  if b.exhausted or b.left <= 0 then {
    b.exhausted = true
    false
  } else {
    b.left = b.left - 1
    true
  }

fn enter(mut b: Budget, max: Int) -> Bool =
  if not spend(b) then false
  else if b.depth >= max then {
    b.exhausted = true
    false
  } else {
    b.depth = b.depth + 1
    true
  }

fn leave(mut b: Budget) -> Unit = {
  b.depth = b.depth - 1
}

effect fn main() -> Unit = {
  var b = { left: 50, depth: 0, exhausted: false, memo: map.set(map.new(), 1, "one") }
  var n = 0
  for i in 0..<40 {
    if enter(b, 3) then { n = n + 1 } else { leave(b) }
  }
  println("${n} ${b.left} ${b.depth} ${b.exhausted} ${map.len(b.memo)}")
}
"#;

#[test]
fn a_mut_record_param_is_lent_and_its_scalar_fields_written_in_place() {
    // A record `mut` param is the caller's place behind `&mut`, as a buffer
    // is: a scalar field write stores into it, the record is never rebuilt.
    let out = render(RECORD).expect("renders");
    for name in ["spend", "enter", "leave"] {
        assert!(header_of(&out, name).contains("&mut AlmdRec"), "{name} borrows the caller's record:\n{out}");
        assert!(!body_of(&out, name).contains(".clone()"), "{name} copies nothing:\n{out}");
    }
    assert_eq!(run_native(RECORD), "3 46 -34 true 1\n");
}

const RECORD_HEAP: &str = r#"
type B = { left: Int, memo: Map[Int, Int], trail: List[Int] }

fn count(n: Int, mut b: B) -> Int =
  if n <= 0 then 0
  else if b.left <= 0 then -1
  else {
    b.left = b.left - 1
    b.trail = b.trail + [n]
    let sub = count(n - 1, b)
    b.memo = map.set(b.memo, n, sub)
    sub + 1
  }

fn two(mut xs: List[Int], mut b: B, x: Int) -> Unit = {
  xs = xs + [x]
  if x % 2 == 0 then { b.trail = b.trail + [x * 10] } else { b.left = b.left + x }
}

fn early(mut b: B, k: Int) -> Int = {
  if k > 3 then { b.left = 0 } else { b.memo = map.set(b.memo, k, k) }
  b.left
}

effect fn main() -> Unit = {
  var b = { left: 4, memo: map.new(), trail: [] }
  let c = count(6, b)
  var xs: List[Int] = []
  for i in 0..<5 { two(xs, b, i) }
  var e = { left: 9, memo: map.new(), trail: [1] }
  let r1 = early(e, 2)
  let r2 = early(e, 5)
  println("${c} ${b.left} ${map.len(b.memo)} ${list.len(b.trail)} ${list.len(xs)} ${r1} ${r2} ${map.len(e.memo)}")
}
"#;

#[test]
fn a_mut_record_param_moves_a_rebound_heap_field_out_of_the_place() {
    // `b.f = op(b.f, ..)` on a lent record: the field leaves the caller's
    // record into the op and the rest moves into the rebuilt record, which
    // `*p = ..` writes back; no field is copied. Several in-out params and
    // a non-tail self call lend the places on.
    let out = render(RECORD_HEAP).expect("renders");
    for name in ["count", "two", "early"] {
        assert!(header_of(&out, name).contains("&mut AlmdRec"), "{name} borrows the caller's record:\n{out}");
        let body = body_of(&out, name);
        assert!(!body.contains(".clone()") && !body.contains(".to_vec()"), "{name} copies no field:\n{body}");
    }
    assert!(header_of(&out, "two").contains("&mut Vec<i64>"), "{out}");
    assert_eq!(run_native(RECORD_HEAP), "3 4 4 7 5 9 0 1\n");
}

const FIELD_LEND: &str = r#"
type Budget = { left: Int, seen: List[Int] }
type St = { name: String, budget: Budget, log: List[String] }

fn spend(mut b: Budget, x: Int) -> Bool =
  if b.left <= 0 then false
  else {
    b.left = b.left - 1
    b.seen = b.seen + [x]
    true
  }

fn step(mut st: St, x: Int) -> Unit = {
  if spend(st.budget, x) then { st.log = st.log + ["ok ${x}"] } else { st.log = st.log + ["no ${x}"] }
}

effect fn main() -> Unit = {
  var st = { name: "s", budget: { left: 3, seen: [] }, log: [] }
  for i in 0..<5 { step(st, i) }
  var b = { left: 1, seen: [9] }
  let got = spend(b, 7)
  var w = { name: "w", budget: { left: 2, seen: [5] }, log: ["start"] }
  let w1 = spend(w.budget, 8)
  let w2 = spend(w.budget, 9)
  let w3 = spend(w.budget, 10)
  println("${st.budget.left} ${list.len(st.budget.seen)} ${list.join(st.log, ",")} ${got} ${b.left} ${list.len(b.seen)} ${w1} ${w2} ${w3} ${w.budget.left} ${list.len(w.budget.seen)} ${list.len(w.log)}")
}
"#;

#[test]
fn a_record_field_passed_to_a_mut_param_lends_its_place() {
    // `spend(st.budget, x)` writes the caller's field: the call lends the
    // field's place (`&mut v0.f1`), never a copy whose writes would be lost.
    let out = render(FIELD_LEND).expect("renders");
    assert!(header_of(&out, "spend").contains("&mut AlmdRec"), "{out}");
    let step = body_of(&out, "step");
    assert!(step.contains("almd_spend(&mut v0.f"), "step lends its record's field:\n{step}");
    assert!(!step.contains(".clone()"), "{step}");
    assert!(out.contains("almd_spend(&mut v"), "{out}");
    assert_eq!(run_native(FIELD_LEND), "0 3 ok 0,ok 1,ok 2,no 3,no 4 true 0 2 true true false 0 3 1\n");
}

const CLOSURE_READ: &str = r#"
type St = { metas: List[Int], log: List[String], n: Int }

fn zonk(st: St, t: Int) -> Int = t + list.len(st.metas)

fn new_meta(mut st: St, k: Int) -> Int = {
  st.metas = st.metas + [k]
  st.n = st.n + 1
  list.len(st.metas)
}

fn settle(mut st: St, ctx: List[Int]) -> Int = {
  let z = ctx |> list.map((t) => zonk(st, t))
  if list.any(z, (t) => t > zonk(st, 100)) then new_meta(st, 7) else new_meta(st, 3)
}

fn by_simp(mut st: St, ctx: List[Int]) -> Int = {
  let a = settle(st, ctx)
  let zs = ctx |> list.map((t) => zonk(st, t) + st.n)
  st.log = st.log + ["z ${list.len(zs)}"]
  a + list.fold(zs, 0, (acc, x) => acc + x)
}

fn checked(mut st: St, ctx: List[Int]) -> Int! = {
  let bad = list.filter(ctx, (t) => zonk(st, t) > 50)
  guard list.is_empty(bad) else err("bad ${list.len(bad)} at ${st.n}")
  ok(new_meta(st, list.len(ctx)))
}

effect fn main() -> Unit = {
  var st = { metas: [1, 2], log: [], n: 0 }
  let r = by_simp(st, [5, 200, 9])
  let r2 = by_simp(st, [1])
  let c1 = checked(st, [1, 2]) ?? -1
  let c2 = checked(st, [1, 99]) ?? -2
  println("${r} ${r2} ${c1} ${c2} ${st.n} ${list.len(st.metas)} ${list.join(st.log, ",")}")
}
"#;

#[test]
fn a_closure_that_only_reads_a_mut_param_for_one_call_keeps_it_lent() {
    // `ctx |> list.map((t) => zonk(st, t))`: the closure reads the place and
    // dies with the call, so `st` stays lent `&mut` and is lent on after it.
    let out = render(CLOSURE_READ).expect("renders");
    for name in ["new_meta", "settle", "by_simp", "checked"] {
        assert!(header_of(&out, name).contains("&mut AlmdRec"), "{name} borrows the caller's record:\n{out}");
    }
    assert!(body_of(&out, "settle").contains("almd_new_meta(&mut *v0"), "{out}");
    assert_eq!(run_native(CLOSURE_READ), "229 11 5 -2 3 5 z 3,z 1\n");
}
