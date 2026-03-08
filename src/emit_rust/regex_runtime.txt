// ---- Regex Runtime ----

#[derive(Clone)]
enum RxNode {
    Lit(char),
    Dot,
    Class(Vec<(char, char)>, bool), // ranges, negated
    AnchorStart,
    AnchorEnd,
    Group(Vec<Vec<RxPiece>>, usize), // alternations, capture index (1-based; 0 = no capture)
}

#[derive(Clone)]
struct RxPiece {
    node: RxNode,
    min: usize,
    max: Option<usize>,
}

struct RxPat {
    alts: Vec<Vec<RxPiece>>,
    ncap: usize,
}

type RxCaps = Vec<Option<(usize, usize)>>;

// ---- Parsing ----

fn rx_compile(pat: &str) -> RxPat {
    let chars: Vec<char> = pat.chars().collect();
    let mut pos = 0usize;
    let mut ncap = 0usize;
    let alts = rx_parse_alts(&chars, &mut pos, &mut ncap, false);
    RxPat { alts, ncap }
}

fn rx_parse_alts(chars: &[char], pos: &mut usize, ncap: &mut usize, in_group: bool) -> Vec<Vec<RxPiece>> {
    let mut alts: Vec<Vec<RxPiece>> = vec![vec![]];
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

fn rx_parse_piece(chars: &[char], pos: &mut usize, ncap: &mut usize) -> RxPiece {
    let node = rx_parse_atom(chars, pos, ncap);
    let (min, max) = if *pos < chars.len() {
        match chars[*pos] {
            '*' => { *pos += 1; (0, None) }
            '+' => { *pos += 1; (1, None) }
            '?' => { *pos += 1; (0, Some(1)) }
            _ => (1, Some(1)),
        }
    } else {
        (1, Some(1))
    };
    RxPiece { node, min, max }
}

fn rx_parse_atom(chars: &[char], pos: &mut usize, ncap: &mut usize) -> RxNode {
    let c = chars[*pos];
    *pos += 1;
    match c {
        '.' => RxNode::Dot,
        '^' => RxNode::AnchorStart,
        '$' => RxNode::AnchorEnd,
        '\\' => rx_parse_escape(chars, pos),
        '[' => rx_parse_class(chars, pos),
        '(' => {
            *ncap += 1;
            let ci = *ncap;
            let alts = rx_parse_alts(chars, pos, ncap, true);
            if *pos < chars.len() && chars[*pos] == ')' { *pos += 1; }
            RxNode::Group(alts, ci)
        }
        _ => RxNode::Lit(c),
    }
}

fn rx_parse_escape(chars: &[char], pos: &mut usize) -> RxNode {
    if *pos >= chars.len() { return RxNode::Lit('\\'); }
    let c = chars[*pos];
    *pos += 1;
    match c {
        'd' => RxNode::Class(vec![('0', '9')], false),
        'D' => RxNode::Class(vec![('0', '9')], true),
        'w' => RxNode::Class(vec![('a', 'z'), ('A', 'Z'), ('0', '9'), ('_', '_')], false),
        'W' => RxNode::Class(vec![('a', 'z'), ('A', 'Z'), ('0', '9'), ('_', '_')], true),
        's' => RxNode::Class(vec![(' ', ' '), ('\t', '\t'), ('\n', '\n'), ('\r', '\r')], false),
        'S' => RxNode::Class(vec![(' ', ' '), ('\t', '\t'), ('\n', '\n'), ('\r', '\r')], true),
        'n' => RxNode::Lit('\n'),
        't' => RxNode::Lit('\t'),
        'r' => RxNode::Lit('\r'),
        _ => RxNode::Lit(c),
    }
}

fn rx_parse_class(chars: &[char], pos: &mut usize) -> RxNode {
    let neg = *pos < chars.len() && chars[*pos] == '^';
    if neg { *pos += 1; }
    let mut ranges: Vec<(char, char)> = vec![];
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
    RxNode::Class(ranges, neg)
}

// ---- Matching ----

fn rx_node_matches(node: &RxNode, c: char) -> bool {
    match node {
        RxNode::Lit(ch) => c == *ch,
        RxNode::Dot => c != '\n',
        RxNode::Class(ranges, neg) => {
            let hit = ranges.iter().any(|&(lo, hi)| c >= lo && c <= hi);
            hit != *neg
        }
        _ => false,
    }
}

fn rx_match_alts(alts: &[Vec<RxPiece>], s: &[char], p: usize, caps: &mut RxCaps) -> Option<usize> {
    for alt in alts {
        let save = caps.clone();
        if let Some(e) = rx_match_seq(alt, 0, s, p, caps) {
            return Some(e);
        }
        *caps = save;
    }
    None
}

fn rx_match_seq(seq: &[RxPiece], si: usize, s: &[char], p: usize, caps: &mut RxCaps) -> Option<usize> {
    if si >= seq.len() { return Some(p); }
    let piece = &seq[si];
    match &piece.node {
        RxNode::AnchorStart => {
            if p == 0 { rx_match_seq(seq, si + 1, s, p, caps) } else { None }
        }
        RxNode::AnchorEnd => {
            if p == s.len() { rx_match_seq(seq, si + 1, s, p, caps) } else { None }
        }
        _ => rx_match_rep(seq, si, s, p, caps, 0),
    }
}

fn rx_match_rep(seq: &[RxPiece], si: usize, s: &[char], p: usize, caps: &mut RxCaps, count: usize) -> Option<usize> {
    let piece = &seq[si];
    let at_max = piece.max.map_or(false, |m| count >= m);
    // Greedy: try to match one more first
    if !at_max {
        let save = caps.clone();
        if let Some(consumed) = rx_match_one(&piece.node, s, p, caps) {
            if consumed > 0 || count == 0 { // prevent infinite loop on zero-width
                if let Some(e) = rx_match_rep(seq, si, s, p + consumed, caps, count + 1) {
                    return Some(e);
                }
            }
        }
        *caps = save;
    }
    // Try rest of sequence if we have enough repetitions
    if count >= piece.min {
        return rx_match_seq(seq, si + 1, s, p, caps);
    }
    None
}

fn rx_match_one(node: &RxNode, s: &[char], p: usize, caps: &mut RxCaps) -> Option<usize> {
    match node {
        RxNode::Lit(_) | RxNode::Dot | RxNode::Class(_, _) => {
            if p < s.len() && rx_node_matches(node, s[p]) { Some(1) } else { None }
        }
        RxNode::Group(alts, ci) => {
            let start = p;
            if let Some(end) = rx_match_alts(alts, s, p, caps) {
                if *ci > 0 {
                    while caps.len() < *ci { caps.push(None); }
                    caps[*ci - 1] = Some((start, end));
                }
                Some(end - p)
            } else {
                None
            }
        }
        _ => None,
    }
}

// ---- Search ----

fn rx_find_at(rx: &RxPat, s: &[char], start: usize) -> Option<(usize, usize, RxCaps)> {
    for i in start..=s.len() {
        let mut caps: RxCaps = vec![None; rx.ncap];
        if let Some(end) = rx_match_alts(&rx.alts, s, i, &mut caps) {
            return Some((i, end, caps));
        }
    }
    None
}

// ---- Public API ----

fn almide_regex_is_match(pat: &str, s: &str) -> bool {
    let rx = rx_compile(pat);
    let chars: Vec<char> = s.chars().collect();
    rx_find_at(&rx, &chars, 0).is_some()
}

fn almide_regex_full_match(pat: &str, s: &str) -> bool {
    let rx = rx_compile(pat);
    let chars: Vec<char> = s.chars().collect();
    let mut caps: RxCaps = vec![None; rx.ncap];
    if let Some(end) = rx_match_alts(&rx.alts, &chars, 0, &mut caps) {
        end == chars.len()
    } else {
        false
    }
}

fn almide_regex_find(pat: &str, s: &str) -> Option<String> {
    let rx = rx_compile(pat);
    let chars: Vec<char> = s.chars().collect();
    rx_find_at(&rx, &chars, 0).map(|(start, end, _)| chars[start..end].iter().collect())
}

fn almide_regex_find_all(pat: &str, s: &str) -> Vec<String> {
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

fn almide_regex_replace(pat: &str, s: &str, rep: &str) -> String {
    let rx = rx_compile(pat);
    let chars: Vec<char> = s.chars().collect();
    let mut result = String::new();
    let mut pos = 0;
    while pos <= chars.len() {
        if let Some((start, end, _)) = rx_find_at(&rx, &chars, pos) {
            result.extend(&chars[pos..start]);
            result.push_str(rep);
            pos = if end > start { end } else { result.push(chars[end]); end + 1 };
        } else {
            result.extend(&chars[pos..]);
            break;
        }
    }
    result
}

fn almide_regex_replace_first(pat: &str, s: &str, rep: &str) -> String {
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

fn almide_regex_split(pat: &str, s: &str) -> Vec<String> {
    let rx = rx_compile(pat);
    let chars: Vec<char> = s.chars().collect();
    let mut results: Vec<String> = vec![];
    let mut pos = 0;
    while pos <= chars.len() {
        if let Some((start, end, _)) = rx_find_at(&rx, &chars, pos) {
            if end == start && start == pos {
                // Zero-width match at current position: take one char and move on
                if pos < chars.len() {
                    results.push(chars[pos..pos + 1].iter().collect());
                    pos = pos + 1;
                } else {
                    break;
                }
                continue;
            }
            results.push(chars[pos..start].iter().collect());
            pos = end;
        } else {
            results.push(chars[pos..].iter().collect());
            break;
        }
    }
    results
}

fn almide_regex_captures(pat: &str, s: &str) -> Option<Vec<String>> {
    let rx = rx_compile(pat);
    if rx.ncap == 0 { return None; }
    let chars: Vec<char> = s.chars().collect();
    if let Some((_, _, caps)) = rx_find_at(&rx, &chars, 0) {
        let result: Vec<String> = caps.iter().map(|c| {
            match c {
                Some((start, end)) => chars[*start..*end].iter().collect(),
                None => String::new(),
            }
        }).collect();
        Some(result)
    } else {
        None
    }
}

// ---- End Regex Runtime ----
