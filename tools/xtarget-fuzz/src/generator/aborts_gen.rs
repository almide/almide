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

/// The closure's value type: `assert_aborts` is generic over it
/// (`f: () -> T`), and a generic is measured at Int AND at String (the
/// 10-16 ruling). The value never reaches the parent — the child aborts
/// before it, and a returning closure's value is dropped in the site fn —
/// so T is the type the child's site fn computes and drops.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fill {
    Unit,
    Int,
    String,
}

impl Fill {
    /// The two instantiations the heap criteria run at, labelled as the
    /// harness labels a generic's (`[vars=Int]`, `[vars=String]`).
    pub const MEASURED: [Fill; 2] = [Fill::Int, Fill::String];

    /// A value of this type computed from the String `s` (a fresh heap
    /// String for `Fill::String`); None for Unit.
    pub fn from_string(self, s: &str) -> Option<String> {
        match self {
            Fill::Unit => None,
            Fill::Int => Some(format!("string.len({s})")),
            Fill::String => Some(format!("{s} + \"!\"")),
        }
    }

    /// `panic(msg)` as an expression of this type: the abort in the
    /// then-branch, a value computed from `guard` in the else-branch.
    pub fn panic_valued(self, msg: &str, guard: &str) -> String {
        match self.from_string(guard) {
            None => format!("panic({msg})"),
            Some(v) => format!("if string.len({guard}) >= 0 then panic({msg}) else {v}"),
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

/// A generated site: up to two bindings of catalogue terms, then `ending`,
/// its tail typed `fill` (the closure's T). One in eight passing sites names
/// [`NEVER_PRINTED`] instead of its text.
pub fn synthesize_site(rng: &mut SplitMix64, catalogue: &[Signature], ending: Ending, fill: Fill) -> Site {
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
            b.stmts.push(fill.panic_valued("\"xf-abort \" + msg", "msg"));
        }
        // An element read is valued by the list's element type.
        Ending::IndexOob => {
            let elem = if fill == Fill::String { GenType::String } else { GenType::Int };
            term(&mut b, GenType::List(Box::new(elem)), "xs");
            b.stmts.push("xs[list.len(xs) + 1]".into());
        }
        Ending::DivZero => {
            term(&mut b, GenType::Int, "a");
            term(&mut b, GenType::Int, "z");
            b.stmts.push(if fill == Fill::String { "int.to_string(a / (z * 0))".into() } else { "a / (z * 0)".into() });
        }
        Ending::Assert => {
            term(&mut b, GenType::String, "s");
            b.stmts.push("assert_eq(s, s + \"!\")".into());
            b.stmts.extend(fill.from_string("s"));
        }
        Ending::EffectErr => {
            term(&mut b, GenType::String, "s");
            b.stmts.push("let text = fs.read_text(\"/no/such/xf-abort-dir/probe-\" + int.to_string(string.len(s)))!".into());
            b.stmts.push(fill.from_string("text").unwrap_or_else(|| "println(text)".into()));
        }
        Ending::Returns => {
            term(&mut b, GenType::String, "s");
            b.stmts.push("println(s)".into());
            b.stmts.extend(fill.from_string("s"));
        }
    }
    let expected = if ending != Ending::Returns && b.rng.chance(1, 8) { NEVER_PRINTED } else { ending.expected() };
    Site { expected: expected.into(), body: b.stmts }
}

/// A campaign's test file: one test with 1..=3 generated sites, each ending
/// drawn from [`ENDINGS`] and each closure's value type T from Unit, Int and
/// String — one campaign compares both measured instantiations.
pub fn synthesize_test(rng: &mut SplitMix64, catalogue: &[Signature]) -> String {
    let sites: Vec<Site> = (0..1 + rng.below(3))
        .map(|_| {
            let ending = *rng.pick(&ENDINGS);
            let fill = *rng.pick(&[Fill::Unit, Fill::Int, Fill::String]);
            synthesize_site(rng, catalogue, ending, fill)
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
/// abort header in it, so n is the size the call's own work scans. The
/// closure is valued `fill` (its T).
pub fn complexity_test(n: u64, k: u64, fill: Fill) -> String {
    let site = Site {
        expected: "PANIC: xf-abort".into(),
        body: vec![format!("let e = string.repeat(\"e\", {n})"), "eprintln(e)".into(), fill.panic_valued("\"xf-abort\"", "e")],
    };
    let body = format!("test \"sized\" {{\n  for _ in 0..<{k} {{\n{}  }}\n}}\n", call(&site, "    "));
    with_header(&body)
}

/// The alloc probe: one site of each ending that aborts, then `k` calls of
/// one site in a loop — every call passes, so the parent reaches its end.
/// Every closure is valued `fill` (its T).
pub fn alloc_test(k: u64, fill: Fill) -> String {
    let v = |s: &str| fill.from_string(s).unwrap_or_default();
    let list = match fill {
        Fill::String => "let xs: List[String] = [\"a\", \"b\"]",
        _ => "let xs: List[Int] = [1, 2]",
    };
    let div = if fill == Fill::String { "int.to_string(10 / z)" } else { "10 / z" };
    let fixed: Vec<Site> = [
        ("PANIC: xf-abort", vec!["let m = \"xf-abort\"".to_string(), fill.panic_valued("m", "m")]),
        ("index out of bounds", vec![list.to_string(), "xs[list.len(xs) + 1]".into()]),
        ("division by zero", vec!["let z: Int = 7 * 0".into(), div.to_string()]),
        ("Error: assertion failed", vec!["let a = \"a\"".into(), "assert_eq(a, \"b\")".into(), v("a")]),
        ("fs.read_text", vec!["let text = fs.read_text(\"/no/such/xf-abort-dir/probe\")!".into(), v("text")]),
    ]
    .into_iter()
    .map(|(e, b)| Site { expected: e.into(), body: b.into_iter().filter(|l| !l.is_empty()).collect() })
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
/// `boom` returns `fill`, so the closure's value (its call) is the T.
pub fn diamond_projects(fill: Fill) -> Vec<super::Project> {
    let (t, v) = match fill {
        Fill::String => ("String", "tag + \"!\""),
        _ => ("Int", "0"),
    };
    let base = format!("pub fn boom(tag: String) -> {t} = if string.len(tag) > 0 then panic(\"xf-diamond \" + tag) else {v}\n");
    let path = format!("import self.base\n\npub fn boom(tag: String) -> {t} = base.boom(tag)\n");
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
                    ("app/src/base.almd".to_string(), base.clone()),
                    ("app/src/left.almd".to_string(), path.clone()),
                    ("app/src/right.almd".to_string(), path.clone()),
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
            for fill in [Fill::Unit, Fill::Int, Fill::String] {
                let s = synthesize_site(&mut SplitMix64::for_program(3, i as u64), &cat, e, fill);
                assert!(s.expected == e.expected() || s.expected == NEVER_PRINTED, "{e:?}: {s:?}");
            }
        }
    }

    /// The closure's tail (its value, T) for each fill: a String fill ends in
    /// a String-valued expression, an Int fill in an Int-valued one.
    #[test]
    fn a_fill_sets_the_closures_value_type() {
        let cat = cat();
        let tail = |e, fill| synthesize_site(&mut SplitMix64::for_program(5, 0), &cat, e, fill).body.last().cloned().unwrap();
        assert_eq!(tail(Ending::Panic, Fill::Unit), "panic(\"xf-abort \" + msg)");
        assert_eq!(tail(Ending::Panic, Fill::String), "if string.len(msg) >= 0 then panic(\"xf-abort \" + msg) else msg + \"!\"");
        assert_eq!(tail(Ending::Panic, Fill::Int), "if string.len(msg) >= 0 then panic(\"xf-abort \" + msg) else string.len(msg)");
        assert_eq!(tail(Ending::DivZero, Fill::String), "int.to_string(a / (z * 0))");
        assert_eq!(tail(Ending::Assert, Fill::String), "s + \"!\"");
        assert_eq!(tail(Ending::EffectErr, Fill::String), "text + \"!\"");
        assert_eq!(tail(Ending::Returns, Fill::Int), "string.len(s)");
        let oob = synthesize_site(&mut SplitMix64::for_program(5, 0), &cat, Ending::IndexOob, Fill::String);
        assert!(oob.body.iter().any(|l| l.starts_with("let xs: List[String] = ")), "{oob:?}");
        // The campaign draws all three.
        let many: Vec<String> = (0..300).map(|i| synthesize_test(&mut SplitMix64::for_program(11, i), &cat)).collect();
        for t in ["else msg + \"!\"", "else string.len(msg)", "panic(\"xf-abort \" + msg)\n"] {
            assert!(many.iter().any(|p| p.contains(t)), "no generated test has `{t}`");
        }
    }

    #[test]
    fn the_complexity_probe_is_one_site_in_a_loop_of_k() {
        for fill in Fill::MEASURED {
            let p = complexity_test(64, 5, fill);
            assert!(is_test_probe(&p));
            assert_eq!(p.matches("testing.assert_aborts(").count(), 1, "{p}");
            assert!(p.contains("for _ in 0..<5 {") && p.contains("string.repeat(\"e\", 64)"), "{p}");
            assert_eq!(complexity_test(64, 5, fill).len(), complexity_test(64, 7, fill).len(), "k must not change the program's size");
        }
        assert!(complexity_test(64, 5, Fill::String).contains("else e + \"!\"\n"));
        assert!(complexity_test(64, 5, Fill::Int).contains("else string.len(e)\n"));
    }

    #[test]
    fn the_alloc_probe_aborts_every_way_then_loops() {
        for fill in Fill::MEASURED {
            let p = alloc_test(8, fill);
            for e in ["PANIC: xf-abort", "index out of bounds", "division by zero", "Error: assertion failed", "fs.read_text"] {
                assert!(p.contains(&format!("testing.assert_aborts(\"{e}\"")), "{e}: {p}");
            }
            assert!(p.contains("for _ in 0..<8 {") && p.contains("import fs\n"), "{p}");
        }
        let s = alloc_test(8, Fill::String);
        for v in ["List[String]", "int.to_string(10 / z)", "a + \"!\"", "text + \"!\"", "else m + \"!\""] {
            assert!(s.contains(v), "{v}: {s}");
        }
    }

    #[test]
    fn each_diamond_path_reaches_the_abort_through_its_own_module() {
        for fill in Fill::MEASURED {
            let ps = diamond_projects(fill);
            assert_eq!(ps.len(), 3);
            for (p, m) in ps.iter().zip(["base", "left", "right"]) {
                let test = &p.files.iter().find(|(f, _)| f == &p.entry).unwrap().1;
                assert!(test.contains(&format!("import self.{m}\n")) && test.contains(&format!("{m}.boom(\"t\")")), "{test}");
                assert!(test.contains(NEVER_PRINTED), "the failing call is there: {test}");
            }
        }
        let base = |fill| diamond_projects(fill)[0].files.iter().find(|(f, _)| f == "app/src/base.almd").unwrap().1.clone();
        assert!(base(Fill::String).contains("-> String = ") && base(Fill::Int).contains("-> Int = "));
    }
}
