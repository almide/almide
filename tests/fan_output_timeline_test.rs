//! ADR-0024 D5/D6 (ADR-0011 D1/D5): a `fan` whose elements run on threads is
//! observed as the sequential evaluation, on EVERY run. Each element's stdout
//! and stderr go to its own timeline, flushed in list order; a trap in
//! element k waits for the elements below it and shows nothing above it.
//!
//! The defect this pins is invisible in one run: before the timelines, three
//! printing arms came out `arm 1, arm 3, arm 2` in some runs and not others
//! (#2594), and C-200's trap fixture showed element 2's line in about 11 of
//! 20 runs. So every program here is built ONCE and run `RUNS` times, and
//! every run must equal the sequential observation byte for byte — stdout,
//! stderr and exit code. The sleeps make the later elements finish FIRST, the
//! order a buffer-less substrate gets wrong. The `fan.map` cases run a second
//! time with `ALMIDE_FAN_SEQUENTIAL=1` (the forced-sequential substrate) and
//! must not change.

use std::process::Command;

const RUNS: usize = 30;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let release = root.join("target/release/almide");
    if release.exists() {
        return release.to_string_lossy().into_owned();
    }
    env!("CARGO_BIN_EXE_almide").to_string()
}

/// Arms finish in reverse order; each prints on both streams.
const BLOCK: &str = r#"import env

effect fn arm(n: Int, ms: Int) -> Result[Int, String] = {
  println("arm ${n} start")
  env.sleep_ms(ms)
  eprintln("arm ${n} note")
  println("arm ${n} end")
  ok(n)
}

effect fn main() -> Unit = {
  println("before")
  let (a, b, c) = fan {
    arm(1, 60)
    arm(2, 30)
    arm(3, 0)
  }
  println("after ${a + b + c}")
}
"#;

/// A nested fan flushes into the element that started it.
const NESTED: &str = r#"import env

effect fn leaf(name: String, ms: Int) -> Result[Int, String] = {
  println("${name} begin")
  env.sleep_ms(ms)
  println("${name} end")
  ok(ms)
}

effect fn inner(tag: String, ms: Int) -> Result[Int, String] = {
  println("${tag} open")
  let (x, y) = fan {
    leaf(tag + ".a", ms)
    leaf(tag + ".b", 0)
  }
  println("${tag} close")
  ok(x + y)
}

effect fn main() -> Unit = {
  let (p, q, r) = fan {
    inner("A", 50)
    inner("B", 0)
    leaf("C", 0)
  }
  println("after ${p + q + r}")
}
"#;

/// Element 2 panics at once; element 1 traps later. The LOWER trap wins.
const LOWER_TRAP_WINS: &str = r#"import env

effect fn e0() -> Result[Int, String] = {
  println("e0 out")
  eprintln("e0 err")
  ok(0)
}

effect fn e1() -> Result[Int, String] = {
  println("e1 start")
  env.sleep_ms(80)
  let zero = int.parse("0") ?? 1
  ok(1 / zero)
}

effect fn e2() -> Result[Int, String] = {
  println("e2 out")
  panic("e2 panics")
  ok(2)
}

effect fn main() -> Unit = {
  let (a, b, c) = fan {
    e0()
    e1()
    e2()
  }
  println("unreachable ${a} ${b} ${c}")
}
"#;

/// A failed assert outside a test is `eprintln` + `process.exit(1)`: the exit
/// waits for element 0 like a trap does.
const ASSERT_IN_ELEMENT: &str = r#"import env

effect fn e(n: Int, ms: Int) -> Result[Int, String] = {
  println("e${n} start")
  env.sleep_ms(ms)
  assert(n != 1)
  println("e${n} end")
  ok(n)
}

effect fn main() -> Unit = {
  let (a, b, c) = fan {
    e(0, 60)
    e(1, 0)
    e(2, 0)
  }
  println("unreachable ${a} ${b} ${c}")
}
"#;

/// The parallel `fan.map` (#2044) runs every element (ADR-0024 D1): in the
/// first map the slow Err at 2 still wins over the fast Err at 9, and in the
/// second a trap ABOVE the Err is reached, exactly as the sequential
/// evaluation reaches it, and aborts after the first map's output.
const MAP_PAR: &str = r#"fn churn(x: Int, n: Int) -> Int = {
  var acc = 0
  var i = 0
  while i < n {
    acc = (acc + x * i) % 1000003
    i = i + 1
  }
  acc
}

effect fn main() -> Unit = {
  println("before")
  let r = fan.map(list.range(0, 16), (x) => if x == 2 then err("bad ${churn(x, 2000000)}") else if x == 9 then err("e9") else ok(x))
  match r {
    ok(v) => println("ok ${list.len(v)}"),
    err(e) => println("err ${e}"),
  }
  let t = fan.map(list.range(0, 16), (x) => if x == 2 then err("bad ${churn(x, 2000000)}") else ok(100 / (x - 9)))
  match t {
    ok(v) => println("ok ${list.len(v)}"),
    err(e) => println("err ${e}"),
  }
  println("after")
}
"#;

struct Case {
    name: &'static str,
    src: Source,
    stdout: &'static str,
    stderr: &'static str,
    code: i32,
    force_sequential_too: bool,
}

enum Source {
    Inline(&'static str),
    Fixture(&'static str),
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "block",
            src: Source::Inline(BLOCK),
            stdout: "before\narm 1 start\narm 1 end\narm 2 start\narm 2 end\narm 3 start\narm 3 end\nafter 6\n",
            stderr: "arm 1 note\narm 2 note\narm 3 note\n",
            code: 0,
            force_sequential_too: false,
        },
        Case {
            name: "nested",
            src: Source::Inline(NESTED),
            stdout: "A open\nA.a begin\nA.a end\nA.b begin\nA.b end\nA close\nB open\nB.a begin\nB.a end\nB.b begin\nB.b end\nB close\nC begin\nC end\nafter 50\n",
            stderr: "",
            code: 0,
            force_sequential_too: false,
        },
        Case {
            name: "trap_waits_for_elements_below",
            src: Source::Fixture("spec/wasm_cross/fan_trap_waits_for_elements_below.almd"),
            stdout: "element 0\n",
            stderr: "Error: division by zero\n",
            code: 1,
            force_sequential_too: false,
        },
        Case {
            name: "lower_trap_wins",
            src: Source::Inline(LOWER_TRAP_WINS),
            stdout: "e0 out\ne1 start\n",
            stderr: "e0 err\nError: division by zero\n",
            code: 1,
            force_sequential_too: false,
        },
        Case {
            name: "assert_in_element",
            src: Source::Inline(ASSERT_IN_ELEMENT),
            stdout: "e0 start\ne0 end\ne1 start\n",
            stderr: "Error: assertion failed\n  at: line 6\n",
            code: 1,
            force_sequential_too: false,
        },
        Case {
            name: "map_par",
            src: Source::Inline(MAP_PAR),
            stdout: "before\nerr bad 42\n",
            stderr: "Error: division by zero\n",
            code: 1,
            force_sequential_too: true,
        },
    ]
}

fn build(dir: &std::path::Path, case: &Case) -> std::path::PathBuf {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let src = match case.src {
        Source::Inline(text) => {
            let p = dir.join(format!("{}.almd", case.name));
            std::fs::write(&p, text).expect("write program");
            p
        }
        Source::Fixture(rel) => root.join(rel),
    };
    let bin = dir.join(case.name);
    let out = Command::new(almide_bin())
        .arg("build")
        .arg(&src)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("spawn almide build");
    assert!(
        out.status.success(),
        "{}: build failed\n{}",
        case.name,
        String::from_utf8_lossy(&out.stderr)
    );
    bin
}

fn run_repeatedly(case: &Case, bin: &std::path::Path, sequential: bool) -> Vec<String> {
    let mut deviations = Vec::new();
    for i in 0..RUNS {
        let mut cmd = Command::new(bin);
        if sequential {
            cmd.env("ALMIDE_FAN_SEQUENTIAL", "1");
        }
        let out = cmd.output().expect("spawn program");
        let (o, e) = (String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        if o != case.stdout || e != case.stderr || out.status.code() != Some(case.code) {
            deviations.push(format!(
                "{} run {i}{}: exit {:?}\n--- stdout\n{o}--- stderr\n{e}",
                case.name,
                if sequential { " (ALMIDE_FAN_SEQUENTIAL=1)" } else { "" },
                out.status.code()
            ));
        }
    }
    deviations
}

#[test]
fn fan_output_is_the_sequential_observation_on_every_run() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut deviations = Vec::new();
    for case in cases() {
        let bin = build(dir.path(), &case);
        deviations.extend(run_repeatedly(&case, &bin, false));
        if case.force_sequential_too {
            deviations.extend(run_repeatedly(&case, &bin, true));
        }
    }
    assert!(
        deviations.is_empty(),
        "{} run(s) differ from the sequential observation:\n{}",
        deviations.len(),
        deviations.join("\n")
    );
}
