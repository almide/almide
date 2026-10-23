// ---- Regex Runtime ----

#[derive(Clone)]
enum AlmideRxNode {
    Lit(char),
    Dot,
    Class(Vec<(char, char)>, bool), // ranges, negated
    AnchorStart,
    AnchorEnd,
    WordBoundary(bool), // true = \b, false = \B
    Group(Vec<Vec<AlmideRxPiece>>, usize), // alternations, capture index (1-based; 0 = no capture)
}

#[derive(Clone)]
struct AlmideRxPiece {
    node: AlmideRxNode,
    min: usize,
    max: Option<usize>,
    lazy: bool,
}

struct AlmideRxPat {
    alts: Vec<Vec<AlmideRxPiece>>,
    ncap: usize,
    multiline: bool, // a leading (?m): ^ and $ also match at line breaks
    size: usize,     // the pattern's length in chars (the walk's step budget scales with it)
}

type AlmideRxCaps = Vec<Option<(usize, usize)>>;

// ---- Parsing ----

fn rx_compile(pat: &str) -> AlmideRxPat {
    let chars: Vec<char> = pat.chars().collect();
    let mut pos = 0usize;
    let mut ncap = 0usize;
    let multiline = chars.starts_with(&['(', '?', 'm', ')']);
    if multiline {
        pos = 4;
    }
    let alts = rx_parse_alts(&chars, &mut pos, &mut ncap, false);
    AlmideRxPat { alts, ncap, multiline, size: chars.len() }
}

fn rx_parse_alts(chars: &[char], pos: &mut usize, ncap: &mut usize, in_group: bool) -> Vec<Vec<AlmideRxPiece>> {
    let mut alts: Vec<Vec<AlmideRxPiece>> = vec![vec![]];
    while *pos < chars.len() {
        if chars[*pos] == ')' && in_group { break; }
        if chars[*pos] == '|' {
            *pos += 1;
            alts.push(vec![]);
            continue;
        }
        let piece = rx_parse_piece(chars, pos, ncap);
        if let Some(last) = alts.last_mut() { last.push(piece); }
    }
    alts
}

fn rx_parse_piece(chars: &[char], pos: &mut usize, ncap: &mut usize) -> AlmideRxPiece {
    let node = rx_parse_atom(chars, pos, ncap);
    let atom_end = *pos;
    let (min, max) = if *pos < chars.len() {
        match chars[*pos] {
            '*' => { *pos += 1; (0, None) }
            '+' => { *pos += 1; (1, None) }
            '?' => { *pos += 1; (0, Some(1)) }
            '{' => {
                // {n}, {n,}, {n,m}. A malformed brace ({, {a}, {,3}) is left in
                // place and lexes as a literal `{` piece next — same fallback
                // most engines use.
                if let Some((min, max, consumed)) = rx_parse_brace(chars, *pos) {
                    *pos += consumed;
                    (min, max)
                } else {
                    (1, Some(1))
                }
            }
            _ => (1, Some(1)),
        }
    } else {
        (1, Some(1))
    };
    let quantified = *pos > atom_end;
    let lazy = quantified && *pos < chars.len() && chars[*pos] == '?';
    if lazy {
        *pos += 1;
    }
    AlmideRxPiece { node, min, max, lazy }
}

/// Parse a `{n}` / `{n,}` / `{n,m}` quantifier starting at the `{` at `start`.
/// Returns (min, max, chars consumed including both braces), or None if the
/// brace expression is malformed (then the `{` stays a literal).
fn rx_parse_brace(chars: &[char], start: usize) -> Option<(usize, Option<usize>, usize)> {
    let mut i = start + 1; // past '{'
    let mut min_digits = String::new();
    while i < chars.len() && chars[i].is_ascii_digit() {
        min_digits.push(chars[i]);
        i += 1;
    }
    if min_digits.is_empty() { return None; }
    let min: usize = min_digits.parse().ok()?;
    let max = if i < chars.len() && chars[i] == ',' {
        i += 1;
        let mut max_digits = String::new();
        while i < chars.len() && chars[i].is_ascii_digit() {
            max_digits.push(chars[i]);
            i += 1;
        }
        if max_digits.is_empty() { None } else { Some(max_digits.parse().ok()?) }
    } else {
        Some(min)
    };
    if i < chars.len() && chars[i] == '}' {
        Some((min, max, i - start + 1))
    } else {
        None
    }
}

fn rx_parse_atom(chars: &[char], pos: &mut usize, ncap: &mut usize) -> AlmideRxNode {
    let c = chars[*pos];
    *pos += 1;
    match c {
        '.' => AlmideRxNode::Dot,
        '^' => AlmideRxNode::AnchorStart,
        '$' => AlmideRxNode::AnchorEnd,
        '\\' => rx_parse_escape(chars, pos),
        '[' => rx_parse_class(chars, pos),
        '(' => {
            let non_capturing = chars[*pos..].starts_with(&['?', ':']);
            let ci = if non_capturing {
                *pos += 2;
                0
            } else {
                *ncap += 1;
                *ncap
            };
            let alts = rx_parse_alts(chars, pos, ncap, true);
            if *pos < chars.len() && chars[*pos] == ')' { *pos += 1; }
            AlmideRxNode::Group(alts, ci)
        }
        _ => AlmideRxNode::Lit(c),
    }
}

fn rx_parse_escape(chars: &[char], pos: &mut usize) -> AlmideRxNode {
    if *pos >= chars.len() { return AlmideRxNode::Lit('\\'); }
    let c = chars[*pos];
    *pos += 1;
    match c {
        'd' => AlmideRxNode::Class(vec![('0', '9')], false),
        'D' => AlmideRxNode::Class(vec![('0', '9')], true),
        'w' => AlmideRxNode::Class(vec![('a', 'z'), ('A', 'Z'), ('0', '9'), ('_', '_')], false),
        'W' => AlmideRxNode::Class(vec![('a', 'z'), ('A', 'Z'), ('0', '9'), ('_', '_')], true),
        's' => AlmideRxNode::Class(vec![(' ', ' '), ('\t', '\t'), ('\n', '\n'), ('\r', '\r')], false),
        'S' => AlmideRxNode::Class(vec![(' ', ' '), ('\t', '\t'), ('\n', '\n'), ('\r', '\r')], true),
        'b' => AlmideRxNode::WordBoundary(true),
        'B' => AlmideRxNode::WordBoundary(false),
        'n' => AlmideRxNode::Lit('\n'),
        't' => AlmideRxNode::Lit('\t'),
        'r' => AlmideRxNode::Lit('\r'),
        _ => AlmideRxNode::Lit(c),
    }
}

fn rx_parse_class(chars: &[char], pos: &mut usize) -> AlmideRxNode {
    let neg = *pos < chars.len() && chars[*pos] == '^';
    if neg { *pos += 1; }
    let mut ranges: Vec<(char, char)> = vec![];
    // A `]` in the FIRST position is a literal member, not the terminator —
    // POSIX, and every engine follows it (PCRE, Python, Rust, Go, JS). Read as
    // a terminator, `[]]` became the empty class followed by a stray `]`, so
    // the pattern silently never matched: no error, no match, no way to see it
    // from the outside (#2129).
    if *pos < chars.len() && chars[*pos] == ']' {
        ranges.push((']', ']'));
        *pos += 1;
    }
    while *pos < chars.len() && chars[*pos] != ']' {
        if chars[*pos] == '\\' && *pos + 1 < chars.len() {
            *pos += 1;
            let esc = chars[*pos];
            *pos += 1;
            match esc {
                'd' => { ranges.push(('0', '9')); continue; }
                'w' => { ranges.extend_from_slice(&[('a','z'),('A','Z'),('0','9'),('_','_')]); continue; }
                's' => { ranges.extend_from_slice(&[(' ',' '),('\t','\t'),('\n','\n'),('\r','\r')]); continue; }
                'D' => { /* not fully supported in class, treat as literal */ ranges.push((esc, esc)); continue; }
                'n' => { ranges.push(('\n', '\n')); continue; }
                't' => { ranges.push(('\t', '\t')); continue; }
                _ => { ranges.push((esc, esc)); continue; }
            }
        }
        let c = chars[*pos];
        *pos += 1;
        if *pos + 1 < chars.len() && chars[*pos] == '-' && chars[*pos + 1] != ']' {
            *pos += 1;
            let end = chars[*pos];
            *pos += 1;
            ranges.push((c, end));
        } else {
            ranges.push((c, c));
        }
    }
    if *pos < chars.len() { *pos += 1; } // skip ]
    AlmideRxNode::Class(ranges, neg)
}

// ---- Matching ----

fn rx_node_matches(node: &AlmideRxNode, c: char) -> bool {
    match node {
        AlmideRxNode::Lit(l) => *l == c,
        AlmideRxNode::Dot => c != '\n',
        AlmideRxNode::Class(ranges, neg) => {
            let hit = ranges.iter().any(|(lo, hi)| *lo <= c && c <= *hi);
            hit != *neg
        }
        _ => false,
    }
}

// ---- Matching (a backtracking walk whose state lives on the heap, #2307) ----
//
// The search order is the one this engine always had: leftmost alternative
// first, greedy quantifiers longest-first, lazy quantifiers shortest-first, an
// empty repetition instance ends its loop, and a group's capture is recorded
// when the group ends and restored when the walk backtracks past that point.
// What #2307 changed is where the walk keeps its state. It used to be
// continuation-passing Rust calls, one frame chain per matched repetition, so
// `^a*b$` on 30,000 `a`s overflowed the main thread's stack. Now the state is
// three vectors, and the call stack stays flat whatever the input's length:
//
// - `choices`: the untried alternatives, newest last. A failure resumes the
//   newest one. Each records how long the other two vectors were when it was
//   pushed, so resuming it also discards everything done after that point.
// - `frames`: what to do when a group instance ends (record the capture, then
//   loop or stop). A frame names its successor by index, so the continuation
//   is a linked list that choices share.
// - `trail`: the capture slots a group end overwrote, restored on backtrack.
//
// A single-atom repetition (`a*`, `[a-z]+`, `\d{2,}`) needs no frame: greedy
// takes as many as it can in a loop and leaves ONE choice that gives them back
// one at a time; lazy leaves one choice that takes one more. The self-hosted
// twin in stdlib/regex_engine.almd walks the same tree in the same order, so
// the two legs agree on every match, capture and failure.

type AlmideRxSeq = Vec<AlmideRxPiece>;

/// The continuation that ends the walk: the whole pattern has matched.
const ALMIDE_RX_DONE: usize = usize::MAX;

/// A position in the pattern: piece `si` of the sequence `seq`.
#[derive(Clone, Copy)]
struct AlmideRxPc<'a> {
    seq: &'a AlmideRxSeq,
    si: usize,
}

impl<'a> AlmideRxPc<'a> {
    fn piece(self) -> &'a AlmideRxPiece {
        &self.seq[self.si]
    }

    fn next(self) -> Self {
        AlmideRxPc { seq: self.seq, si: self.si + 1 }
    }
}

/// One step of the walk. `k` is the continuation: a frame index, or DONE.
#[derive(Clone, Copy)]
enum AlmideRxGoal<'a> {
    /// Match the sequence from `pc` at `p`, then run `k`.
    Seq { pc: AlmideRxPc<'a>, p: usize, k: usize },
    /// Try the alternatives `alts[ai..]` at `p`, each followed by `k`.
    Alts { alts: &'a Vec<AlmideRxSeq>, ai: usize, p: usize, k: usize },
    /// Start one more instance of the repeated group at `pc` (`count` so far).
    Inst { pc: AlmideRxPc<'a>, p: usize, count: usize, k: usize },
    /// The lazy single atom at `pc` has matched `count` chars ending at `p`.
    Lazy { pc: AlmideRxPc<'a>, p: usize, count: usize, k: usize },
    /// (choices only) The greedy single atom at `pc` matched from `start`:
    /// continue after `count` of its chars, then offer one fewer.
    Back { pc: AlmideRxPc<'a>, start: usize, count: usize, k: usize },
    /// Run continuation `k` at `p`.
    Run { k: usize, p: usize },
    Fail,
}

/// The continuation after instance `count` of the repeated group at `pc`,
/// which began at `start`.
#[derive(Clone, Copy)]
struct AlmideRxFrame<'a> {
    pc: AlmideRxPc<'a>,
    start: usize,
    count: usize,
    next: usize,
}

struct AlmideRxChoice<'a> {
    goal: AlmideRxGoal<'a>,
    frames: usize,
    trail: usize,
}

struct AlmideRxVm<'a> {
    cx: AlmideRxCx<'a>,
    /// full_match: a match must end exactly here.
    end_at: Option<usize>,
    /// Only `captures` reads the slots, so only it pays for writing them.
    track: bool,
    caps: AlmideRxCaps,
    frames: Vec<AlmideRxFrame<'a>>,
    choices: Vec<AlmideRxChoice<'a>>,
    trail: Vec<(usize, Option<(usize, usize)>)>,
    /// Steps taken (a goal, or a character a single-atom loop reads) and the
    /// most it may take (usize::MAX = no bound). Past the bound the walk gives
    /// up and `gave_up` says so: the Pike VM answers instead.
    steps: usize,
    budget: usize,
    gave_up: bool,
}

struct AlmideRxCx<'a> {
    s: &'a [char],
    multiline: bool,
}

fn rx_is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn rx_at_boundary(s: &[char], p: usize) -> bool {
    let before = p > 0 && rx_is_word(s[p - 1]);
    let after = p < s.len() && rx_is_word(s[p]);
    before != after
}

fn rx_zero_width(cx: &AlmideRxCx, node: &AlmideRxNode, p: usize) -> Option<bool> {
    match node {
        AlmideRxNode::AnchorStart => Some(p == 0 || (cx.multiline && cx.s[p - 1] == '\n')),
        AlmideRxNode::AnchorEnd => Some(p == cx.s.len() || (cx.multiline && cx.s[p] == '\n')),
        AlmideRxNode::WordBoundary(want) => Some(rx_at_boundary(cx.s, p) == *want),
        _ => None,
    }
}

impl<'a> AlmideRxVm<'a> {
    fn new(rx: &AlmideRxPat, s: &'a [char], end_at: Option<usize>, track: bool) -> Self {
        AlmideRxVm {
            cx: AlmideRxCx { s, multiline: rx.multiline },
            end_at,
            track,
            caps: vec![None; rx.ncap],
            frames: Vec::new(),
            choices: Vec::new(),
            trail: Vec::new(),
            steps: 0,
            budget: usize::MAX,
            gave_up: false,
        }
    }

    /// The end of the first match the walk reaches from `p`, if any; `caps`
    /// then holds that match's groups.
    fn exec(&mut self, alts: &'a Vec<AlmideRxSeq>, p: usize) -> Option<usize> {
        self.frames.clear();
        self.choices.clear();
        self.trail.clear();
        self.caps.iter_mut().for_each(|c| *c = None);
        let mut goal = self.alts(alts, 0, p, ALMIDE_RX_DONE);
        loop {
            self.steps += 1;
            if self.steps > self.budget {
                self.gave_up = true;
                return None;
            }
            goal = match goal {
                AlmideRxGoal::Seq { pc, p, k } => self.seq(pc, p, k),
                AlmideRxGoal::Alts { alts, ai, p, k } => self.alts(alts, ai, p, k),
                AlmideRxGoal::Inst { pc, p, count, k } => self.inst(pc, p, count, k),
                AlmideRxGoal::Lazy { pc, p, count, k } => self.lazy(pc, p, count, k),
                AlmideRxGoal::Run { k: ALMIDE_RX_DONE, p } if self.end_at.map_or(true, |e| e == p) => return Some(p),
                AlmideRxGoal::Run { k, p } if k != ALMIDE_RX_DONE => self.resume(k, p),
                _ => self.backtrack()?,
            }
        }
    }

    fn push(&mut self, goal: AlmideRxGoal<'a>) {
        self.choices.push(AlmideRxChoice { goal, frames: self.frames.len(), trail: self.trail.len() });
    }

    /// Resume the newest untried alternative, first undoing every capture
    /// write and dropping every frame made since it was left. None = no
    /// alternative is left: the match fails at this start position.
    fn backtrack(&mut self) -> Option<AlmideRxGoal<'a>> {
        let top = self.choices.len().checked_sub(1)?;
        let AlmideRxChoice { goal, frames, trail } = self.choices[top];
        for (slot, saved) in self.trail.drain(trail..).rev() {
            self.caps[slot] = saved;
        }
        self.frames.truncate(frames);
        if let AlmideRxGoal::Back { pc, start, count, k } = goal {
            // Give back one more char next time, down to the minimum.
            if count > pc.piece().min {
                self.choices[top].goal = AlmideRxGoal::Back { pc, start, count: count - 1, k };
            } else {
                self.choices.pop();
            }
            return Some(AlmideRxGoal::Seq { pc: pc.next(), p: start + count, k });
        }
        self.choices.pop();
        Some(goal)
    }

    fn alts(&mut self, alts: &'a Vec<AlmideRxSeq>, ai: usize, p: usize, k: usize) -> AlmideRxGoal<'a> {
        let Some(seq) = alts.get(ai) else { return AlmideRxGoal::Fail };
        if ai + 1 < alts.len() {
            self.push(AlmideRxGoal::Alts { alts, ai: ai + 1, p, k });
        }
        AlmideRxGoal::Seq { pc: AlmideRxPc { seq, si: 0 }, p, k }
    }

    fn seq(&mut self, mut pc: AlmideRxPc<'a>, p: usize, k: usize) -> AlmideRxGoal<'a> {
        while let Some(piece) = pc.seq.get(pc.si) {
            match rx_zero_width(&self.cx, &piece.node, p) {
                Some(true) => pc.si += 1,
                Some(false) => return AlmideRxGoal::Fail,
                None if matches!(piece.node, AlmideRxNode::Group(..)) => return self.rep(pc, p, 0, k),
                None if piece.lazy => return self.lazy(pc, p, 0, k),
                None => return self.greedy(pc, p, k),
            }
        }
        AlmideRxGoal::Run { k, p }
    }

    fn atom_at(&self, pc: AlmideRxPc<'a>, p: usize) -> bool {
        self.cx.s.get(p).is_some_and(|&c| rx_node_matches(&pc.piece().node, c))
    }

    /// A greedy single atom takes as many chars as it can, hands over to the
    /// rest of the sequence, and leaves one choice that gives them back.
    fn greedy(&mut self, pc: AlmideRxPc<'a>, p: usize, k: usize) -> AlmideRxGoal<'a> {
        let piece = pc.piece();
        let max = piece.max.unwrap_or(usize::MAX);
        let mut n = 0;
        while n < max && self.atom_at(pc, p + n) {
            n += 1;
        }
        self.steps += n;
        if n < piece.min {
            return AlmideRxGoal::Fail;
        }
        if n > piece.min {
            self.push(AlmideRxGoal::Back { pc, start: p, count: n - 1, k });
        }
        AlmideRxGoal::Seq { pc: pc.next(), p: p + n, k }
    }

    /// A lazy single atom hands over to the rest as soon as it has its
    /// minimum, leaving one choice that takes one more char.
    fn lazy(&mut self, pc: AlmideRxPc<'a>, mut p: usize, mut count: usize, k: usize) -> AlmideRxGoal<'a> {
        let piece = pc.piece();
        let max = piece.max.unwrap_or(usize::MAX);
        loop {
            let more = count < max && self.atom_at(pc, p);
            if count >= piece.min {
                if more {
                    self.push(AlmideRxGoal::Lazy { pc, p: p + 1, count: count + 1, k });
                }
                return AlmideRxGoal::Seq { pc: pc.next(), p, k };
            }
            if !more {
                return AlmideRxGoal::Fail;
            }
            p += 1;
            count += 1;
            self.steps += 1;
        }
    }

    /// The repeated group at `pc` has matched `count` instances ending at `p`:
    /// greedy tries one more instance before the rest, lazy the rest first.
    fn rep(&mut self, pc: AlmideRxPc<'a>, p: usize, count: usize, k: usize) -> AlmideRxGoal<'a> {
        let piece = pc.piece();
        let more = piece.max.map_or(true, |m| count < m);
        let stop = AlmideRxGoal::Seq { pc: pc.next(), p, k };
        match (count >= piece.min, more, piece.lazy) {
            (false, false, _) => AlmideRxGoal::Fail,
            (true, false, _) => stop,
            (false, true, _) => self.inst(pc, p, count, k),
            (true, true, true) => {
                self.push(AlmideRxGoal::Inst { pc, p, count, k });
                stop
            }
            (true, true, false) => {
                self.push(stop);
                self.inst(pc, p, count, k)
            }
        }
    }

    /// One instance of the group at `pc`: its alternatives, each continuing
    /// into a frame that records the capture and loops.
    fn inst(&mut self, pc: AlmideRxPc<'a>, p: usize, count: usize, k: usize) -> AlmideRxGoal<'a> {
        let AlmideRxNode::Group(alts, _) = &pc.piece().node else { return AlmideRxGoal::Fail };
        let frame = self.frames.len();
        self.frames.push(AlmideRxFrame { pc, start: p, count: count + 1, next: k });
        self.alts(alts, 0, p, frame)
    }

    /// A group instance ended at `p`: record its capture, then loop or stop.
    fn resume(&mut self, k: usize, p: usize) -> AlmideRxGoal<'a> {
        let f = self.frames[k];
        // The newest frame with no choice above it has no other reader left.
        if k + 1 == self.frames.len() && k >= self.choices.last().map_or(0, |c| c.frames) {
            self.frames.pop();
        }
        let piece = f.pc.piece();
        if let AlmideRxNode::Group(_, ci) = piece.node {
            if ci > 0 && self.track {
                self.capture(ci - 1, f.start, p);
            }
        }
        if p != f.start {
            return self.rep(f.pc, p, f.count, f.next);
        }
        // An empty instance counts, but never loops.
        if f.count < piece.min {
            AlmideRxGoal::Fail
        } else {
            AlmideRxGoal::Seq { pc: f.pc.next(), p, k: f.next }
        }
    }

    /// With no choice left a failure ends the attempt and the slots are
    /// reset before the next one, so there is nothing to restore.
    fn capture(&mut self, slot: usize, start: usize, end: usize) {
        if !self.choices.is_empty() {
            self.trail.push((slot, self.caps[slot]));
        }
        self.caps[slot] = Some((start, end));
    }
}

// Leftmost match at or after `start` by the backtracking walk: (start, end);
// `vm.caps` holds its groups. None with `vm.gave_up` set: the walk ran out of
// steps before it knew.
fn rx_backtrack_find_at<'a>(vm: &mut AlmideRxVm<'a>, rx: &'a AlmideRxPat, start: usize) -> Option<(usize, usize)> {
    for i in start..=vm.cx.s.len() {
        if let Some(end) = vm.exec(&rx.alts, i) {
            return Some((i, end));
        }
        if vm.gave_up {
            return None;
        }
    }
    None
}

// ---- Linear-time matching: a Pike VM (#4019) ----
//
// The backtracking walk above can take time exponential in the input's length:
// `(a+)+$` on `"aaaa…a!"` tries every way of splitting the `a`s between the two
// loops. The Pike VM gives the SAME answers — the walk's first success, its
// span and every group — in time proportional to the program's size times the
// input's length. The pattern is compiled to a program (Thompson's NFA with
// the walk's preferences as instruction order) and every thread of it is
// advanced one input character at a time, in priority order; two threads that
// reach the same state at the same position have the same future, so only the
// higher-priority one is kept. The walk's first success is the highest-priority
// thread that reaches MATCH, so cutting every lower thread there gives its answer.
//
// What makes a "state" here is (pc, d). The walk has one rule a plain NFA does
// not: an instance of a repeated group that matched nothing counts but never
// loops ("an empty instance counts, but never loops", `resume` above). Whether
// an instance is empty depends on where it began, so two threads at one pc can
// differ in their futures. Only instances that began at the CURRENT position can
// end empty, and instances nest, so the open instances that began here are
// always the innermost ones: `d` counts the open instances (of groups whose body
// can match empty — the "tracked" ones) that began BEFORE here, and the rest
// began here. Reading a character makes every open instance old.
//
// Counted repetition is expanded: `x{2,4}` becomes x x (x (x)?)?, one copy of
// the body per count, so the count is part of pc. A huge count therefore makes a
// huge program; past `ALMIDE_RX_PIKE_STATES` states the pattern is matched by
// the backtracking walk instead (same answers, no linear-time bound) rather
// than allocating without limit. The self-hosted twin in
// stdlib/regex_engine.almd compiles the same program and runs the same VM, so
// the two legs still agree on every match, capture and failure.

/// The most (pc, d) states a compiled pattern may have before it is matched by
/// the backtracking walk instead. Every ordinary pattern is far below it; what
/// reaches it is a counted repetition in the thousands (`.{0,10000}`) or nested
/// counts multiplying (`((a){100}){100}`).
const ALMIDE_RX_PIKE_STATES: usize = 20000;

/// A target that is never taken: the empty instance of a repetition that has
/// not reached its minimum fails.
const ALMIDE_RX_NOWHERE: usize = usize::MAX;

#[derive(Clone, Copy)]
enum AlmideRxInst {
    /// Read one character the atom accepts.
    Char(usize),
    /// A zero-width test (`^`, `$`, `\b`, `\B`).
    Assert(usize),
    /// Go on at both, the first preferred.
    Split(usize, usize),
    Jmp(usize),
    /// An instance of capturing group `ci` (1-based) begins here.
    GStart(usize),
    /// An instance of a group ends: record capture `ci` (0 = none), then go on
    /// at `nonempty`, or at `empty` when the instance matched nothing (only a
    /// tracked group, whose `outer` is the d count outside it, can tell).
    GEnd { ci: usize, tracked: bool, outer: usize, empty: usize, nonempty: usize },
    Match,
    Fail,
}

struct AlmideRxProg {
    insts: Vec<AlmideRxInst>,
    /// The atoms the Char and Assert instructions name.
    nodes: Vec<AlmideRxNode>,
    /// The number of tracked group instances open at each pc.
    depth: Vec<usize>,
    /// States per pc: one more than the deepest nesting of tracked groups.
    width: usize,
    /// The atoms a match can begin with, when every match begins by reading
    /// one (no assertion or empty match before the first character). A start
    /// position whose character none of them accepts cannot begin a match, so
    /// the search skips it rather than starting threads that die at once.
    first: Option<Vec<usize>>,
}

/// The atoms read first from pc 0 — None when an assertion or MATCH can come
/// before any of them.
fn rx_first_atoms(insts: &[AlmideRxInst]) -> Option<Vec<usize>> {
    let mut seen = vec![false; insts.len()];
    let mut todo = vec![0usize];
    let mut atoms: Vec<usize> = Vec::new();
    while let Some(pc) = todo.pop() {
        if pc >= insts.len() || seen[pc] {
            continue;
        }
        seen[pc] = true;
        match insts[pc] {
            AlmideRxInst::Char(node) => atoms.push(node),
            AlmideRxInst::Assert(_) | AlmideRxInst::Match => return None,
            AlmideRxInst::Split(a, b) => todo.extend([a, b]),
            AlmideRxInst::Jmp(a) => todo.push(a),
            AlmideRxInst::GStart(_) => todo.push(pc + 1),
            AlmideRxInst::GEnd { empty, nonempty, .. } => todo.extend([empty, nonempty]),
            AlmideRxInst::Fail => {}
        }
    }
    Some(atoms)
}

struct AlmideRxComp<'a> {
    insts: Vec<AlmideRxInst>,
    nodes: Vec<AlmideRxNode>,
    /// Each AST atom's index in `nodes`, so its copies share one entry.
    index: std::collections::HashMap<*const AlmideRxNode, usize>,
    pat: std::marker::PhantomData<&'a AlmideRxPat>,
    depth: Vec<usize>,
    cur: usize,
    deepest: usize,
    over: bool,
}

/// Can `alts` match without reading a character? Assertions count as
/// matching nothing, so this over-approximates, which is the safe side: a group
/// wrongly called nullable only carries a little more state.
fn rx_nullable(alts: &[AlmideRxSeq]) -> bool {
    alts.iter().any(|seq| {
        seq.iter().all(|piece| {
            piece.min == 0
                || match &piece.node {
                    AlmideRxNode::AnchorStart | AlmideRxNode::AnchorEnd | AlmideRxNode::WordBoundary(_) => true,
                    AlmideRxNode::Group(inner, _) => rx_nullable(inner),
                    _ => false,
                }
        })
    })
}

impl<'a> AlmideRxComp<'a> {
    fn node(&mut self, node: &'a AlmideRxNode) -> usize {
        let next = self.nodes.len();
        let i = *self.index.entry(node as *const AlmideRxNode).or_insert(next);
        if i == next {
            self.nodes.push(node.clone());
        }
        i
    }

    fn emit(&mut self, inst: AlmideRxInst) -> usize {
        self.insts.push(inst);
        self.depth.push(self.cur);
        if self.insts.len() * (self.deepest + 1) > ALMIDE_RX_PIKE_STATES {
            self.over = true;
        }
        self.insts.len() - 1
    }

    fn here(&self) -> usize {
        self.insts.len()
    }

    /// Point every placeholder target in `at` (a Split's second arm, a Jmp, or
    /// a GEnd's empty / nonempty arm, marked usize::MAX - 1) at `to`.
    fn patch(&mut self, at: &[usize], to: usize) {
        const HOLE: usize = usize::MAX - 1;
        for &i in at {
            self.insts[i] = match self.insts[i] {
                AlmideRxInst::Split(a, b) => AlmideRxInst::Split(if a == HOLE { to } else { a }, if b == HOLE { to } else { b }),
                AlmideRxInst::Jmp(_) => AlmideRxInst::Jmp(to),
                AlmideRxInst::GEnd { ci, tracked, outer, empty, nonempty } => AlmideRxInst::GEnd {
                    ci,
                    tracked,
                    outer,
                    empty: if empty == HOLE { to } else { empty },
                    nonempty: if nonempty == HOLE { to } else { nonempty },
                },
                other => other,
            };
        }
    }

    /// The alternatives in order, each one's end jumping past the rest.
    fn alts(&mut self, alts: &'a [AlmideRxSeq]) {
        const HOLE: usize = usize::MAX - 1;
        let mut ends = Vec::new();
        for (i, seq) in alts.iter().enumerate() {
            if self.over {
                return;
            }
            if i + 1 < alts.len() {
                let split = self.emit(AlmideRxInst::Split(self.here() + 1, HOLE));
                self.seq(seq);
                ends.push(self.emit(AlmideRxInst::Jmp(HOLE)));
                let next = self.here();
                self.patch(&[split], next);
            } else {
                self.seq(seq);
            }
        }
        let end = self.here();
        self.patch(&ends, end);
    }

    fn seq(&mut self, seq: &'a [AlmideRxPiece]) {
        for piece in seq {
            if self.over {
                return;
            }
            match &piece.node {
                // A quantifier on a zero-width piece is parsed and ignored.
                AlmideRxNode::AnchorStart | AlmideRxNode::AnchorEnd | AlmideRxNode::WordBoundary(_) => {
                    let n = self.node(&piece.node);
                    self.emit(AlmideRxInst::Assert(n));
                }
                AlmideRxNode::Group(alts, ci) => self.group(piece, alts, *ci),
                node => self.atom(piece, node),
            }
        }
    }

    /// A single-character atom: `min` copies, then the optional ones —
    /// greedy prefers another character, lazy prefers to stop.
    fn atom(&mut self, piece: &'a AlmideRxPiece, node: &'a AlmideRxNode) {
        const HOLE: usize = usize::MAX - 1;
        if piece.max.is_some_and(|mx| piece.min > mx) {
            self.emit(AlmideRxInst::Fail);
            return;
        }
        let n = self.node(node);
        for _ in 0..piece.min {
            if self.over {
                return;
            }
            self.emit(AlmideRxInst::Char(n));
        }
        let mut exits = Vec::new();
        match piece.max {
            None => {
                let top = self.here();
                let split = self.emit(AlmideRxInst::Split(HOLE, HOLE));
                let body = self.emit(AlmideRxInst::Char(n));
                self.emit(AlmideRxInst::Jmp(top));
                let exit = self.here();
                self.insts[split] = if piece.lazy { AlmideRxInst::Split(exit, body) } else { AlmideRxInst::Split(body, exit) };
            }
            Some(mx) => {
                for _ in piece.min..mx {
                    if self.over {
                        return;
                    }
                    let split = self.emit(AlmideRxInst::Split(HOLE, HOLE));
                    let body = self.emit(AlmideRxInst::Char(n));
                    self.insts[split] = if piece.lazy { AlmideRxInst::Split(HOLE, body) } else { AlmideRxInst::Split(body, HOLE) };
                    exits.push(split);
                }
                let exit = self.here();
                self.patch(&exits, exit);
            }
        }
    }

    /// A group with its repetition, one copy of the body per count the walk
    /// keeps apart: R_c (the decision after c instances) then instance c.
    /// Below `min` an instance is required; at or above it greedy prefers one
    /// more, lazy prefers to stop; an unbounded repetition's last copy loops.
    fn group(&mut self, piece: &'a AlmideRxPiece, alts: &'a [AlmideRxSeq], ci: usize) {
        const HOLE: usize = usize::MAX - 1;
        let (min, max, lazy) = (piece.min, piece.max, piece.lazy);
        if max.is_some_and(|mx| min > mx) {
            self.emit(AlmideRxInst::Fail);
            return;
        }
        let tracked = (min, max) != (1, Some(1)) && rx_nullable(alts);
        let outer = self.cur;
        if tracked && outer + 1 > self.deepest {
            self.deepest = outer + 1;
        }
        // The last count with its own copy: an unbounded repetition loops at
        // `min`, a bounded one stops at `max`.
        let last = max.unwrap_or(min);
        let mut exits = Vec::new();
        let mut c = 0;
        loop {
            if self.over {
                return;
            }
            let more = max.map_or(true, |mx| c < mx);
            if !more {
                break; // R_max: stop, which is the exit itself
            }
            let r_c = self.here();
            if c >= min {
                let split = if lazy { self.emit(AlmideRxInst::Split(HOLE, r_c + 1)) } else { self.emit(AlmideRxInst::Split(r_c + 1, HOLE)) };
                exits.push(split);
            }
            // instance c
            if tracked {
                self.cur += 1;
            }
            if ci > 0 {
                self.emit(AlmideRxInst::GStart(ci));
            }
            self.alts(alts);
            let loops = max.is_none() && c == min;
            let empty = if c + 1 < min { ALMIDE_RX_NOWHERE } else { HOLE };
            let nonempty = if loops { r_c } else { self.here() + 1 };
            let end = self.emit(AlmideRxInst::GEnd { ci, tracked, outer, empty, nonempty });
            if tracked {
                self.cur -= 1;
            }
            exits.push(end);
            if loops || c >= last {
                break;
            }
            c += 1;
        }
        let exit = self.here();
        self.patch(&exits, exit);
    }
}

/// The pattern's program, or None when it has more states than the Pike VM
/// is given (see ALMIDE_RX_PIKE_STATES).
fn rx_program(rx: &AlmideRxPat) -> Option<AlmideRxProg> {
    let mut comp = AlmideRxComp {
        insts: Vec::new(),
        nodes: Vec::new(),
        index: std::collections::HashMap::new(),
        pat: std::marker::PhantomData,
        depth: Vec::new(),
        cur: 0,
        deepest: 0,
        over: false,
    };
    comp.alts(&rx.alts);
    comp.emit(AlmideRxInst::Match);
    if comp.over {
        return None;
    }
    let first = rx_first_atoms(&comp.insts);
    Some(AlmideRxProg { insts: comp.insts, nodes: comp.nodes, depth: comp.depth, width: comp.deepest + 1, first })
}

/// One list of threads, in priority order: each at a pc with its d, and its
/// `nslots` slots ([match start, then per group: start, end, instance start]).
struct AlmideRxThreads {
    pcs: Vec<(usize, usize)>,
    slots: Vec<usize>,
    /// The stamp each state was last added under; a state is in this list when
    /// its stamp is the list's.
    seen: Vec<u64>,
    stamp: u64,
}

impl AlmideRxThreads {
    fn new(states: usize) -> Self {
        AlmideRxThreads { pcs: Vec::new(), slots: Vec::new(), seen: vec![0; states], stamp: 0 }
    }

    fn clear(&mut self) {
        self.pcs.clear();
        self.slots.clear();
        self.stamp += 1;
    }
}

enum AlmideRxStep {
    Go(usize, usize),
    Restore(usize, usize),
}

/// The slot that holds no position.
const ALMIDE_RX_UNSET: usize = usize::MAX;

/// The VM's working memory, kept across the searches of one call (find_all,
/// replace and split search once per match).
struct AlmideRxPike {
    clist: AlmideRxThreads,
    nlist: AlmideRxThreads,
    stack: Vec<AlmideRxStep>,
    scratch: Vec<usize>,
    /// The slots of the match found (slot 0 = its start).
    found: Vec<usize>,
    /// Only `captures` reads the groups, so only it carries them.
    track: bool,
    nslots: usize,
}

impl AlmideRxPike {
    fn new(prog: &AlmideRxProg, ncap: usize, track: bool) -> Self {
        let states = prog.insts.len() * prog.width;
        let nslots = if track { 1 + 3 * ncap } else { 1 };
        AlmideRxPike {
            clist: AlmideRxThreads::new(states),
            nlist: AlmideRxThreads::new(states),
            stack: Vec::new(),
            scratch: vec![ALMIDE_RX_UNSET; nslots],
            found: vec![ALMIDE_RX_UNSET; nslots],
            track,
            nslots,
        }
    }

    /// Add the thread at (pc, d) at text position `p`, and everything it
    /// reaches without reading, to the next list (`next`) or the current one,
    /// in priority order. The scratch slots are the thread's; the closure
    /// changes them and puts them back.
    fn add(&mut self, prog: &AlmideRxProg, cx: &AlmideRxCx, next: bool, pc: usize, d: usize, p: usize) {
        let w = prog.width;
        let list = if next { &mut self.nlist } else { &mut self.clist };
        let slots = &mut self.scratch;
        let stack = &mut self.stack;
        stack.push(AlmideRxStep::Go(pc, d));
        while let Some(step) = stack.pop() {
            let (pc, d) = match step {
                AlmideRxStep::Restore(slot, v) => {
                    slots[slot] = v;
                    continue;
                }
                AlmideRxStep::Go(pc, d) => (pc, d),
            };
            let key = pc * w + d;
            if list.seen[key] == list.stamp {
                continue;
            }
            list.seen[key] = list.stamp;
            match prog.insts[pc] {
                AlmideRxInst::Char(_) | AlmideRxInst::Match => {
                    list.pcs.push((pc, d));
                    list.slots.extend_from_slice(slots);
                }
                AlmideRxInst::Assert(node) => {
                    if rx_zero_width(cx, &prog.nodes[node], p) == Some(true) {
                        stack.push(AlmideRxStep::Go(pc + 1, d));
                    }
                }
                AlmideRxInst::Split(a, b) => {
                    stack.push(AlmideRxStep::Go(b, d));
                    stack.push(AlmideRxStep::Go(a, d));
                }
                AlmideRxInst::Jmp(a) => stack.push(AlmideRxStep::Go(a, d)),
                AlmideRxInst::GStart(ci) => {
                    if self.track {
                        let slot = 3 * ci;
                        stack.push(AlmideRxStep::Restore(slot, slots[slot]));
                        slots[slot] = p;
                    }
                    stack.push(AlmideRxStep::Go(pc + 1, d));
                }
                AlmideRxInst::GEnd { ci, tracked, outer, empty, nonempty } => {
                    if ci > 0 && self.track {
                        let (s0, s1) = (3 * ci - 2, 3 * ci - 1);
                        stack.push(AlmideRxStep::Restore(s0, slots[s0]));
                        stack.push(AlmideRxStep::Restore(s1, slots[s1]));
                        slots[s0] = slots[3 * ci];
                        slots[s1] = p;
                    }
                    // The instance began here when it is not among the `d`
                    // instances that began before.
                    let is_empty = tracked && d <= outer;
                    let to = if is_empty { empty } else { nonempty };
                    if to != ALMIDE_RX_NOWHERE {
                        stack.push(AlmideRxStep::Go(to, d.min(outer)));
                    }
                }
                AlmideRxInst::Fail => {}
            }
        }
    }

    /// Start a thread for a match beginning at `p` in the current list (`next`
    /// false) or the next one.
    fn seed(&mut self, prog: &AlmideRxProg, cx: &AlmideRxCx, next: bool, p: usize) {
        self.scratch.iter_mut().for_each(|s| *s = ALMIDE_RX_UNSET);
        self.scratch[0] = p;
        self.add(prog, cx, next, 0, 0, p);
    }

    /// The first position at or after `p` where a match can begin by reading
    /// one of `first`; None when there is none.
    fn skip(prog: &AlmideRxProg, cx: &AlmideRxCx, first: &[usize], p: usize) -> Option<usize> {
        (p..cx.s.len()).find(|&q| first.iter().any(|&node| rx_node_matches(&prog.nodes[node], cx.s[q])))
    }

    /// The walk's first success from a start at or after `start` (only at
    /// `start` when `anchored`), ending at `end_at` when that is given: its
    /// end, with its slots in `found`. With `any`, the first match found,
    /// whichever it is.
    fn search(&mut self, prog: &AlmideRxProg, cx: &AlmideRxCx, start: usize, anchored: bool, end_at: Option<usize>, any: bool) -> Option<usize> {
        let mut found: Option<usize> = None;
        let len = cx.s.len();
        let first = if anchored { None } else { prog.first.as_deref() };
        let mut p = start;
        if let Some(first) = first {
            p = Self::skip(prog, cx, first, p)?;
        }
        self.clist.clear();
        self.seed(prog, cx, false, p);
        loop {
            self.nlist.clear();
            let c = cx.s.get(p).copied();
            for i in 0..self.clist.pcs.len() {
                let (pc, _) = self.clist.pcs[i];
                let at = i * self.nslots;
                match prog.insts[pc] {
                    AlmideRxInst::Match => {
                        if end_at.map_or(true, |e| e == p) {
                            self.found.copy_from_slice(&self.clist.slots[at..at + self.nslots]);
                            found = Some(p);
                            if any {
                                return found;
                            }
                            // Every thread after this one has lower priority.
                            break;
                        }
                    }
                    AlmideRxInst::Char(node) => {
                        if c.is_some_and(|c| rx_node_matches(&prog.nodes[node], c)) {
                            self.scratch.copy_from_slice(&self.clist.slots[at..at + self.nslots]);
                            self.add(prog, cx, true, pc + 1, prog.depth[pc], p + 1);
                        }
                    }
                    _ => {}
                }
            }
            if p >= len {
                break;
            }
            if found.is_none() && !anchored {
                // A match starting further right has the lowest priority.
                match first {
                    // With nothing in flight, go straight to the next place a
                    // match can begin.
                    Some(first) if self.nlist.pcs.is_empty() => {
                        let Some(q) = Self::skip(prog, cx, first, p + 1) else { break };
                        self.clist.clear();
                        self.seed(prog, cx, false, q);
                        p = q;
                        continue;
                    }
                    _ => self.seed(prog, cx, true, p + 1),
                }
            }
            if self.nlist.pcs.is_empty() && (found.is_some() || anchored) {
                break;
            }
            std::mem::swap(&mut self.clist, &mut self.nlist);
            p += 1;
        }
        found
    }
}

/// A pattern compiled once: its tree (the walk's input), its program (None =
/// over the VM's state budget), and the VM's working memory for the plain and
/// the capture-tracking search, kept for the next call.
struct AlmideRxCompiled {
    rx: AlmideRxPat,
    prog: Option<AlmideRxProg>,
    pikes: std::cell::RefCell<[Option<AlmideRxPike>; 2]>,
}

fn rx_compiled(pat: &str) -> AlmideRxCompiled {
    let rx = rx_compile(pat);
    let prog = rx_program(&rx);
    AlmideRxCompiled { rx, prog, pikes: std::cell::RefCell::new([None, None]) }
}

/// How many compiled patterns a thread keeps.
const ALMIDE_RX_CACHE: usize = 16;

thread_local! {
    /// The patterns this thread compiled most recently, newest first: a loop
    /// that calls `regex.is_match(pat, line)` per line builds the program once.
    static ALMIDE_RX_COMPILED: std::cell::RefCell<Vec<(String, std::rc::Rc<AlmideRxCompiled>)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

fn rx_cached(pat: &str) -> std::rc::Rc<AlmideRxCompiled> {
    ALMIDE_RX_COMPILED.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(i) = cache.iter().position(|(p, _)| p == pat) {
            let hit = cache.remove(i);
            let rc = hit.1.clone();
            cache.insert(0, hit);
            return rc;
        }
        let rc = std::rc::Rc::new(rx_compiled(pat));
        cache.insert(0, (pat.to_string(), rc.clone()));
        cache.truncate(ALMIDE_RX_CACHE);
        rc
    })
}

/// How many walk steps per (pattern char × text char) a call may spend before
/// the Pike VM takes over. An ordinary pattern needs a few steps per text
/// char, so it never comes near the bound; one that would backtrack without
/// end reaches it after time linear in the text, and the VM finishes the job.
const ALMIDE_RX_WALK_STEPS: usize = 4;

/// A compiled pattern over one text. The backtracking walk runs first, under
/// a step budget linear in the text's length: it is the faster engine on the
/// patterns people write, and it gives the same answers as the Pike VM. When
/// the budget runs out the VM answers instead, so a call never takes more than
/// linear time. A pattern too large for the VM keeps the walk, unbounded.
struct AlmideRxMatcher<'a> {
    c: &'a AlmideRxCompiled,
    track: bool,
    /// The VM's working memory, once the walk has given up.
    pike: Option<AlmideRxPike>,
    vm: AlmideRxVm<'a>,
}

impl<'a> AlmideRxMatcher<'a> {
    fn new(c: &'a AlmideRxCompiled, s: &'a [char], track: bool) -> Self {
        let mut vm = AlmideRxVm::new(&c.rx, s, None, track);
        if c.prog.is_some() {
            vm.budget = ALMIDE_RX_WALK_STEPS.saturating_mul(c.rx.size + 1).saturating_mul(s.len() + 1);
        }
        AlmideRxMatcher { c, track, pike: None, vm }
    }

    /// The VM, from the pattern's kept working memory or new — None when the
    /// walk still has steps left (or the pattern has no program).
    fn pike(&mut self) -> Option<(&AlmideRxProg, &mut AlmideRxPike)> {
        if !self.vm.gave_up {
            return None;
        }
        let prog = self.c.prog.as_ref()?;
        if self.pike.is_none() {
            let kept = self.c.pikes.borrow_mut()[self.track as usize].take();
            self.pike = Some(kept.unwrap_or_else(|| AlmideRxPike::new(prog, self.c.rx.ncap, self.track)));
        }
        Some((prog, self.pike.as_mut()?))
    }

    /// Leftmost match at or after `start`: (start, end); `caps` then holds
    /// its groups (when tracked).
    fn find_at(&mut self, start: usize) -> Option<(usize, usize)> {
        if !self.vm.gave_up {
            let found = rx_backtrack_find_at(&mut self.vm, &self.c.rx, start);
            if !self.vm.gave_up {
                return found;
            }
        }
        let cx = AlmideRxCx { s: self.vm.cx.s, multiline: self.vm.cx.multiline };
        let (prog, pike) = self.pike()?;
        let end = pike.search(prog, &cx, start, false, None, false)?;
        let (first, track) = (pike.found[0], pike.track);
        if track {
            let found = pike.found.clone();
            for (i, cap) in self.vm.caps.iter_mut().enumerate() {
                let (s0, s1) = (found[3 * i + 1], found[3 * i + 2]);
                *cap = if s0 == ALMIDE_RX_UNSET { None } else { Some((s0, s1)) };
            }
        }
        Some((first, end))
    }

    fn is_match(&mut self) -> bool {
        self.find_at(0).is_some()
    }

    fn full_match(&mut self) -> bool {
        let len = self.vm.cx.s.len();
        if !self.vm.gave_up {
            self.vm.end_at = Some(len);
            let matched = self.vm.exec(&self.c.rx.alts, 0).is_some();
            if !self.vm.gave_up {
                return matched;
            }
        }
        let cx = AlmideRxCx { s: self.vm.cx.s, multiline: self.vm.cx.multiline };
        match self.pike() {
            Some((prog, pike)) => pike.search(prog, &cx, 0, true, Some(len), false).is_some(),
            None => false,
        }
    }

    fn caps(&self) -> &AlmideRxCaps {
        &self.vm.caps
    }
}

impl Drop for AlmideRxMatcher<'_> {
    /// Hand the VM's working memory back for the pattern's next call.
    fn drop(&mut self) {
        if let Some(pike) = self.pike.take() {
            self.c.pikes.borrow_mut()[self.track as usize] = Some(pike);
        }
    }
}

// ---- Public API ----

pub fn almide_regex_is_match(pat: &str, s: &str) -> bool {
    let rx = rx_cached(pat);
    let chars: Vec<char> = s.chars().collect();
    let mut m = AlmideRxMatcher::new(&rx, &chars, false);
    m.is_match()
}

pub fn almide_regex_full_match(pat: &str, s: &str) -> bool {
    let rx = rx_cached(pat);
    let chars: Vec<char> = s.chars().collect();
    let mut m = AlmideRxMatcher::new(&rx, &chars, false);
    m.full_match()
}

pub fn almide_regex_find(pat: &str, s: &str) -> Option<AlmideString> { let __almide_v: Option<String> = (|| {
    let rx = rx_cached(pat);
    let chars: Vec<char> = s.chars().collect();
    let mut vm = AlmideRxMatcher::new(&rx, &chars, false);
    vm.find_at(0).map(|(start, end)| chars[start..end].iter().collect())
})(); __almide_v.into_almide() }

pub fn almide_regex_find_all(pat: &str, s: &str) -> Vec<AlmideString> { let __almide_v: Vec<String> = (|| {
    let rx = rx_cached(pat);
    let chars: Vec<char> = s.chars().collect();
    let mut vm = AlmideRxMatcher::new(&rx, &chars, false);
    let mut results: Vec<String> = vec![];
    let mut pos = 0;
    while pos <= chars.len() {
        if let Some((start, end)) = vm.find_at(pos) {
            results.push(chars[start..end].iter().collect());
            pos = if end > start { end } else { end + 1 };
        } else {
            break;
        }
    }
    results
})(); __almide_v.into_almide() }

pub fn almide_regex_replace(pat: &str, s: &str, rep: &str) -> AlmideString { let __almide_v: String = (|| {
    let rx = rx_cached(pat);
    let chars: Vec<char> = s.chars().collect();
    let mut vm = AlmideRxMatcher::new(&rx, &chars, false);
    let mut result = String::new();
    let mut pos = 0;
    while pos <= chars.len() {
        if let Some((start, end)) = vm.find_at(pos) {
            result.extend(&chars[pos..start]);
            result.push_str(rep);
            pos = if end > start {
                end
            } else {
                // Zero-width match: emit the char at `end` and step past it so the
                // search advances. At end-of-string there is no char to emit
                // (`end == chars.len()`); guard the index so we don't panic and
                // simply advance past the end to terminate the loop.
                if end < chars.len() {
                    result.push(chars[end]);
                }
                end + 1
            };
        } else {
            result.extend(&chars[pos..]);
            break;
        }
    }
    result
})(); __almide_v.into_almide() }

pub fn almide_regex_replace_first(pat: &str, s: &str, rep: &str) -> AlmideString { let __almide_v: String = (|| {
    let rx = rx_cached(pat);
    let chars: Vec<char> = s.chars().collect();
    let mut vm = AlmideRxMatcher::new(&rx, &chars, false);
    if let Some((start, end)) = vm.find_at(0) {
        let mut result = String::new();
        result.extend(&chars[..start]);
        result.push_str(rep);
        result.extend(&chars[end..]);
        result
    } else {
        s.to_string()
    }
})(); __almide_v.into_almide() }

/// A field is the text BETWEEN two matches, so the split point and the scan
/// position are two different things — conflating them is what made a
/// zero-width match eat a character (#2129): `split("^", "abc")` answered
/// `["a", "bc"]`, moving the `a` out of the field it belongs to, where Python,
/// Rust and Go all answer `["", "abc"]`.
///
/// The rule is the one those three share: walk the matches exactly as
/// `find_all` does (an empty match advances the scan by one character so the
/// walk terminates), emit the text from the previous match's END to this
/// match's START as a field, and ALWAYS emit the trailing field — including
/// when it is empty, which is how `split("$", "a")` is `["a", ""]` rather than
/// `["a"]`. Sharing `find_all`'s walk is what keeps the two answers consistent
/// with each other, which matters more than matching any one engine's
/// treatment of an empty alternation arm (there the three disagree among
/// themselves).
pub fn almide_regex_split(pat: &str, s: &str) -> Vec<AlmideString> { let __almide_v: Vec<String> = (|| {
    let rx = rx_cached(pat);
    let chars: Vec<char> = s.chars().collect();
    let mut vm = AlmideRxMatcher::new(&rx, &chars, false);
    let mut results: Vec<String> = vec![];
    let (mut last, mut scan) = (0usize, 0usize);
    while scan <= chars.len() {
        let Some((start, end)) = vm.find_at(scan) else { break };
        results.push(chars[last..start].iter().collect());
        last = end;
        scan = if end > start { end } else { end + 1 };
    }
    results.push(chars[last..].iter().collect());
    results
})(); __almide_v.into_almide() }

/// Index 0 is the WHOLE match, 1.. are the groups — the shape every mainstream
/// regex API uses (Rust `Captures::get(0)`, Python `m.group(0)`, JS `match[0]`,
/// PCRE), and the one `docs/stdlib/regex.md` always documented. The groups-only
/// return this replaced made `caps[1]` silently mean the SECOND group to anyone
/// carrying the universal habit over (almide#1432).
///
/// `None` now means exactly one thing: the pattern did not match. It previously
/// also came back for a pattern with NO groups, conflating "matched nothing to
/// capture" with "did not match" — with a full match at index 0 that case has a
/// real answer, `some([whole])`.
pub fn almide_regex_captures(pat: &str, s: &str) -> Option<Vec<AlmideString>> { let __almide_v: Option<Vec<String>> = (|| {
    let rx = rx_cached(pat);
    let chars: Vec<char> = s.chars().collect();
    let mut vm = AlmideRxMatcher::new(&rx, &chars, true);
    let (mstart, mend) = vm.find_at(0)?;
    let mut result = vec![chars[mstart..mend].iter().collect::<String>()];
    result.extend(vm.caps().iter().map(|c| match c {
        Some((start, end)) => chars[*start..*end].iter().collect(),
        None => String::new(),
    }));
    Some(result)
})(); __almide_v.into_almide() }

// ---- End Regex Runtime ----
