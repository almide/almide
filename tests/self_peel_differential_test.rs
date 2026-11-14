//! The wasm leg's one-level inlining of an accumulator loop's own call
//! (`almide_ir::self_peel`, run after `accum_tre` in crates/almide-wasm
//! `emit.rs`) never changes what a program prints or how it exits, fires
//! only on the accumulator loop, and never deepens the stack.
//!
//! Every program is built and run twice on the wasm leg: as shipped, and
//! with the peel switched off (`ALMIDE_SELF_PEEL_OFF=1`). Whether the peel
//! fired is read off the emitted module: the two builds differ byte for byte
//! exactly when a body was copied.
//!
//! * Generated accumulator recursions (both operators, every spelling of the
//!   bound test, either operand order, steps 1..=4, seven base expressions
//!   including the wrapping ones): the peeled and the unpeeled run both
//!   print what a direct evaluation of the SOURCE definition gives, computed
//!   here in wrapping 64-bit arithmetic.
//! * Shapes the accumulator rewrite refuses (two parameters, mutual
//!   recursion, three self-calls, a non-commutative operator, a Float, a
//!   division that aborts, an effect per call): the two builds are the same
//!   bytes, so effects and aborts are where they were.
//! * A recursion as deep as its argument: wherever the unpeeled build
//!   finishes, the peeled one does, with the same answer.

use std::collections::HashMap;
use std::process::{Command, Output};

fn almide(dir: &std::path::Path, args: &[&str], off: bool) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.args(args).current_dir(dir);
    if off {
        c.env("ALMIDE_SELF_PEEL_OFF", "1");
    } else {
        c.env_remove("ALMIDE_SELF_PEEL_OFF");
    }
    c.output().expect("run the compiler")
}

struct Built {
    wasm: Vec<u8>,
    stdout: String,
    stderr: String,
    code: Option<i32>,
}

/// Build `src` to wasm and run it, with the peel on or off, in a directory
/// of its own (nothing is shared between the two builds).
fn build_and_run(name: &str, src: &str, off: bool) -> Built {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(format!("{name}.almd")), src).unwrap();
    let file = format!("{name}.almd");
    let built = almide(dir.path(), &["build", &file, "--target", "wasm", "-o", "out.wasm"], off);
    assert!(built.status.success(), "{name}: build failed (peel off: {off})\n{}", String::from_utf8_lossy(&built.stderr));
    let wasm = std::fs::read(dir.path().join("out.wasm")).expect("the built module");
    let run = almide(dir.path(), &["run", &file, "--target", "wasm"], off);
    Built {
        wasm,
        stdout: String::from_utf8_lossy(&run.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&run.stderr).into_owned(),
        code: run.status.code(),
    }
}

// ── generated accumulator recursions ────────────────────────────────────

#[derive(Clone, Copy)]
enum Op {
    Add,
    Mul,
}

#[derive(Clone, Copy)]
struct Shape {
    op: Op,
    /// The recursive branch runs exactly when `n >= bound`.
    bound: i64,
    /// How the bound test is spelled (0..6).
    test: u8,
    /// The two self-call steps, in source order.
    steps: (i64, i64),
    base: u8,
}

const BASES: [&str; 7] = [
    "n",
    "n * n - 3",
    "9223372036854775807",
    "-9223372036854775807 - n",
    "3",
    "n * 7 + 1",
    "if n < 0 then 0 - n else n + n",
];

fn base_value(base: u8, n: i64) -> i64 {
    match base {
        0 => n,
        1 => n.wrapping_mul(n).wrapping_sub(3),
        2 => i64::MAX,
        3 => (-i64::MAX).wrapping_sub(n),
        4 => 3,
        5 => n.wrapping_mul(7).wrapping_add(1),
        _ => {
            if n < 0 {
                0i64.wrapping_sub(n)
            } else {
                n.wrapping_add(n)
            }
        }
    }
}

impl Shape {
    fn source(&self, name: &str) -> String {
        let (a, b) = self.steps;
        let op = match self.op {
            Op::Add => "+",
            Op::Mul => "*",
        };
        let rec = format!("{name}(n - {a}) {op} {name}(n - {b})");
        let base = BASES[self.base as usize];
        let l = self.bound;
        let body = match self.test {
            0 => format!("if n < {l} then {base} else {rec}"),
            1 => format!("if n <= {} then {base} else {rec}", l - 1),
            2 => format!("if n >= {l} then {rec} else {base}"),
            3 => format!("if n > {} then {rec} else {base}", l - 1),
            4 => format!("if {l} <= n then {rec} else {base}"),
            _ => format!("if {l} > n then {base} else {rec}"),
        };
        format!("fn {name}(n: Int) -> Int = {body}\n")
    }

    /// The source definition, evaluated directly.
    fn eval(&self, n: i64, memo: &mut HashMap<i64, i64>) -> i64 {
        if n < self.bound {
            return base_value(self.base, n);
        }
        if let Some(v) = memo.get(&n) {
            return *v;
        }
        let (l, r) = (self.eval(n - self.steps.0, memo), self.eval(n - self.steps.1, memo));
        let v = match self.op {
            Op::Add => l.wrapping_add(r),
            Op::Mul => l.wrapping_mul(r),
        };
        memo.insert(n, v);
        v
    }
}

fn shapes() -> Vec<Shape> {
    let mut x = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = |m: u64| {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x % m
    };
    (0..72)
        .map(|i| Shape {
            op: if i % 2 == 0 { Op::Add } else { Op::Mul },
            bound: next(5) as i64 - 1,
            test: (i % 6) as u8,
            steps: (1 + next(4) as i64, 1 + next(4) as i64),
            base: (i % 7) as u8,
        })
        .collect()
}

const ARGS: std::ops::RangeInclusive<i64> = -3..=17;

#[test]
fn generated_accumulator_recursions_print_the_source_definition_peeled_and_unpeeled() {
    let shapes = shapes();
    let mut src = String::new();
    let mut expected = String::new();
    let mut main = String::from("fn main() -> Unit = {\n");
    for (i, s) in shapes.iter().enumerate() {
        let name = format!("f{i}");
        src.push_str(&s.source(&name));
        let mut memo = HashMap::new();
        let holes: Vec<String> = ARGS.map(|n| format!("${{{name}({n})}}")).collect();
        main.push_str(&format!("  println(\"{}\")\n", holes.join(" ")));
        let values: Vec<String> = ARGS.map(|n| s.eval(n, &mut memo).to_string()).collect();
        expected.push_str(&values.join(" "));
        expected.push('\n');
    }
    main.push_str("}\n");
    src.push_str(&main);

    let peeled = build_and_run("generated", &src, false);
    let unpeeled = build_and_run("generated", &src, true);
    assert_eq!(unpeeled.code, Some(0), "unpeeled: {}", unpeeled.stderr);
    assert_eq!(peeled.code, Some(0), "peeled: {}", peeled.stderr);
    assert_eq!(unpeeled.stdout, expected, "the unpeeled build does not print the source definition");
    assert_eq!(peeled.stdout, expected, "the peeled build does not print the source definition");
    assert_ne!(peeled.wasm, unpeeled.wasm, "the peel did not fire on any generated accumulator recursion");
}

// ── where it must not fire ──────────────────────────────────────────────

/// Both builds are the same module and print the same thing with the same
/// exit code; returns that output.
fn same_module(name: &str, src: &str) -> Built {
    let peeled = build_and_run(name, src, false);
    let unpeeled = build_and_run(name, src, true);
    assert_eq!(peeled.wasm, unpeeled.wasm, "{name}: the peel fired on a shape the accumulator rewrite refuses");
    assert_eq!((&peeled.stdout, peeled.code), (&unpeeled.stdout, unpeeled.code), "{name}: the two runs differ");
    assert_eq!(peeled.stderr, unpeeled.stderr, "{name}: the two runs report differently");
    peeled
}

#[test]
fn a_plain_fn_is_the_must_not_fire_control_and_fib_is_the_must_fire_one() {
    let plain = same_module("plain", "fn twice(n: Int) -> Int = n + n\nfn main() -> Unit = println(\"${twice(21)}\")\n");
    assert_eq!(plain.stdout, "42\n");
    let fib = "fn fib(n: Int) -> Int = if n < 2 then n else fib(n - 1) + fib(n - 2)\nfn main() -> Unit = println(\"${fib(20)}\")\n";
    let (peeled, unpeeled) = (build_and_run("fib", fib, false), build_and_run("fib", fib, true));
    assert_eq!((peeled.stdout.as_str(), unpeeled.stdout.as_str()), ("6765\n", "6765\n"));
    assert_ne!(peeled.wasm, unpeeled.wasm, "the switch does not reach the peel, or the peel no longer fires on fib");
}

#[test]
fn recursions_that_are_not_the_accumulator_loop_are_left_alone() {
    let two_params = same_module(
        "two_params",
        "fn walk(n: Int, k: Int) -> Int = if n < 2 then n + k else walk(n - 1, k) + walk(n - 2, k)\nfn main() -> Unit = println(\"${walk(15, 1)}\")\n",
    );
    assert_eq!(two_params.stdout, "1597\n");
    let mutual = same_module(
        "mutual",
        "fn ping(n: Int) -> Int = if n < 2 then n else pong(n - 1) + pong(n - 2)\nfn pong(n: Int) -> Int = if n < 2 then 1 else ping(n - 1) + ping(n - 2)\nfn main() -> Unit = println(\"${ping(12)} ${pong(12)}\")\n",
    );
    assert_eq!(mutual.stdout, "188 189\n");
    let three = same_module(
        "three_calls",
        "fn trib(n: Int) -> Int = if n < 3 then n else trib(n - 1) + trib(n - 2) + trib(n - 3)\nfn main() -> Unit = println(\"${trib(12)}\")\n",
    );
    assert_eq!(three.stdout, "778\n");
    let minus = same_module(
        "minus",
        "fn alt(n: Int) -> Int = if n < 2 then n else alt(n - 1) - alt(n - 2)\nfn main() -> Unit = println(\"${alt(14)}\")\n",
    );
    assert_eq!(minus.stdout, "1\n");
    let float = same_module(
        "float",
        "fn ffib(n: Float) -> Float = if n < 2.0 then n else ffib(n - 1.0) + ffib(n - 2.0)\nfn main() -> Unit = println(\"${ffib(12.0)}\")\n",
    );
    assert_eq!(float.stdout, "144\n");
}

#[test]
fn an_effect_or_an_abort_per_call_stays_where_it_was() {
    let effect = same_module(
        "effect",
        "effect fn walk(n: Int) -> Int = {\n  println(\"enter ${n}\")\n  if n < 2 then n else walk(n - 1)! + walk(n - 2)!\n}\neffect fn main() -> Unit = {\n  let r = walk(4)!\n  println(\"${r}\")\n}\n",
    );
    assert_eq!(effect.stdout, "enter 4\nenter 3\nenter 2\nenter 1\nenter 0\nenter 1\nenter 2\nenter 1\nenter 0\n3\n");
    let abort = same_module(
        "abort",
        "fn frac(n: Int) -> Int = if n < 2 then 10 / n else frac(n - 1) + frac(n - 2)\nfn main() -> Unit = {\n  println(\"before\")\n  println(\"${frac(5)}\")\n}\n",
    );
    assert_eq!(abort.stdout, "before\n");
    assert_ne!(abort.code, Some(0), "the division by zero in the base case must abort");
    assert!(abort.stderr.contains("division by zero"), "stderr: {}", abort.stderr);
}

// ── depth ───────────────────────────────────────────────────────────────

#[test]
fn the_peeled_build_finishes_wherever_the_unpeeled_one_does() {
    // `chain(n)` is n through a call chain n deep: the loop's step leaves
    // the recursive range at once (to a base case worth 1), so every round
    // makes exactly one call.
    let mut finished = 0;
    for n in [1_000i64, 4_000, 8_000, 16_000, 32_000, 64_000, 128_000, 256_000] {
        let src = format!(
            "fn chain(n: Int) -> Int = if n < 1 then (if n < 0 - 500000 then 1 else 0) else chain(n - 1) + chain(n - 1000000)\nfn main() -> Unit = println(\"${{chain({n})}}\")\n"
        );
        let unpeeled = build_and_run("chain", &src, true);
        let peeled = build_and_run("chain", &src, false);
        assert_ne!(peeled.wasm, unpeeled.wasm, "the peel did not fire on the chain");
        if unpeeled.code == Some(0) {
            finished += 1;
            assert_eq!(unpeeled.stdout, format!("{n}\n"));
            assert_eq!(
                (peeled.stdout.as_str(), peeled.code),
                (unpeeled.stdout.as_str(), Some(0)),
                "depth {n}: the unpeeled build finishes and the peeled one does not\n{}",
                peeled.stderr
            );
        } else if peeled.code == Some(0) {
            assert_eq!(peeled.stdout, format!("{n}\n"), "depth {n}: the peeled build finishes with another answer");
        }
    }
    assert!(finished >= 2, "the unpeeled build finished at {finished} depths: the chain no longer measures depth");
}
