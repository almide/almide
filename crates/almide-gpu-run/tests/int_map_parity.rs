//! CPU ⇄ GPU parity for pure `(Int) -> Int` maps.
//!
//! Each case lowers ONE source to ONE linked `IrProgram`. The CPU leg is the
//! IR interpreter (the 3-way oracle's third leg) running a `main` that prints
//! `f(x)` per input; the GPU leg is the kernel emitted from the same IR,
//! dispatched over the same inputs. The outputs must be identical, including
//! where `+ - *` wrap at the 64-bit edges.
//!
//! No adapter (no GPU and no software Vulkan) fails the test when
//! `ALMIDE_GPU_REQUIRED` is set; otherwise it says SKIP on stderr, by name.

use almide_frontend::canonicalize;
use almide_frontend::check::Checker;
use almide_frontend::lower::lower_program;
use almide_gpu_run::device::Gpu;
use almide_gpu_run::kernel::int_map_kernel;
use almide_interp::Interpreter;
use almide_lang::lexer::Lexer;
use almide_lang::parser::Parser;

fn lower(src: &str) -> almide_ir::IrProgram {
    let mut parser = Parser::new(Lexer::tokenize(src));
    let mut prog = parser.parse().expect("parse failed");
    assert!(parser.errors.is_empty(), "parse errors: {:?}", parser.errors);
    let canon = canonicalize::canonicalize_program(&prog, std::iter::empty());
    let mut checker = Checker::from_env(canon.env);
    let diags = checker.infer_program(&mut prog);
    let errors: Vec<_> = diags.iter().filter(|d| d.level == almide_frontend::diagnostic::Level::Error).collect();
    assert!(errors.is_empty(), "type errors: {errors:#?}");
    let mut ir = lower_program(&prog, &checker.env, &checker.type_map);
    almide_driver::link_ir(&mut ir);
    ir
}

/// The inputs every kernel runs on: small values, signs, and the 32- and
/// 64-bit edges where a carry, a borrow or a wrap happens.
fn inputs() -> Vec<i64> {
    let mut xs = vec![0, 1, -1, 2, -2, 7, 1000, -1001, 65_535, 65_536, 46_341, -46_341];
    xs.extend([u32::MAX as i64, 1 << 32, -(1 << 32), (1 << 32) + 1, (1 << 31) - 1, -(1 << 31)]);
    xs.extend([3_037_000_499, 3_037_000_500, -3_037_000_500]);
    xs.extend([i64::MAX, i64::MIN, i64::MAX - 1, i64::MIN + 1, 0x5555_5555_5555_5555, -0x0123_4567_89ab_cdef]);
    xs
}

fn almide_lit(x: i64) -> String {
    if x == i64::MIN { "int.min_value()".to_string() } else { x.to_string() }
}

fn gpu() -> Option<Gpu> {
    match Gpu::new() {
        Ok(g) => {
            eprintln!("GPU leg on {}", g.adapter);
            Some(g)
        }
        Err(e) => {
            assert!(std::env::var_os("ALMIDE_GPU_REQUIRED").is_none(), "ALMIDE_GPU_REQUIRED is set and {e}");
            eprintln!("SKIP int_map_parity: {e}");
            None
        }
    }
}

/// Run `entry` from `kernels` on both legs and compare.
fn assert_parity(kernels: &str, entry: &str) {
    let xs = inputs();
    let list = xs.iter().map(|x| almide_lit(*x)).collect::<Vec<_>>().join(", ");
    let src = format!("{kernels}\nfn main() -> Unit = {{\n  for x in [{list}] {{\n    println(int.to_string({entry}(x)))\n  }}\n}}\n");
    let ir = lower(&src);
    let out = Interpreter::new(&ir).run_main();
    assert_eq!(out.exit_code(), 0, "CPU leg failed: {}", out.stderr);
    let cpu: Vec<i64> = out.stdout.lines().map(|l| l.parse().expect("an Int line")).collect();
    assert_eq!(cpu.len(), xs.len());
    let wgsl = int_map_kernel(&ir, entry).unwrap_or_else(|r| panic!("`{entry}` refused: {r}"));
    let Some(gpu) = gpu() else { return };
    let dev = gpu.run_int_map(&wgsl, &xs).unwrap_or_else(|e| panic!("GPU leg failed: {e}\n{wgsl}"));
    for (i, x) in xs.iter().enumerate() {
        assert_eq!(dev[i], cpu[i], "`{entry}({x})`: GPU {} vs CPU {}\n{wgsl}", dev[i], cpu[i]);
    }
}

#[test]
fn a_polynomial_wraps_identically() {
    assert_parity("fn poly(x: Int) -> Int = x * x * 3 + x * 7 - 11", "poly");
}

#[test]
fn a_large_multiplier_wraps_identically() {
    let src = "fn lcg(x: Int) -> Int = {\n  let a = x * 6364136223846793005 + 1442695040888963407\n  let b = a * a\n  b - a * 31\n}";
    assert_parity(src, "lcg");
}

#[test]
fn branches_and_comparisons_agree_at_the_signed_edges() {
    let src = "fn clamp(x: Int) -> Int = if x < -1000 then -1000 else if x > 1000 then 1000 else x\n\
               fn sign(x: Int) -> Int = if x > 0 and x != 7 then 1 else if not (x >= 0) then -1 else 0\n\
               fn both(x: Int) -> Int = clamp(x) * 10 + sign(x) - (if x <= 0 or x == 1000 then 5 else -5)";
    assert_parity(src, "both");
}

#[test]
fn helper_calls_and_negation_agree() {
    let src = "fn sq(x: Int) -> Int = x * x\n\
               fn mix(x: Int) -> Int = {\n  let a = sq(x) + 1\n  let b = a * a - x\n  if a > b then a - b else -(b - a)\n}";
    assert_parity(src, "mix");
}

#[test]
fn division_is_refused_by_name() {
    let ir = lower("fn half(x: Int) -> Int = x / 2\nfn main() -> Unit = println(int.to_string(half(4)))\n");
    let r = int_map_kernel(&ir, "half").expect_err("division must be refused");
    assert!(r.0.contains("`/` or `%`"), "{r}");
}

#[test]
fn recursion_is_refused_by_name() {
    let ir = lower("fn down(x: Int) -> Int = if x <= 0 then 0 else down(x - 1)\nfn main() -> Unit = println(int.to_string(down(3)))\n");
    assert!(int_map_kernel(&ir, "down").is_err(), "recursion must be refused");
}

#[test]
fn a_non_int_signature_is_refused() {
    let ir = lower("fn len(s: String) -> Int = string.len(s)\nfn main() -> Unit = println(int.to_string(len(\"ab\")))\n");
    let r = int_map_kernel(&ir, "len").expect_err("a String param must be refused");
    assert!(r.0.contains("(Int) -> Int"), "{r}");
}

#[test]
fn an_empty_list_needs_no_dispatch() {
    let Some(gpu) = gpu() else { return };
    let ir = lower("fn id(x: Int) -> Int = x\nfn main() -> Unit = println(int.to_string(id(1)))\n");
    let wgsl = int_map_kernel(&ir, "id").expect("id lowers");
    assert_eq!(gpu.run_int_map(&wgsl, &[]), Ok(Vec::new()));
}
