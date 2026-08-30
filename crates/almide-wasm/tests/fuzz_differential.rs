//! Differential fuzzing of the wasm leg — the first verification net that
//! is INDEPENDENT of the hand-picked 590-fixture corpus (flight-grade gap
//! A-1, ratified 2026-08-19).
//!
//! A seeded, type-directed generator produces Almide SOURCE (so the whole
//! pipeline is exercised: parse → check → lower → emit), aimed at the
//! currently-supported slice surface. For every program:
//!   - checker rejection → counted (generator noise, not a finding);
//!   - emit refusal → counted (the honest-refusal path, not a finding);
//!   - emit success + interpreter exit != 0 → the abort-parity-pending
//!     class, same policy as the burn-up gate: not compared;
//!   - emit success + interpreter exit == 0 → the wasm run MUST succeed
//!     and match the interpreter's stdout byte-for-byte. Anything else is
//!     a FINDING and fails the test with the reproducing seed.
//!
//! The seed range is FIXED (0..N) so CI is deterministic — a green run is
//! a ratchet, not a dice roll. Exploration beyond the range:
//! `ALMIDE_FUZZ_ITERS=5000 ALMIDE_FUZZ_BASE=123456 cargo test ...`.

mod harness;
#[path = "gen/host_oracle.rs"]
mod host_oracle;
use harness::run_wasm;

// ── deterministic RNG (no deps, reproducible everywhere) ────────────────

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        // splitmix-style scramble so small seeds diverge immediately.
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(0x1234_5678_9ABC_DEF1))
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// Uniform in 0..n (n > 0).
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }
}

// ── typed program generator ─────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum Ty {
    Int,
    Bool,
    Str,
    OptInt,
    ListInt,
    /// The fixed preamble record `Pt { px: Int, py: Int }`.
    Rec,
    /// An ANONYMOUS record `{ ax: Int, ay: String }` — field order in the
    /// literal is shuffled half the time (mutant 015's survival exposed
    /// that nothing exercised by-name field placement).
    AnonRec,
    /// The fixed preamble variant `Tr = | Lf(Int) | Nd(Int, Int) | Mt`.
    Vart,
    /// `Map[Int, String]` — exercises entry layout with mixed slot widths.
    MapIS,
    /// `Float` — arithmetic fuzzed freely; printing routes through the
    /// linked self-host Dragon4, the same formatter the oracle uses.
    Float,
    /// `Set[Int]`.
    SetInt,
}

impl Ty {
    fn name(self) -> &'static str {
        match self {
            Ty::Int => "Int",
            Ty::Bool => "Bool",
            Ty::Str => "String",
            Ty::OptInt => "Int?",
            Ty::ListInt => "List[Int]",
            Ty::Rec => "Pt",
            Ty::AnonRec => "{ ax: Int, ay: String }",
            Ty::Vart => "Tr",
            Ty::MapIS => "Map[Int, String]",
            Ty::Float => "Float",
            Ty::SetInt => "Set[Int]",
        }
    }
}

struct Var {
    name: String,
    ty: Ty,
    mutable: bool,
}

include!("gen/stmts_ext.rs");

struct Gen {
    rng: Rng,
    vars: Vec<Var>,
    next_id: usize,
    out: String,
    indent: usize,
    /// The fuel/fs arms need `effect fn main` (and fs its import) —
    /// the header is chosen AFTER generation.
    needs_effect: bool,
    needs_fs: bool,
    needs_fuel_helper: bool,
}

// NB: i64::MIN itself cannot be written as one literal (it is -(MAX+1));
// the corpus's i64_min_literal fixture covers that edge instead.
const INT_POOL: &[i64] = &[0, 1, 2, 3, 7, 10, -1, -5, 42, 999, i64::MAX, i64::MIN + 1, 1 << 40];
const STR_POOL: &[&str] = &["", "a", "hello", "第二行", "🦀", "x y z", "-"];
const FLOAT_POOL: &[&str] = &[
    "0.0", "1.0", "0.5", "-3.25", "2.5", "0.1", "1000000.0", "-0.001",
];

impl Gen {
    fn new(seed: u64) -> Gen {
        Gen { rng: Rng::new(seed), vars: Vec::new(), next_id: 0, out: String::new(), indent: 1, needs_effect: false, needs_fs: false, needs_fuel_helper: false }
    }

    fn fresh(&mut self) -> String {
        self.next_id += 1;
        format!("v{}", self.next_id)
    }

    fn line(&mut self, s: &str) {
        for _ in 0..self.indent {
            self.out.push_str("  ");
        }
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn var_of(&mut self, ty: Ty) -> Option<String> {
        let hits: Vec<&Var> = self.vars.iter().filter(|v| v.ty == ty).collect();
        if hits.is_empty() {
            return None;
        }
        let i = self.rng.below(hits.len());
        Some(hits[i].name.clone())
    }

    /// An expression of type `ty`. `depth` bounds recursion.
    fn expr(&mut self, ty: Ty, depth: usize) -> String {
        if depth == 0 {
            return self.leaf(ty);
        }
        match ty {
            Ty::Int => self.expr_int(depth),
            Ty::Bool => self.expr_bool(depth),
            Ty::Str => self.expr_str(depth),
            Ty::OptInt => self.expr_opt(depth),
            Ty::ListInt => self.expr_list(depth),
            Ty::AnonRec => self.expr_anon(depth),
            Ty::Rec => self.expr_rec(depth),
            Ty::Vart => self.expr_vart(depth),
            Ty::MapIS => self.expr_map(depth),
            Ty::Float => self.expr_float(depth),
            Ty::SetInt => self.expr_set(depth),
        }
    }

    fn expr_int(&mut self, depth: usize) -> String {
        match self.rng.below(8) {
                0 | 1 => self.leaf(Ty::Int),
                2 => {
                    let op = ["+", "-", "*"][self.rng.below(3)];
                    format!("({} {} {})", self.expr(Ty::Int, depth - 1), op, self.expr(Ty::Int, depth - 1))
                }
                // Division/modulo: usually a nonzero literal divisor (the
                // compared class), sometimes a COMPUTED `(e % 3)` divisor
                // that CAN be zero — the guarded-abort leg (exit 1 +
                // stdout-before-abort, C-002) gets fuzz coverage too.
                3 => {
                    let op = ["/", "%"][self.rng.below(2)];
                    let divisor = if self.rng.below(4) == 0 {
                        format!("({} % 3)", self.expr(Ty::Int, depth - 1))
                    } else {
                        [3, 7, 10][self.rng.below(3)].to_string()
                    };
                    format!("({} {op} {divisor})", self.expr(Ty::Int, depth - 1))
                }
                4 => format!(
                    "(if {} then {} else {})",
                    self.expr(Ty::Bool, depth - 1),
                    self.expr(Ty::Int, depth - 1),
                    self.expr(Ty::Int, depth - 1)
                ),
                5 if depth >= 2 => {
                    let (acc, x) = (self.fresh(), self.fresh());
                    let src = self.expr(Ty::ListInt, depth - 1);
                    let init = self.expr(Ty::Int, depth - 1);
                    self.vars.push(Var { name: acc.clone(), ty: Ty::Int, mutable: false });
                    self.vars.push(Var { name: x.clone(), ty: Ty::Int, mutable: false });
                    let body = self.expr(Ty::Int, depth - 1);
                    self.vars.pop();
                    self.vars.pop();
                    format!("list.fold({src}, {init}, ({acc}, {x}) => {body})")
                }
                5 => match self.var_of(Ty::AnonRec) {
                    Some(v) => format!("{v}.ax"),
                    None => match self.var_of(Ty::Rec) {
                        Some(v) => format!("{v}.{}", ["px", "py"][self.rng.below(2)]),
                        None => self.leaf(Ty::Int),
                    },
                },
                6 => match self.var_of(Ty::Rec) {
                    Some(v) => format!("{v}.{}", ["px", "py"][self.rng.below(2)]),
                    None => match self.var_of(Ty::MapIS) {
                        Some(m) => format!("map.len({m})"),
                        None => match self.var_of(Ty::SetInt) {
                            Some(sv) => format!("set.len({sv})"),
                            None => self.leaf(Ty::Int),
                        },
                    },
                },
                _ => format!("({} ?? {})", self.expr(Ty::OptInt, depth - 1), self.expr(Ty::Int, depth - 1)),
        }
    }

    fn expr_bool(&mut self, depth: usize) -> String {
            match self.rng.below(5) {
                0 => self.leaf(Ty::Bool),
                1 | 2 => {
                    let op = ["==", "!=", "<", ">", "<=", ">="][self.rng.below(6)];
                    format!("({} {} {})", self.expr(Ty::Int, depth - 1), op, self.expr(Ty::Int, depth - 1))
                }
                3 => {
                    let op = ["and", "or"][self.rng.below(2)];
                    format!("({} {} {})", self.expr(Ty::Bool, depth - 1), op, self.expr(Ty::Bool, depth - 1))
                }
                _ => format!("(not {})", self.expr(Ty::Bool, depth - 1)),
            }
    }

    fn expr_str(&mut self, depth: usize) -> String {
            match (self.rng.below(6), self.var_of(Ty::MapIS)) {
                (0, Some(m)) => format!(
                    "map.get_or({m}, {}, {})",
                    self.expr(Ty::Int, depth.saturating_sub(1)),
                    self.leaf(Ty::Str)
                ),
                _ => self.str_expr_core(depth),
            }
    }

    fn expr_opt(&mut self, depth: usize) -> String {
            match self.rng.below(4) {
                0 => "none".to_string(),
                1 | 2 => format!("some({})", self.expr(Ty::Int, depth - 1)),
                _ => self.leaf(Ty::OptInt),
            }
    }


    fn expr_anon(&mut self, depth: usize) -> String {
            {
                let ax = self.expr(Ty::Int, depth.saturating_sub(1));
                let ay = self.expr(Ty::Str, depth.saturating_sub(1));
                if self.rng.chance(50) {
                    format!("{{ ay: {ay}, ax: {ax} }}")
                } else {
                    format!("{{ ax: {ax}, ay: {ay} }}")
                }
            }
    }

    fn expr_rec(&mut self, depth: usize) -> String {
            match self.rng.below(3) {
                0 | 1 => format!(
                    "Pt {{ px: {}, py: {} }}",
                    self.expr(Ty::Int, depth - 1),
                    self.expr(Ty::Int, depth - 1)
                ),
                // Functional update — the SpreadRecord path.
                _ => match self.var_of(Ty::Rec) {
                    Some(v) => format!("{{ ...{v}, px: {} }}", self.expr(Ty::Int, depth - 1)),
                    None => format!(
                        "Pt {{ px: {}, py: {} }}",
                        self.expr(Ty::Int, depth - 1),
                        self.expr(Ty::Int, depth - 1)
                    ),
                },
            }
    }

    fn expr_vart(&mut self, depth: usize) -> String {
            match self.rng.below(4) {
                0 => format!("Lf({})", self.expr(Ty::Int, depth - 1)),
                1 => format!(
                    "Nd({}, {})",
                    self.expr(Ty::Int, depth - 1),
                    self.expr(Ty::Int, depth - 1)
                ),
                2 => "Mt".to_string(),
                _ => self.leaf(Ty::Vart),
            }
    }

    fn expr_float(&mut self, depth: usize) -> String {
            match self.rng.below(5) {
                0 | 1 => self.leaf(Ty::Float),
                2 | 3 => {
                    let op = ["+", "-", "*"][self.rng.below(3)];
                    format!(
                        "({} {} {})",
                        self.expr(Ty::Float, depth - 1),
                        op,
                        self.expr(Ty::Float, depth - 1)
                    )
                }
                _ => self.leaf(Ty::Float),
            }
    }

    fn expr_map(&mut self, depth: usize) -> String {
            match (self.rng.below(3), self.var_of(Ty::MapIS)) {
                (0, Some(v)) | (1, Some(v)) => format!(
                    "map.set({v}, {}, {})",
                    self.expr(Ty::Int, depth - 1),
                    self.expr(Ty::Str, depth.saturating_sub(1))
                ),
                _ => "map.new()".to_string(),
            }
    }

    fn expr_set(&mut self, depth: usize) -> String {
            match (self.rng.below(3), self.var_of(Ty::SetInt)) {
                (0, Some(v)) | (1, Some(v)) => {
                    format!("set.insert({v}, {})", self.expr(Ty::Int, depth - 1))
                }
                (2, _) => format!("set.from_list({})", self.expr(Ty::ListInt, depth - 1)),
                _ => "set.new()".to_string(),
            }
    }

    fn str_expr_core(&mut self, depth: usize) -> String {
        match self.rng.below(6) {
            5 => format!("float.to_string({})", self.expr(Ty::Float, depth.saturating_sub(1))),
            0 | 1 => self.leaf(Ty::Str),
            2 => format!("({} + {})", self.expr(Ty::Str, depth - 1), self.expr(Ty::Str, depth - 1)),
            3 => format!("int.to_string({})", self.expr(Ty::Int, depth - 1)),
            _ => {
                let a = self.expr(Ty::Int, depth - 1);
                let b = self.expr(Ty::Str, depth - 1);
                format!("\"${{{a}}}~${{{b}}}\"")
            }
        }
    }

    fn leaf(&mut self, ty: Ty) -> String {
        if self.rng.chance(50)
            && let Some(v) = self.var_of(ty)
        {
            return v;
        }
        let i = self.rng.next() as usize;
        match ty {
            Ty::Int => INT_POOL[i % INT_POOL.len()].to_string(),
            Ty::Bool => if i.is_multiple_of(2) { "true" } else { "false" }.to_string(),
            Ty::Str => format!("\"{}\"", STR_POOL[i % STR_POOL.len()]),
            Ty::OptInt => {
                if i.is_multiple_of(2) {
                    "none".to_string()
                } else {
                    format!("some({})", INT_POOL[i % INT_POOL.len()])
                }
            }
            Ty::ListInt => "[1, 2]".to_string(),
            Ty::Rec => "Pt { px: 1, py: 2 }".to_string(),
            Ty::AnonRec => "{ ax: 7, ay: \"z\" }".to_string(),
            Ty::Vart => ["Lf(3)", "Nd(4, 5)", "Mt"][i % 3].to_string(),
            Ty::MapIS => "map.new()".to_string(),
            Ty::SetInt => "set.new()".to_string(),
            Ty::Float => {
                FLOAT_POOL[i % FLOAT_POOL.len()].to_string()
            }
        }
    }

    fn stmt(&mut self, depth: usize) {
        match self.rng.below(if host_oracle_mode() { 19 } else { 18 }) {
            0..=2 => self.stmt_bind(depth),
            3 | 4 => self.stmt_println(depth),
            5 => self.stmt_assign(depth),
            6 => self.stmt_if(depth),
            7 => self.stmt_for_range(depth),
            8 => self.stmt_match_opt(depth),
            9 => self.stmt_match_variant(depth),
            10 => self.stmt_tuple_destructure(depth),
            11 => self.stmt_enumerate_loop(depth),
            12 => self.stmt_map_from_list(depth),
            13 => self.stmt_range_bind(),
            14 => self.stmt_fuse_pipeline(depth),
            15 => self.stmt_div_edges(),
            16 => self.stmt_fuel_region(),
            17 => self.stmt_cell_capture(),
            18 => self.stmt_fs_roundtrip(),
            _ => self.stmt_push(depth),
        }
    }


    fn stmt_bind(&mut self, depth: usize) {
        let ty = [
            Ty::Int,
            Ty::Bool,
            Ty::Str,
            Ty::OptInt,
            Ty::ListInt,
            Ty::Rec,
            Ty::Vart,
            Ty::MapIS,
            Ty::SetInt,
            Ty::Float,
            Ty::AnonRec,
        ][self.rng.below(11)];
        let name = self.fresh();
        let mutable = self.rng.chance(40);
        let kw = if mutable { "var" } else { "let" };
        let value = self.expr(ty, depth);
        self.line(&format!("{kw} {name}: {} = {value}", ty.name()));
        if ty == Ty::AnonRec {
            // Read BOTH members immediately: any by-name-placement error
            // (mutant 015's class) is observable in every program that
            // binds an anonymous record, whatever the literal's order.
            self.line(&format!("println(\"${{{name}.ax}}|${{{name}.ay}}\")"));
        }
        self.vars.push(Var { name, ty, mutable });
    }

    fn stmt_assign(&mut self, depth: usize) {
        let muts: Vec<(String, Ty)> =
            self.vars.iter().filter(|v| v.mutable).map(|v| (v.name.clone(), v.ty)).collect();
        if muts.is_empty() {
            return self.stmt_bind(depth);
        }
        let (name, ty) = muts[self.rng.below(muts.len())].clone();
        let value = self.expr(ty, depth);
        self.line(&format!("{name} = {value}"));
    }

    fn stmt_println(&mut self, depth: usize) {
        // Interpolation with 1-3 parts over Int/Bool/Str exprs, or a plain
        // string expression.
        if self.rng.chance(60) {
            let n = 1 + self.rng.below(3);
            let mut parts = String::new();
            for k in 0..n {
                if k > 0 {
                    parts.push_str([" ", "|", ""][self.rng.below(3)]);
                }
                let ty = [Ty::Int, Ty::Bool, Ty::Str, Ty::Float, Ty::ListInt]
                    [self.rng.below(5)];
                let e = self.expr(ty, depth.saturating_sub(1));
                parts.push_str(&format!("${{{e}}}"));
            }
            self.line(&format!("println(\"{parts}\")"));
        } else {
            let e = self.expr(Ty::Str, depth);
            self.line(&format!("println({e})"));
        }
    }

    fn stmt_if(&mut self, depth: usize) {
        let cond = self.expr(Ty::Bool, depth);
        self.line(&format!("if {cond} then {{"));
        self.indent += 1;
        let n_before = self.vars.len();
        self.stmt_println(depth.saturating_sub(1));
        self.vars.truncate(n_before);
        self.indent -= 1;
        self.line("} else {");
        self.indent += 1;
        self.stmt_println(depth.saturating_sub(1));
        self.vars.truncate(n_before);
        self.indent -= 1;
        self.line("}");
    }

    fn stmt_for_range(&mut self, depth: usize) {
        let name = self.fresh();
        let hi = 1 + self.rng.below(4);
        self.line(&format!("for {name} in 0..<{hi} {{"));
        self.indent += 1;
        self.vars.push(Var { name, ty: Ty::Int, mutable: false });
        self.stmt_println(depth.saturating_sub(1));
        self.vars.pop();
        self.indent -= 1;
        self.line("}");
    }

    fn stmt_match_opt(&mut self, depth: usize) {
        // A bare `none` subject has no type context — use a typed var
        // (binding one when none is in scope) or a `some(...)`.
        let subj = match self.var_of(Ty::OptInt) {
            Some(v) => v,
            None if self.rng.chance(50) => {
                let name = self.fresh();
                let value = self.expr(Ty::OptInt, depth);
                self.line(&format!("let {name}: Int? = {value}"));
                self.vars.push(Var { name: name.clone(), ty: Ty::OptInt, mutable: false });
                name
            }
            None => format!("some({})", self.expr(Ty::Int, depth)),
        };
        let bound = self.fresh();
        self.line(&format!("match {subj} {{"));
        self.indent += 1;
        self.line(&format!("some({bound}) => println(\"s ${{{bound}}}\"),"));
        self.line("none => println(\"n\"),");
        self.indent -= 1;
        self.line("}");
    }

    fn stmt_match_variant(&mut self, depth: usize) {
        let subj = self.expr(Ty::Vart, depth);
        let (a, b) = (self.fresh(), self.fresh());
        self.line(&format!("match {subj} {{"));
        self.indent += 1;
        if self.rng.chance(30) {
            // A literal ctor arm before the binding arm.
            self.line("Lf(0) => println(\"z\"),");
        }
        self.line(&format!("Lf({a}) => println(\"L ${{{a}}}\"),"));
        self.line(&format!("Nd({a}, {b}) => println(\"N ${{{a}}}|${{{b}}}\"),"));
        self.line("Mt => println(\"M\"),");
        self.indent -= 1;
        self.line("}");
    }





    fn program(mut self) -> String {
        let n = 3 + self.rng.below(6);
        for _ in 0..n {
            self.stmt(2);
        }
        // Always end with an observation so no program is a silent no-op.
        self.stmt_println(2);
        let body = std::mem::take(&mut self.out);
        let mut p = String::new();
        if self.needs_fs {
            p.push_str("import fs\n");
        }
        p.push_str("type Pt = { px: Int, py: Int }\n");
        p.push_str("type Tr = | Lf(Int) | Nd(Int, Int) | Mt\n");
        if self.needs_fuel_helper {
            p.push_str("fn fz_loop(k: Int) -> Int = {\n  var s = 0\n  for i in 0..<k {\n    s = s + i\n  }\n  s\n}\n");
        }
        if self.needs_effect {
            p.push_str("effect fn main() -> Unit = {\n");
        } else {
            p.push_str("fn main() -> Unit = {\n");
        }
        p.push_str(&body);
        p.push_str("}\n");
        p
    }
}

/// The host-oracle mode flag (arm selection must be deterministic per
/// (seed, mode), so the env is read, not the clock).
fn host_oracle_mode() -> bool {
    std::env::var("ALMIDE_FUZZ_HOST_ORACLE").is_ok()
}

/// Probe hook (used by the temporary rejection-sampling probe).
#[allow(dead_code)]
pub fn gen_program_for_probe(seed: u64) -> String {
    Gen::new(seed).program()
}

// ── finding auto-reduction (V-4, rustlantis' --reduce shape) ────────────

/// Does `src` still show a wasm-leg defect? Returns a short description.
/// Any pipeline refusal (checker, emitter) means "no" — reduction may only
/// keep changes that PRESERVE the divergence.
fn still_diverges(src: &str) -> Option<String> {
    let ir = almide_spine::s5::lower_to_ir("reduce.almd", src).ok()?;
    let bytes = almide_wasm::emit_program(&ir).ok()?;
    let interp = almide_spine::s5::run_file("reduce.almd", src).ok()?;
    if interp.exit < 0 {
        return None; // oracle abstained — nothing to preserve
    }
    match run_wasm(&bytes) {
        Err(e) => Some(format!("wasm leg failed: {e}")),
        Ok(r) if r.stdout != interp.stdout || r.exit != interp.exit => Some(format!(
            "interp (exit {}):\n{}\nwasm (exit {}):\n{}",
            interp.exit, interp.stdout, r.exit, r.stdout
        )),
        Ok(_) => None,
    }
}

/// Shrink a diverging program: repeatedly drop line windows (8,4,2,1) while
/// the divergence survives. Brace-unbalanced or use-before-def removals are
/// rejected by the pipeline itself, so soundness is free.
fn reduce(src: &str) -> String {
    let mut cur: Vec<String> = src.lines().map(str::to_string).collect();
    let mut changed = true;
    while changed {
        changed = false;
        for window in [8usize, 4, 2, 1] {
            let mut i = 1; // keep the `fn main` header line
            while i + window < cur.len() {
                // keep the closing brace line intact
                let mut cand = cur.clone();
                cand.drain(i..i + window);
                let cand_src = cand.join("\n");
                if still_diverges(&cand_src).is_some() {
                    cur = cand;
                    changed = true;
                } else {
                    i += 1;
                }
            }
        }
    }
    cur.join("\n")
}

// ── the differential driver ─────────────────────────────────────────────

struct Tally {
    checker_rejected: usize,
    emit_refused: usize,
    /// Interp exit != 0: compared like any run (stdout-before-abort +
    /// exit code) — counted apart so generator drift toward aborts stays
    /// visible in the report.
    abort_class: usize,
    /// The ORACLE abstained (interp exit -2 Unsupported / -3 fuel): the
    /// wasm leg ran but had nothing to compare against. Kept as its own
    /// VISIBLE class — a growing number here is a growing blind spot
    /// (the interp's #1226 heap-bridge burn-down shrinks it).
    oracle_abstained: usize,
    host_compared: usize,
    compared: usize,
}

fn run_seed(seed: u64, tally: &mut Tally) -> Result<(), String> {
    let src = Gen::new(seed).program();
    let path = format!("fuzz_{seed}.almd");
    let ir = match almide_spine::s5::lower_to_ir(&path, &src) {
        Ok(ir) => ir,
        Err(_) => {
            tally.checker_rejected += 1;
            return Ok(());
        }
    };
    let bytes = match almide_wasm::emit_program(&ir) {
        Ok(b) => b,
        Err(almide_wasm::EmitError::Unsupported(_)) => {
            tally.emit_refused += 1;
            return Ok(());
        }
    };
    let interp = almide_spine::s5::run_file(&path, &src)
        .map_err(|e| format!("seed {seed}: interpreter harness error: {e}\n--- src ---\n{src}"))?;
    if interp.exit == -2 || interp.exit == -3 {
        // The THIRD oracle: where the reference interpreter abstains
        // (host fs, over-cap materializations, matrix), the RELEASED
        // native binary referees — set ALMIDE_FUZZ_HOST_ORACLE to its
        // path (CI downloads the pinned release; locally the installed
        // almide). Without it the program stays an honest abstention.
        if let Ok(oracle) = std::env::var("ALMIDE_FUZZ_HOST_ORACLE") {
            // JURISDICTION: the pinned oracle referees only constructs
            // it is demonstrably sound on. Outside it: fuel regions
            // (pre-C-320 — stale ok(0), its #1572) and anon records
            // (its wasm leg misplaces by-name shuffled fields — the
            // class our mutant 015 pins). Both stay interp-refereed in
            // the normal mode; the host leg's unique value is the fs
            // boundary, which neither marker touches.
            if src.contains("fan.bounded") || src.contains("ax:") {
                tally.oracle_abstained += 1;
                return Ok(());
            }
            let (native_stdout, native_stderr, native_exit) =
                host_oracle::native_run(&oracle, seed, &src)?;
            // A native REFUSAL (wall / compiler diagnostic: nonzero exit,
            // EMPTY stdout, non-"Error:" stderr) is the oracle declining
            // to referee — the a877 native walls fan.bounded outside its
            // v1 subset — never a divergence. A genuine runtime abort
            // announces itself as "Error: ..." and stays compared.
            if native_exit != 0
                && native_stdout.is_empty()
                && !native_stderr.contains("Error: ")
            {
                tally.oracle_abstained += 1;
                return Ok(());
            }
            let run = run_wasm(&bytes).map_err(|e| {
                format!("seed {seed}: wasm leg failed to run (host-oracle path): {e}\n--- src ---\n{src}")
            })?;
            if run.stdout != native_stdout || run.exit != native_exit {
                return Err(format!(
                    "seed {seed}: HOST-ORACLE DIVERGENCE\n--- native (exit {native_exit}) ---\n{native_stdout}\n--- wasm (exit {}) ---\n{}\n--- src ---\n{src}",
                    run.exit, run.stdout
                ));
            }
            tally.host_compared += 1;
            return Ok(());
        }
        tally.oracle_abstained += 1;
        return Ok(());
    }
    let run = run_wasm(&bytes).map_err(|e| {
        let reduced = reduce(&src);
        format!(
            "seed {seed}: wasm leg failed to run: {e}\n--- src ---\n{src}\n--- reduced (V-4) ---\n{reduced}"
        )
    })?;
    // Abort rows compare too (stdout-before-abort + exit code): abort
    // parity is a CLAIMED surface now, not a skipped class. stderr stays
    // out of the comparison until the guarded-abort message contract
    // (native runtime error text) is emitted on the wasm leg.
    if run.stdout != interp.stdout || run.exit != interp.exit {
        let reduced = reduce(&src);
        return Err(format!(
            "seed {seed}: DIVERGENCE\n--- interp (exit {}) ---\n{}\n--- wasm (exit {}) ---\n{}\n--- src ---\n{src}\n--- reduced (V-4) ---\n{reduced}\n(permanence rule V-5: land the reduced case as spec/wasm_cross/fuzz_found_*.almd in the fixing PR)",
            interp.exit, interp.stdout, run.exit, run.stdout
        ));
    }
    if interp.exit != 0 {
        tally.abort_class += 1;
    } else {
        tally.compared += 1;
    }
    Ok(())
}

#[test]
fn differential_fuzz_fixed_seed_range() {
    let iters: u64 = std::env::var("ALMIDE_FUZZ_ITERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200);
    let base: u64 =
        std::env::var("ALMIDE_FUZZ_BASE").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut tally = Tally {
        checker_rejected: 0,
        emit_refused: 0,
        abort_class: 0,
        oracle_abstained: 0,
        host_compared: 0,
        compared: 0,
    };
    let mut findings: Vec<String> = Vec::new();
    for seed in base..base + iters {
        if let Err(f) = run_seed(seed, &mut tally) {
            findings.push(f);
        }
    }
    println!(
        "fuzz: {} compared / {} abort-compared / {} host-compared / {} ORACLE-ABSTAINED / {} emit-refused / {} checker-rejected (of {iters})",
        tally.compared,
        tally.abort_class,
        tally.host_compared,
        tally.oracle_abstained,
        tally.emit_refused,
        tally.checker_rejected
    );
    assert!(
        findings.is_empty(),
        "{} differential findings:\n{}",
        findings.len(),
        findings.join("\n\n")
    );
    // The net must actually bite: if generator drift ever collapses the
    // compared count, this gate goes red instead of silently passing.
    let min_compared = iters / 4;
    assert!(
        tally.compared as u64 >= min_compared,
        "only {} compared runs (< {min_compared}) — the generator no longer reaches the supported surface",
        tally.compared
    );
}

