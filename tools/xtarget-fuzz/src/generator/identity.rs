//! The **identity family** (#1332): generated programs whose expected
//! output is known BY CONSTRUCTION.
//!
//! ## Why this exists
//!
//! The rest of the fuzzer is *differential*: it runs a program on two
//! legs and compares them. That is structurally blind to a bug in
//! anything the two legs SHARE — native-v1 and wasm share the whole
//! frontend, `almide-mir` and the linked IR, so a miscompile there makes
//! both legs identically wrong and the vote comes back unanimous. #1322
//! (a scalar let-alias miscompile that survived 394 green parity
//! fixtures) is exactly that failure mode.
//!
//! The reference interpreter (`InterpOracle`) is an independent third
//! judge, but it *abstains* on a large slice of the language, and an
//! abstention is not a verdict.
//!
//! ## The oracle
//!
//! This module builds each program **backwards from its answer** (the
//! Rustlantis move). Every accumulator starts at a literal `K`, and every
//! statement group emitted between the initializer and the `println` is an
//! **identity transformer**: whatever it does to the accumulator, it
//! provably restores it. So the program must print `K` — a literal that is
//! visible in its own source — and *no* second execution is needed to
//! judge it. One leg being wrong is a finding, even when the other leg
//! agrees.
//!
//! Soundness rests on two properties, both structural rather than
//! measured:
//!
//! 1. **Every `Block` is an identity by algebra.** Each carries its own
//!    inverse (`+n`/`-n`, `*m`/`/m`, `xor n`/`xor n`, swap/swap,
//!    snapshot/restore), or is compensated by a constant the generator
//!    computed while emitting (`while` trip-count × step, `0..<t`'s
//!    triangular number), or is balanced across both arms of a branch so
//!    the taken arm cannot matter. A block's *body* is itself a list of
//!    identity blocks, so nesting composes: the value at a closing op is
//!    exactly what the opening op produced.
//! 2. **No arithmetic can overflow or truncate.** [`Gen::bound`] carries a
//!    conservative upper bound on `|acc|` through generation and refuses
//!    any block that would push it past [`MAX_BOUND`]. Integer division is
//!    the only truncating operation, and it appears solely as the closing
//!    half of a `*m` / `/m` pair, where the dividend is exactly `m ×
//!    (opening value)` — exact for every i64 in range.
//!
//! Neither property depends on the compiler being correct, which is the
//! whole point.
//!
//! ## What it deliberately does NOT cover
//!
//! Int scalars and their control flow, only. No strings, floats, Unicode,
//! collections-as-data, effects, or generics — those have no cheap
//! by-construction inverse, and the existing type-directed synthesis
//! covers them under the differential oracle. A narrow family with a real
//! oracle beats a broad one with none.

use crate::rng::SplitMix64;

/// Conservative ceiling on `|accumulator|` anywhere in a generated
/// program. Two orders of magnitude of headroom under `i64::MAX / 8`, so
/// even the widest intermediate a block can build (`acc * m`, `u + v`)
/// stays exact.
const MAX_BOUND: i64 = 1 << 40;

/// Accumulator count, inclusive range. More than one lets blocks move
/// values BETWEEN mutable slots (the `Swap` shape), which is where scalar
/// alias bugs live.
const MIN_ACCS: i64 = 1;
const MAX_ACCS: i64 = 3;

/// Top-level block count, inclusive range.
const MIN_BLOCKS: i64 = 3;
const MAX_BLOCKS: i64 = 12;

/// Maximum nesting depth of block bodies.
const MAX_DEPTH: usize = 3;

/// Hard ceiling on the TOTAL number of blocks in one program, nested
/// bodies included. `MAX_BLOCKS` bounds only the top level; without this
/// a run of body-bearing draws at depth 0–2 could still fan out.
const TOTAL_BLOCK_BUDGET: usize = 64;

/// Loop trip counts stay small: a nested body runs `trips` times per
/// enclosing level, and the campaign's throughput is the scarce resource.
const MAX_TRIPS: i64 = 6;

/// The `list.get(xs, 0) ?? POISON` fallback. Never taken when the
/// compiler is correct; taking it makes the failure loud rather than
/// accidentally equal to the expected value.
const POISON: i64 = -999_999_919;

/// A condition whose *value* is irrelevant — both arms of every branch
/// that uses one are compensated, so the generator never needs to know
/// which way it goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cond {
    /// `aN > k`
    Gt(i64),
    /// `aN % m == 0`
    ModZero(i64),
    /// A literal — keeps the trivially-foldable case in the sample.
    Lit(bool),
}

impl Cond {
    fn render(&self, acc: &str) -> String {
        match self {
            Cond::Gt(k) => format!("{acc} > {k}"),
            Cond::ModZero(m) => format!("{acc} % {m} == 0"),
            Cond::Lit(b) => b.to_string(),
        }
    }
}

/// One identity transformer. Every variant restores every accumulator to
/// the value it had on entry — see the module docs for why.
#[derive(Debug, Clone)]
pub enum Block {
    /// `acc += n` … `acc -= n`
    AddSub { acc: usize, n: i64, body: Vec<Block> },
    /// `let s = acc; acc += perturb;` … `acc = s`.
    /// THE #1322 shape: sound only if `let s = acc` copies the scalar
    /// instead of aliasing the mutable slot.
    Snapshot { acc: usize, perturb: i64, body: Vec<Block> },
    /// Three-way swap of two accumulators, twice.
    Swap { a: usize, b: usize, body: Vec<Block> },
    /// `acc = 0 - acc` … `acc = 0 - acc`
    Negate { acc: usize, body: Vec<Block> },
    /// `acc = int.bxor(acc, n)` … same again (xor is self-inverse).
    Xor { acc: usize, n: i64, body: Vec<Block> },
    /// `acc = acc * m` … `acc = acc / m` — exact because the body is an
    /// identity, so the dividend is exactly `m × (opening value)`.
    MulDiv { acc: usize, m: i64, body: Vec<Block> },
    /// Round trip through named top-level fns (`xf_add` / `xf_sub`).
    FnRound { acc: usize, n: i64, body: Vec<Block> },
    /// Round trip through two closures bound in scope.
    ClosureRound { acc: usize, n: i64, body: Vec<Block> },
    /// `acc += match k {…}` … `acc -= match k {…}` on the SAME let-bound
    /// subject, so whichever arm is taken cancels itself.
    MatchBalanced { acc: usize, arms: [i64; 3], body: Vec<Block> },
    /// `if c then {acc += n} else {acc -= n}` … then the mirror image.
    BranchBalanced { acc: usize, n: i64, cond: Cond, body: Vec<Block> },
    /// Both arms of a branch contain the SAME identity body, so the taken
    /// arm cannot matter — real lexical nesting inside a branch.
    BranchNest { acc: usize, cond: Cond, body: Vec<Block> },
    /// An identity body run `trips` times inside a `while` — identity^n.
    WhileNest { acc: usize, trips: i64, body: Vec<Block> },
    /// Loop-CARRIED scalar state: `trips` iterations each adding `d`,
    /// compensated afterwards by the literal `d * trips`. With `alias`,
    /// the loop body reads the accumulator into a `let` first (#1322
    /// inside loop-carried state).
    WhileCarry { acc: usize, d: i64, trips: i64, alias: bool },
    /// `for i in 0..<t { acc += i }`, compensated by the triangular number.
    ForRange { acc: usize, trips: i64 },
    /// `for x in [..] { acc += x }` over a list whose elements sum to zero
    /// BY CONSTRUCTION (`[p, q, -p, -q]`) — no compensation needed.
    ForListZero { acc: usize, items: Vec<i64> },
    /// `let p = (acc, w); let (u, v) = p; acc = u + v - w`
    TupleRound { acc: usize, w: i64 },
    /// `let xs = [acc, w]; acc = list.get(xs, 0) ?? POISON`
    ListRound { acc: usize, w: i64 },
    /// `acc = { let b = acc + n; b - n }` — block expression as an rvalue.
    BlockExpr { acc: usize, n: i64 },
}

impl Block {
    /// The nested identity body, for the shrinker's structural walk.
    fn body(&self) -> Option<&Vec<Block>> {
        match self {
            Block::AddSub { body, .. }
            | Block::Snapshot { body, .. }
            | Block::Swap { body, .. }
            | Block::Negate { body, .. }
            | Block::Xor { body, .. }
            | Block::MulDiv { body, .. }
            | Block::FnRound { body, .. }
            | Block::ClosureRound { body, .. }
            | Block::MatchBalanced { body, .. }
            | Block::BranchBalanced { body, .. }
            | Block::BranchNest { body, .. }
            | Block::WhileNest { body, .. } => Some(body),
            Block::WhileCarry { .. }
            | Block::ForRange { .. }
            | Block::ForListZero { .. }
            | Block::TupleRound { .. }
            | Block::ListRound { .. }
            | Block::BlockExpr { .. } => None,
        }
    }

    fn body_mut(&mut self) -> Option<&mut Vec<Block>> {
        match self {
            Block::AddSub { body, .. }
            | Block::Snapshot { body, .. }
            | Block::Swap { body, .. }
            | Block::Negate { body, .. }
            | Block::Xor { body, .. }
            | Block::MulDiv { body, .. }
            | Block::FnRound { body, .. }
            | Block::ClosureRound { body, .. }
            | Block::MatchBalanced { body, .. }
            | Block::BranchBalanced { body, .. }
            | Block::BranchNest { body, .. }
            | Block::WhileNest { body, .. } => Some(body),
            _ => None,
        }
    }

    fn uses_helper_fns(&self) -> bool {
        if matches!(self, Block::FnRound { .. }) {
            return true;
        }
        self.body()
            .is_some_and(|b| b.iter().any(Block::uses_helper_fns))
    }
}

/// A complete self-checking program, still in structured form so the
/// shrinker can remove blocks without ever leaving the identity family.
#[derive(Debug, Clone)]
pub struct Plan {
    /// Initial (and therefore final, and therefore expected) value of
    /// each accumulator.
    pub accs: Vec<i64>,
    pub blocks: Vec<Block>,
    /// Print every accumulator between top-level blocks, not just at the
    /// end. Localizes a failure to a block at the cost of making the
    /// accumulators observable earlier (which can inhibit folding).
    pub checkpoints: bool,
}

impl Plan {
    /// Total block count, including nested bodies — the size measure the
    /// shrinker minimizes.
    pub fn size(&self) -> usize {
        count(&self.blocks)
    }
}

fn count(blocks: &[Block]) -> usize {
    blocks
        .iter()
        .map(|b| 1 + b.body().map_or(0, |x| count(x)))
        .sum()
}

// ── generation ──

struct Gen<'r> {
    rng: &'r mut SplitMix64,
    n_accs: usize,
    /// Conservative upper bound on `|acc|` reachable so far. Monotone
    /// non-decreasing: a block is only emitted if its opening operation
    /// keeps this under [`MAX_BOUND`].
    bound: i64,
    /// Remaining block budget, shared across the whole tree.
    budget: usize,
}

impl Gen<'_> {
    fn acc(&mut self) -> usize {
        self.rng.below(self.n_accs as u32) as usize
    }

    /// Would raising the bound to `next` blow the ceiling?
    fn affords(&self, next: i64) -> bool {
        next > 0 && next <= MAX_BOUND
    }

    fn blocks(&mut self, depth: usize, n: usize) -> Vec<Block> {
        let mut out = Vec::new();
        for _ in 0..n {
            if self.budget == 0 {
                break;
            }
            self.budget -= 1;
            out.push(self.gen_block(depth));
        }
        out
    }

    /// A nested body: shrinks with depth, and empty at the depth cap.
    fn body(&mut self, depth: usize) -> Vec<Block> {
        if depth >= MAX_DEPTH || self.budget == 0 {
            return Vec::new();
        }
        let n = self.rng.in_range(0, 2) as usize;
        self.blocks(depth + 1, n)
    }

    /// A body guaranteed non-empty (for shapes whose braces must not be
    /// empty). Falls back to a single free block at the depth cap.
    fn body_nonempty(&mut self, depth: usize, acc: usize) -> Vec<Block> {
        let b = self.body(depth);
        if b.is_empty() {
            vec![Block::Negate { acc, body: Vec::new() }]
        } else {
            b
        }
    }

    /// Selection weights, indexed exactly like [`Gen::build`]'s `which`.
    /// Biased toward the three shapes #1332 names: loop-carried scalar
    /// state, let-of-var aliasing, branch-arm assignment.
    const BLOCK_WEIGHTS: &'static [u32] = &[
        6, // 0  AddSub
        9, // 1  Snapshot        ← the #1322 let-of-var shape
        5, // 2  Swap
        3, // 3  Negate
        3, // 4  Xor
        3, // 5  MulDiv
        3, // 6  FnRound
        3, // 7  ClosureRound
        4, // 8  MatchBalanced
        7, // 9  BranchBalanced  ← branch-arm assignment
        5, // 10 BranchNest
        5, // 11 WhileNest
        9, // 12 WhileCarry      ← loop-carried scalar state
        4, // 13 ForRange
        4, // 14 ForListZero
        3, // 15 TupleRound
        3, // 16 ListRound
        3, // 17 BlockExpr
    ];

    fn gen_block(&mut self, depth: usize) -> Block {
        // A handful of attempts (a block can decline on the magnitude
        // bound), then a block that costs no headroom at all.
        for _ in 0..8 {
            let which = self.rng.pick_weighted(Self::BLOCK_WEIGHTS);
            if let Some(b) = self.build(which, depth) {
                return b;
            }
        }
        let acc = self.acc();
        Block::Negate { acc, body: Vec::new() }
    }

    fn build(&mut self, which: usize, depth: usize) -> Option<Block> {
        let acc = self.acc();
        match which {
            0 => {
                let n = self.rng.in_range(1, 10_000);
                let next = self.bound + n;
                if !self.affords(next) {
                    return None;
                }
                self.bound = next;
                Some(Block::AddSub { acc, n, body: self.body(depth) })
            }
            1 => {
                let perturb = self.nonzero(1, 10_000);
                let next = self.bound + perturb.abs();
                if !self.affords(next) {
                    return None;
                }
                self.bound = next;
                Some(Block::Snapshot { acc, perturb, body: self.body(depth) })
            }
            2 => {
                if self.n_accs < 2 {
                    return None;
                }
                let a = acc;
                let b = (a + 1 + self.rng.below(self.n_accs as u32 - 1) as usize) % self.n_accs;
                Some(Block::Swap { a, b, body: self.body(depth) })
            }
            3 => Some(Block::Negate { acc, body: self.body(depth) }),
            4 => {
                let n = self.rng.in_range(1, 65_535);
                // |x ^ n| <= 2 * (|x| + n) for every i64 pair in range.
                let next = 2 * (self.bound + n);
                if !self.affords(next) {
                    return None;
                }
                self.bound = next;
                Some(Block::Xor { acc, n, body: self.body(depth) })
            }
            5 => {
                let m = self.rng.in_range(2, 7);
                let next = self.bound.saturating_mul(m);
                if !self.affords(next) {
                    return None;
                }
                self.bound = next;
                Some(Block::MulDiv { acc, m, body: self.body(depth) })
            }
            6 => {
                let n = self.rng.in_range(1, 10_000);
                let next = self.bound + n;
                if !self.affords(next) {
                    return None;
                }
                self.bound = next;
                Some(Block::FnRound { acc, n, body: self.body(depth) })
            }
            7 => {
                let n = self.rng.in_range(1, 10_000);
                let next = self.bound + n;
                if !self.affords(next) {
                    return None;
                }
                self.bound = next;
                Some(Block::ClosureRound { acc, n, body: self.body(depth) })
            }
            8 => {
                let arms = [
                    self.rng.in_range(-10_000, 10_000),
                    self.rng.in_range(-10_000, 10_000),
                    self.rng.in_range(-10_000, 10_000),
                ];
                let widest = arms.iter().map(|a| a.abs()).max().unwrap_or(0);
                let next = self.bound + widest;
                if !self.affords(next) {
                    return None;
                }
                self.bound = next;
                Some(Block::MatchBalanced { acc, arms, body: self.body(depth) })
            }
            9 => {
                let n = self.rng.in_range(1, 10_000);
                let next = self.bound + n;
                if !self.affords(next) {
                    return None;
                }
                self.bound = next;
                let cond = self.cond();
                Some(Block::BranchBalanced { acc, n, cond, body: self.body(depth) })
            }
            10 => {
                let cond = self.cond();
                Some(Block::BranchNest { acc, cond, body: self.body_nonempty(depth, acc) })
            }
            11 => {
                let trips = self.rng.in_range(1, MAX_TRIPS);
                Some(Block::WhileNest { acc, trips, body: self.body(depth) })
            }
            12 => {
                let d = self.nonzero(1, 1_000);
                let trips = self.rng.in_range(1, MAX_TRIPS);
                let next = self.bound + d.abs() * trips;
                if !self.affords(next) {
                    return None;
                }
                self.bound = next;
                let alias = self.rng.chance(1, 2);
                Some(Block::WhileCarry { acc, d, trips, alias })
            }
            13 => {
                let trips = self.rng.in_range(1, 24);
                let next = self.bound + trips * trips;
                if !self.affords(next) {
                    return None;
                }
                self.bound = next;
                Some(Block::ForRange { acc, trips })
            }
            14 => {
                let p = self.nonzero(1, 5_000);
                let q = self.nonzero(1, 5_000);
                let next = self.bound + 2 * (p.abs() + q.abs());
                if !self.affords(next) {
                    return None;
                }
                self.bound = next;
                // Sum is zero by construction; the order is shuffled so the
                // prefix sums are not monotone.
                let mut items = vec![p, q, -p, -q];
                for i in (1..items.len()).rev() {
                    let j = self.rng.below(i as u32 + 1) as usize;
                    items.swap(i, j);
                }
                Some(Block::ForListZero { acc, items })
            }
            15 => {
                let w = self.rng.in_range(-10_000, 10_000);
                let next = self.bound + w.abs();
                if !self.affords(next) {
                    return None;
                }
                self.bound = next;
                Some(Block::TupleRound { acc, w })
            }
            16 => {
                let w = self.rng.in_range(-10_000, 10_000);
                Some(Block::ListRound { acc, w })
            }
            _ => {
                let n = self.rng.in_range(1, 10_000);
                let next = self.bound + n;
                if !self.affords(next) {
                    return None;
                }
                self.bound = next;
                Some(Block::BlockExpr { acc, n })
            }
        }
    }

    fn cond(&mut self) -> Cond {
        match self.rng.below(4) {
            0 => Cond::Gt(self.rng.in_range(-100, 100)),
            1 => Cond::ModZero(self.rng.in_range(2, 5)),
            2 => Cond::Lit(true),
            _ => Cond::Lit(false),
        }
    }

    /// A magnitude in `[lo, hi]` with a random sign, never zero.
    fn nonzero(&mut self, lo: i64, hi: i64) -> i64 {
        let v = self.rng.in_range(lo.max(1), hi);
        if self.rng.chance(1, 2) {
            -v
        } else {
            v
        }
    }
}

/// Deterministically build one identity plan from the RNG state.
pub fn plan(rng: &mut SplitMix64) -> Plan {
    let n_accs = rng.in_range(MIN_ACCS, MAX_ACCS) as usize;
    let mut accs = Vec::with_capacity(n_accs);
    for i in 0..n_accs {
        // Distinct by construction, so a `Swap` that fails to restore is
        // observable rather than accidentally equal.
        accs.push(rng.in_range(-1_000, 1_000) * 4 + i as i64);
    }
    let n_blocks = rng.in_range(MIN_BLOCKS, MAX_BLOCKS) as usize;
    let start_bound = accs.iter().map(|a| a.abs()).max().unwrap_or(0) + 1;
    let checkpoints = rng.chance(1, 2);

    let blocks = {
        let mut g = Gen { rng, n_accs, bound: start_bound, budget: TOTAL_BLOCK_BUDGET };
        let mut out = Vec::new();
        for _ in 0..n_blocks {
            if g.budget == 0 {
                break;
            }
            g.budget -= 1;
            out.push(g.gen_block(0));
        }
        out
    };

    Plan { accs, blocks, checkpoints }
}

// ── rendering ──

/// The marker every expected-stdout line carries in the generated source,
/// so a saved `repro.almd` states its own oracle and
/// [`expected_from_source`] can recover it without the `(seed, index)`.
pub const EXPECT_MARKER: &str = "// @expect ";

struct Renderer {
    lines: Vec<String>,
    fresh: usize,
    indent: usize,
}

impl Renderer {
    fn line(&mut self, s: impl AsRef<str>) {
        let mut out = String::with_capacity(self.indent * 2 + s.as_ref().len());
        for _ in 0..self.indent {
            out.push_str("  ");
        }
        out.push_str(s.as_ref());
        self.lines.push(out);
    }

    fn name(&mut self, prefix: &str) -> String {
        self.fresh += 1;
        format!("{prefix}{}", self.fresh)
    }

    fn blocks(&mut self, blocks: &[Block]) {
        for b in blocks {
            self.block(b);
        }
    }

    fn match_expr(&mut self, lhs: &str, op: char, subject: &str, arms: &[i64; 3]) {
        self.line(format!("{lhs} = {lhs} {op} match {subject} {{"));
        self.indent += 1;
        self.line(format!("0 => {},", arms[0]));
        self.line(format!("1 => {},", arms[1]));
        self.line(format!("_ => {},", arms[2]));
        self.indent -= 1;
        self.line("}");
    }

    fn block(&mut self, b: &Block) {
        match b {
            Block::AddSub { acc, n, body } => {
                let a = acc_name(*acc);
                self.line(format!("{a} = {a} + {n}"));
                self.blocks(body);
                self.line(format!("{a} = {a} - {n}"));
            }
            Block::Snapshot { acc, perturb, body } => {
                let a = acc_name(*acc);
                let s = self.name("snap");
                self.line(format!("let {s} = {a}"));
                self.line(format!("{a} = {a} + {perturb}"));
                self.blocks(body);
                self.line(format!("{a} = {s}"));
            }
            Block::Swap { a, b: b2, body } => {
                let x = acc_name(*a);
                let y = acc_name(*b2);
                let t1 = self.name("swp");
                self.line(format!("let {t1} = {x}"));
                self.line(format!("{x} = {y}"));
                self.line(format!("{y} = {t1}"));
                self.blocks(body);
                let t2 = self.name("swp");
                self.line(format!("let {t2} = {x}"));
                self.line(format!("{x} = {y}"));
                self.line(format!("{y} = {t2}"));
            }
            Block::Negate { acc, body } => {
                let a = acc_name(*acc);
                self.line(format!("{a} = 0 - {a}"));
                self.blocks(body);
                self.line(format!("{a} = 0 - {a}"));
            }
            Block::Xor { acc, n, body } => {
                let a = acc_name(*acc);
                self.line(format!("{a} = int.bxor({a}, {n})"));
                self.blocks(body);
                self.line(format!("{a} = int.bxor({a}, {n})"));
            }
            Block::MulDiv { acc, m, body } => {
                let a = acc_name(*acc);
                self.line(format!("{a} = {a} * {m}"));
                self.blocks(body);
                self.line(format!("{a} = {a} / {m}"));
            }
            Block::FnRound { acc, n, body } => {
                let a = acc_name(*acc);
                self.line(format!("{a} = xf_add({a}, {n})"));
                self.blocks(body);
                self.line(format!("{a} = xf_sub({a}, {n})"));
            }
            Block::ClosureRound { acc, n, body } => {
                let a = acc_name(*acc);
                let f = self.name("up");
                let g = self.name("dn");
                self.line(format!("let {f} = (x: Int) => x + {n}"));
                self.line(format!("let {g} = (x: Int) => x - {n}"));
                self.line(format!("{a} = {f}({a})"));
                self.blocks(body);
                self.line(format!("{a} = {g}({a})"));
            }
            Block::MatchBalanced { acc, arms, body } => {
                let a = acc_name(*acc);
                let k = self.name("key");
                self.line(format!("let {k} = {a} % 3"));
                self.match_expr(&a, '+', &k, arms);
                self.blocks(body);
                self.match_expr(&a, '-', &k, arms);
            }
            Block::BranchBalanced { acc, n, cond, body } => {
                let a = acc_name(*acc);
                let c = self.name("cnd");
                self.line(format!("let {c} = {}", cond.render(&a)));
                self.line(format!(
                    "if {c} then {{ {a} = {a} + {n} }} else {{ {a} = {a} - {n} }}"
                ));
                self.blocks(body);
                self.line(format!(
                    "if {c} then {{ {a} = {a} - {n} }} else {{ {a} = {a} + {n} }}"
                ));
            }
            Block::BranchNest { acc, cond, body } => {
                let a = acc_name(*acc);
                self.line(format!("if {} then {{", cond.render(&a)));
                self.indent += 1;
                self.blocks(body);
                self.indent -= 1;
                self.line("} else {");
                self.indent += 1;
                self.blocks(body);
                self.indent -= 1;
                self.line("}");
            }
            Block::WhileNest { acc, trips, body } => {
                let i = self.name("it");
                self.line(format!("var {i} = 0"));
                self.line(format!("while {i} < {trips} {{"));
                self.indent += 1;
                self.blocks(body);
                self.line(format!("{i} = {i} + 1"));
                self.indent -= 1;
                self.line("}");
                // `it` is read by the loop condition, so no unused binding —
                // but the accumulator must still be referenced if the body
                // was empty, or the loop is dead code the checker may warn on.
                let a = acc_name(*acc);
                self.line(format!("{a} = {a} + 0"));
            }
            Block::WhileCarry { acc, d, trips, alias } => {
                let a = acc_name(*acc);
                let i = self.name("it");
                self.line(format!("var {i} = 0"));
                self.line(format!("while {i} < {trips} {{"));
                self.indent += 1;
                if *alias {
                    let s = self.name("cur");
                    self.line(format!("let {s} = {a}"));
                    self.line(format!("{a} = {s} + {d}"));
                } else {
                    self.line(format!("{a} = {a} + {d}"));
                }
                self.line(format!("{i} = {i} + 1"));
                self.indent -= 1;
                self.line("}");
                self.line(format!("{a} = {a} - {}", d * trips));
            }
            Block::ForRange { acc, trips } => {
                let a = acc_name(*acc);
                let i = self.name("ix");
                self.line(format!("for {i} in 0..<{trips} {{"));
                self.indent += 1;
                self.line(format!("{a} = {a} + {i}"));
                self.indent -= 1;
                self.line("}");
                self.line(format!("{a} = {a} - {}", trips * (trips - 1) / 2));
            }
            Block::ForListZero { acc, items } => {
                let a = acc_name(*acc);
                let x = self.name("el");
                let rendered: Vec<String> = items.iter().map(|v| v.to_string()).collect();
                self.line(format!("for {x} in [{}] {{", rendered.join(", ")));
                self.indent += 1;
                self.line(format!("{a} = {a} + {x}"));
                self.indent -= 1;
                self.line("}");
            }
            Block::TupleRound { acc, w } => {
                let a = acc_name(*acc);
                let p = self.name("pr");
                let u = self.name("fst");
                let v = self.name("snd");
                self.line(format!("let {p} = ({a}, {w})"));
                self.line(format!("let ({u}, {v}) = {p}"));
                self.line(format!("{a} = {u} + {v} - {w}"));
            }
            Block::ListRound { acc, w } => {
                let a = acc_name(*acc);
                let xs = self.name("lst");
                self.line(format!("let {xs}: List[Int] = [{a}, {w}]"));
                self.line(format!("{a} = list.get({xs}, 0) ?? {POISON}"));
            }
            Block::BlockExpr { acc, n } => {
                let a = acc_name(*acc);
                let t = self.name("tmp");
                self.line(format!("{a} = {{"));
                self.indent += 1;
                self.line(format!("let {t} = {a} + {n}"));
                self.line(format!("{t} - {n}"));
                self.indent -= 1;
                self.line("}");
            }
        }
    }
}

fn acc_name(i: usize) -> String {
    format!("a{i}")
}

/// Render a plan to `(source, expected_stdout)`. The two are produced by
/// the same walk, so they cannot drift.
pub fn render(plan: &Plan) -> (String, String) {
    let mut r = Renderer { lines: Vec::new(), fresh: 0, indent: 1 };

    for (i, v) in plan.accs.iter().enumerate() {
        r.line(format!("var {} = {v}", acc_name(i)));
    }

    let mut expected = String::new();
    let checkpoint = |accs: &[i64]| -> String {
        accs.iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join("|")
    };

    for (i, b) in plan.blocks.iter().enumerate() {
        r.block(b);
        if plan.checkpoints {
            let interp: Vec<String> = (0..plan.accs.len())
                .map(|j| format!("${{{}}}", acc_name(j)))
                .collect();
            r.line(format!("println(\"c{i}={}\")", interp.join("|")));
            expected.push_str(&format!("c{i}={}\n", checkpoint(&plan.accs)));
        }
    }

    for (i, v) in plan.accs.iter().enumerate() {
        let a = acc_name(i);
        r.line(format!("println(\"{a}=${{{a}}}\")"));
        expected.push_str(&format!("{a}={v}\n"));
    }

    let needs_helpers = plan.blocks.iter().any(Block::uses_helper_fns);

    let mut src = String::new();
    src.push_str("// generated by xtarget-fuzz — identity family (#1332)\n");
    src.push_str("//\n");
    src.push_str("// Every statement group below is an IDENTITY on the accumulators by\n");
    src.push_str("// construction, so this program's output is known without running it:\n");
    for line in expected.lines() {
        src.push_str(EXPECT_MARKER);
        src.push_str(line);
        src.push('\n');
    }
    src.push('\n');
    if needs_helpers {
        src.push_str("fn xf_add(x: Int, d: Int) -> Int = x + d\n\n");
        src.push_str("fn xf_sub(x: Int, d: Int) -> Int = x - d\n\n");
    }
    src.push_str("fn main() -> Unit = {\n");
    for l in &r.lines {
        src.push_str(l);
        src.push('\n');
    }
    src.push_str("}\n");

    (src, expected)
}

/// Recover the by-construction oracle from a saved identity program.
/// Returns `None` for any source without `@expect` lines (every other
/// family), so the ladder falls back to the differential oracle.
pub fn expected_from_source(src: &str) -> Option<String> {
    let mut out = String::new();
    let mut any = false;
    for line in src.lines() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix(EXPECT_MARKER) {
            any = true;
            out.push_str(rest);
            out.push('\n');
        }
    }
    any.then_some(out)
}

// ── structural shrinking ──
//
// Text-level delta debugging cannot be used here: deleting a line from an
// identity program usually deletes half of an inverse PAIR, which silently
// changes the expected value and turns every candidate into a false
// "still reproduces". So the shrinker works on the PLAN, where every
// candidate is an identity program by the same construction as the
// original and re-renders its own expected output.

/// Smaller plans to try, coarse→fine. Each is still an identity program.
pub fn shrink(plan: &Plan) -> Vec<Plan> {
    let n = plan.size();
    let mut out = Vec::new();

    // 1. Drop one block (with its whole body) — the biggest cut first.
    for i in 0..n {
        let mut c = plan.clone();
        if remove_nth(&mut c.blocks, i, &mut 0) {
            out.push(c);
        }
    }
    // 2. Replace a block by its body (peel one wrapper).
    for i in 0..n {
        let mut c = plan.clone();
        if unwrap_nth(&mut c.blocks, i, &mut 0) {
            out.push(c);
        }
    }
    // 3. Empty a body in place (keep the wrapper, lose its contents).
    for i in 0..n {
        let mut c = plan.clone();
        if clear_nth(&mut c.blocks, i, &mut 0) {
            out.push(c);
        }
    }
    // 4. Drop checkpoint printing (shortest possible observable).
    if plan.checkpoints {
        let mut c = plan.clone();
        c.checkpoints = false;
        out.push(c);
    }
    out
}

/// Remove the `target`-th block in pre-order. `seen` is the running
/// pre-order counter.
fn remove_nth(blocks: &mut Vec<Block>, target: usize, seen: &mut usize) -> bool {
    for i in 0..blocks.len() {
        if *seen == target {
            blocks.remove(i);
            return true;
        }
        *seen += 1;
        if let Some(body) = blocks[i].body_mut() {
            if remove_nth(body, target, seen) {
                return true;
            }
        }
    }
    false
}

/// Replace the `target`-th block with its own body (splice).
fn unwrap_nth(blocks: &mut Vec<Block>, target: usize, seen: &mut usize) -> bool {
    for i in 0..blocks.len() {
        if *seen == target {
            let Some(body) = blocks[i].body().cloned() else {
                return false;
            };
            if body.is_empty() {
                return false;
            }
            blocks.splice(i..=i, body);
            return true;
        }
        *seen += 1;
        if let Some(body) = blocks[i].body_mut() {
            if unwrap_nth(body, target, seen) {
                return true;
            }
        }
    }
    false
}

/// Empty the `target`-th block's body in place.
fn clear_nth(blocks: &mut [Block], target: usize, seen: &mut usize) -> bool {
    for b in blocks.iter_mut() {
        if *seen == target {
            // `BranchNest` renders its body into BOTH arms; emptying it
            // would produce `if c then { } else { }`, which is not a shape
            // the language accepts. The wrapper is dropped instead by the
            // remove/unwrap passes.
            if matches!(b, Block::BranchNest { .. }) {
                return false;
            }
            return match b.body_mut() {
                Some(body) if !body.is_empty() => {
                    body.clear();
                    true
                }
                _ => false,
            };
        }
        *seen += 1;
        if let Some(body) = b.body_mut() {
            if clear_nth(body, target, seen) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parses(src: &str) -> bool {
        let tokens = almide::lexer::Lexer::tokenize(src);
        let mut parser = almide::parser::Parser::new(tokens);
        parser.parse().is_ok()
    }

    /// The whole family rests on this: `(seed, index)` ⇒ one plan.
    #[test]
    fn planning_is_deterministic() {
        for index in [0u64, 1, 13, 400] {
            let a = render(&plan(&mut SplitMix64::for_program(99, index)));
            let b = render(&plan(&mut SplitMix64::for_program(99, index)));
            assert_eq!(a.0, b.0, "source diverged at index {index}");
            assert_eq!(a.1, b.1, "expected stdout diverged at index {index}");
        }
    }

    /// Every generated program must parse — a syntax bug here would show
    /// up as a wave of `GeneratorReject`s and mask real findings.
    #[test]
    fn generated_programs_parse() {
        for index in 0..300u64 {
            let p = plan(&mut SplitMix64::for_program(0xDEED, index));
            let (src, _) = render(&p);
            assert!(parses(&src), "identity program {index} did not parse:\n{src}");
        }
    }

    /// The oracle must be recoverable from the source alone, so a saved
    /// `repro.almd` is self-describing.
    #[test]
    fn expected_round_trips_through_the_source() {
        for index in 0..50u64 {
            let p = plan(&mut SplitMix64::for_program(7, index));
            let (src, expected) = render(&p);
            assert_eq!(
                expected_from_source(&src).as_deref(),
                Some(expected.as_str()),
                "index {index}"
            );
        }
    }

    /// A non-identity source must not be mistaken for one.
    #[test]
    fn plain_sources_have_no_oracle() {
        assert_eq!(expected_from_source("fn main() -> Unit = { println(\"x\") }"), None);
    }

    /// The declared bound discipline: no rendered literal, and no
    /// reachable accumulator value, can approach i64 overflow.
    #[test]
    fn accumulator_bound_is_respected() {
        for index in 0..300u64 {
            let p = plan(&mut SplitMix64::for_program(5, index));
            for v in &p.accs {
                assert!(v.abs() <= 4_004, "init out of range: {v}");
            }
            let (src, _) = render(&p);
            // Every integer literal the renderer emits is bounded by the
            // generator's own magnitude caps.
            for tok in src.split(|c: char| !(c.is_ascii_digit() || c == '-')) {
                if let Ok(v) = tok.parse::<i64>() {
                    assert!(
                        v.abs() <= 1_000_000_000,
                        "literal {v} out of the declared range at index {index}"
                    );
                }
            }
        }
    }

    /// Shrink candidates must stay inside the family: still parseable,
    /// still self-describing, and strictly smaller (or checkpoint-free).
    #[test]
    fn shrink_candidates_are_valid_identity_programs() {
        for index in 0..40u64 {
            let p = plan(&mut SplitMix64::for_program(31, index));
            for c in shrink(&p) {
                let (src, expected) = render(&c);
                assert!(parses(&src), "shrunk candidate did not parse:\n{src}");
                assert_eq!(expected_from_source(&src).as_deref(), Some(expected.as_str()));
                assert!(
                    c.size() < p.size() || (p.checkpoints && !c.checkpoints),
                    "candidate did not shrink"
                );
            }
        }
    }

    /// A plan with no blocks still renders a valid, self-checking program
    /// — the shrinker's fixed point.
    #[test]
    fn empty_plan_renders() {
        let p = Plan { accs: vec![7], blocks: Vec::new(), checkpoints: false };
        let (src, expected) = render(&p);
        assert!(parses(&src));
        assert_eq!(expected, "a0=7\n");
    }
}
