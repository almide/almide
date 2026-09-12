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

// ---- Matching (continuation-passing backtracker) ----
//
// Every matcher takes a continuation `k` that receives the end position (and
// the capture table) once the piece it is responsible for has matched; `k`
// returns Some(final_end) when the rest of the pattern matches from there and
// None to demand backtracking. The twin self-host engine in
// stdlib/regex_engine.almd keeps the same continuation frames on the heap, so
// the two legs agree on every backtracking order: leftmost alternative first,
// greedy quantifiers longest-first, lazy quantifiers shortest-first, and an
// empty repetition instance ends the loop.

type AlmideRxK<'a> = &'a mut dyn FnMut(usize, &mut AlmideRxCaps) -> Option<usize>;

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

fn rx_alts(cx: &AlmideRxCx, alts: &[Vec<AlmideRxPiece>], p: usize, caps: &mut AlmideRxCaps, k: AlmideRxK) -> Option<usize> {
    for alt in alts {
        if let Some(e) = rx_seq(cx, alt, 0, p, caps, k) {
            return Some(e);
        }
    }
    None
}

fn rx_seq(cx: &AlmideRxCx, seq: &[AlmideRxPiece], si: usize, p: usize, caps: &mut AlmideRxCaps, k: AlmideRxK) -> Option<usize> {
    if si >= seq.len() {
        return k(p, caps);
    }
    match rx_zero_width(cx, &seq[si].node, p) {
        Some(true) => rx_seq(cx, seq, si + 1, p, caps, k),
        Some(false) => None,
        None => rx_rep(cx, seq, si, p, caps, 0, k),
    }
}

// The rest of the sequence after `count` instances of piece `si`.
fn rx_rep_rest(cx: &AlmideRxCx, seq: &[AlmideRxPiece], si: usize, p: usize, caps: &mut AlmideRxCaps, count: usize, k: AlmideRxK) -> Option<usize> {
    if count < seq[si].min {
        None
    } else {
        rx_seq(cx, seq, si + 1, p, caps, k)
    }
}

// One more instance of piece `si` (then loop), or hand over to the rest.
fn rx_rep_more(cx: &AlmideRxCx, seq: &[AlmideRxPiece], si: usize, p: usize, caps: &mut AlmideRxCaps, count: usize, k: AlmideRxK) -> Option<usize> {
    let piece = &seq[si];
    let under_max = piece.max.map_or(true, |m| count < m);
    if !under_max {
        return None;
    }
    rx_one(cx, &piece.node, p, caps, &mut |e: usize, caps: &mut AlmideRxCaps| {
        if e == p {
            // An empty instance: count it, but never loop on it.
            rx_rep_rest(cx, seq, si, e, caps, count + 1, k)
        } else {
            rx_rep(cx, seq, si, e, caps, count + 1, k)
        }
    })
}

fn rx_rep(cx: &AlmideRxCx, seq: &[AlmideRxPiece], si: usize, p: usize, caps: &mut AlmideRxCaps, count: usize, k: AlmideRxK) -> Option<usize> {
    if seq[si].lazy {
        if let Some(e) = rx_rep_rest(cx, seq, si, p, caps, count, k) {
            return Some(e);
        }
        rx_rep_more(cx, seq, si, p, caps, count, k)
    } else {
        if let Some(e) = rx_rep_more(cx, seq, si, p, caps, count, k) {
            return Some(e);
        }
        rx_rep_rest(cx, seq, si, p, caps, count, k)
    }
}

// One instance of an atom at `p`; a group's capture is recorded before the
// continuation runs and restored when the continuation backtracks.
fn rx_one(cx: &AlmideRxCx, node: &AlmideRxNode, p: usize, caps: &mut AlmideRxCaps, k: AlmideRxK) -> Option<usize> {
    match node {
        AlmideRxNode::Group(alts, ci) => {
            let ci = *ci;
            rx_alts(cx, alts, p, caps, &mut |e: usize, caps: &mut AlmideRxCaps| {
                if ci == 0 {
                    return k(e, caps);
                }
                let saved = caps[ci - 1];
                caps[ci - 1] = Some((p, e));
                let r = k(e, caps);
                if r.is_none() {
                    caps[ci - 1] = saved;
                }
                r
            })
        }
        _ => match rx_zero_width(cx, node, p) {
            Some(true) => k(p, caps),
            Some(false) => None,
            None => {
                if p < cx.s.len() && rx_node_matches(node, cx.s[p]) {
                    k(p + 1, caps)
                } else {
                    None
                }
            }
        },
    }
}

fn rx_match_from(rx: &AlmideRxPat, s: &[char], p: usize, caps: &mut AlmideRxCaps, k: AlmideRxK) -> Option<usize> {
    let cx = AlmideRxCx { s, multiline: rx.multiline };
    rx_alts(&cx, &rx.alts, p, caps, k)
}

// Leftmost match at or after `start`: (start, end, captures).
fn rx_find_at(rx: &AlmideRxPat, s: &[char], start: usize) -> Option<(usize, usize, AlmideRxCaps)> {
    for i in start..=s.len() {
        let mut caps: AlmideRxCaps = vec![None; rx.ncap];
        if let Some(end) = rx_match_from(rx, s, i, &mut caps, &mut |e: usize, _caps: &mut AlmideRxCaps| Some(e)) {
            return Some((i, end, caps));
        }
    }
    None
}


// ---- Public API ----

pub fn almide_regex_is_match(pat: &str, s: &str) -> bool {
    let rx = rx_compile(pat);
    let chars: Vec<char> = s.chars().collect();
    rx_find_at(&rx, &chars, 0).is_some()
}

pub fn almide_regex_full_match(pat: &str, s: &str) -> bool {
    let rx = rx_compile(pat);
    let chars: Vec<char> = s.chars().collect();
    let mut caps: AlmideRxCaps = vec![None; rx.ncap];
    let len = chars.len();
    rx_match_from(&rx, &chars, 0, &mut caps, &mut |e: usize, _caps: &mut AlmideRxCaps| if e == len { Some(e) } else { None }).is_some()
}

pub fn almide_regex_find(pat: &str, s: &str) -> Option<String> {
    let rx = rx_compile(pat);
    let chars: Vec<char> = s.chars().collect();
    rx_find_at(&rx, &chars, 0).map(|(start, end, _)| chars[start..end].iter().collect())
}

pub fn almide_regex_find_all(pat: &str, s: &str) -> Vec<String> {
    let rx = rx_compile(pat);
    let chars: Vec<char> = s.chars().collect();
    let mut results: Vec<String> = vec![];
    let mut pos = 0;
    while pos <= chars.len() {
        if let Some((start, end, _)) = rx_find_at(&rx, &chars, pos) {
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
    let mut result = String::new();
    let mut pos = 0;
    while pos <= chars.len() {
        if let Some((start, end, _)) = rx_find_at(&rx, &chars, pos) {
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
    if let Some((start, end, _)) = rx_find_at(&rx, &chars, 0) {
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
    let mut results: Vec<String> = vec![];
    let (mut last, mut scan) = (0usize, 0usize);
    while scan <= chars.len() {
        let Some((start, end, _)) = rx_find_at(&rx, &chars, scan) else { break };
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
    let (mstart, mend, caps) = rx_find_at(&rx, &chars, 0)?;
    let mut result = vec![chars[mstart..mend].iter().collect::<String>()];
    result.extend(caps.iter().map(|c| match c {
        Some((start, end)) => chars[*start..*end].iter().collect(),
        None => String::new(),
    }));
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    // {n} / {n,} / {n,m} quantifiers (previously unsupported: they silently
    // never matched — almide#845).
    #[test]
    fn brace_quantifiers() {
        assert!(almide_regex_is_match(r"\d{3}", "abc123"));
        assert!(!almide_regex_is_match(r"\d{4}", "abc123"));
        assert!(almide_regex_full_match(r"\d{3}", "123"));
        assert!(!almide_regex_full_match(r"\d{3}", "12"));
        assert!(!almide_regex_full_match(r"\d{3}", "1234"));
        assert!(almide_regex_full_match(r"\d{2,}", "12345"));
        assert!(!almide_regex_full_match(r"\d{6,}", "12345"));
        assert!(almide_regex_full_match(r"\d{2,4}", "123"));
        assert!(!almide_regex_full_match(r"\d{2,4}", "12345"));
        assert_eq!(almide_regex_find(r"a{2,3}", "caaaab"), Some("aaa".to_string()));
        assert_eq!(
            almide_regex_captures(r"(\d{4})-(\d{2})-(\d{2})", "on 2026-03-09."),
            Some(vec![
                "2026-03-09".to_string(),
                "2026".to_string(),
                "03".to_string(),
                "09".to_string(),
            ])
        );
        assert_eq!(almide_regex_replace(r"o{2}", "foo book", "0"), "f0 b0k");
        // No groups: `none` means "did not match", so a matching group-less
        // pattern answers its whole match rather than being indistinguishable
        // from a miss (almide#1432).
        assert_eq!(
            almide_regex_captures(r"b+", "aabbb!"),
            Some(vec!["bbb".to_string()])
        );
        assert_eq!(almide_regex_captures(r"z+", "aabbb!"), None);
        // Grouped repetition
        assert!(almide_regex_full_match(r"(ab){2}", "abab"));
        assert!(!almide_regex_full_match(r"(ab){2}", "ab"));
        // {0,n} and zero-width safety
        assert!(almide_regex_full_match(r"a{0,2}b", "b"));
        assert!(almide_regex_full_match(r"a{0,2}b", "aab"));
        // Malformed braces stay literal
        assert!(almide_regex_is_match(r"a\{x", "a{x"));
        assert!(almide_regex_full_match(r"a{x}", "a{x}"));
        assert!(almide_regex_full_match(r"a{,3}", "a{,3}"));
        assert!(almide_regex_full_match(r"a{2", "a{2"));
    }

    // Regression: `replace` over a pattern that matches EMPTY at end-of-string
    // (e.g. `x*`, `a*`, ``) must NOT panic indexing `chars[len]`. The zero-width
    // advance emits the char at the match position and skips it at end-of-string.
    #[test]
    fn replace_empty_match_at_end_no_panic() {
        assert_eq!(almide_regex_replace("x*", "ab", "-"), "-a-b-");
        assert_eq!(almide_regex_replace("", "ab", "-"), "-a-b-");
        assert_eq!(almide_regex_replace("a*", "aaa", "-"), "--");
        assert_eq!(almide_regex_replace("a*", "bbb", "-"), "-b-b-b-");
        assert_eq!(almide_regex_replace("b?", "abc", "-"), "-a--c-");
        assert_eq!(almide_regex_replace("x*", "", "-"), "-");
        assert_eq!(almide_regex_replace("", "", "-"), "-");
        // multibyte: zero-width positions land on scalar boundaries.
        assert_eq!(almide_regex_replace("x*", "本a", "-"), "-本-a-");
    }

    #[test]
    fn replace_first_empty_match() {
        assert_eq!(almide_regex_replace_first("x*", "ab", "-"), "-ab");
        assert_eq!(almide_regex_replace_first("a*", "aaa", "-"), "-");
        assert_eq!(almide_regex_replace_first("a*", "bbb", "-"), "-bbb");
        assert_eq!(almide_regex_replace_first("x*", "", "-"), "-");
    }

    // Empty alternation arms (an empty arm = empty Seq that matches length-0).
    #[test]
    fn empty_alternation_arms() {
        assert!(almide_regex_is_match("a|", "zzz")); // trailing empty arm
        assert!(almide_regex_is_match("|a", "zzz")); // leading empty arm
        assert!(almide_regex_is_match("a||b", "zzz")); // middle empty arm
        assert!(almide_regex_is_match("a|||", "zzz")); // triple
        assert_eq!(almide_regex_find("a|", "bza"), Some(String::new())); // empty match at 0
        assert!(almide_regex_full_match("a|", "")); // empty arm full-matches ""
        assert_eq!(almide_regex_replace("a|", "abc", "-"), "--b-c-");
        assert_eq!(
            almide_regex_captures("(a|)", "b"),
            Some(vec![String::new(), String::new()])
        );
    }
}

// ---- End Regex Runtime ----
