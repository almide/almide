//! The negative-count rule (#4147), held over its whole family by
//! proofs/negative-count-family.toml.
//!
//! A negative count used to mean "everything" in `list.take` / `string.take`
//! (the `n as usize` cast) and "nothing" in `drop` / `slice`, while
//! `bytes.take` aborted, so `rows |> list.take(limit)` with `?limit=-1`
//! returned every row. One rule now holds for every count-taking function of
//! the sequence modules: a negative count is 0, a range bound clamps into
//! [0, len], and a size (chunk, window) aborts at 0 and below.
//!
//! Two checks:
//! 1. The population — every `Int` parameter of every public fn of list,
//!    string, bytes and matrix, read from `almide compile <m> --json` — equals
//!    the ledger's rows, so a new count-taking function cannot escape the rule
//!    and a stale row cannot linger.
//! 2. Every `[[cell]]` runs with N = 0, -1 and i64::MIN on native, on the
//!    embedded wasm host and (when wasmtime is on PATH) on a stock run of the
//!    built artifact: a count or bound cell prints the same line for every N
//!    on every leg; a size cell aborts with exit 1 and "must be positive" on
//!    every leg for every N.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

const MODULES: &[&str] = &["list", "string", "bytes", "matrix"];
const NS: &[(&str, &str)] = &[("0", "0"), ("-1", "(0 - 1)"), ("min", "(0 - 9223372036854775807 - 1)")];

const HELPERS: &str = r#"import matrix

fn show_l(xs: List[Int]) -> String = "[" + list.join(list.map(xs, (x) => int.to_string(x)), ",") + "]"
fn show_f(xs: List[Float]) -> String = "[" + list.join(list.map(xs, (x) => float.to_string(x)), ",") + "]"
fn show_s(xs: List[String]) -> String = "[" + list.join(xs, ",") + "]"
fn show_ll(xs: List[List[Int]]) -> String = list.join(list.map(xs, (x) => show_l(x)), ";")
fn show_b(b: Bytes) -> String = show_l(bytes.to_list(b))
fn show_lp(p: (List[Int], List[Int])) -> String = show_l(p.0) + "|" + show_l(p.1)
fn show_bp(p: (Bytes, Bytes)) -> String = show_b(p.0) + "|" + show_b(p.1)
fn show_sp(p: (String, String)) -> String = p.0 + "|" + p.1
fn show_oa(o: List[Int]?) -> String = match o {
  some(xs) => "some" + show_l(xs),
  none => "none",
}
fn show_of(o: List[Float]?) -> String = match o {
  some(xs) => "some" + show_f(xs),
  none => "none",
}
fn show_shape(p: (Int, Int)) -> String = int.to_string(p.0) + "x" + int.to_string(p.1)
fn show_m(m: Matrix) -> String = show_shape(matrix.shape(m)) + " " + show_f(list.flat_map(matrix.to_lists(m), (r) => r))
fn cap_list(n: Int) -> List[Int] = list.with_capacity(n) + [1]
fn copy_from_probe(n: Int) -> Bytes = {
  var b = bytes.zeros(3)
  bytes.copy_from(b, bytes.from_list([7, 8, 9]), 0, 0, n)
  b
}
"#;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

struct Cell {
    slot: String,
    class: String,
    probe: String,
}

fn ledger() -> (Vec<Cell>, BTreeMap<String, String>) {
    let text = std::fs::read_to_string(root().join("proofs/negative-count-family.toml")).unwrap();
    let v: toml::Value = toml::from_str(&text).unwrap();
    let cells = v["cell"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| Cell {
            slot: c["slot"].as_str().unwrap().to_string(),
            class: c["class"].as_str().unwrap().to_string(),
            probe: c["probe"].as_str().unwrap().to_string(),
        })
        .collect::<Vec<_>>();
    for c in &cells {
        assert!(matches!(c.class.as_str(), "count" | "bound" | "size"), "{}: unknown class {}", c.slot, c.class);
        assert!(c.probe.contains('N'), "{}: the probe never uses N", c.slot);
    }
    let mut outside = BTreeMap::new();
    for (class, slots) in v["outside"].as_table().unwrap() {
        for s in slots.as_array().unwrap() {
            let s = s.as_str().unwrap().to_string();
            assert!(outside.insert(s.clone(), class.clone()).is_none(), "{s} is listed twice under [outside]");
        }
    }
    (cells, outside)
}

/// Every (module.fn.param) whose parameter type is `Int`, from the compiler's
/// own module interface.
fn population() -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for m in MODULES {
        let o = Command::new(almide()).args(["compile", m, "--json"]).current_dir(root()).output().unwrap();
        assert!(o.status.success(), "almide compile {m} --json failed:\n{}", String::from_utf8_lossy(&o.stderr));
        let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
        for f in v["functions"].as_array().unwrap() {
            let name = f["name"].as_str().unwrap();
            if name.starts_with('_') {
                continue;
            }
            for p in f["params"].as_array().unwrap() {
                if p["type"]["kind"].as_str() == Some("int") {
                    out.insert(format!("{m}.{name}.{}", p["name"].as_str().unwrap()));
                }
            }
        }
    }
    out
}

#[test]
fn every_int_parameter_of_the_sequence_modules_has_a_row() {
    let (cells, outside) = ledger();
    let pop = population();
    assert!(pop.len() > 100, "the population read from the interfaces is implausibly small: {}", pop.len());
    let mut rows = BTreeSet::new();
    for c in &cells {
        assert!(rows.insert(c.slot.clone()), "{} has two [[cell]] rows", c.slot);
    }
    for s in outside.keys() {
        assert!(rows.insert(s.clone()), "{s} is both a [[cell]] and listed under [outside]");
    }
    let missing: Vec<_> = pop.difference(&rows).collect();
    let stale: Vec<_> = rows.difference(&pop).collect();
    assert!(
        missing.is_empty(),
        "Int parameters with no row in proofs/negative-count-family.toml (add a [[cell]] if it is a count, \
         a range bound or a size, else list it under [outside] with its class): {missing:?}"
    );
    assert!(stale.is_empty(), "rows naming no Int parameter of the modules (delete them): {stale:?}");
}

fn run(file: &Path, wasm: bool) -> (Option<i32>, String, String) {
    let mut c = Command::new(almide());
    c.arg("run").arg(file);
    if wasm {
        c.args(["--target", "wasm"]);
    }
    let o = c.current_dir(root()).output().unwrap();
    (o.status.code(), String::from_utf8_lossy(&o.stdout).into_owned(), String::from_utf8_lossy(&o.stderr).into_owned())
}

fn stock(file: &Path, dir: &Path) -> (Option<i32>, String, String) {
    let wasm = dir.join(format!("{}.wasm", file.file_stem().unwrap().to_string_lossy()));
    let b = Command::new(almide())
        .args(["build", file.to_str().unwrap(), "--target", "wasm", "-o", wasm.to_str().unwrap()])
        .current_dir(root())
        .output()
        .unwrap();
    assert!(b.status.success(), "stock build of {} failed:\n{}", file.display(), String::from_utf8_lossy(&b.stderr));
    let o = Command::new("wasmtime").arg("run").arg(&wasm).output().unwrap();
    (o.status.code(), String::from_utf8_lossy(&o.stdout).into_owned(), String::from_utf8_lossy(&o.stderr).into_owned())
}

/// The line each (slot, N) printed, from one leg's stdout.
fn lines_of(out: &str) -> BTreeMap<(String, String), String> {
    out.lines()
        .filter_map(|l| {
            let mut it = l.splitn(3, ' ');
            let (slot, n, rest) = (it.next()?, it.next()?, it.next().unwrap_or(""));
            Some(((slot.to_string(), n.to_string()), rest.to_string()))
        })
        .collect()
}

#[test]
fn a_negative_count_or_bound_answers_what_zero_answers_on_every_leg() {
    let (cells, _) = ledger();
    let dir = tempfile::tempdir().unwrap();
    let mut body = String::new();
    let mut want = Vec::new();
    for c in cells.iter().filter(|c| c.class != "size") {
        for (lab, n) in NS {
            body.push_str(&format!("  println(\"{} {} \" + {})\n", c.slot, lab, c.probe.replace('N', n)));
            want.push((c.slot.clone(), lab.to_string()));
        }
    }
    let file = dir.path().join("negative_counts.almd");
    std::fs::write(&file, format!("{HELPERS}\neffect fn main() -> Unit = {{\n{body}}}\n")).unwrap();
    let mut legs = vec![("native", run(&file, false)), ("embedded wasm", run(&file, true))];
    if wasmtime_available() {
        legs.push(("stock wasm", stock(&file, dir.path())));
    } else {
        eprintln!("SKIP stock-wasm leg: wasmtime not on PATH — enforced on Linux CI");
    }
    let mut failures = Vec::new();
    let mut first: Option<BTreeMap<(String, String), String>> = None;
    for (leg, (code, out, err)) in &legs {
        assert_eq!(*code, Some(0), "{leg}: the probe program did not run cleanly:\n{err}");
        let got = lines_of(out);
        for k in &want {
            assert!(got.contains_key(k), "{leg}: no line for {k:?}");
        }
        for c in cells.iter().filter(|c| c.class != "size") {
            let zero = &got[&(c.slot.clone(), "0".to_string())];
            for lab in ["-1", "min"] {
                let v = &got[&(c.slot.clone(), lab.to_string())];
                if v != zero {
                    failures.push(format!("{leg}: {} ({}) N={lab} printed {v:?}, N=0 printed {zero:?}", c.slot, c.class));
                }
            }
        }
        match &first {
            None => first = Some(got),
            Some(f) => {
                for (k, v) in &got {
                    if f.get(k) != Some(v) {
                        failures.push(format!("{leg} differs from native at {k:?}: {v:?} vs {:?}", f.get(k)));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "the negative-count rule does not hold:\n{}", failures.join("\n"));
}

#[test]
fn a_size_of_zero_or_less_aborts_on_every_leg() {
    let (cells, _) = ledger();
    let dir = tempfile::tempdir().unwrap();
    let stock_leg = wasmtime_available();
    let mut failures = Vec::new();
    for c in cells.iter().filter(|c| c.class == "size") {
        for (lab, n) in NS {
            let file = dir.path().join(format!("{}_{}.almd", c.slot.replace('.', "_"), if *lab == "-1" { "m1" } else { lab }));
            std::fs::write(&file, format!("{HELPERS}\neffect fn main() -> Unit = println({})\n", c.probe.replace('N', n))).unwrap();
            let mut legs = vec![("native", run(&file, false)), ("embedded wasm", run(&file, true))];
            if stock_leg {
                legs.push(("stock wasm", stock(&file, dir.path())));
            }
            for (leg, (code, out, err)) in legs {
                if code != Some(1) || !err.contains("must be positive") {
                    failures.push(format!("{leg}: {} N={lab} exited {code:?}, stdout {out:?}, stderr {err:?}", c.slot));
                }
            }
        }
    }
    assert!(failures.is_empty(), "a size of 0 or less must abort with exit 1:\n{}", failures.join("\n"));
}
