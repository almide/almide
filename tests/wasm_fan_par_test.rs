//! #3003 stage 1 (ADR-0011 §D2a): a pure scalar `fan` chunk map runs on
//! separate instances of the module on the embedded host, and on every
//! other host exactly as before (sequentially in its own instance). The
//! observation never depends on which happened.
//!
//! Each program runs natively, on the embedded host (`almide run --target
//! wasm`), and — when wasmtime is on PATH — as the stock artifact `almide
//! build --target wasm` writes; all three must print the same bytes and exit
//! the same way. `ALMIDE_DBG_FAN` names the route the embedded host took, so
//! the test also pins WHICH programs are offered and served.
use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

fn run(dir: &std::path::Path, extra: &[&str], dbg: bool) -> Run {
    let mut cmd = Command::new(almide());
    cmd.current_dir(dir).args(["run", "m.almd"]).args(extra);
    if dbg {
        // the parallel route is what these tests pin: the cost model (#3341)
        // would keep their small chunks sequential
        cmd.env("ALMIDE_DBG_FAN", "1").env("ALMIDE_FAN_COST_OFF", "1");
    }
    let out = cmd.output().expect("spawn almide");
    Run {
        code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// Native, embedded wasm and the stock artifact agree; returns the
/// embedded run's debug stderr (the route it took).
fn same_on_every_leg(src: &str) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("m.almd"), src).expect("write");
    let native = run(dir.path(), &[], false);
    let wasm = run(dir.path(), &["--target", "wasm"], false);
    assert_eq!(wasm.stdout, native.stdout, "embedded stdout\nnative stderr: {}\nwasm stderr: {}", native.stderr, wasm.stderr);
    assert_eq!(wasm.code, native.code, "embedded exit code; wasm stderr: {}", wasm.stderr);
    assert_eq!(wasm.stderr, native.stderr, "embedded stderr");
    let artifact = dir.path().join("m.wasm");
    let built = Command::new(almide())
        .current_dir(dir.path())
        .args(["build", "m.almd", "--target", "wasm", "-o", artifact.to_str().unwrap()])
        .output()
        .expect("spawn almide");
    assert!(built.status.success(), "stock build: {}", String::from_utf8_lossy(&built.stderr));
    if Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success()) {
        let stock = Command::new("wasmtime").arg(&artifact).output().expect("wasmtime");
        assert_eq!(String::from_utf8_lossy(&stock.stdout), native.stdout, "stock artifact stdout");
        assert_eq!(stock.status.code(), native.code, "stock artifact exit code");
    }
    run(dir.path(), &["--target", "wasm"], true).stderr
}

#[test]
fn a_tuple_chunk_map_is_served_on_separate_instances() {
    let src = r#"fn work(i: Int, k: Int) -> (Int, Float, Bool) = {
  var acc = 0
  for j in 0..<(1000 + i) {
    acc = acc + (j * k) % 7
  }
  (acc, int.to_float(acc) / 2.0, acc % 2 == 0)
}

effect fn main() -> Unit = {
  let k = 3
  let rs = fan {
    list.map(list.range(0, 12), (i) => work(i, k))
  }
  for r in rs {
    let (a, b, c) = r
    println("${a} ${b} ${c}")
  }
}
"#;
    let route = same_on_every_leg(src);
    assert!(route.contains("__fan_site_0: served on separate instances (12 elements"), "{route}");
}

#[test]
fn a_scalar_chunk_map_with_float_and_bool_captures_is_served() {
    let src = r#"effect fn main() -> Unit = {
  let scale = 1.5
  let flip = true
  let xs = list.range(0, 9)
  let ys = fan {
    list.map(xs, (x) => if flip then int.to_float(x) * scale else 0.0)
  }
  println(list.join(list.map(ys, (y) => float.to_string(y)), ","))
}
"#;
    let route = same_on_every_leg(src);
    assert!(route.contains("served on separate instances (9 elements"), "{route}");
}

#[test]
fn a_chunk_that_aborts_falls_back_and_aborts_like_native() {
    // Element 5 indexes past the end: the child traps, the host answers
    // "not served", and the sequential run aborts exactly as native does.
    let src = r#"fn pick(i: Int) -> Int = {
  let xs = [10, 20, 30, 40, 50]
  xs[i]
}

effect fn main() -> Unit = {
  println("start")
  let rs = fan {
    list.map(list.range(0, 6), (i) => pick(i))
  }
  println(int.to_string(list.len(rs)))
}
"#;
    let route = same_on_every_leg(src);
    assert!(route.contains("not served (a chunk failed), sequential"), "{route}");
}

#[test]
fn an_effectful_chunk_is_not_offered() {
    // A chunk that prints keeps its side effects in list order: it is never
    // moved to another instance.
    let src = r#"effect fn main() -> Unit = {
  let rs = fan {
    list.map(list.range(0, 4), (i) => {
      println("elem ${i}")
      i * 2
    })
  }
  println(int.to_string(list.len(rs)))
}
"#;
    let route = same_on_every_leg(src);
    assert!(!route.contains("instance-parallel offer"), "{route}");
}

#[test]
fn a_chunk_reading_a_top_level_let_is_not_offered() {
    // A child instance never runs `main`, so a top-level let is unset there.
    let src = r#"let BASE = 100

effect fn main() -> Unit = {
  let rs = fan {
    list.map(list.range(0, 4), (i) => i + BASE)
  }
  println(int.to_string(list.len(rs)))
}
"#;
    let route = same_on_every_leg(src);
    assert!(!route.contains("instance-parallel offer"), "{route}");
}

#[test]
fn a_chunk_map_in_a_sibling_module_is_served() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("almide.toml"), "[package]\nname = \"app\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(
        dir.path().join("src/work.almd"),
        "fn sq(i: Int) -> Int = i * i\n\neffect fn squares(n: Int) -> List[Int] = fan {\n  list.map(list.range(0, n), (i) => sq(i))\n}\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("src/main.almd"),
        "import self.work\neffect fn main() -> Unit = {\n  let s = work.squares(6)!\n  println(list.join(list.map(s, (x) => int.to_string(x)), \",\"))\n}\n",
    )
    .unwrap();
    let go = |extra: &[&str], dbg: bool| {
        let mut cmd = Command::new(almide());
        cmd.current_dir(dir.path()).args(["run", "src/main.almd"]).args(extra);
        if dbg {
            cmd.env("ALMIDE_DBG_FAN", "1").env("ALMIDE_FAN_COST_OFF", "1");
        }
        cmd.output().expect("spawn almide")
    };
    let native = go(&[], false);
    let wasm = go(&["--target", "wasm"], true);
    assert_eq!(String::from_utf8_lossy(&native.stdout), "0,1,4,9,16,25\n");
    assert_eq!(wasm.stdout, native.stdout);
    let route = String::from_utf8_lossy(&wasm.stderr);
    assert!(route.contains("served on separate instances (6 elements"), "{route}");
}

const REUSE_SRC: &str = r#"fn sq(i: Int, k: Int) -> Int = {
  var acc = 0
  for j in 0..<(200 + i) {
    acc = acc + (j * k + i) % 11
  }
  acc
}

fn halve(i: Int) -> (Int, Float) = (i / 2, int.to_float(i) / 2.0)

fn pick(i: Int) -> Int = {
  let xs = [1, 2, 3]
  xs[i]
}

effect fn round(k: Int) -> Unit = {
  let a = fan {
    list.map(list.range(0, 9), (i) => sq(i, k))
  }
  let b = fan {
    list.map(list.range(0, 7), (i) => halve(i + k))
  }
  println(list.join(list.map(a, (x) => int.to_string(x)), ","))
  for p in b {
    let (q, r) = p
    println("${q} ${r}")
  }
}

effect fn main() -> Unit = {
  round(1)!
  round(2)!
  let picked = fan {
    list.map(list.range(0, 3), (i) => pick(i))
  }
  println(int.to_string(list.len(picked)))
  round(3)!
}
"#;

#[test]
fn pooled_workers_serve_every_site_and_every_call() {
    // Two sites, three calls each, and between them an offer whose every
    // chunk succeeds: the same pooled workers serve all seven offers.
    let route = same_on_every_leg(REUSE_SRC);
    assert_eq!(route.matches("served on separate instances").count(), 7, "{route}");
}

#[test]
fn the_worker_cap_changes_nothing_observable_on_either_leg() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("m.almd"), REUSE_SRC).expect("write");
    let base = run(dir.path(), &[], false).stdout;
    assert_eq!(base.lines().count(), 25, "the program ran: {base}");
    for n in ["1", "2", "3"] {
        for extra in [&[][..], &["--target", "wasm"][..]] {
            let out = Command::new(almide())
                .current_dir(dir.path())
                .args(["run", "m.almd"])
                .args(extra)
                .env("ALMIDE_FAN_THREADS", n)
                .output()
                .expect("spawn almide");
            assert_eq!(String::from_utf8_lossy(&out.stdout), base, "ALMIDE_FAN_THREADS={n} {extra:?}");
        }
    }
}

#[test]
fn a_failed_offer_after_a_served_one_falls_back_like_native() {
    // The first offer's element 3 traps: its workers are dropped and the map
    // aborts sequentially, like native. Nothing is offered after the abort,
    // so this pins the abort path with workers in the pool from an earlier
    // offer.
    let src = r#"fn sq(i: Int) -> Int = i * i

fn pick(i: Int) -> Int = {
  let xs = [10, 20, 30]
  xs[i]
}

effect fn main() -> Unit = {
  let a = fan {
    list.map(list.range(0, 8), (i) => sq(i))
  }
  println(int.to_string(list.len(a)))
  let b = fan {
    list.map(list.range(0, 5), (i) => pick(i))
  }
  println(int.to_string(list.len(b)))
}
"#;
    let route = same_on_every_leg(src);
    assert!(route.contains("served on separate instances (8 elements"), "{route}");
    assert!(route.contains("not served (a chunk failed), sequential"), "{route}");
}

/// The route the cost model picks on its own (no ablation): the embedded
/// run's debug stderr, after checking native and wasm agree with and
/// without `ALMIDE_FAN_COST_OFF`.
fn route_by_cost_model(src: &str, args: &[&str]) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("m.almd"), src).expect("write");
    let go = |wasm: bool, off: bool, dbg: bool| {
        let mut cmd = Command::new(almide());
        cmd.current_dir(dir.path()).args(["run", "m.almd"]);
        if wasm {
            cmd.args(["--target", "wasm"]);
        }
        cmd.arg("--").args(args);
        if off {
            cmd.env("ALMIDE_FAN_COST_OFF", "1");
        }
        if dbg {
            cmd.env("ALMIDE_DBG_FAN", "1");
        }
        cmd.output().expect("spawn almide")
    };
    let native = go(false, false, false);
    assert!(native.status.success(), "{}", String::from_utf8_lossy(&native.stderr));
    for (wasm, off) in [(false, true), (true, false), (true, true)] {
        let o = go(wasm, off, false);
        assert_eq!(o.stdout, native.stdout, "wasm={wasm} cost_off={off}");
        assert_eq!(o.status.code(), native.status.code(), "wasm={wasm} cost_off={off}");
    }
    String::from_utf8_lossy(&go(true, false, true).stderr).into_owned()
}

const COST_SRC: &str = r#"import env

fn work(i: Int, iters: Int) -> Int = {
  var acc = 0
  for j in 0..<iters {
    acc = (acc + j * (i + 1)) % 1000003
  }
  acc
}

effect fn main() -> Unit = {
  let iters = int.parse(list.get(env.args(), 0) ?? "10") ?? 10
  for k in 0..<3 {
    let rs = fan {
      list.map(list.range(0, 8), (i) => work(i + k, iters))
    }
    println(list.join(list.map(rs, (x) => int.to_string(x)), ","))
  }
}
"#;

#[test]
fn the_cost_model_keeps_a_cheap_chunk_map_sequential() {
    // ~10 iterations per element: far below the measured break-even. The
    // site's first offer goes parallel (no measurement yet); the next two
    // know better.
    let route = route_by_cost_model(COST_SRC, &["10"]);
    let lines: Vec<&str> = route.lines().filter(|l| l.contains("__fan_site_0: served")).collect();
    assert_eq!(lines.len(), 3, "{route}");
    assert!(lines[0].contains("separate instances (8 elements"), "{route}");
    assert!(lines[1..].iter().all(|l| l.contains("the cost model kept it sequential (8 elements)")), "{route}");
}

#[test]
fn the_cost_model_runs_an_expensive_chunk_map_in_parallel() {
    // ~3M iterations per element (milliseconds): far above the break-even,
    // on any machine that has two cores.
    if std::thread::available_parallelism().map_or(1, |p| p.get()) < 2 {
        return;
    }
    let route = route_by_cost_model(COST_SRC, &["3000000"]);
    assert_eq!(route.matches("served on separate instances (8 elements").count(), 3, "{route}");
}
