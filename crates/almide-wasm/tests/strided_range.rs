//! The strided ×2 range loop (src/stride.rs): a loop over a range whose
//! body skips one parity of the loop variable — tested directly, or through
//! an odd multiplier and an offset (in the immutable lets that open a `for`
//! body, or in the maps before a fused chain's filter) — runs over the other
//! parity only, two values per trip.
//!
//! DIFFERENTIAL: every generated loop (bounds at both ends of `Int`,
//! negative, empty, single-value and inclusive ranges; multipliers and
//! offsets of both signs; both skip spellings; fused chains with a sum or an
//! order-sensitive fold, stages after the filter) runs on the wasm leg, on
//! the interpreter (the definition) and — when the `almide` binary is at
//! hand — on the native leg, and all three print the same order-sensitive
//! digest. FIRES / MUST NOT FIRE: the emitter's own count of strided loops
//! says which shapes took the lane. The controls keep the rolled loop: an
//! even multiplier, a remainder test against 1 (sign-dependent), a test by
//! 3, a `break`, a second `continue`, a test of a `var`, a mutable let, a
//! filter after a map that is not pure arithmetic, a second filter first.

mod harness;
use harness::run_wasm;

const MAX: &str = "9223372036854775807";
const MIN: &str = "(-9223372036854775807 - 1)";

/// One generated function body and whether it must take the lane.
struct Case {
    body: String,
    fires: bool,
}

fn for_case(bounds: &str, lets: &str, skip: &str, extra: &str, fires: bool) -> Case {
    let body = format!(
        "  var acc = 0\n  var n = 0\n  var flag = 0\n  for i in {bounds} {{\n{lets}\n    {skip}\n    acc = (acc * 31 + i % 1000 + y % 1000) % 1000000007\n    n = n + 1\n{extra}\n  }}\n  acc * 1000003 + n * 7 + flag\n"
    );
    Case { body, fires }
}

fn chain_case(a: &str, b: &str, stages: &str, end: &str, fires: bool) -> Case {
    Case { body: format!("  list.range({a}, {b}){stages}{end}\n"), fires }
}

fn program(fns: &[(String, &Case)]) -> String {
    let defs: String = fns.iter().map(|(n, c)| format!("fn {n}() -> Int = {{\n{}}}\n", c.body)).collect();
    let prints: String = fns.iter().map(|(n, _)| format!("  println(\"${{{n}()}}\")\n")).collect();
    format!("{defs}\nfn main() -> Unit = {{\n{prints}}}\n")
}

fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    let ranges = [
        "0..<0", "0..<1", "1..<2", "0..<10", "1..<10", "-10..<10", "-7..<-1", "5..<3", "3...3", "4...3", "-9...9", "0...1000",
    ];
    let shapes: [(&str, &str, bool); 8] = [
        ("    let y = i", "if y % 2 != 0 then continue", true),
        ("    let y = i", "guard y % 2 == 0 else continue", true),
        ("    let x = i\n    let y = x * 3", "if y % 2 != 0 then continue", true),
        ("    let y = i * 5 + 1", "if y % 2 == 0 then continue", true),
        ("    let x = i * 7 - 4\n    let y = 2 - x", "guard not (y % 2 != 0) else continue", true),
        ("    let y = 3 * i", "if not (0 == y % 2) then continue", true),
        ("    let y = i * 2 + 1", "if y % 2 != 0 then continue", false),
        ("    let y = i * 3", "if y % 2 == 1 then continue", false),
    ];
    for r in ranges {
        for (lets, skip, fires) in shapes {
            out.push(for_case(r, lets, skip, "", fires));
        }
    }
    // The ends of Int, with offsets that cannot overflow there.
    for (b, lets) in [
        (format!("({MAX} - 6)..<{MAX}"), "    let y = i - 1"),
        (format!("({MAX} - 6)...{MAX}"), "    let y = i - 1"),
        (format!("({MAX} - 1)...{MAX}"), "    let y = i"),
        (format!("{MAX}...{MAX}"), "    let y = i"),
        (format!("{MIN}..<({MIN} + 7)"), "    let y = i + 1"),
        (format!("{MIN}...({MIN} + 6)"), "    let y = i + 1"),
        (format!("{MIN}...{MIN}"), "    let y = i"),
    ] {
        out.push(for_case(&b, lets, "if y % 2 != 0 then continue", "", true));
        out.push(for_case(&b, lets, "guard y % 2 != 0 else continue", "", true));
    }
    for (lets, skip, extra) in [
        ("    let y = i", "if y % 3 != 0 then continue", ""),
        ("    let y = i", "if y % 2 != 0 then continue", "    if n == 3 then break"),
        ("    let y = i", "if y % 2 != 0 then continue", "    if i % 5 == 0 then continue"),
        ("    flag = flag + i\n    let y = flag", "if y % 2 != 0 then continue", ""),
        ("    var y = i\n    y = y + 1", "if y % 2 != 0 then continue", ""),
    ] {
        for r in ["-10..<10", "0...25"] {
            out.push(for_case(r, lets, skip, extra, false));
        }
    }
    // Fused chains over `list.range`: maps before the filter, stages after
    // it, a sum or an order-sensitive fold.
    let sum = " |> list.sum";
    let fold = " |> list.fold(0, (a, x) => (a * 31 + x % 1000) % 1000000007)";
    for (a, b) in [("0", "0"), ("0", "1"), ("1", "2"), ("-37", "1001"), ("-8", "-1"), ("6", "2"), ("0", "9")] {
        for (stages, fires) in [
            (" |> list.map((x) => x * 3) |> list.filter((x) => x % 2 == 0)", true),
            (" |> list.filter((x) => x % 2 != 0)", true),
            (" |> list.map((x) => 5 - x) |> list.map((x) => x * 7) |> list.filter((x) => not (x % 2 == 0)) |> list.map((x) => x + 1)", true),
            (" |> list.map((x) => x * 2) |> list.filter((x) => x % 2 == 0)", false),
            (" |> list.map((x) => x * 3) |> list.filter((x) => x % 3 == 0)", false),
            (" |> list.map((x) => x / 3) |> list.filter((x) => x % 2 == 0)", false),
            (" |> list.filter((x) => x > 0) |> list.filter((x) => x % 2 == 0)", false),
        ] {
            out.push(chain_case(a, b, stages, sum, fires));
            out.push(chain_case(a, b, stages, fold, fires));
        }
    }
    for (a, b) in [(format!("({MAX} - 6)"), MAX.to_string()), (MIN.to_string(), format!("({MIN} + 7)"))] {
        out.push(chain_case(&a, &b, " |> list.filter((x) => x % 2 == 0) |> list.map((x) => x % 1000)", sum, true));
    }
    out
}

fn almide_bin() -> Option<std::path::PathBuf> {
    let p = std::env::var_os("ALMIDE_BIN")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/release/almide"));
    p.exists().then_some(p)
}

#[test]
fn strided_range_loops_agree_on_every_leg_and_fire_only_on_their_shape() {
    let cases = cases();
    let mut wrong_lane = Vec::new();
    let mut want_lines = Vec::new();
    for (k, c) in cases.iter().enumerate() {
        let name = format!("c{k}");
        let src = program(&[(name.clone(), c)]);
        let file = format!("{name}.almd");
        let ir = almide_spine::s5::lower_to_ir(&file, &src).unwrap_or_else(|e| panic!("{name}: {e:?}\n{src}"));
        let before = almide_wasm::strided_loops_emitted();
        let bytes = almide_wasm::emit_program(&ir).unwrap_or_else(|e| panic!("{name} emits: {e:?}\n{src}"));
        let fired = almide_wasm::strided_loops_emitted() - before;
        // Every emission pass of the program counts its loop once.
        if (fired > 0) != c.fires {
            wrong_lane.push(format!("{name} fired {fired}, want {}:\n{src}", c.fires));
        }
        let w = run_wasm(&bytes).expect("runs");
        let i = almide_spine::s5::run_file(&file, &src).expect("interp runs");
        assert_eq!((w.exit, i.exit), (0, 0), "{name}: wasm {} / interp {}\n{src}", w.stderr, i.stderr);
        assert_eq!(w.stdout, i.stdout, "{name}: wasm and the interpreter disagree\n{src}");
        want_lines.push(i.stdout.trim_end().to_string());
    }
    assert!(wrong_lane.is_empty(), "{} case(s) took the wrong lane:\n{}", wrong_lane.len(), wrong_lane.join("\n"));
    let n_fire = cases.iter().filter(|c| c.fires).count();
    assert!(n_fire >= 120 && cases.len() - n_fire >= 60, "the matrix shrank: {n_fire} firing, {} controls", cases.len() - n_fire);

    // The native leg: every case in one program, one line each.
    let Some(bin) = almide_bin() else {
        eprintln!("strided_range: NATIVE LEG NOT RUN — no almide binary (set ALMIDE_BIN)");
        return;
    };
    let named: Vec<(String, &Case)> = cases.iter().enumerate().map(|(k, c)| (format!("c{k}"), c)).collect();
    let dir = std::env::temp_dir().join(format!("almide-strided-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let path = dir.join("strided.almd");
    std::fs::write(&path, program(&named)).expect("write");
    let out = std::process::Command::new(&bin).args(["run", path.to_str().expect("utf8")]).output().expect("native runs");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(out.status.success(), "native: {}", String::from_utf8_lossy(&out.stderr));
    let got: Vec<String> = String::from_utf8_lossy(&out.stdout).lines().map(str::to_string).collect();
    assert_eq!(got, want_lines, "the native leg disagrees with the interpreter");
    eprintln!("strided_range: native leg ran {} cases", got.len());
}
