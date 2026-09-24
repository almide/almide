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
    AlmideRxPat { alts, ncap, multiline }
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
        alts.last_mut().unwrap().push(piece);
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

// Leftmost match at or after `start`: (start, end); `vm.caps` holds its groups.
fn rx_find_at<'a>(vm: &mut AlmideRxVm<'a>, rx: &'a AlmideRxPat, start: usize) -> Option<(usize, usize)> {
    (start..=vm.cx.s.len()).find_map(|i| vm.exec(&rx.alts, i).map(|end| (i, end)))
}

// ---- Public API ----

pub fn almide_regex_is_match(pat: &str, s: &str) -> bool {
    let rx = rx_compile(pat);
    let chars: Vec<char> = s.chars().collect();
    let mut vm = AlmideRxVm::new(&rx, &chars, None, false);
    rx_find_at(&mut vm, &rx, 0).is_some()
}

pub fn almide_regex_full_match(pat: &str, s: &str) -> bool {
    let rx = rx_compile(pat);
    let chars: Vec<char> = s.chars().collect();
    AlmideRxVm::new(&rx, &chars, Some(chars.len()), false).exec(&rx.alts, 0).is_some()
}

pub fn almide_regex_find(pat: &str, s: &str) -> Option<String> {
    let rx = rx_compile(pat);
    let chars: Vec<char> = s.chars().collect();
    let mut vm = AlmideRxVm::new(&rx, &chars, None, false);
    rx_find_at(&mut vm, &rx, 0).map(|(start, end)| chars[start..end].iter().collect())
}

pub fn almide_regex_find_all(pat: &str, s: &str) -> Vec<String> {
    let rx = rx_compile(pat);
    let chars: Vec<char> = s.chars().collect();
    let mut vm = AlmideRxVm::new(&rx, &chars, None, false);
    let mut results: Vec<String> = vec![];
    let mut pos = 0;
    while pos <= chars.len() {
        if let Some((start, end)) = rx_find_at(&mut vm, &rx, pos) {
            results.push(chars[start..end].iter().collect());
            pos = if end > start { end } else { end + 1 };
        } else {
            break;
        }
    }
    results
}

pub fn almide_regex_replace(pat: &str, s: &str, rep: &str) -> String {
    let rx = rx_compile(pat);
    let chars: Vec<char> = s.chars().collect();
    let mut vm = AlmideRxVm::new(&rx, &chars, None, false);
    let mut result = String::new();
    let mut pos = 0;
    while pos <= chars.len() {
        if let Some((start, end)) = rx_find_at(&mut vm, &rx, pos) {
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
}

pub fn almide_regex_replace_first(pat: &str, s: &str, rep: &str) -> String {
    let rx = rx_compile(pat);
    let chars: Vec<char> = s.chars().collect();
    let mut vm = AlmideRxVm::new(&rx, &chars, None, false);
    if let Some((start, end)) = rx_find_at(&mut vm, &rx, 0) {
        let mut result = String::new();
        result.extend(&chars[..start]);
        result.push_str(rep);
        result.extend(&chars[end..]);
        result
    } else {
        s.to_string()
    }
}

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
pub fn almide_regex_split(pat: &str, s: &str) -> Vec<String> {
    let rx = rx_compile(pat);
    let chars: Vec<char> = s.chars().collect();
    let mut vm = AlmideRxVm::new(&rx, &chars, None, false);
    let mut results: Vec<String> = vec![];
    let (mut last, mut scan) = (0usize, 0usize);
    while scan <= chars.len() {
        let Some((start, end)) = rx_find_at(&mut vm, &rx, scan) else { break };
        results.push(chars[last..start].iter().collect());
        last = end;
        scan = if end > start { end } else { end + 1 };
    }
    results.push(chars[last..].iter().collect());
    results
}

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
pub fn almide_regex_captures(pat: &str, s: &str) -> Option<Vec<String>> {
    let rx = rx_compile(pat);
    let chars: Vec<char> = s.chars().collect();
    let mut vm = AlmideRxVm::new(&rx, &chars, None, true);
    let (mstart, mend) = rx_find_at(&mut vm, &rx, 0)?;
    let mut result = vec![chars[mstart..mend].iter().collect::<String>()];
    result.extend(vm.caps.iter().map(|c| match c {
        Some((start, end)) => chars[*start..*end].iter().collect(),
        None => String::new(),
    }));
    Some(result)
}

// ---- End Regex Runtime ----
