//! Pin for the mono-clone `mutated_params` drop (the DDD gauntlet's c3):
//! `fn go[C: Counter](mut c: C)` mutating through a protocol-bound
//! mut-self method must write back to the caller's var. The
//! specialization clone used to lose the `mutated_params` flag, so the
//! move-mode rewrite AND the C-132 wall both missed the instance —
//! silent value semantics, `13/10` instead of `13/13` (2026-08-26).

mod harness;
use harness::run_wasm;

const SRC: &str = r#"protocol Counter {
  fn bump(mut self: Self, by: Int) -> Unit
  fn read(self) -> Int
}

type Tally: Counter = { n: Int }

fn Tally.bump(mut self: Tally, by: Int) -> Unit = { self.n = self.n + by }

fn Tally.read(self) -> Int = self.n

fn go[C: Counter](mut c: C) -> Int = {
  c.bump(1)
  c.bump(2)
  c.read()
}

fn main() -> Unit = {
  var t = Tally { n: 10 }
  let r = go(t)
  println("${r}/${t.n}")
}
"#;

#[test]
fn mono_instance_keeps_mut_param_writeback() {
    let ir = almide_spine::s5::lower_to_ir("mut_param_mono.almd", SRC).expect("lowers");
    let bytes = almide_wasm::emit_program(&ir).expect("emits");
    let run = run_wasm(&bytes).expect("runs");
    assert_eq!(run.exit, 0, "stderr: {}", run.stderr);
    let wasm_out = run.stdout;

    // The oracle: the interpreter IS the definition (ARCHITECTURE.md §6.6).
    let interp = almide_spine::s5::run_file("mut_param_mono.almd", SRC).expect("interp runs");
    assert_eq!(interp.exit, 0, "oracle run failed: {}", interp.stderr);
    assert_eq!(wasm_out, interp.stdout, "mono clone lost the mut-param writeback");
    // Belt and braces: pin the semantics explicitly, so a bug in BOTH
    // legs cannot silently agree.
    assert_eq!(wasm_out, "13/13\n");
}
