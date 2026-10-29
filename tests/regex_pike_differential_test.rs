//! #4019: the Pike VM answers what the backtracking walk answered.
//!
//! `runtime/rs/src/regex.rs` carries both engines: the linear-time Pike VM
//! every pattern now runs on, and the backtracking walk that used to be the
//! only one. The VM runs an expanded program, or past its state budget a
//! counted one (#4083) — and the counted program is asked every question too,
//! as if every pattern were past the budget. This test includes the shipped
//! file — not a copy — and asks the engines the same questions: whether the pattern matches, the leftmost
//! match's span, every capture group, the full sequence of `find_all` spans, and
//! `full_match`. They must agree on all of it.
//!
//! The questions come from three places:
//! - every pattern and subject the repo already asks: the reference table
//!   `proofs/regex/reference-answers.tsv` and every `regex.<op>("pat", "s")`
//!   call written in `spec/` and `proofs/` (each pattern also against a set of
//!   shared subjects);
//! - a generated corpus (fixed-seed SplitMix64) whose grammar leans on what
//!   makes the two engines differ if the VM is wrong: nested repeated groups,
//!   groups that can match empty, counted and lazy repetition of groups,
//!   assertions inside groups, empty alternation arms — over a small alphabet
//!   so matches are ambiguous;
//! - the pathological patterns, which must finish on long input (they are
//!   exponential for the walk, so only the VM is timed on them).
//!
//! The wasm leg runs the self-hosted twin in `stdlib/regex_engine.almd`; C-032
//! (`tests/regex_fuzz_test.rs`, `spec/wasm_cross/regex_*`) holds it to the
//! native answers.

#[allow(dead_code, clippy::all, clippy::pedantic)]
mod rt {
    pub type AlmideString = String;
    fn almide_abort(msg: impl std::fmt::Display) -> ! {
        panic!("{msg}")
    }
    pub trait IntoAlmide<T> {
        fn into_almide(self) -> T;
    }
    impl<T> IntoAlmide<T> for T {
        fn into_almide(self) -> T {
            self
        }
    }

    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/runtime/rs/src/regex.rs"));

    /// The engines `answers` can ask: the walk, the VM on the program the
    /// public API compiles, and the VM on the counted program (#4083) even
    /// where the expanded one would fit.
    #[derive(Clone, Copy, PartialEq)]
    pub enum Engine {
        Walk,
        Pike,
        Counted,
    }

    /// Everything one engine answers about (pat, s), as text.
    pub fn answers(pat: &str, s: &str, engine: Engine) -> Option<String> {
        let pike = engine != Engine::Walk;
        // The VM side goes through the per-thread cache, so its reuse of a
        // compiled program and working memory is what is tested.
        let c = match engine {
            Engine::Pike => rx_cached(pat),
            Engine::Walk => {
                let mut c = rx_compiled(pat);
                c.prog = None;
                std::rc::Rc::new(c)
            }
            Engine::Counted => {
                let mut c = rx_compiled(pat);
                c.prog = rx_program_as(&c.rx, true);
                std::rc::Rc::new(c)
            }
        };
        if c.prog.is_none() && pike {
            return None;
        }
        let chars: Vec<char> = s.chars().collect();
        let mut m = AlmideRxMatcher::new(&c, &chars, true);
        // The VM side starts where the walk gave up: from the first search.
        m.vm.gave_up = pike;
        let mut out = String::new();
        out += &format!("is_match={} ", m.is_match());
        // leftmost match + groups
        match m.find_at(0) {
            Some((a, b)) => {
                out += &format!("find=({a},{b}) caps={:?} ", m.caps());
            }
            None => out += "find=none ",
        }
        // find_all's walk
        let mut pos = 0;
        let mut spans = Vec::new();
        while pos <= chars.len() {
            let Some((a, b)) = m.find_at(pos) else { break };
            spans.push((a, b));
            pos = if b > a { b } else { b + 1 };
        }
        out += &format!("all={spans:?} ");
        drop(m);
        let mut f = AlmideRxMatcher::new(&c, &chars, false);
        f.vm.gave_up = pike;
        out += &format!("full={}", f.full_match());
        Some(out)
    }

    /// Why the public API refuses `pat` (C-395), or None.
    pub fn check(pat: &str) -> Option<String> {
        rx_check(pat)
    }

    /// The pattern's state count by the checker's count (no expansion).
    pub fn counted_states(pat: &str) -> Option<u64> {
        rx_states(pat).ok()
    }

    /// The expanded program's state count, None past the budget (the
    /// pattern is then compiled counted).
    pub fn pike_states(pat: &str) -> Option<usize> {
        let rx = rx_compile(pat);
        rx_program_as(&rx, false).map(|p| p.insts.len() * p.width)
    }

    /// The program the public API runs: (instructions, counter registers).
    pub fn program_shape(pat: &str) -> (usize, usize) {
        let rx = rx_compile(pat);
        let p = rx_program(&rx).expect("every pattern has a program");
        (p.insts.len(), p.regs)
    }

    pub fn pike_is_match(pat: &str, s: &str) -> bool {
        almide_regex_is_match(pat, s)
    }

    pub fn pike_full_match(pat: &str, s: &str) -> bool {
        almide_regex_full_match(pat, s)
    }

    /// captures through the public entry point, its marks read back
    pub fn pike_captures(pat: &str, s: &str) -> Option<Vec<Option<String>>> {
        almide_regex_captures(pat, s).map(|cs| cs.into_iter().map(|c| c.strip_prefix('1').map(str::to_string)).collect())
    }

    /// is_match on the COUNTED program, the VM from the first search: what
    /// runs past the state limit if the limit is raised.
    pub fn counted_is_match(pat: &str, s: &str) -> bool {
        let mut c = rx_compiled(pat);
        // These patterns are refused, so rx_compiled stood the empty pattern
        // in for them (#4195); compile the real one here.
        c.rx = rx_compile(pat);
        c.prog = rx_program_as(&c.rx, true);
        let chars: Vec<char> = s.chars().collect();
        let mut m = AlmideRxMatcher::new(&c, &chars, false);
        m.vm.gave_up = true;
        m.is_match()
    }

    pub fn find_all(pat: &str, s: &str) -> Vec<String> {
        almide_regex_find_all(pat, s)
    }

    /// find_all's spans with the walk, the VM, or the hybrid the public API
    /// runs (walk first, VM once the walk's budget is spent).
    pub fn spans(pat: &str, s: &str, engine: &str) -> (Vec<(usize, usize)>, bool) {
        let c = if engine == "walk" {
            let mut c = rx_compiled(pat);
            c.prog = None;
            std::rc::Rc::new(c)
        } else {
            rx_cached(pat)
        };
        let chars: Vec<char> = s.chars().collect();
        let mut m = AlmideRxMatcher::new(&c, &chars, true);
        m.vm.gave_up = engine == "pike";
        let mut pos = 0;
        let mut out = Vec::new();
        while pos <= chars.len() {
            let Some((a, b)) = m.find_at(pos) else { break };
            out.push((a, b));
            pos = if b > a { b } else { b + 1 };
        }
        (out, m.vm.gave_up)
    }
}

use std::path::Path;

fn repo() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

// ── PRNG (SplitMix64), the same generator tests/regex_fuzz_test.rs uses ──
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
        xs[self.below(xs.len())]
    }
}

const ATOMS: &[&str] = &["a", "a", "b", "b", "c", ".", "[ab]", "[^a]", "\\w", "\\s", "\\b", "\\B", "^", "$", "本"];
const QUANTS: &[&str] = &[
    "", "", "", "", "*", "+", "?", "*?", "+?", "??", "{2}", "{0,2}", "{1,3}", "{2,}", "{0,}", "{1,2}?", "{2,}?",
    "{0}", "{3,1}",
];

fn gen_alts(rng: &mut Rng, depth: usize) -> String {
    let n = 1 + rng.below(if depth == 0 { 3 } else { 2 });
    (0..n).map(|_| gen_seq(rng, depth)).collect::<Vec<_>>().join("|")
}

fn gen_seq(rng: &mut Rng, depth: usize) -> String {
    let n = if rng.below(6) == 0 { 0 } else { 1 + rng.below(3) };
    let mut s = String::new();
    for _ in 0..n {
        if depth < 2 && rng.below(3) == 0 {
            let open = if rng.below(3) == 0 { "(?:" } else { "(" };
            s += &format!("{open}{}){}", gen_alts(rng, depth + 1), rng.pick(QUANTS));
        } else {
            s += rng.pick(ATOMS);
            s += rng.pick(QUANTS);
        }
    }
    s
}

fn gen_pattern(rng: &mut Rng) -> String {
    let mut p = gen_alts(rng, 0);
    if rng.below(8) == 0 {
        p = format!("(?m){p}");
    }
    p
}

fn gen_subject(rng: &mut Rng) -> String {
    let alpha = ["a", "a", "a", "b", "b", "c", " ", "\n", "本"];
    let n = rng.below(9);
    (0..n).map(|_| rng.pick(&alpha)).collect()
}

const SUBJECTS: &[&str] = &[
    "", "a", "ab", "aab", "abab", "aaaa", "ba", "abc", "a b\nab", "本a本", "hello world", "user@host.com", "12-34",
];

/// Undo an Almide string literal's escapes (`\\`, `\"`, `\n`, `\t`, `\r`).
fn unescape(lit: &str) -> String {
    let mut out = String::new();
    let mut it = lit.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some(o) => out.push(o),
            None => out.push('\\'),
        }
    }
    out
}

/// Every `regex.<op>("pat"[, "s"])` written with literal arguments under `dir`.
fn scrape(dir: &Path, out: &mut Vec<(String, Option<String>)>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            scrape(&p, out);
            continue;
        }
        if p.extension().and_then(|x| x.to_str()) != Some("almd") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&p) else { continue };
        let mut rest = text.as_str();
        while let Some(i) = rest.find("regex.") {
            rest = &rest[i + 6..];
            let Some(paren) = rest.find('(') else { break };
            if !rest[..paren].chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                continue;
            }
            let args = &rest[paren + 1..];
            let lits = string_literals(args, 2);
            if let Some(pat) = lits.first() {
                out.push((unescape(pat), lits.get(1).map(|s| unescape(s))));
            }
        }
    }
}

/// Up to `n` leading `"..."` arguments of a call (stops at the first argument
/// that is not a plain literal).
fn string_literals(args: &str, n: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut s = args.trim_start();
    while out.len() < n && s.starts_with('"') && !s.starts_with("\"\"\"") {
        let b = s.as_bytes();
        let mut i = 1;
        while i < b.len() && b[i] != b'"' {
            if b[i] == b'\\' {
                i += 1;
            }
            i += 1;
        }
        if i >= b.len() {
            break;
        }
        out.push(s[1..i].to_string());
        s = s[i + 1..].trim_start();
        if !s.starts_with(',') {
            break;
        }
        s = s[1..].trim_start();
    }
    out
}

fn compare(pat: &str, s: &str, failures: &mut Vec<String>, checked: &mut usize) {
    if std::env::var_os("REGEX_PIKE_TRACE").is_some() {
        eprintln!("{pat:?} {s:?}");
    }
    let old = rt::answers(pat, s, rt::Engine::Walk).expect("the walk always answers");
    for engine in [rt::Engine::Pike, rt::Engine::Counted] {
        let Some(new) = rt::answers(pat, s, engine) else { continue };
        *checked += 1;
        if new != old && failures.len() < 40 {
            let name = if engine == rt::Engine::Pike { "pike" } else { "counted" };
            failures.push(format!("pattern {pat:?} on {s:?}\n  walk: {old}\n  {name}: {new}"));
        }
    }
}

#[test]
fn pike_vm_answers_what_the_backtracking_walk_answered() {
    let mut failures = Vec::new();
    let mut checked = 0usize;

    // The reference table.
    let tsv = std::fs::read_to_string(repo().join("proofs/regex/reference-answers.tsv")).expect("reference table");
    let mut cases: Vec<(String, Option<String>)> = tsv
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            (f.len() >= 3).then(|| (f[1].to_string(), Some(f[2].to_string())))
        })
        .collect();
    // Every literal call in the spec and proofs.
    scrape(&repo().join("spec"), &mut cases);
    scrape(&repo().join("proofs"), &mut cases);
    cases.sort();
    cases.dedup();
    let mut pats: Vec<String> = cases.iter().map(|(p, _)| p.clone()).collect();
    pats.sort();
    pats.dedup();
    for (pat, s) in &cases {
        if let Some(s) = s {
            compare(pat, s, &mut failures, &mut checked);
        }
    }
    for pat in &pats {
        for s in SUBJECTS {
            compare(pat, s, &mut failures, &mut checked);
        }
    }
    let written = checked;

    // The generated corpus.
    let mut rng = Rng(0x4019_A11C_E5ED_0001);
    let n: usize = std::env::var("REGEX_PIKE_CASES").ok().and_then(|v| v.parse().ok()).unwrap_or(6000);
    for _ in 0..n {
        let pat = gen_pattern(&mut rng);
        for _ in 0..6 {
            let s = gen_subject(&mut rng);
            compare(&pat, &s, &mut failures, &mut checked);
        }
    }
    assert!(written > 5_000, "the written corpus shrank to {written} questions");
    assert!(
        failures.is_empty(),
        "{} of {checked} questions answered differently; first ones:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The shapes that are exponential for the walk finish on long input, and
/// their answers are the obvious ones.
#[test]
fn pathological_patterns_finish_on_long_input() {
    let a100k = "a".repeat(100_000);
    let started = std::time::Instant::now();
    assert!(!rt::pike_is_match("(a+)+b", &(a100k.clone() + "!")));
    assert!(rt::pike_is_match("(a+)+$", &a100k));
    assert!(!rt::pike_full_match("(a+)+", &(a100k.clone() + "!")));
    assert!(!rt::pike_is_match("(a|aa)*b", &a100k));
    assert!(!rt::pike_full_match("(a*)*b", &a100k));
    assert!(!rt::pike_is_match("^(a|a?)+$", &(a100k.clone() + "b")));
    assert!(!rt::pike_is_match("(\\w+\\s?)+$", &(a100k.clone() + "!")));
    let caps = rt::pike_captures("^(a+)+$", &a100k).expect("matches");
    assert_eq!(caps[0].as_deref().map(str::len), Some(100_000));
    let elapsed = started.elapsed();
    // Each line is ~100k steps of a handful of states: well under a second in
    // a debug build. The walk needs longer than the universe's age for any.
    assert!(elapsed.as_secs() < 30, "the pathological rows took {elapsed:?}");
}

/// The public API runs the walk first and hands over to the VM when the walk's
/// step budget runs out — possibly in the middle of a find_all, after the walk
/// has already answered the earlier matches. The hand-over must not show.
#[test]
fn the_walk_hands_over_to_the_vm_without_a_seam() {
    let s = format!("ab aab {}! ab aaab", "a".repeat(3000));
    for pat in ["(a+)+b", "(a|aa)+b", "(a*)*b", "a+b", "ab"] {
        let (pike, _) = rt::spans(pat, &s, "pike");
        let (hybrid, gave_up) = rt::spans(pat, &s, "hybrid");
        assert_eq!(hybrid, pike, "{pat}: the hybrid's spans differ from the VM's");
        if pat != "ab" {
            assert!(gave_up, "{pat}: the walk was expected to run out of steps on the long run of a's (even a+b is quadratic for it: every start rescans the run)");
        } else {
            assert!(!gave_up, "{pat}: a pattern the walk answers in linear time stays on the walk");
        }
        let texts: Vec<String> = pike.iter().map(|&(a, b)| s.chars().skip(a).take(b - a).collect()).collect();
        assert_eq!(rt::find_all(pat, &s), texts);
    }
    // where the walk can answer, the walk and the VM agree on the same text
    let short = "ab aab aaab";
    for pat in ["(a+)+b", "(a|aa)+b", "(a*)*b"] {
        assert_eq!(rt::spans(pat, short, "walk").0, rt::spans(pat, short, "pike").0);
    }
}

/// A huge count does not allocate a huge program: past the state budget the
/// pattern is compiled counted, one copy of each repetition's body and a
/// counter register per nesting level (#4083).
#[test]
fn a_huge_count_stays_bounded() {
    assert!(rt::pike_states("a{10}").is_some());
    assert!(rt::pike_states("(ab){0,1000}").is_some());
    assert_eq!(rt::program_shape("a{10}").1, 0, "a small count stays expanded");
    assert!(rt::pike_states("a{1000000}").is_none());
    assert!(rt::pike_states("((a){1000}){1000}").is_none());
    assert_eq!(rt::program_shape("a{1000000}"), (5, 1));
    let (insts, regs) = rt::program_shape("((a){1000}){1000}");
    assert!(insts < 20 && regs == 2, "a counted program is the size of the pattern: {insts} instructions, {regs} registers");
    let s = "a".repeat(2000);
    assert!(rt::pike_full_match("a{2000}", &s));
    // the public API refuses such a pattern (C-395)
    assert_eq!(rt::check("a{1000000}|a").as_deref(), Some("pattern too large: more than 20000 states"));
    assert_eq!(rt::check("((a){1000}){1000}").as_deref(), Some("pattern too large: more than 20000 states"));
}

/// C-395: the checker counts a pattern's states without expanding it, and the
/// count IS the expanded program's (instructions times states per
/// instruction), so the limit it enforces is the bound the VM runs under.
/// Over every written pattern and the generated corpus, wherever the expanded
/// program fits.
#[test]
fn the_counted_states_are_the_expanded_programs() {
    let mut pats: Vec<String> = Vec::new();
    let mut cases = Vec::new();
    scrape(&repo().join("spec"), &mut cases);
    scrape(&repo().join("proofs"), &mut cases);
    pats.extend(cases.into_iter().map(|(p, _)| p));
    let mut rng = Rng(0x4083_C395_0000_0001);
    for _ in 0..20_000 {
        pats.push(gen_pattern(&mut rng));
    }
    for (count, body) in [(2800, "(a|a)"), (300, "((a|a){10})"), (9000, "a"), (9999, "a"), (1000, "(a|a?)")] {
        pats.push(format!("{body}{{0,{count}}}b"));
        pats.push(format!("^{body}{{1,{count}}}$"));
    }
    let mut compared = 0;
    for p in &pats {
        let (Some(counted), Some(expanded)) = (rt::counted_states(p), rt::pike_states(p)) else { continue };
        assert_eq!(counted, expanded as u64, "{p:?}: the checker counts {counted} states, the expanded program has {expanded}");
        compared += 1;
    }
    assert!(compared > 10_000, "only {compared} patterns compared");
}

/// C-395 + #4083: every accepted pattern matches in time proportional to at
/// most 20,000 states times the subject's length. Shapes exponential for the
/// walk, at the limit, on 100,000 characters through the public API; and the
/// nested pattern just past it is refused instead of run.
///
/// Negative control (recorded in docs/roadmap/active/security-by-construction.md
/// row 9): with the limit check removed (ALMIDE_RX_MAX_STATES raised past
/// it), `((a|aa){1,100}){1,30}b` — 20,001 states, the first refused — runs on
/// the counted program with every state live at every character, and on
/// 100,000 characters this test then took 25.7 s and 46.5 s in two release runs (70 s for
/// the bare call in a benchmark): past its 10 s.
#[test]
fn patterns_at_the_state_limit_finish_on_long_input() {
    let a100k = "a".repeat(100_000);
    let rows = ["^(a|a){0,2800}b", "^((a|a){10}){1,300}b", "^(a|a?){3,1000}$", "^(a|a){2,2800}(a*)$"];
    for pat in rows {
        let states = rt::counted_states(pat).expect("well-formed");
        assert!(states > 15_000 && states <= 20_000, "{pat} has {states} states; the rows are meant to sit at the limit");
    }
    let started = std::time::Instant::now();
    assert!(!rt::pike_is_match("^(a|a){0,2800}b", &a100k));
    assert!(rt::pike_is_match("^(a|a){0,2800}b", &(a100k[..2_000].to_string() + "b")));
    assert!(!rt::pike_full_match("((a|a){10}){1,300}b", &(a100k.clone() + "!")));
    assert!(!rt::pike_is_match("^(a|a?){3,1000}$", &(a100k.clone() + "b")));
    let caps = rt::pike_captures("^(a|a){2,2800}(a*)$", &a100k[..20_000]).expect("matches");
    assert_eq!(caps.iter().map(|c| c.as_deref().map_or(0, str::len)).collect::<Vec<_>>(), vec![20_000, 1, 17_200]);
    // just past the limit: refused before it runs. With the limit removed it
    // runs, and the time limit below is what fails (the negative control).
    let nested = "((a|aa){1,100}){1,30}b";
    match rt::check(nested) {
        Some(reason) => assert_eq!(reason, "pattern too large: more than 20000 states"),
        None => assert!(!rt::pike_is_match(nested, &a100k)),
    }
    let elapsed = started.elapsed();
    // The rows take well under a second in a release build and a few in a
    // debug one; without the limit the nested row alone takes 25-70 s.
    assert!(elapsed.as_secs() < 10, "the rows at the limit took {elapsed:?}");
}

/// #4083: past the expanded program's state budget the VM runs a counted
/// program, one copy of each repeated body and a counter per nesting level.
/// The public API refuses such a pattern (C-395), so this is what runs if the
/// limit is ever raised: shapes exponential for the walk, each count fixed by
/// the position, on 100,000 characters.
#[test]
fn counted_patterns_finish_on_long_input() {
    let a100k = "a".repeat(100_000);
    let rows = ["^(a|a){0,30000}b", "^((a|a)(a|a)){0,30000}b", "((a|a){10}){1,3000}b", "^(a|a?){3,25000}$"];
    for pat in rows {
        assert!(rt::pike_states(pat).is_none(), "{pat} is meant to be past the expanded program's budget");
    }
    let started = std::time::Instant::now();
    assert!(!rt::counted_is_match("^(a|a){0,30000}b", &a100k));
    assert!(rt::counted_is_match("^(a|a){0,30000}b", &(a100k[..20_000].to_string() + "b")));
    assert!(!rt::counted_is_match("^((a|a)(a|a)){0,30000}b", &a100k));
    assert!(!rt::counted_is_match("^((a|a){10}){1,3000}b", &(a100k.clone() + "!")));
    assert!(!rt::counted_is_match("^(a|a?){3,25000}$", &(a100k.clone() + "b")));
    let elapsed = started.elapsed();
    // A handful of live states per character: well under a second in a
    // release build, a few in a debug one.
    assert!(elapsed.as_secs() < 60, "the counted rows took {elapsed:?}");
}

// ── the wasm leg over the same grammar ──

fn almd_lit(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '$' => out.push_str("\\$"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

fn run(dir: &Path, args: &[&str]) -> String {
    let bin = std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string());
    let out = std::process::Command::new(bin).args(args).current_dir(dir).output().expect("spawn almide");
    assert!(out.status.success(), "almide {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The self-hosted engine with its entry points renamed `rxt_*` and the
/// walk's step budget fixed at `budget` (0 = every call goes to the Pike VM,
/// -1 = the walk answers every call); `counted` compiles every program counted
/// (#4083), as a pattern past the expanded program's state budget is. The file
/// is checked as the stdlib's own because the run passes `--stdlib-authority`
/// for its directory, which is what lets it use the `prim` floor outside the
/// bundle (#4100: a `stdlib/` layout alone no longer grants that).
fn engine_copy(budget: &str, counted: bool) -> String {
    let src = std::fs::read_to_string(repo().join("stdlib/regex_engine.almd")).expect("engine source");
    let line = "prim.store64(m + 136, 4 * (pe - pat0 + 1) * (te - ts + 1))";
    assert!(src.contains(line), "the engine's step-budget line moved; update this test");
    let mut out = src.replace(line, &format!("prim.store64(m + 136, {budget})"));
    if counted {
        let cap = "fn __rx_pike_states() -> Int = 20000";
        assert!(out.contains(cap), "the engine's state budget moved; update this test");
        out = out.replace(cap, "fn __rx_pike_states() -> Int = 0");
    }
    for op in ["is_match", "full_match", "find_all", "find", "replace_first", "replace", "split", "captures", "check"] {
        let def = format!("fn regex_{op}(");
        assert!(out.contains(&def), "no {def} in the engine");
        out = out.replace(&def, &format!("fn rxt_{op}("));
    }
    out
}

/// C-032's fuzz (tests/regex_fuzz_test.rs) puts at most one group in a
/// pattern, and on short inputs the wasm leg's walk answers before its step
/// budget runs out, so the self-hosted Pike VM would go untested by it. This
/// runs the nested grammar above three ways and requires one answer: the
/// native public API, the wasm engine forced onto its Pike VM, and the wasm
/// engine forced onto its walk.
#[test]
fn nested_groups_agree_native_and_wasm() {
    let wasmtime = std::process::Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success());
    if !wasmtime {
        eprintln!("nested_groups_agree_native_and_wasm: skipping (no wasmtime)");
        return;
    }
    let seed: u64 = std::env::var("REGEX_PIKE_WASM_SEED").ok().and_then(|v| v.parse().ok()).unwrap_or(0x4019_0000_0000_0002);
    let mut rng = Rng(seed);
    let n: usize = std::env::var("REGEX_PIKE_WASM_CASES").ok().and_then(|v| v.parse().ok()).unwrap_or(300);
    let mut calls = Vec::new();
    // Every generated pattern's verdict is compared (the public
    // regex.compile against the engine's own checker: C-395's reasons), and
    // an accepted one is run through every operation. `RX(p)` is
    // `regex.new(p)` in the public program and `p` in the engine copy, whose
    // entry points take the pattern itself.
    for _ in 0..n {
        let raw = gen_pattern(&mut rng);
        let pat = almd_lit(&raw);
        let s = almd_lit(&gen_subject(&mut rng));
        calls.push(format!("  println(reason(NS_check({pat})))"));
        if rt::check(&raw).is_some() {
            continue;
        }
        calls.push(format!("  println(olist(NS_captures(RX({pat}), {s})))"));
        calls.push(format!("  println(\"[\" + list.join(NS_find_all(RX({pat}), {s}), \"|\") + \"]\")"));
        calls.push(format!("  println(\"${{NS_full_match(RX({pat}), {s})}} ${{NS_is_match(RX({pat}), {s})}}\")"));
        calls.push(format!("  println(\"[\" + list.join(NS_split(RX({pat}), {s}), \"|\") + \"]\")"));
        calls.push(format!("  println(NS_replace(RX({pat}), {s}, \"<>\") + \" \" + NS_replace_first(RX({pat}), {s}, \"<>\"))"));
        calls.push(format!("  println(ostr(NS_find(RX({pat}), {s})))"));
    }
    let main = format!("fn ostr(o: String?) -> String = match o {{ some(x) => \"<\" + x + \">\", none => \"NONE\" }}\n\nfn main() -> Unit = {{\n{}\n}}\n", calls.join("\n"));
    // the public API: a Regex, captures as String?, compile's Result
    let public_body = format!(
        "fn olist(o: List[String?]?) -> String = match o {{ some(xs) => \"[\" + list.join(xs |> list.map((x) => x ?? \"~\"), \"|\") + \"]\", none => \"NONE\" }}\n\n\
         fn reason(r: Result[regex.Regex, String]) -> String = match r {{ ok(_) => \"OK\", err(e) => e }}\n\n{}",
        main.replace("NS_check(", "regex.compile(").replace("NS_full_match(", "regex.is_full_match(").replace("RX(", "regex.new(").replace("NS_", "regex.")
    );
    // the engine copy: the pattern itself, captures marked "1"+text / "0", the check's reason ("" = OK)
    let engine_body = format!(
        "fn olist(o: List[String]?) -> String = match o {{ some(xs) => \"[\" + list.join(xs |> list.map((x) => if string.starts_with(x, \"1\") then string.drop(x, 1) else \"~\"), \"|\") + \"]\", none => \"NONE\" }}\n\n\
         fn reason(r: String) -> String = if r == \"\" then \"OK\" else r\n\n{}",
        main.replace("RX(", "(").replace("NS_", "rxt_")
    );
    let dir = tempfile::tempdir().expect("tempdir");
    let public = dir.path().join("public.almd");
    std::fs::write(&public, format!("import regex\n\n{public_body}")).expect("write");
    let native = run(dir.path(), &["run", public.to_str().expect("path")]);
    for (budget, counted, engine) in [("0", false, "Pike VM"), ("0", true, "counted Pike VM"), ("0 - 1", false, "walk")] {
        let sub = dir.path().join(format!("{}/stdlib", engine.replace(' ', "_")));
        std::fs::create_dir_all(&sub).expect("mkdir");
        let copy = sub.join("regex_engine.almd");
        std::fs::write(&copy, format!("{}\n{}", engine_copy(budget, counted), engine_body)).expect("write");
        // The copy is modified, so its text is not the embedded engine's:
        // the invoker grants it stdlib authority explicitly (#4100).
        let wasm = run(dir.path(), &["run", copy.to_str().expect("path"), "--target", "wasm", "--stdlib-authority", sub.to_str().expect("path")]);
        if native != wasm {
            let first = native
                .lines()
                .zip(wasm.lines())
                .enumerate()
                .find(|(_, (a, b))| a != b)
                .map(|(i, (a, b))| format!("output line {i}: native={a:?} wasm={b:?} (near call: {})", calls.get(i).map_or("", |c| c.as_str())))
                .unwrap_or_else(|| "(lengths differ)".to_string());
            panic!("the wasm engine's {engine} diverged from native over nested groups: {first}");
        }
    }
    assert!(native.lines().count() >= calls.len(), "the program printed less than it asked");
}

/// The negative control for the grant above: the same modified copy without
/// `--stdlib-authority` is refused (E085), so the authority comes from the
/// invoker's flag and not from the `stdlib/` layout a dependency could copy.
#[test]
fn a_modified_stdlib_copy_without_the_flag_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sub = dir.path().join("vm/stdlib");
    std::fs::create_dir_all(&sub).expect("mkdir");
    let copy = sub.join("regex_engine.almd");
    std::fs::write(&copy, engine_copy("0", false)).expect("write");
    let bin = std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string());
    let check = |extra: &[&str]| {
        std::process::Command::new(&bin)
            .args(["check", copy.to_str().expect("path")])
            .args(extra)
            .current_dir(dir.path())
            .output()
            .expect("spawn almide")
    };
    let refused = check(&[]);
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(!refused.status.success() && stderr.contains("E085"), "a modified stdlib copy passed without the flag:\n{stderr}");
    let granted = check(&["--stdlib-authority", sub.to_str().expect("path")]);
    assert!(granted.status.success(), "the granted copy was refused:\n{}", String::from_utf8_lossy(&granted.stderr));
}
