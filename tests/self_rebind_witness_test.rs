//! #4339: a self-rebind `x = x` of a droppable `var` inside a loop.
//!
//! The Assign route (crates/almide-wasm/src/stmts.rs `lower_assign`) shares a
//! borrowed Var rhs (`rc_inc_top`) BEFORE it releases the old occupant, so
//! `x = x` nets to zero on one live block. The ownership witness once logged
//! the two the other way round (release, then share). The release reached 0
//! first, and on a line the owned-line probe (#3229) guards, the share then
//! looked like a resurrection of a dead block. At a large trip count the
//! loop's body is emitted unrolled onto the frame's owned line, so
//! `almide-verify` rejected `main` (`REJECT ownership main`) and the wasm
//! build refused to write a module whose bytes are balanced. At 3 iterations
//! the body stayed in a branch the probe does not guard, which hid the
//! recording order.
//!
//! Each program is judged on the structural wasm leg:
//!   1. its stdout is the recorded one, with the allocation counters armed,
//!      and the run ends with every block freed (`allocs == frees`, none
//!      live): the bytes are balanced, so a reject is the recorder's;
//!   2. every frame of the shipped emission pass carries an ownership
//!      certificate the verifier accepts, and `main`'s is recorded (not
//!      declined);
//!   3. the bundle `almide verify --emit` writes, the one `almide build
//!      --target wasm` judges, is CERTIFIED by the verifier's own bundle
//!      reader.
//!
//! The control: the d-before-a recording is rejected by the same checker on
//! a line it guards. That is the shape the old order produced.

use std::process::Command;

struct Probe {
    label: &'static str,
    src: &'static str,
    stdout: &'static str,
}

/// The issue's program: a dead TypedBody built from the self-rebound var.
const TYPED_1024: &str = r#"import http
effect fn main() -> Unit = {
  var x: Bytes = bytes.zeros(200000)
  var acc = 0
  var i = 0
  while i < 1024 {
    x = x
    let r: TypedBody = http.typed("text/csv", x)
    acc = acc + 1
    acc = acc + 1
    i = i + 1
  }
  acc = acc + bytes.len(x)
  println("${acc}")
}
"#;

const PROBES: &[Probe] = &[
    Probe { label: "dead TypedBody from a self-rebound var, 1024 iterations", src: TYPED_1024, stdout: "202048\n" },
    Probe {
        label: "bare self-rebind, 1024 iterations",
        src: r#"effect fn main() -> Unit = {
  var x: Bytes = bytes.zeros(200000)
  var acc = 0
  var i = 0
  while i < 1024 {
    x = x
    acc = acc + 2
    i = i + 1
  }
  acc = acc + bytes.len(x)
  println("${acc}")
}
"#,
        stdout: "202048\n",
    },
    Probe {
        label: "self-rebind then an alias, 3 iterations",
        src: r#"effect fn main() -> Unit = {
  var x: Bytes = bytes.zeros(200000)
  var acc = 0
  var i = 0
  while i < 3 {
    x = x
    let r = x
    acc = acc + bytes.len(r)
    i = i + 1
  }
  println("${acc}")
}
"#,
        stdout: "600000\n",
    },
];

fn ir_of(p: &Probe) -> almide::ir::IrProgram {
    almide::wasm_leg::lower_to_ir("self_rebind.almd", p.src).unwrap_or_else(|e| panic!("{}: front: {e}", p.label))
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

/// The bundle the CLI producer writes, judged by the verifier's bundle reader.
fn bundle_outcome(p: &Probe) -> (almide_verify::bundle::Outcome, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("self_rebind.almd");
    std::fs::write(&source, p.src).expect("source");
    let bundle = dir.path().join("self_rebind.bundle");
    // The exit status is the verifier's when one is found beside the binary,
    // 127 when not; either way `--emit` writes the bundle, judged here.
    let o = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["verify", source.to_str().expect("path"), "--emit", bundle.to_str().expect("path")])
        .current_dir(dir.path())
        .output()
        .expect("almide verify");
    let text = std::fs::read(&bundle)
        .unwrap_or_else(|e| panic!("{}: no bundle ({e}):\n{}", p.label, String::from_utf8_lossy(&o.stderr)));
    let parsed = almide_verify::bundle::parse(&text).unwrap_or_else(|e| panic!("{}: bundle: {e}", p.label));
    let (verdicts, outcome) = almide_verify::bundle::verify(&parsed);
    let rejected: Vec<String> =
        verdicts.iter().filter(|v| !v.accepted).map(|v| format!("{} {}", v.property.name(), v.function)).collect();
    (outcome, rejected.join(", "))
}

/// Every finding for one probe; empty when it holds.
fn judge(p: &Probe) -> Vec<String> {
    let mut bad = Vec::new();
    let (out, exit, c) = wasm_counted(p);
    if (out.as_str(), exit) != (p.stdout, 0) {
        bad.push(format!("wasm leg: exit {exit}, stdout {out:?}, recorded {:?}", p.stdout));
    }
    if c.allocs != c.frees || c.live() != 0 {
        bad.push(format!("allocation balance: {c}"));
    }
    let frames = shipped_frames(p);
    for (name, cert) in &frames {
        if !cert.starts_with('!') && !almide_verify::check(almide_verify::Property::Ownership, cert.as_bytes()) {
            bad.push(format!("frame {name}: the verifier rejects {cert:?}"));
        }
    }
    match frames.iter().find(|(n, _)| n == "main") {
        Some((_, cert)) if cert.starts_with('!') => bad.push(format!("main declined: {cert:?}")),
        Some(_) => {}
        None => bad.push("no main frame".into()),
    }
    let (outcome, rejected) = bundle_outcome(p);
    if outcome != almide_verify::bundle::Outcome::Certified {
        bad.push(format!("bundle {}: rejected [{rejected}]", outcome.label()));
    }
    bad
}

#[test]
fn a_self_rebound_var_in_a_loop_certifies_and_frees_every_block() {
    let findings: Vec<String> =
        PROBES.iter().flat_map(|p| judge(p).into_iter().map(move |f| format!("{}: {f}", p.label))).collect();
    assert!(findings.is_empty(), "{}", findings.join("\n"));
}

/// The control: on an owned line (a top-level `i` first), a release that
/// reaches 0 before the share is a resurrection, and the checker rejects it.
/// The emitted order (share, then release) is accepted.
#[test]
fn the_release_before_share_order_is_what_the_checker_rejects() {
    let check = |w: &str| almide_verify::check(almide_verify::Property::Ownership, w.as_bytes());
    assert!(!check("ibdabdam\n"), "release-then-share on an owned line must be rejected");
    assert!(check("ibadbadm\n"), "share-then-release on an owned line must be accepted");
}
