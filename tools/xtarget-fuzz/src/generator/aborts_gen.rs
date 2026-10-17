//! Test files for the `testing.assert_aborts` inspection (`xtarget-fuzz
//! inspect testing.assert_aborts`, judged by `inspect/aborts.rs`).
//!
//! `assert_aborts` is callable only directly in a test block (E099), so every
//! probe is a TEST FILE that `almide test` runs, marked `// @xf-test` on its
//! first line. Each call's closure must capture nothing (E098), so a
//! generated closure body starts from an empty scope: its terms are literals
//! and catalogue calls, bound by `let`s inside the closure, and it ends in
//! one of the defined aborts (ADR-0029 D4) — or, on the failing paths, in no
//! abort or under an expected text the abort does not carry.

use super::catalogue::Signature;
use super::term::{gen_expr, Builder, START_FUEL};
use super::types::GenType;
use crate::rng::SplitMix64;

/// The first line of every test-file probe.
pub const TEST_MARKER: &str = "// @xf-test\n";

/// Is `src` a test-file probe (run by `almide test`, not `almide run`)?
pub fn is_test_probe(src: &str) -> bool {
    src.starts_with(TEST_MARKER)
}

/// How a generated closure ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ending {
    /// `panic("xf-abort " + s)`: C-219's `PANIC: <msg>`.
    Panic,
    /// `xs[list.len(xs) + 1]`: C-067.
    IndexOob,
    /// `a / (b * 0)`: ALS-T6.
    DivZero,
    /// `assert_eq(s, s + "!")`: the T18 block (the closure is a program body).
    Assert,
    /// `fs.read_text(<a missing path>)!`: an effect err ends the child as an
    /// effect `main`'s does, `Error: <msg>`.
    EffectErr,
    /// No abort: the failing path `found: returned normally`.
    Returns,
}

pub const ENDINGS: [Ending; 6] = [Ending::Panic, Ending::IndexOob, Ending::DivZero, Ending::Assert, Ending::EffectErr, Ending::Returns];

impl Ending {
    /// The expected text a passing call names (for `Returns`, the text its
    /// failing call names).
    pub fn expected(self) -> &'static str {
        match self {
            Ending::Panic | Ending::Returns => "PANIC: xf-abort",
            Ending::IndexOob => "index out of bounds",
            Ending::DivZero => "division by zero",
            Ending::Assert => "Error: assertion failed",
            Ending::EffectErr => "fs.read_text",
        }
    }
}

/// An expected text no defined abort prints: the failing path whose block
/// names the abort that did happen (`found: abort "…"`).
pub const NEVER_PRINTED: &str = "xf-never-printed";

/// One `assert_aborts` call: the expected text and the closure's body lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Site {
    pub expected: String,
    pub body: Vec<String>,
}

/// The catalogue a closure draws from: the served handlers' pure catalogue
/// (`serve_gen::handler_catalogue`), minus `testing` — an assert there is
/// its own abort, and `assert_aborts` inside a closure is E099.
pub fn abort_catalogue(catalogue: &[Signature]) -> Vec<Signature> {
    super::serve_gen::handler_catalogue(catalogue).into_iter().filter(|s| s.module != "testing").collect()
}

fn goal(rng: &mut SplitMix64) -> GenType {
    match rng.below(4) {
        0 => GenType::Int,
        1 => GenType::Bool,
        2 => GenType::List(Box::new(GenType::Int)),
        _ => GenType::String,
    }
}

/// A generated site: up to two bindings of catalogue terms, then `ending`.
/// One in eight passing sites names [`NEVER_PRINTED`] instead of its text.
pub fn synthesize_site(rng: &mut SplitMix64, catalogue: &[Signature], ending: Ending) -> Site {
    let mut b = Builder { rng, catalogue, scope: Vec::new(), stmts: Vec::new(), fuel: 0, fresh: 0, effect: false };
    fn term(b: &mut Builder, ty: GenType, name: &str) {
        b.fuel = START_FUEL;
        let e = gen_expr(b, &ty);
        b.stmts.push(format!("let {name}: {} = {e}", ty.render()));
    }
    let extra = b.rng.below(3);
    for i in 0..extra {
        let g = goal(b.rng);
        term(&mut b, g, &format!("x{i}"));
    }
    match ending {
        Ending::Panic => {
            term(&mut b, GenType::String, "msg");
            b.stmts.push("panic(\"xf-abort \" + msg)".into());
        }
        Ending::IndexOob => {
            term(&mut b, GenType::List(Box::new(GenType::Int)), "xs");
            b.stmts.push("xs[list.len(xs) + 1]".into());
        }
        Ending::DivZero => {
            term(&mut b, GenType::Int, "a");
            term(&mut b, GenType::Int, "z");
            b.stmts.push("a / (z * 0)".into());
        }
        Ending::Assert => {
            term(&mut b, GenType::String, "s");
            b.stmts.push("assert_eq(s, s + \"!\")".into());
        }
        Ending::EffectErr => {
            term(&mut b, GenType::String, "s");
            b.stmts.push("let text = fs.read_text(\"/no/such/xf-abort-dir/probe-\" + int.to_string(string.len(s)))!".into());
            b.stmts.push("println(text)".into());
        }
        Ending::Returns => {
            term(&mut b, GenType::String, "s");
            b.stmts.push("println(s)".into());
        }
    }
    let expected = if ending != Ending::Returns && b.rng.chance(1, 8) { NEVER_PRINTED } else { ending.expected() };
    Site { expected: expected.into(), body: b.stmts }
}

/// A campaign's test file: one test with 1..=3 generated sites, each ending
/// drawn from [`ENDINGS`].
pub fn synthesize_test(rng: &mut SplitMix64, catalogue: &[Signature]) -> String {
    let sites: Vec<Site> = (0..1 + rng.below(3))
        .map(|_| {
            let ending = *rng.pick(&ENDINGS);
            synthesize_site(rng, catalogue, ending)
        })
        .collect();
    test_file("generated", &sites)
}

/// The `testing.assert_aborts(...)` call of one site, indented by `pad`.
pub fn call(site: &Site, pad: &str) -> String {
    let mut s = format!("{pad}testing.assert_aborts(\"{}\", () => {{\n", site.expected);
    for line in &site.body {
        s.push_str(&format!("{pad}  {line}\n"));
    }
    s.push_str(&format!("{pad}}})\n"));
    s
}

/// A test file of one test, `name`, making `sites`' calls in order.
pub fn test_file(name: &str, sites: &[Site]) -> String {
    let mut body = format!("test \"{name}\" {{\n");
    for site in sites {
        body.push_str(&call(site, "  "));
    }
    body.push_str("}\n");
    with_header(&body)
}

fn with_header(body: &str) -> String {
    let imports = super::catalogue::imports_for(&format!("testing.assert_aborts {body}"));
    format!("{TEST_MARKER}// generated by xtarget-fuzz (assert_aborts inspection)\n{imports}\n{body}")
}

/// The complexity probe: `k` calls of ONE site (a loop, so the program is the
/// same size at every k) whose child writes `n` bytes to stderr and then
/// panics. The parent reads the child's stderr whole and finds its last
/// abort header in it, so n is the size the call's own work scans.
pub fn complexity_test(n: u64, k: u64) -> String {
    let body = format!(
        "test \"sized\" {{\n  for _ in 0..<{k} {{\n    testing.assert_aborts(\"PANIC: xf-abort\", () => {{\n      eprintln(string.repeat(\"e\", {n}))\n      panic(\"xf-abort\")\n    }})\n  }}\n}}\n"
    );
    with_header(&body)
}

/// The alloc probe: one site of each ending that aborts, then `k` calls of
/// one site in a loop — every call passes, so the parent reaches its end.
pub fn alloc_test(k: u64) -> String {
    let fixed: Vec<Site> = [
        ("PANIC: xf-abort", vec!["panic(\"xf-abort\")"]),
        ("index out of bounds", vec!["let xs: List[Int] = [1, 2]", "xs[list.len(xs) + 1]"]),
        ("division by zero", vec!["let z: Int = 7 * 0", "10 / z"]),
        ("Error: assertion failed", vec!["assert_eq(\"a\", \"b\")"]),
        ("fs.read_text", vec!["let text = fs.read_text(\"/no/such/xf-abort-dir/probe\")!", "println(text)"]),
    ]
    .into_iter()
    .map(|(e, b)| Site { expected: e.into(), body: b.into_iter().map(String::from).collect() })
    .collect();
    let mut body = String::from("test \"alloc\" {\n");
    for s in &fixed {
        body.push_str(&call(s, "  "));
    }
    body.push_str(&format!("  for _ in 0..<{k} {{\n"));
    body.push_str(&call(&fixed[0], "    "));
    body.push_str("  }\n}\n");
    with_header(&body)
}

/// The diamond's three projects: the closure reaches the same aborting fn
/// directly (`base.boom`) or through `left.boom` / `right.boom`, wrappers
/// over it. `assert_aborts` itself cannot be wrapped (E099), so the diamond
/// is the abort's: the same test reached through the module structure the
/// closure may name. Each test makes a passing call, then a failing one, so
/// both the pass and the failure block are compared across the paths.
pub fn diamond_projects() -> Vec<super::Project> {
    let base = "pub fn boom(tag: String) -> Int = if string.len(tag) > 0 then panic(\"xf-diamond \" + tag) else 0\n";
    let path = "import self.base\n\npub fn boom(tag: String) -> Int = base.boom(tag)\n";
    ["base", "left", "right"]
        .into_iter()
        .map(|m| {
            let callee = format!("{m}.boom(\"t\")");
            let sites = [
                Site { expected: "PANIC: xf-diamond t".into(), body: vec![callee.clone()] },
                Site { expected: NEVER_PRINTED.into(), body: vec![callee] },
            ];
            let test = test_file("diamond", &sites).replacen("import testing\n", &format!("import self.{m}\nimport testing\n"), 1);
            super::Project {
                files: vec![
                    ("app/almide.toml".to_string(), "[package]\nname = \"app\"\nversion = \"0.1.0\"\n".to_string()),
                    ("app/src/base.almd".to_string(), base.to_string()),
                    ("app/src/left.almd".to_string(), path.to_string()),
                    ("app/src/right.almd".to_string(), path.to_string()),
                    ("app/src/main_test.almd".to_string(), test),
                ],
                entry: "app/src/main_test.almd".to_string(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cat() -> Vec<Signature> {
        abort_catalogue(&super::super::build_catalogue())
    }

    #[test]
    fn generated_tests_are_marked_deterministic_and_capture_free_by_construction() {
        let cat = cat();
        assert!(cat.iter().all(|s| !s.effect && s.module != "testing"));
        let gen = |i| synthesize_test(&mut SplitMix64::for_program(11, i), &cat);
        assert_eq!(gen(4), gen(4));
        let many: Vec<String> = (0..300).map(gen).collect();
        for t in &many {
            assert!(is_test_probe(t), "{t}");
            assert!(t.contains("import testing\n"), "{t}");
            assert!(t.contains("test \"generated\" {\n  testing.assert_aborts(\""), "{t}");
            // Nothing in a closure names a test local: the test body holds
            // only the calls themselves.
            assert!(!t.contains("\n  let "), "a binding outside a closure:\n{t}");
        }
        for e in ["xs[list.len(xs) + 1]", "a / (z * 0)", "assert_eq(s, s + \"!\")", "panic(\"xf-abort \" + msg)", "fs.read_text(", NEVER_PRINTED] {
            assert!(many.iter().any(|t| t.contains(e)), "no generated test reaches `{e}`");
        }
        assert!(many.iter().filter(|t| t.contains("fs.read_text(")).all(|t| t.contains("import fs\n")));
    }

    #[test]
    fn each_ending_names_the_text_its_abort_prints() {
        let cat = cat();
        for (i, e) in ENDINGS.into_iter().enumerate() {
            let s = synthesize_site(&mut SplitMix64::for_program(3, i as u64), &cat, e);
            assert!(s.expected == e.expected() || s.expected == NEVER_PRINTED, "{e:?}: {s:?}");
        }
    }

    #[test]
    fn the_complexity_probe_is_one_site_in_a_loop_of_k() {
        let p = complexity_test(64, 5);
        assert!(is_test_probe(&p));
        assert_eq!(p.matches("testing.assert_aborts(").count(), 1, "{p}");
        assert!(p.contains("for _ in 0..<5 {") && p.contains("string.repeat(\"e\", 64)"), "{p}");
        assert_eq!(complexity_test(64, 5).len(), complexity_test(64, 7).len(), "k must not change the program's size");
    }

    #[test]
    fn the_alloc_probe_aborts_every_way_then_loops() {
        let p = alloc_test(8);
        for e in ["PANIC: xf-abort", "index out of bounds", "division by zero", "Error: assertion failed", "fs.read_text"] {
            assert!(p.contains(&format!("testing.assert_aborts(\"{e}\"")), "{e}: {p}");
        }
        assert!(p.contains("for _ in 0..<8 {") && p.contains("import fs\n"), "{p}");
    }

    #[test]
    fn each_diamond_path_reaches_the_abort_through_its_own_module() {
        let ps = diamond_projects();
        assert_eq!(ps.len(), 3);
        for (p, m) in ps.iter().zip(["base", "left", "right"]) {
            let test = &p.files.iter().find(|(f, _)| f == &p.entry).unwrap().1;
            assert!(test.contains(&format!("import self.{m}\n")) && test.contains(&format!("{m}.boom(\"t\")")), "{test}");
            assert!(test.contains(NEVER_PRINTED), "the failing call is there: {test}");
        }
    }
}
