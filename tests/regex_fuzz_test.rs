//! Differential fuzz lock for the regex engine (the verification-class artifact
//! backing the rt-oracle-registry flip of the 9 `rt_regex.rs` routines).
//!
//! ## What it proves
//!
//! The WASM regex engine (`crates/almide-codegen/src/emit_wasm/rt_regex.rs`) is a
//! hand-written reimplementation of the NATIVE oracle
//! (`runtime/rs/src/regex.rs`). This test generates a deterministic battery of
//! `(pattern, haystack)` cases from a grammar covering the supported feature set
//! (alternation, classes incl. internal escapes, escape atoms incl. negated +
//! escaped literals, quantifiers, anchors, captures, multibyte + emoji
//! haystacks), batches them into ONE `.almd` program per public op, and asserts
//! the native-target (= oracle, runs `almide_rt`) and WASM-target outputs are
//! byte-identical. Batching amortizes the per-program wasm build cost over
//! hundreds of cases.
//!
//! `replace` / `replace_first` ARE fuzzed: the historical native-oracle bug
//! (index-out-of-bounds panic whenever the pattern can match empty at
//! end-of-string, e.g. `a*`, ``, `x?`) is fixed in `runtime/rs/src/regex.rs`
//! (the zero-width advance now guards `chars[end]` at end-of-string), so both ops
//! are safe to drive against arbitrary empty-matching patterns.
//!
//! The grammar also emits EMPTY alternation arms (leading `|a`, trailing `a|`,
//! middle `a||b`) and the fully-empty pattern, exercising the empty-Seq matcher
//! path that previously diverged native↔wasm (wasm dropped trailing empty arms).
//!
//! Determinism: a fixed-seed SplitMix64 PRNG (`SEED` below). The generated
//! battery is also materialized to `spec/wasm_cross/regex_fuzz_batch.almd` (a
//! committed sample) so the `wasm_cross_target_spec` gate runs it too.
//!
//! Requires the `almide` release binary (ALMIDE_BIN / target/release/almide) and
//! wasmtime or Node WASI. Skips cleanly when neither is present.

use std::path::Path;
use std::process::Command;

/// Documented PRNG seed — change only with intent (regenerates the whole battery).
const SEED: u64 = 0x5EED_1234_ABCD_F00D;
/// Cases per op batch in the live differential (committed CI size).
const CASES_PER_OP: usize = 220;

/// Cases-per-op actually used: `REGEX_FUZZ_CASES` env override (for a one-time
/// larger sweep) else the committed `CASES_PER_OP`.
fn cases_per_op() -> usize {
    std::env::var("REGEX_FUZZ_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(CASES_PER_OP)
}
/// Cases per op written to the committed `spec/wasm_cross` sample (smaller, so the
/// gate stays fast).
const SAMPLE_CASES_PER_OP: usize = 30;

// ── PRNG (SplitMix64) ──
struct Rng(u64);
impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
    fn pick<T: Copy>(&mut self, xs: &[T]) -> T {
        let i = self.below(xs.len());
        xs[i]
    }
}

// ── Grammar ──
// Atoms both engines support, kept ASCII for class/escape semantics (native
// \d\w\s are ASCII-only; multibyte appears in haystacks and as literal atoms).
const LITERALS: &[&str] = &["a", "b", "c", "z", "本", "é", "🦀"];
const ESCAPES: &[&str] = &["\\d", "\\D", "\\w", "\\W", "\\s", "\\S", "\\.", "\\+", "\\\\"];
const CLASSES: &[&str] = &[
    "[abc]", "[a-z]", "[^a-z]", "[0-9]", "[^0-9]", "[\\d]", "[^\\d]", "[\\w]",
    "[a-z0-9]", "[本語]", "[.+*]",
];
const QUANTS: &[&str] = &["", "", "", "*", "+", "?"];

fn gen_atom(rng: &mut Rng) -> String {
    match rng.below(5) {
        0 => ".".to_string(),
        1 => rng.pick(LITERALS).to_string(),
        2 => rng.pick(ESCAPES).to_string(),
        3 => rng.pick(CLASSES).to_string(),
        _ => rng.pick(LITERALS).to_string(),
    }
}

fn gen_piece(rng: &mut Rng) -> String {
    format!("{}{}", gen_atom(rng), rng.pick(QUANTS))
}

/// A single alternative: a short sequence of pieces (occasionally EMPTY, to
/// exercise empty-arm matching), optionally one capture group.
fn gen_seq(rng: &mut Rng, allow_group: bool) -> String {
    // 1-in-5 alternatives are empty (n == 0) → an empty Seq.
    let n = if rng.below(5) == 0 { 0 } else { 1 + rng.below(3) };
    let mut s = String::new();
    // `rng.below(n)` requires n > 0; an empty Seq (n == 0) never hosts a group.
    let group_at = if allow_group && n > 0 && rng.below(3) == 0 { rng.below(n) as i64 } else { -1 };
    for i in 0..n {
        if i as i64 == group_at {
            // a small inner alternation inside a capture group; one arm is
            // occasionally empty (`(a|)`, `(|a)`) to exercise empty group arms.
            let lhs = if rng.below(4) == 0 { String::new() } else { gen_piece(rng) };
            let rhs = if rng.below(4) == 0 { String::new() } else { gen_piece(rng) };
            let inner = format!("{lhs}|{rhs}");
            s.push('(');
            s.push_str(&inner);
            s.push(')');
            s.push_str(rng.pick(QUANTS));
        } else {
            s.push_str(&gen_piece(rng));
        }
    }
    s
}

/// A full pattern: 1-3 top-level alternatives, optional anchors.
fn gen_pattern(rng: &mut Rng, allow_group: bool) -> String {
    let nalts = 1 + rng.below(3);
    let mut alts = Vec::new();
    for _ in 0..nalts {
        alts.push(gen_seq(rng, allow_group));
    }
    let mut p = alts.join("|");
    if rng.below(4) == 0 {
        p = format!("^{}", p);
    }
    if rng.below(4) == 0 {
        p = format!("{}$", p);
    }
    p
}

const HAYSTACKS: &[&str] = &[
    "", "a", "abc", "aaa", "a1b2c3", "hello world", "ABC123xyz", "  spaced  ",
    "user@host.com", "..++..", "z9z9z9", "no_digits_here", "日本語", "café",
    "🦀rust🦀", "本a本b本", "a\tb\nc", "end", "start end", "xYz",
];

fn gen_haystack(rng: &mut Rng) -> String {
    if rng.below(3) == 0 {
        // assemble a small random string from the alphabet so matches are frequent
        let alpha = ["a", "b", "c", "1", "2", " ", "_", "本", "@", "."];
        let n = rng.below(12);
        (0..n).map(|_| rng.pick(&alpha)).collect()
    } else {
        rng.pick(HAYSTACKS).to_string()
    }
}

/// Escape a Rust/Almide string literal for embedding in source.
fn almd_lit(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The public ops driven differentially. `replace`/`replace_first` are now safe
/// to fuzz against empty-matching patterns (the native end-of-string index guard
/// is fixed). Each maps a `(pat, hay)` to a `println(...)` line of a stable
/// rendering. The replacement is a fixed sentinel (`<R>`) inserted verbatim —
/// the engine's behavior under fuzzing is the match/advance logic, not the rep.
const OPS: &[&str] = &[
    "is_match", "find", "find_all", "full_match", "captures", "split",
    "replace", "replace_first",
];

/// Replacement sentinel for the two replace ops (verbatim insertion).
const REP: &str = "<R>";

fn render_call(op: &str, pat: &str, hay: &str) -> String {
    let p = almd_lit(pat);
    let h = almd_lit(hay);
    let r = almd_lit(REP);
    match op {
        "is_match" => format!("  println(b2s(regex.is_match({p}, {h})))\n"),
        "full_match" => format!("  println(b2s(regex.full_match({p}, {h})))\n"),
        "find" => format!("  println(ostr(regex.find({p}, {h})))\n"),
        "find_all" => format!("  println(\"[\" + string.join(regex.find_all({p}, {h}), \"|\") + \"]\")\n"),
        "split" => format!("  println(\"[\" + string.join(regex.split({p}, {h}), \"|\") + \"]\")\n"),
        "captures" => format!("  println(olist(regex.captures({p}, {h})))\n"),
        "replace" => format!("  println(\"<\" + regex.replace({p}, {h}, {r}) + \">\")\n"),
        "replace_first" => format!("  println(\"<\" + regex.replace_first({p}, {h}, {r}) + \">\")\n"),
        _ => unreachable!(),
    }
}

const PRELUDE: &str = "\
import regex

fn b2s(b: Bool) -> String = if b then \"T\" else \"F\"
fn ostr(o: Option[String]) -> String = match o {{ some(s) => s, none => \"NONE\" }}
fn olist(o: Option[List[String]]) -> String =
  match o {{ some(xs) => \"[\" + string.join(xs, \"|\") + \"]\", none => \"NONE\" }}

fn main() -> Unit = {{
";

fn build_program(rng: &mut Rng, op: &str, cases: usize) -> String {
    let mut src = PRELUDE.replace("{{", "{").replace("}}", "}");
    let allow_group = op == "captures";
    for _ in 0..cases {
        let pat = gen_pattern(rng, allow_group);
        let hay = gen_haystack(rng);
        src.push_str(&render_call(op, &pat, &hay));
    }
    src.push_str("}\n");
    src
}

// ── binary / runners ──
fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

fn run_native(src_path: &Path) -> Option<String> {
    let out = Command::new(almide_bin())
        .args(["run", src_path.to_str().unwrap()])
        .output()
        .ok()?;
    if !out.status.success() {
        panic!(
            "native run failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

fn run_wasm(src_path: &Path, wasm_path: &Path) -> Option<String> {
    let build = Command::new(almide_bin())
        .args([
            "build",
            src_path.to_str().unwrap(),
            "--target",
            "wasm",
            "-o",
            wasm_path.to_str().unwrap(),
        ])
        .output()
        .ok()?;
    if !build.status.success() {
        panic!(
            "wasm build failed:\n{}",
            String::from_utf8_lossy(&build.stderr)
        );
    }
    // wasmtime preferred; fall back to Node WASI.
    if let Ok(out) = Command::new("wasmtime")
        .arg(wasm_path.to_str().unwrap())
        .output()
    {
        if out.status.code() != Some(127) {
            if !out.status.success() {
                panic!(
                    "wasm run (wasmtime) failed:\n{}",
                    String::from_utf8_lossy(&out.stderr)
                );
            }
            return Some(String::from_utf8_lossy(&out.stdout).to_string());
        }
    }
    let js = format!(
        "const{{readFileSync}}=require('fs');const{{WASI}}=require('wasi');\
         const w=new WASI({{version:'preview1',args:[],env:{{}}}});\
         const m=new WebAssembly.Module(readFileSync('{}'));\
         const i=new WebAssembly.Instance(m,w.getImportObject());w.start(i);",
        wasm_path.to_str().unwrap().replace('\\', "/")
    );
    let dir = wasm_path.parent().unwrap();
    let js_path = dir.join("run.cjs");
    std::fs::write(&js_path, js).ok()?;
    let out = Command::new("node").arg(js_path.to_str().unwrap()).output().ok()?;
    if !out.status.success() {
        panic!("wasm run (node) failed:\n{}", String::from_utf8_lossy(&out.stderr));
    }
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

fn tools_available() -> bool {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return false;
    }
    let wasmtime = Command::new("wasmtime").arg("--version").output().is_ok();
    let node = Command::new("node").arg("--version").output().is_ok();
    wasmtime || node
}

/// Core: generate `cases` per op, run native (oracle) vs wasm, assert identical.
#[test]
fn regex_differential_fuzz() {
    if !tools_available() {
        eprintln!("regex_differential_fuzz: skipping (no almide+wasm runtime)");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let cases = cases_per_op();
    let mut rng = Rng(SEED);
    for op in OPS {
        let src = build_program(&mut rng, op, cases);
        let src_path = dir.path().join(format!("fuzz_{op}.almd"));
        let wasm_path = dir.path().join(format!("fuzz_{op}.wasm"));
        std::fs::write(&src_path, &src).unwrap();

        let native = run_native(&src_path).expect("native");
        let wasm = run_wasm(&src_path, &wasm_path).expect("wasm");

        if native != wasm {
            // find first differing line for a small repro
            let nl: Vec<&str> = native.lines().collect();
            let wl: Vec<&str> = wasm.lines().collect();
            let mut first = String::from("(lengths differ)");
            for (i, (a, b)) in nl.iter().zip(wl.iter()).enumerate() {
                if a != b {
                    first = format!("line {i}: native={a:?} wasm={b:?}");
                    break;
                }
            }
            panic!(
                "regex op `{op}` diverged native↔wasm.\nFirst diff: {first}\n\
                 (regenerate / inspect: seed={SEED:#x}, cases={cases})"
            );
        }
    }
}

/// Materialize a small deterministic sample into the committed wasm_cross corpus
/// so the spec gate exercises the fuzz grammar too. Run with
/// `ALMIDE_WRITE_FUZZ_CORPUS=1` to (re)generate the file; otherwise this test is
/// a no-op assertion that the committed file exists.
#[test]
fn regex_fuzz_corpus_present() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("spec/wasm_cross/regex_fuzz_batch.almd");
    if std::env::var("ALMIDE_WRITE_FUZZ_CORPUS").is_ok() {
        let mut rng = Rng(SEED ^ 0xA5A5_A5A5_A5A5_A5A5);
        let mut src = String::from(
            "// GENERATED by tests/regex_fuzz_test.rs (regex_fuzz_corpus_present,\n\
             // ALMIDE_WRITE_FUZZ_CORPUS=1). Deterministic SplitMix64 sample of the\n\
             // regex differential grammar. The wasm_cross gate runs this on native\n\
             // and wasm and asserts byte-identical. Do not edit by hand.\n",
        );
        let body = {
            let mut s = PRELUDE.replace("{{", "{").replace("}}", "}");
            for op in OPS {
                let allow_group = *op == "captures";
                for _ in 0..SAMPLE_CASES_PER_OP {
                    let pat = gen_pattern(&mut rng, allow_group);
                    let hay = gen_haystack(&mut rng);
                    s.push_str(&render_call(op, &pat, &hay));
                }
            }
            s.push_str("}\n");
            s
        };
        src.push_str(&body);
        std::fs::write(&path, src).unwrap();
        eprintln!("wrote {}", path.display());
    }
    assert!(
        path.exists(),
        "missing {} — run with ALMIDE_WRITE_FUZZ_CORPUS=1 to generate",
        path.display()
    );
}
