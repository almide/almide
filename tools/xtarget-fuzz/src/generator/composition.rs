//! The **composition family**: operator interleavings whose answer is known
//! BY CONSTRUCTION.
//!
//! ## Why this exists
//!
//! The bug class this family hunts is one cell at a time in the hand-written
//! spec: `??` over a tuple payload (#1904, #1942), `guard … else err(…)!`
//! in an effect fn (#1926), a tuple-pattern lambda as a pipe stage (#1873),
//! a match-bound String captured by a closure (#1925, #1948). Each is an
//! OPERATOR × PAYLOAD CLASS × POSITION cell, and a hand-maintained matrix
//! fills only the cells someone has already stepped on. This family draws
//! the product.
//!
//! ## The oracle
//!
//! Every probe starts from a payload that carries the literal `K` (an Int,
//! a String of length K, a tuple or record holding K) and wraps it in a
//! chain of operator round trips that each hand the SAME payload back:
//! `some(x) ?? d`, `ok(x) ?? d`, `ok(x)!`, `ok(x)? ?? d`, `x |> id`,
//! `match some(x) { some(v) => v, none => d }`, `(x, 7).0`. The fallback
//! `d` always carries `K + 1`, so a wrong branch, a wrong field, or a wrong
//! layout changes the printed number. The program prints `K` once per probe
//! — a literal visible in its own source — so one leg is judged alone
//! (`SelfCheckFailure`), and a program `almide check` refuses is a
//! generator defect (`GeneratorReject`), never a finding. The three rungs
//! "check green ⇒ native compiles ⇒ legs agree" are the existing ladder.
//!
//! Stage 1 (this file): operators `??` (Option and Result), `!`, `?`, `|>`,
//! `match`, tuple wrap/extract; payloads Int / String / tuple-with-heap /
//! record; positions tail, `let`-bound, call argument. Stage 2 adds
//! `guard … else err(…)!`, the match subject, string interpolation, closure
//! bodies, and nested `Option[Result[…]]` payloads.

use crate::rng::SplitMix64;

pub use super::identity::EXPECT_MARKER;

/// What the probe's value is wrapped in. Each class has a literal for `K`,
/// a literal for the fallback `K + 1`, and an extraction back to the Int.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Payload {
    /// `K` itself.
    Int,
    /// A String of length K (`string.repeat("a", K)`), read back by `string.len`.
    Str,
    /// `(K, "t")` — a tuple with a heap element beside the scalar (#1942).
    Tuple,
    /// `P { n: K, s: "r" }` — a record with a heap field beside the scalar.
    Record,
}

const PAYLOADS: [Payload; 4] = [Payload::Int, Payload::Str, Payload::Tuple, Payload::Record];

impl Payload {
    fn suffix(self) -> &'static str {
        match self {
            Payload::Int => "int",
            Payload::Str => "str",
            Payload::Tuple => "tup",
            Payload::Record => "rec",
        }
    }

    fn ty(self) -> &'static str {
        match self {
            Payload::Int => "Int",
            Payload::Str => "String",
            Payload::Tuple => "(Int, String)",
            Payload::Record => "P",
        }
    }

    /// The literal carrying `n`.
    fn literal(self, n: i64) -> String {
        match self {
            Payload::Int => n.to_string(),
            Payload::Str => format!("string.repeat(\"a\", {n})"),
            Payload::Tuple => format!("({n}, \"t\")"),
            Payload::Record => format!("P {{ n: {n}, s: \"r\" }}"),
        }
    }

    /// Read the Int back out of an expression of this payload type.
    fn extract(self, e: &str) -> String {
        match self {
            Payload::Int => e.to_string(),
            Payload::Str => format!("string.len({e})"),
            Payload::Tuple => format!("{e}.0"),
            Payload::Record => format!("{e}.n"),
        }
    }
}

/// One operator round trip: takes the payload expression, hands the same
/// payload back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// `some(x) ?? d`
    OptCoalesce,
    /// `ok(x) ?? d`
    ResCoalesce,
    /// `ok(x)!` — needs an effect fn.
    Bang,
    /// `ok(x)? ?? d` — Result → Option, then the fallback.
    Question,
    /// `x |> id`
    Pipe,
    /// `match some(x) { some(v) => v, none => d }`
    MatchSome,
    /// `(x, 7).0` — the payload as a tuple element, extracted again.
    TupleWrap,
}

const OPS: [Op; 7] = [
    Op::OptCoalesce,
    Op::ResCoalesce,
    Op::Bang,
    Op::Question,
    Op::Pipe,
    Op::MatchSome,
    Op::TupleWrap,
];

/// Where the wrapped expression sits in its fn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    /// `fn p() -> T = <expr>`
    Tail,
    /// `fn p() -> T = { let v = <expr>\n v }`
    Let,
    /// `fn p() -> T = id(<expr>)`
    CallArg,
}

const POSITIONS: [Position; 3] = [Position::Tail, Position::Let, Position::CallArg];

#[derive(Debug, Clone)]
pub struct Probe {
    pub payload: Payload,
    pub ops: Vec<Op>,
    pub position: Position,
    /// Rendered as an `effect fn` (required when `ops` holds `Bang`).
    pub effect: bool,
}

/// A complete self-checking program in structured form, so the shrinker
/// never leaves the family.
#[derive(Debug, Clone)]
pub struct Plan {
    /// The value every probe must print.
    pub k: i64,
    pub probes: Vec<Probe>,
}

impl Plan {
    pub fn size(&self) -> usize {
        self.probes.len()
    }
}

/// Draw a plan: 1–4 probes, each 1–3 operators deep.
pub fn plan(rng: &mut SplitMix64) -> Plan {
    let k = rng.in_range(1, 9);
    let n = 1 + rng.below(4) as usize;
    let probes = (0..n)
        .map(|_| {
            let depth = 1 + rng.below(3) as usize;
            let ops: Vec<Op> = (0..depth).map(|_| *rng.pick(&OPS)).collect();
            let needs_effect = ops.contains(&Op::Bang);
            Probe {
                payload: *rng.pick(&PAYLOADS),
                ops,
                position: *rng.pick(&POSITIONS),
                effect: needs_effect || rng.chance(1, 2),
            }
        })
        .collect();
    Plan { k, probes }
}

/// `indent` is the column of the line the expression starts on: `almide fmt`
/// lays a `match` out over several lines with its arms two columns in from
/// that line, and a committed sample must pass the fmt gate untouched.
fn wrap(op: Op, e: &str, p: Payload, k: i64, indent: usize) -> String {
    let s = p.suffix();
    let d = p.literal(k + 1);
    let pad = " ".repeat(indent);
    match op {
        Op::OptCoalesce => format!("(w_some_{s}({e}) ?? {d})"),
        Op::ResCoalesce => format!("(w_ok_{s}({e}) ?? {d})"),
        Op::Bang => format!("w_ok_{s}({e})!"),
        Op::Question => format!("(w_ok_{s}({e})? ?? {d})"),
        Op::Pipe => format!("({e} |> id_{s})"),
        Op::MatchSome => {
            format!("(match w_some_{s}({e}) {{\n{pad}  some(v) => v,\n{pad}  none => {d},\n{pad}}})")
        }
        Op::TupleWrap => format!("({e}, 7).0"),
    }
}

/// Render the plan to source plus the stdout it must produce.
pub fn render(plan: &Plan) -> (String, String) {
    let k = plan.k;
    let mut expected = String::new();
    let mut fns = String::new();
    let mut main_lines = Vec::new();

    for (i, pr) in plan.probes.iter().enumerate() {
        let indent = match pr.position {
            Position::Let => 2,
            Position::Tail | Position::CallArg => 0,
        };
        let mut e = pr.payload.literal(k);
        for op in &pr.ops {
            e = wrap(*op, &e, pr.payload, k, indent);
        }
        let s = pr.payload.suffix();
        let body = match pr.position {
            Position::Tail => e,
            Position::Let => format!("{{\n  let v = {e}\n  v\n}}"),
            Position::CallArg => format!("id_{s}({e})"),
        };
        let kw = if pr.effect { "effect fn" } else { "fn" };
        fns.push_str(&format!("{kw} probe_{i}() -> {} = {body}\n\n", pr.payload.ty()));
        let call = if pr.effect { format!("probe_{i}()!") } else { format!("probe_{i}()") };
        main_lines.push(format!(
            "  println(\"p{i}=\" + int.to_string({}))",
            pr.payload.extract(&call)
        ));
        expected.push_str(&format!("p{i}={k}\n"));
    }

    let mut src = String::new();
    src.push_str("// generated by xtarget-fuzz — composition family\n");
    src.push_str("//\n");
    src.push_str("// Every probe wraps the literal K in operator round trips that hand the\n");
    src.push_str("// same payload back; the fallbacks carry K + 1. Known without running:\n");
    for line in expected.lines() {
        src.push_str(EXPECT_MARKER);
        src.push_str(line);
        src.push('\n');
    }
    let mut used: Vec<Payload> = plan.probes.iter().map(|p| p.payload).collect();
    used.sort_by_key(|p| p.suffix());
    used.dedup();
    if used.contains(&Payload::Record) {
        src.push_str("type P = { n: Int, s: String }\n\n");
    }
    // Spelled the way `almide fmt` spells it (`T?`, one blank line between
    // top-level items), so a committed sample passes the fmt gate untouched.
    for p in used {
        let (s, t) = (p.suffix(), p.ty());
        src.push_str(&format!("fn id_{s}(x: {t}) -> {t} = x\n\n"));
        src.push_str(&format!("fn w_ok_{s}(x: {t}) -> Result[{t}, String] = ok(x)\n\n"));
        src.push_str(&format!("fn w_some_{s}(x: {t}) -> {t}? = some(x)\n\n"));
    }
    src.push_str(&fns);
    src.push_str("effect fn main() -> Unit = {\n");
    for l in &main_lines {
        src.push_str(l);
        src.push('\n');
    }
    src.push_str("}\n");
    (src, expected)
}

/// Smaller plans to try, coarse→fine. Each is still a composition program
/// with its own expected output.
pub fn shrink(plan: &Plan) -> Vec<Plan> {
    let mut out = Vec::new();
    // 1. Drop one probe.
    if plan.probes.len() > 1 {
        for i in 0..plan.probes.len() {
            let mut p = plan.clone();
            p.probes.remove(i);
            out.push(p);
        }
    }
    // 2. Peel the outermost operator of one probe.
    for i in 0..plan.probes.len() {
        if plan.probes[i].ops.is_empty() {
            continue;
        }
        let mut p = plan.clone();
        p.probes[i].ops.pop();
        p.probes[i].effect = p.probes[i].effect && plan.probes[i].effect;
        out.push(p);
    }
    // 3. Move a probe to tail position.
    for i in 0..plan.probes.len() {
        if plan.probes[i].position == Position::Tail {
            continue;
        }
        let mut p = plan.clone();
        p.probes[i].position = Position::Tail;
        out.push(p);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parses(src: &str) -> bool {
        let tokens = almide::lexer::Lexer::tokenize(src);
        let mut parser = almide::parser::Parser::new(tokens);
        parser.parse().is_ok()
    }

    #[test]
    fn composition_programs_parse_and_declare_their_answer() {
        for index in 0..300u64 {
            let mut rng = SplitMix64::for_program(0xC0DE, index);
            let p = plan(&mut rng);
            let (src, expected) = render(&p);
            assert!(parses(&src), "composition program {index} did not parse:\n{src}");
            assert_eq!(
                super::super::identity::expected_from_source(&src).as_deref(),
                Some(expected.as_str()),
                "the @expect header must round-trip"
            );
            assert_eq!(expected.lines().count(), p.size());
        }
    }

    /// The family promises well-typed-by-construction programs: a `check`
    /// refusal is a generator defect (`GeneratorReject`), never a finding.
    /// Runs against the workspace `almide` binary when one is built, and
    /// is a no-op otherwise (the parse gate above always runs).
    #[test]
    fn composition_programs_pass_almide_check() {
        let bin = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/release/almide");
        if !bin.exists() {
            eprintln!("skipping: no workspace almide binary");
            return;
        }
        let dir = std::env::temp_dir().join(format!("xtarget-composition-check-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for index in 0..120u64 {
            let mut rng = SplitMix64::for_program(0xC4EC, index);
            let (src, _) = render(&plan(&mut rng));
            let file = dir.join(format!("p{index}.almd"));
            std::fs::write(&file, &src).unwrap();
            let out = std::process::Command::new(&bin)
                .args(["check", file.to_str().unwrap()])
                .output()
                .expect("spawn almide");
            assert!(
                out.status.success(),
                "composition program {index} was rejected by almide check:\n{}\n{src}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_shrink_candidate_is_still_a_composition_program() {
        for index in 0..100u64 {
            let mut rng = SplitMix64::for_program(0x5EED, index);
            let p = plan(&mut rng);
            for c in shrink(&p) {
                let (src, expected) = render(&c);
                assert!(parses(&src), "shrink candidate did not parse:\n{src}");
                assert_eq!(expected.lines().count(), c.size());
                assert!(c.probes.iter().all(|pr| pr.effect || !pr.ops.contains(&Op::Bang)));
            }
        }
    }
}
