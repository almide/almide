//! The incumbent emitter's `bytes.repeat` arm (`lower_bytes_repeat`) takes
//! the repeat family's shared 2^31-byte ceiling (C-054/C-161, #4154): a
//! result over it is `Error: repeat result too large` + exit 1, the line the
//! native runtime and the self-hosted `bytes_repeat` print — never the
//! structural `out of memory` die, which only a size UNDER the ceiling that
//! the floor cannot satisfy may reach. A count whose product wraps i64 hits
//! the same line, because the ceiling is judged by division.

mod harness;
use harness::run_wasm;

fn run(src: &str) -> (i32, String, String) {
    let ir = almide_spine::s5::lower_to_ir("probe.almd", src).expect("front");
    let bytes = almide_wasm::emit_program(&ir).expect("emit");
    let r = run_wasm(&bytes).expect("run");
    (r.exit, r.stdout, r.stderr)
}

#[test]
fn bytes_repeat_over_the_ceiling_is_the_repeat_abort() {
    for n in ["1073741825", "9223372036854775807"] {
        let src = format!(
            "fn main() -> Unit = {{\n  println(\"${{bytes.len(bytes.repeat(bytes.from_list([1, 2]), 3))}}\")\n  println(\"${{bytes.len(bytes.repeat(bytes.from_list([1, 2]), {n}))}}\")\n}}\n"
        );
        let (exit, out, err) = run(&src);
        assert_eq!(exit, 1, "n={n}: {out} / {err}");
        assert_eq!(out, "6\n", "n={n}");
        assert_eq!(err, "Error: repeat result too large\n", "n={n}");
    }
}
