//! C-066, the exits the corpus cannot carry (#3374, #3377).
//!
//! A `break` / `continue` out of a match arm that binds a named list rest
//! `[h, ..t]`, an enclosing arm that keeps its rest across an inner loop's
//! `break`, and a `break` or guard return out of a `for (k, v) in m` walk must
//! each release the rest block / the walk's hold on the map exactly once on
//! the structural wasm leg. The fixtures `spec/wasm_cross/leak_free_list_rest_exit.almd`
//! and `spec/wasm_cross/leak_free_map_walk_exit.almd` carry every other exit
//! shape (`!`, guard return out of an arm, recursive tail call, `continue`);
//! these four live here because the incumbent native MIR brick walls them (it
//! has no break / early-return control flow, #2739), and a corpus fixture
//! would have added rows to the shrink-only `proofs/walled-real-baseline.txt`.
//!
//! Each program is judged three ways, in ONE test (the witness sink is
//! process-global):
//!   1. the NATIVE binary (`almide build --target rust`) and the in-process
//!      structural wasm leg print the same stdout with the same exit code,
//!      and that stdout is the recorded one;
//!   2. the wasm module, emitted with the allocation counters armed, ends
//!      with `allocs == frees` and no block live (`allocs − frees −
//!      region_reclaimed == 0`) after a loop that drives the exit many times —
//!      a leak of one block per exit would read as a non-zero live count;
//!   3. every frame of the shipped emission pass carries a witness
//!      certificate the portable checker (almide-verify) accepts, and the
//!      exit function's certificate is the pinned one (the release on the
//!      jump / return edge).
//!
//! What it does NOT prove: native's own heap balance (Rust's drops are not
//! counted), and the wasm counters see only blocks the structural allocator
//! hands out — a host-side buffer is out of their view, as it is for the
//! corpus alloc ledger.

use std::process::Command;

struct Probe {
    label: &'static str,
    exit_fn: &'static str,
    src: &'static str,
    stdout: &'static str,
    cert: &'static str,
}

const PROBES: &[Probe] = &[
    Probe {
        label: "list rest, break out of the arm",
        exit_fn: "arm_break",
        src: r#"fn arm_break(xs: List[Int]) -> Int = {
  var acc = 0
  var i = 0
  while i < 4 {
    i = i + 1
    match xs {
      [h, ..t] => {
        if h + i > 4 then break
        else ()
        acc = acc + list.len(t)
      },
      [] => {
        acc = acc + 100
      },
    }
  }
  acc
}

effect fn main() -> Unit = {
  var total = 0
  for i in 0..<40 {
    total = total + arm_break([i % 5, 1, 2, 3]) + arm_break([])
  }
  println("${arm_break([4, 1, 2])} ${arm_break([1, 7])} ${arm_break([])}")
  println(int.to_string(total))
}
"#,
        stdout: "0 3 400\n16240\n",
        cert: "\n\n{idx|}{|ibd}\n",
    },
    Probe {
        label: "list rest kept by the enclosing arm across an inner break",
        exit_fn: "outer_keeps",
        src: r#"fn outer_keeps(xs: List[Int]) -> Int = match xs {
  [h, ..t] => {
    var acc = 0
    for x in t {
      match t {
        [_, ..u] => {
          if x > h then break
          else ()
          acc = acc + list.len(u)
        },
        [] => (),
      }
    }
    acc + list.len(t)
  },
  [] => 0,
}

effect fn main() -> Unit = {
  var total = 0
  for i in 0..<40 {
    total = total + outer_keeps([i % 5, 1, 2, 3])
  }
  println("${outer_keeps([2, 1, 3, 1])} ${outer_keeps([9, 1, 2])} ${outer_keeps([])}")
  println(int.to_string(total))
}
"#,
        stdout: "5 4 0\n264\n",
        cert: "\n{|ibbd}\n\n{idx|}{|ibd}\n",
    },
    Probe {
        label: "map walk, break out of the body",
        exit_fn: "walk_break",
        src: r#"fn walk_break(m: Map[String, List[Int]]) -> Int = {
  var found = 0
  for (_, v) in m {
    if list.len(v) > 2 then {
      found = list.len(v)
      break
    } else ()
  }
  found
}

effect fn main() -> Unit = {
  var total = 0
  for i in 0..<30 {
    let m = map.from_list([
      ("a", [1]),
      ("b" + int.to_string(i), list.repeat(i, i % 4)),
      ("c", [5, 6])
    ])
    total = total + walk_break(m)
  }
  let m = map.from_list([
    ("x", [1]),
    ("y", [1, 2, 3])
  ])
  let one = map.from_list([
    ("z", [1])
  ])
  println("${walk_break(m)} ${walk_break(one)}")
  println(int.to_string(total))
}
"#,
        stdout: "3 0\n21\n",
        cert: "\nad\n\n\n",
    },
    Probe {
        label: "map walk, guard return out of the body",
        exit_fn: "walk_guard",
        src: r#"fn walk_guard(m: Map[String, List[Int]]) -> Int = {
  for (k, v) in m {
    guard list.len(v) <= 2 else list.len(v) + string.len(k)
  }
  0
}

effect fn main() -> Unit = {
  var total = 0
  for i in 0..<30 {
    let m = map.from_list([
      ("a", [1]),
      ("b" + int.to_string(i), list.repeat(i, i % 4)),
      ("c", [5, 6])
    ])
    total = total + walk_guard(m)
  }
  let m = map.from_list([
    ("x", [1]),
    ("y", [1, 2, 3])
  ])
  let one = map.from_list([
    ("z", [1])
  ])
  println("${walk_guard(m)} ${walk_guard(one)}")
  println(int.to_string(total))
}
"#,
        stdout: "4 0\n40\n",
        cert: "\nad\n\n\n",
    },
];

/// The native leg: build with the CLI this package ships and run the binary.
fn native(p: &Probe) -> (String, i32) {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("main.almd");
    std::fs::write(&source, p.src).expect("source");
    let artifact = dir.path().join("native");
    let built = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["build", source.to_str().expect("path"), "--target", "rust", "-o", artifact.to_str().expect("path")])
        .output()
        .expect("almide build");
    assert!(built.status.success(), "{}: native build:\n{}", p.label, String::from_utf8_lossy(&built.stderr));
    let out = Command::new(&artifact).output().expect("run native");
    (String::from_utf8_lossy(&out.stdout).to_string(), out.status.code().unwrap_or(-1))
}

fn ir_of(p: &Probe) -> almide::ir::IrProgram {
    almide::wasm_leg::lower_to_ir("leak_free_exit.almd", p.src).unwrap_or_else(|e| panic!("{}: front: {e}", p.label))
}

/// The wasm leg, in process, with the allocation counters armed.
fn wasm_counted(p: &Probe) -> (String, i32, almide_wasm_run::AllocCount) {
    let ir = ir_of(p);
    let bytes = {
        let _armed = almide_wasm::alloc_count::CountGuard::set();
        almide_wasm::emit_program(&ir).unwrap_or_else(|e| panic!("{}: the structural leg lowers it: {e:?}", p.label))
    };
    let r = almide_wasm_run::run_wasm(&bytes).expect("run wasm");
    let c = r.alloc_count.expect("the armed module exports the counters");
    (r.stdout, r.exit, c)
}

/// The shipped emission pass's frames, `(function name, certificate)`.
fn shipped_frames(p: &Probe) -> Vec<(String, String)> {
    let ir = ir_of(p);
    almide_wasm::witness::start_collecting();
    let emitted = almide_wasm::emit_program(&ir);
    let shipped = almide_wasm::witness::take_shipped();
    assert!(emitted.is_ok(), "{}: the structural leg lowers it", p.label);
    shipped.unwrap_or_else(|| panic!("{}: a pass shipped", p.label)).1
}

/// Every finding for one probe; empty when it holds.
fn judge(p: &Probe) -> Vec<String> {
    let mut bad = Vec::new();
    let (n_out, n_exit) = native(p);
    let (w_out, w_exit, c) = wasm_counted(p);
    if (n_out.as_str(), n_exit) != (p.stdout, 0) {
        bad.push(format!("native leg: exit {n_exit}, stdout {n_out:?}, recorded {:?}", p.stdout));
    }
    if (w_out.as_str(), w_exit) != (n_out.as_str(), n_exit) {
        bad.push(format!("wasm leg differs from native: exit {w_exit} vs {n_exit}, {w_out:?} vs {n_out:?}"));
    }
    // The probe drives the exit through allocating calls many times over.
    if c.allocs <= 50 {
        bad.push(format!("too few allocations to judge: {c}"));
    }
    if c.allocs != c.frees || c.live() != 0 {
        bad.push(format!("allocation balance: {c}"));
    }
    let frames = shipped_frames(p);
    for (name, cert) in &frames {
        if cert.starts_with('!') || !almide_verify::check(almide_verify::Property::Ownership, cert.as_bytes()) {
            bad.push(format!("frame {name} is not certified: {cert:?}"));
        }
    }
    let got = frames.iter().find(|(n, _)| n == p.exit_fn).map(|(_, c)| c.as_str());
    if got != Some(p.cert) {
        bad.push(format!("{} certificate: {got:?}, pinned {:?}", p.exit_fn, p.cert));
    }
    bad
}

#[test]
fn every_walled_exit_shape_agrees_with_native_frees_every_block_and_certifies() {
    let findings: Vec<String> = PROBES
        .iter()
        .flat_map(|p| judge(p).into_iter().map(move |f| format!("{}: {f}", p.label)))
        .collect();
    assert!(findings.is_empty(), "{}", findings.join("\n"));
}
