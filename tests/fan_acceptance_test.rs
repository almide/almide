//! Fan acceptance tests (docs/design/fan-semantics-audit.md): pin what the
//! seven fan forms promise TODAY, on four legs: native as compiled (the cost
//! model decides), native forced parallel (`ALMIDE_FAN_COST_OFF=1`), native
//! sequential (`ALMIDE_FAN_SEQUENTIAL=1`) and wasm. Every leg must print the
//! same stdout, and that stdout is pinned verbatim.
//!
//! Nothing here depends on wall time: elements are made slow or fast to force
//! completion order away from input order, but no expected line names a
//! completion order. The `#[ignore]`d tests at the end are the known
//! divergences the audit found; each names its issue and passes once that
//! issue is fixed. Promises that only an ADR-0028 decision can make are in
//! that ADR's test plan, not here.
use std::process::Command;

fn almide(file: &std::path::Path, args: &[&str], env: Option<(&str, &str)>) -> (String, String, i32) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.args(args.iter().take(1)).arg(file).args(args.iter().skip(1));
    if let Some((k, v)) = env {
        c.env(k, v);
    }
    let out = c.output().expect("almide");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

fn write(name: &str, source: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join(name);
    std::fs::write(&file, source).expect("source");
    (dir, file)
}

/// Run on all four legs and require byte-identical stdout equal to `expected`.
fn four_legs(name: &str, source: &str, expected: &str) {
    let (_dir, file) = write(name, source);
    let legs: [(&str, &[&str], Option<(&str, &str)>); 4] = [
        ("native", &["run"], None),
        ("native forced parallel", &["run"], Some(("ALMIDE_FAN_COST_OFF", "1"))),
        ("native sequential", &["run"], Some(("ALMIDE_FAN_SEQUENTIAL", "1"))),
        ("wasm", &["run", "--target", "wasm"], None),
    ];
    for (leg, args, env) in legs {
        let (out, err, code) = almide(&file, args, env);
        assert_eq!(code, 0, "{name} [{leg}] exit {code}: {err}");
        assert_eq!(out, expected, "{name} [{leg}] stdout");
    }
}

const HEAVY: &str = r#"fn heavy(n: Int) -> Int = {
  var acc = 0
  var i = 0
  while i < n { acc = (acc + i * i) % 1000003; i = i + 1 }
  acc
}
"#;

#[test]
fn results_and_output_follow_input_order_when_completion_order_is_reversed() {
    let src = format!(
        r#"import env
{HEAVY}
effect fn arm(name: String, ms: Int) -> Result[String, String] = {{
  env.sleep_ms(ms)
  println("end ${{name}}")
  ok(name)
}}

effect fn main() -> Unit = {{
  // A finishes last, C first
  let (a, b, c) = fan {{ arm("A", 120)!, arm("B", 60)!, arm("C", 0)! }}
  println("block ${{a}}${{b}}${{c}}")
  // pure parallel map: element 0 is by far the slowest
  let xs = fan.map(list.range(0, 8), (i) => ok(heavy(if i == 0 then 3000000 else 1000) % 97 + i * 1000))!
  println("map ${{xs}}")
  let ys = fan {{ list.map(list.range(0, 8), (i) => heavy(if i == 0 then 3000000 else 1000) % 97 + i) }}
  println("listmap ${{ys}}")
}}
"#
    );
    four_legs(
        "order.almd",
        &src,
        "end A\nend B\nend C\nblock ABC\n\
         map [36, 1050, 2050, 3050, 4050, 5050, 6050, 7050]\n\
         listmap [36, 51, 52, 53, 54, 55, 56, 57]\n",
    );
}

#[test]
fn the_lowest_index_error_is_returned_and_every_element_still_runs() {
    let src = format!(
        r#"import env
{HEAVY}
effect fn job(x: Int) -> Result[Int, String] = {{
  println("run ${{x}}")
  if x == 2 or x == 4 then err("e${{x}}") else ok(x * 10)
}}

effect fn slow_err(name: String, ms: Int) -> Result[Int, String] = {{
  env.sleep_ms(ms)
  println("ran ${{name}}")
  err("${{name}} err")
}}

effect fn ok_arm(name: String) -> Result[Int, String] = {{
  println("ran ${{name}}")
  ok(1)
}}

fn show(r: Result[List[Int], String]) -> Unit = match r {{
  ok(v) => println("ok ${{v}}"),
  err(e) => println("err ${{e}}"),
}}

effect fn block() -> Result[Int, String] = {{
  // A fails last in time, B first: A's error is returned, and C still runs
  let (a, b, c) = fan {{ slow_err("A", 120)!, slow_err("B", 0)!, ok_arm("C")! }}
  ok(a + b + c)
}}

effect fn main() -> Unit = {{
  show(fan.map(list.range(1, 6), (x) => job(x)))
  // pure parallel callback: index 3 fails slowly, index 6 fails at once
  show(fan.map(list.range(0, 8), (i) => if i == 3 then err("bad${{heavy(2000000) % 1 + i}}") else if i == 6 then err("bad${{i}}") else ok(heavy(1000))))
  match block() {{
    ok(v) => println("block ok ${{v}}"),
    err(e) => println("block err ${{e}}"),
  }}
  let s = fan.settle(list.range(1, 6), (x) => job(x))
  println("settle ${{list.len(s)}}")
}}
"#
    );
    four_legs(
        "errors.almd",
        &src,
        "run 1\nrun 2\nrun 3\nrun 4\nrun 5\nerr e2\n\
         err bad3\n\
         ran A\nran B\nran C\nblock err A err\n\
         run 1\nrun 2\nrun 3\nrun 4\nrun 5\nsettle 5\n",
    );
}

#[test]
fn any_runs_nothing_after_the_winner_and_race_picks_least_cost_then_input_order() {
    let src = format!(
        r#"{HEAVY}
effect fn probe(x: Int) -> Result[Int, String] = {{
  println("probe ${{x}}")
  if x < 0 then err("neg ${{x}}") else ok(x)
}}

effect fn arm(name: String, v: Int) -> Result[Int, String] = {{
  println("ran ${{name}}")
  if v < 0 then err(name) else ok(v)
}}

effect fn main() -> Unit = {{
  match fan.any([-1, -2, 5, 7, -3], (x) => probe(x)) {{
    ok(v) => println("any map ok ${{v}}"),
    err(e) => println("any map err ${{e}}"),
  }}
  match fan.any {{ arm("a", -1), arm("b", 20), arm("c", 30) }} {{
    ok(v) => println("any block ok ${{v}}"),
    err(e) => println("any block err ${{e}}"),
  }}
  // cheaper second arm, cheaper first arm, a tie
  let r1 = fan.race {{ heavy(5000) % 7 + 100, heavy(10) % 7 + 200 }} ?? -1
  let r2 = fan.race {{ heavy(10) % 7 + 100, heavy(5000) % 7 + 200 }} ?? -1
  let r3 = fan.race {{ heavy(10) % 7 + 100, heavy(10) % 7 + 200 }} ?? -1
  println("race ${{r1}} ${{r2}} ${{r3}}")
  // mapper: two cheapest elements tie, the earlier one wins
  let m1 = fan.race([5000, 10, 10], (n) => ok(heavy(n) % 7 + n)) ?? -1
  println("race map ${{m1}}")
  // an Err element disqualifies itself even though it is cheaper
  let m2 = fan.race([10, 5000], (n) => if n == 10 then err("no") else ok(n)) ?? -1
  println("race err-skip ${{m2}}")
}}
"#
    );
    four_legs(
        "anyrace.almd",
        &src,
        "probe -1\nprobe -2\nprobe 5\nany map ok 5\n\
         ran a\nran b\nany block ok 20\n\
         race 205 105 105\nrace map 15\nrace err-skip 5000\n",
    );
}

#[test]
fn empty_input_all_failed_and_over_budget_have_fixed_answers() {
    let src = format!(
        r#"{HEAVY}
effect fn neg(x: Int) -> Result[Int, String] = if x < 0 then err("neg") else ok(x)

fn show(tag: String, r: Result[Int, String]) -> Unit = match r {{
  ok(v) => println("${{tag}} ok ${{v}}"),
  err(e) => println("${{tag}} err ${{e}}"),
}}

fn show_list(tag: String, r: Result[List[Int], String]) -> Unit = match r {{
  ok(v) => println("${{tag}} ok ${{v}}"),
  err(e) => println("${{tag}} err ${{e}}"),
}}

effect fn main() -> Unit = {{
  let empty: List[Int] = []
  show_list("map empty", fan.map(empty, (x) => neg(x)))
  println("settle empty ${{list.len(fan.settle(empty, (x) => neg(x)))}}")
  show("any empty", fan.any(empty, (x) => neg(x)))
  show("any allfail", fan.any([-1, -2], (x) => neg(x)))
  show_list("map allfail", fan.map([-1, -2], (x) => neg(x)))
  show("race empty", fan.race(empty, (x) => ok(x)))
  show("race allfail", fan.race([1, 2], (x) => if x > 0 then err("x") else ok(x)))
  // heavy(1000) spends 1002 units = 3006 ns
  show("bounded at", fan.bounded(compute.ns(3006)) {{ heavy(1000) }})
  show("bounded under", fan.bounded(compute.ns(3005)) {{ heavy(1000) }})
  show("race over budget", fan.race(compute.us(1)) {{ heavy(100000), heavy(200000) }})
  show("race budget admits", fan.race(compute.ns(3006)) {{ heavy(100000), heavy(1000) }})
}}
"#
    );
    four_legs(
        "edges.almd",
        &src,
        "map empty ok []\nsettle empty 0\n\
         any empty err fan.any: all candidates failed\n\
         any allfail err fan.any: all candidates failed\n\
         map allfail err neg\n\
         race empty err fan.race: no branch completed within budget\n\
         race allfail err fan.race: no branch completed within budget\n\
         bounded at ok 832504\n\
         bounded under err fan.bounded: budget exhausted\n\
         race over budget err fan.race: no branch completed within budget\n\
         race budget admits ok 832504\n",
    );
}

#[test]
fn effects_already_performed_are_kept_when_a_body_is_cut_or_an_element_fails() {
    let src = r#"fn loud(n: Int) -> Int = {
  var i = 0
  while i < n { println("iter"); i = i + 1 }
  i
}

effect fn job(x: Int) -> Result[Int, String] = {
  println("side effect ${x}")
  if x == 1 then err("one") else ok(x)
}

effect fn main() -> Unit = {
  // the cut stops the body at a charge site; its earlier output stays
  let b = fan.bounded(compute.ns(30)) { loud(100) } ?? -1
  println("bounded cut ${b}")
  match fan.map([0, 1, 2], (x) => job(x)) {
    ok(v) => println("map ok ${v}"),
    err(e) => println("map err ${e}"),
  }
}
"#;
    four_legs(
        "effects.almd",
        src,
        "iter\niter\niter\niter\niter\niter\niter\niter\niter\nbounded cut -1\n\
         side effect 0\nside effect 1\nside effect 2\nmap err one\n",
    );
}

/// `almide check` on both targets must refuse with `code`.
fn refused(name: &str, source: &str, code: &str) {
    let (_dir, file) = write(name, source);
    for args in [&["check"][..], &["check", "--target", "wasm"][..]] {
        let (out, err, status) = almide(&file, args, None);
        assert_ne!(status, 0, "{name} {args:?} must be refused");
        assert!(format!("{out}{err}").contains(code), "{name} {args:?}: expected {code}, got:\n{out}{err}");
    }
}

#[test]
fn effects_inside_race_bounded_and_timeout_are_refused() {
    let eff = "import env\neffect fn eff() -> Result[Int, String] = { env.sleep_ms(1)\n ok(1) }\n";
    refused("race_block.almd", &format!("{eff}effect fn main() -> Unit = {{\n  let r = fan.race {{ eff(), eff() }} ?? 0\n  println(\"${{r}}\")\n}}\n"), "E006");
    refused("race_map.almd", &format!("{eff}effect fn main() -> Unit = {{\n  let r = fan.race([1, 2], (x) => eff()) ?? 0\n  println(\"${{r}}\")\n}}\n"), "E006");
    refused("bounded.almd", &format!("{eff}effect fn main() -> Unit = {{\n  let r = fan.bounded(compute.ms(1)) {{ eff() }} ?? 0\n  println(\"${{r}}\")\n}}\n"), "E006");
    refused("timeout.almd", &format!("{eff}effect fn main() -> Unit = {{\n  let r = fan.timeout(duration.ms(1)) {{ eff() }} ?? 0\n  println(\"${{r}}\")\n}}\n"), "E006");
}

#[test]
fn fan_outside_an_effect_fn_and_a_captured_var_are_refused() {
    refused(
        "pure_ctx.almd",
        "fn f() -> Int = fan.race { 1, 2 } ?? 0\neffect fn main() -> Unit = println(\"${f()}\")\n",
        "E007",
    );
    refused(
        "var_capture.almd",
        "fn id(x: Int) -> Int = x\neffect fn main() -> Unit = {\n  var c = 1\n  let (a, b) = fan { id(c), id(2) }\n  c = 3\n  println(\"${a} ${b} ${c}\")\n}\n",
        "E008",
    );
}

// ---- known divergences (docs/design/fan-semantics-audit.md §Discrepancies) ----

#[test]
#[ignore = "#3778: the native meter charges a loop-free wrapper's entry, wasm does not"]
fn a_loop_free_wrapper_costs_the_same_on_every_leg() {
    let src = format!(
        "{HEAVY}fn wrap(n: Int) -> Int = heavy(n)\neffect fn main() -> Unit = {{\n  let b = fan.bounded(compute.ns(3006)) {{ wrap(1000) }} ?? -1\n  println(\"${{b}}\")\n}}\n"
    );
    four_legs("wrap.almd", &src, "832504\n");
}

#[test]
#[ignore = "#3778: the native meter charges string concatenation, wasm does not"]
fn a_cut_body_that_interpolates_stops_at_the_same_point_on_every_leg() {
    let src = "fn loud(n: Int) -> Int = {\n  var i = 0\n  while i < n { println(\"i ${i}\"); i = i + 1 }\n  i\n}\neffect fn main() -> Unit = {\n  let b = fan.bounded(compute.ns(30)) { loud(100) } ?? -1\n  println(\"cut ${b}\")\n}\n";
    four_legs("cut.almd", src, "i 0\ni 1\ni 2\ni 3\ni 4\ni 5\ni 6\ni 7\ni 8\ncut -1\n");
}

#[test]
#[ignore = "#3779: native fan { list.map } reports the first trap in time"]
fn a_trap_in_a_parallel_list_map_is_the_lowest_index_one() {
    let src = format!(
        "{HEAVY}fn elem(x: Int, z: Int) -> Int =\n  if x == 0 then {{ let big = heavy(30000000) + 5\n [1, 2][big] }}\n  else if x == 7 then 10 / z\n  else heavy(1000)\neffect fn main() -> Unit = {{\n  let z = list.len(list.range(0, 0))\n  let ys = fan {{ list.map(list.range(0, 8), (x) => elem(x, z)) }}\n  println(\"${{ys}}\")\n}}\n"
    );
    let (_dir, file) = write("trap.almd", &src);
    for (args, env) in [(&["run"][..], None), (&["run"][..], Some(("ALMIDE_FAN_SEQUENTIAL", "1"))), (&["run", "--target", "wasm"][..], None)] {
        let (_, err, code) = almide(&file, args, env);
        assert_ne!(code, 0);
        assert!(err.contains("index out of bounds"), "{args:?} {env:?}: {err}");
    }
}

#[test]
#[ignore = "#3780: native runs fan arms on threads even when they share process state"]
fn fan_arms_that_share_process_state_agree_across_targets() {
    let src = "import env\neffect fn reader() -> Result[String, String] = {\n  env.sleep_ms(300)\n  ok(env.get(\"FAN_ACC_KEY\") ?? \"unset\")\n}\neffect fn writer() -> Result[String, String] = {\n  env.set(\"FAN_ACC_KEY\", \"written\")\n  ok(\"w\")\n}\neffect fn main() -> Unit = {\n  let (r, w) = fan { reader()!, writer()! }\n  println(\"${r} ${w}\")\n}\n";
    four_legs("env.almd", src, "unset w\n");
}
