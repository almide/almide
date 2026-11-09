//! The interp's own reading of `json.parse(text)` (ADR-0033): an independent
//! recursive-descent parser over the text, used where the self-hosted body
//! (`stdlib/json_parse.almd`) abstains.
//!
//! The body carves the whole document into ONE Bytes family laid out for the
//! structural wasm leg's Value boxes and leaves through `prim.load_handle` of
//! an interior address — a layout this arena does not model (it models
//! value_core's blocks), so the body's success path abstains here. Its
//! REFUSAL path needs no family and still votes on its own. Where it
//! abstains, `dispatch_json.rs` answers with this parser and materializes the
//! document in value_core's layout, recorded as a bridge fallback.
//!
//! The grammar and every err text are clause-for-clause the body's (ALS-T3:
//! C-087, C-400, C-401, C-433), NOT read from any runtime (no oracle
//! circularity):
//!   - one value between JSON whitespace (space, tab, LF, CR), else
//!     `json: trailing data at pos N`;
//!   - strings checked in full (unterminated, raw control character, unknown
//!     escape, bad `\u`, lone surrogate, noncharacter); a high+low pair joins;
//!   - object keys are strings; a repeated key (DECODED) is refused at the
//!     repeat's opening quote. The body checks the first 16 keys as they are
//!     read and defers the rest to one index whose earliest repeat wins over a
//!     later refusal in the same object — which is exactly the earliest
//!     repeat in document order, so one check as each key is read is the same
//!     answer;
//!   - a number token is the run of `0-9 + - . e E` and must match the
//!     RFC 8259 grammar whole; a fraction or exponent makes a Float, else Int;
//!     an out-of-range one passes int.parse's / float.parse's err text (the
//!     caller supplies those, from the interp's own `int.parse` /
//!     `float.parse`);
//!   - an array or object opened inside JSON_MAX_DEPTH others is
//!     `json_depth_error()`.
//!
//! Positions in messages are CHAR indices of the byte offset where the body
//! names them.

use crate::value::DynNode;
use almide_base::json_limits::JSON_MAX_DEPTH;

/// Why the text is not a JSON document: the first refusal in document order.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Refusal {
    /// `unexpected end of input` (no position).
    EndOfInput,
    /// `expected bool` / `expected null` — a literal the body names without
    /// a position.
    Plain(&'static str),
    /// `json_depth_error()` (no position).
    Depth,
    /// `unexpected char 'c' at pos N` — a value cannot start at this byte.
    UnexpectedChar(usize),
    /// `<what> at pos N`.
    At(&'static str, usize),
    /// A grammatical number token `[start, end)` whose value is out of range:
    /// the err text is int.parse's (Int) or float.parse's (Float) for it.
    Number { start: usize, end: usize, float: bool },
}

const TRAILING_DATA: &str = "json: trailing data";
const UNTERMINATED: &str = "json: unterminated string";
const CONTROL: &str = "json: control character in string";
const UNKNOWN_ESCAPE: &str = "json: unknown escape";
const BAD_UNICODE: &str = "json: invalid unicode escape";
const LONE_SURROGATE: &str = "json: lone surrogate";
const NONCHARACTER: &str = "json: noncharacter";
const TRAILING_COMMA: &str = "json: trailing comma";
const UNQUOTED_KEY: &str = "json: unquoted key";
const DUPLICATE_KEY: &str = "json: duplicate key";
const EXPECTED_COLON: &str = "json: expected ':'";
const INVALID_NUMBER: &str = "json: invalid number";

/// Parse `text` as one JSON document.
pub(crate) fn parse(text: &str) -> Result<DynNode, Refusal> {
    let p = Parser { s: text, t: text.as_bytes() };
    let (v, e) = p.value(0, 0)?;
    let q = p.ws(e);
    if q < p.t.len() {
        return Err(Refusal::At(TRAILING_DATA, q));
    }
    Ok(v)
}

/// The refusal's err text. `number_err(token, float)` answers int.parse's
/// (`float == false`) or float.parse's err text for an out-of-range token.
pub(crate) fn message(
    text: &str,
    r: &Refusal,
    number_err: &mut dyn FnMut(&str, bool) -> Option<String>,
) -> Option<String> {
    Some(match r {
        Refusal::EndOfInput => "unexpected end of input".to_string(),
        Refusal::Plain(what) => what.to_string(),
        Refusal::Depth => almide_base::json_limits::json_depth_error(),
        Refusal::UnexpectedChar(p) => {
            let c: String = text.get(*p..)?.chars().next().map(String::from).unwrap_or_default();
            format!(
                "unexpected char '{}' at pos {}",
                almide_base::err_text::almide_rt_errtext_escape(&c),
                char_pos(text, *p)
            )
        }
        Refusal::At(what, p) => format!("{} at pos {}", what, char_pos(text, *p)),
        Refusal::Number { start, end, float } => number_err(text.get(*start..*end)?, *float)?,
    })
}

/// The CHAR index of byte offset `p`: the non-continuation bytes before it.
pub(crate) fn char_pos(text: &str, p: usize) -> usize {
    text.as_bytes()[..p.min(text.len())].iter().filter(|b| !(0x80..0xC0).contains(*b)).count()
}

/// I-JSON §2.1: U+FDD0..U+FDEF and every U+xFFFE / U+xFFFF.
fn nonchar(cp: u32) -> bool {
    (0xFDD0..=0xFDEF).contains(&cp) || cp % 0x10000 >= 0xFFFE
}

enum After {
    Close(usize),
    Next(usize),
}

struct Parser<'t> {
    s: &'t str,
    t: &'t [u8],
}

impl<'t> Parser<'t> {
    fn ws(&self, mut p: usize) -> usize {
        while p < self.t.len() && matches!(self.t[p], b' ' | b'\t' | b'\n' | b'\r') {
            p += 1;
        }
        p
    }

    fn digits(&self, mut p: usize) -> usize {
        while p < self.t.len() && self.t[p].is_ascii_digit() {
            p += 1;
        }
        p
    }

    /// The value at `p` (whitespace first) with `depth` containers around it:
    /// the value and the offset after it.
    fn value(&self, p: usize, depth: usize) -> Result<(DynNode, usize), Refusal> {
        let q = self.ws(p);
        let Some(&b) = self.t.get(q) else {
            return Err(Refusal::EndOfInput);
        };
        match b {
            b'"' => {
                let (s, e) = self.string(q)?;
                Ok((DynNode::Str(s), e + 1))
            }
            b'{' | b'[' if depth >= JSON_MAX_DEPTH => Err(Refusal::Depth),
            b'{' => self.object(q + 1, depth + 1),
            b'[' => self.array(q + 1, depth + 1),
            b't' | b'f' | b'n' => self.literal(q, b),
            b'-' | b'0'..=b'9' => self.number(q),
            _ => Err(Refusal::UnexpectedChar(q)),
        }
    }

    fn literal(&self, q: usize, b: u8) -> Result<(DynNode, usize), Refusal> {
        let (word, node, what) = match b {
            b't' => ("true", DynNode::Bool(true), "expected bool"),
            b'f' => ("false", DynNode::Bool(false), "expected bool"),
            _ => ("null", DynNode::Null, "expected null"),
        };
        if self.t[q..].starts_with(word.as_bytes()) {
            Ok((node, q + word.len()))
        } else {
            Err(Refusal::Plain(what))
        }
    }

    fn number(&self, p: usize) -> Result<(DynNode, usize), Refusal> {
        let n = self.t.len();
        // The run of `0-9 + - . e E`: the token the grammar must match whole.
        let mut end = p;
        while end < n && matches!(self.t[end], b'0'..=b'9' | b'+' | b'-' | b'.' | b'e' | b'E') {
            end += 1;
        }
        if self.num_end(p, end) != Some(end) {
            return Err(Refusal::At(INVALID_NUMBER, p));
        }
        let lead = if self.t[p] == b'-' { p + 1 } else { p };
        let tok = &self.s[p..end];
        if self.digits(lead) < end {
            // A fraction or exponent: a Float, refused when it is not finite
            // (the body's decimal-exponent test: <= 308 fits, >= 310 never,
            // 309 asks float.parse — all three the finiteness of the
            // correctly rounded value).
            match tok.parse::<f64>() {
                Ok(f) if f.is_finite() => Ok((DynNode::Float(f), end)),
                _ => Err(Refusal::Number { start: p, end, float: true }),
            }
        } else {
            match tok.parse::<i64>() {
                Ok(i) => Ok((DynNode::Int(i), end)),
                Err(_) => Err(Refusal::Number { start: p, end, float: false }),
            }
        }
    }

    /// `-? (0 | [1-9][0-9]*) (. [0-9]+)? ([eE] [+-]? [0-9]+)?` from `p`
    /// inside the run ending at `end`: where the match ends, or `None`.
    fn num_end(&self, p: usize, end: usize) -> Option<usize> {
        let byte = |i: usize| if i < end { self.t[i] } else { 0 };
        let p1 = if byte(p) == b'-' { p + 1 } else { p };
        let p2 = self.digits(p1);
        if p2 == p1 || p2 > end || (self.t[p1] == b'0' && p2 > p1 + 1) {
            return None;
        }
        let p3 = if byte(p2) == b'.' {
            let p3 = self.digits(p2 + 1);
            if p3 == p2 + 1 {
                return None;
            }
            p3
        } else {
            p2
        };
        if matches!(byte(p3), b'e' | b'E') {
            let ps = if matches!(byte(p3 + 1), b'+' | b'-') { p3 + 2 } else { p3 + 1 };
            let p4 = self.digits(ps);
            if p4 == ps {
                return None;
            }
            Some(p4)
        } else {
            Some(p3)
        }
    }

    /// After a member that ended at `np`: the close, the next member's
    /// offset, or the refusal.
    fn after(&self, np: usize, close: u8) -> Result<After, Refusal> {
        let q = self.ws(np);
        match self.t.get(q) {
            None => Err(Refusal::EndOfInput),
            Some(&c) if c == close => Ok(After::Close(q)),
            Some(b',') => {
                let g = self.ws(q + 1);
                if self.t.get(g) == Some(&close) {
                    Err(Refusal::At(TRAILING_COMMA, q))
                } else {
                    Ok(After::Next(g))
                }
            }
            Some(_) => Err(Refusal::At(
                if close == b']' { "json: expected ',' or ']'" } else { "json: expected ',' or '}'" },
                q,
            )),
        }
    }

    /// `p` is just past `[`.
    fn array(&self, p: usize, depth: usize) -> Result<(DynNode, usize), Refusal> {
        let f = self.ws(p);
        let mut items = Vec::new();
        if self.t.get(f) == Some(&b']') {
            return Ok((DynNode::Arr(items), f + 1));
        }
        let mut q = f;
        loop {
            let (v, np) = self.value(q, depth)?;
            items.push(v);
            match self.after(np, b']')? {
                After::Close(c) => return Ok((DynNode::Arr(items), c + 1)),
                After::Next(n) => q = n,
            }
        }
    }

    /// `p` is just past `{`.
    fn object(&self, p: usize, depth: usize) -> Result<(DynNode, usize), Refusal> {
        let f = self.ws(p);
        let mut pairs: Vec<(String, DynNode)> = Vec::new();
        if self.t.get(f) == Some(&b'}') {
            return Ok((DynNode::Obj(pairs), f + 1));
        }
        let mut seen = std::collections::HashSet::new();
        let mut q = f;
        loop {
            match self.t.get(q) {
                None => return Err(Refusal::EndOfInput),
                Some(b'"') => {}
                Some(_) => return Err(Refusal::At(UNQUOTED_KEY, q)),
            }
            let (k, e) = self.string(q)?;
            if !seen.insert(k.clone()) {
                return Err(Refusal::At(DUPLICATE_KEY, q));
            }
            let c = self.ws(e + 1);
            match self.t.get(c) {
                None => return Err(Refusal::EndOfInput),
                Some(b':') => {}
                Some(_) => return Err(Refusal::At(EXPECTED_COLON, c)),
            }
            let (v, np) = self.value(c + 1, depth)?;
            pairs.push((k, v));
            match self.after(np, b'}')? {
                After::Close(c) => return Ok((DynNode::Obj(pairs), c + 1)),
                After::Next(n) => q = n,
            }
        }
    }

    /// Exactly four ASCII hex digits at `p`, as the code unit.
    fn hex4(&self, p: usize) -> Option<u32> {
        let h = self.t.get(p..p.checked_add(4)?)?;
        h.iter().try_fold(0u32, |acc, &b| Some(acc * 16 + (b as char).to_digit(16)?))
    }

    /// The string whose opening quote is at `open`: its decoded text and the
    /// closing quote's offset.
    fn string(&self, open: usize) -> Result<(String, usize), Refusal> {
        let mut out = String::new();
        let mut p = open + 1;
        loop {
            let Some(&b) = self.t.get(p) else {
                return Err(Refusal::At(UNTERMINATED, open));
            };
            match b {
                b'"' => return Ok((out, p)),
                0..=31 => return Err(Refusal::At(CONTROL, p)),
                b'\\' => {
                    let Some(&x) = self.t.get(p + 1) else {
                        return Err(Refusal::At(UNTERMINATED, open));
                    };
                    let simple = match x {
                        b'"' | b'\\' | b'/' => Some(x as char),
                        b'b' => Some('\u{8}'),
                        b'f' => Some('\u{c}'),
                        b'n' => Some('\n'),
                        b'r' => Some('\r'),
                        b't' => Some('\t'),
                        _ => None,
                    };
                    if let Some(c) = simple {
                        out.push(c);
                        p += 2;
                    } else if x == b'u' {
                        let (c, w) = self.unicode_escape(p)?;
                        out.push(c);
                        p += w;
                    } else {
                        return Err(Refusal::At(UNKNOWN_ESCAPE, p));
                    }
                }
                _ => {
                    // `p` is a char boundary: continuation bytes are consumed
                    // with their lead.
                    let c = self.s[p..].chars().next().expect("a char boundary inside the text");
                    if b >= 0xEF && nonchar(c as u32) {
                        return Err(Refusal::At(NONCHARACTER, p));
                    }
                    out.push(c);
                    p += c.len_utf8();
                }
            }
        }
    }

    /// `\uXXXX` (a high+low pair joined) at `p`, the backslash: the char and
    /// the escape's width.
    fn unicode_escape(&self, p: usize) -> Result<(char, usize), Refusal> {
        let Some(unit) = self.hex4(p + 2) else {
            return Err(Refusal::At(BAD_UNICODE, p));
        };
        let (cp, w) = if (0xD800..=0xDBFF).contains(&unit) {
            let lo = if self.t.get(p + 6) == Some(&b'\\') && self.t.get(p + 7) == Some(&b'u') {
                self.hex4(p + 8)
            } else {
                None
            };
            match lo {
                Some(lo) if (0xDC00..=0xDFFF).contains(&lo) => {
                    (0x10000 + (unit - 0xD800) * 1024 + (lo - 0xDC00), 12)
                }
                _ => return Err(Refusal::At(LONE_SURROGATE, p)),
            }
        } else if (0xDC00..=0xDFFF).contains(&unit) {
            return Err(Refusal::At(LONE_SURROGATE, p));
        } else {
            (unit, 6)
        };
        if nonchar(cp) {
            return Err(Refusal::At(NONCHARACTER, p));
        }
        Ok((char::from_u32(cp).expect("a scalar value"), w))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(text: &str) -> String {
        let r = parse(text).expect_err("a refusal");
        message(text, &r, &mut |tok, float| Some(format!("{}:{tok}", if float { "f" } else { "i" })))
            .unwrap()
    }

    #[test]
    fn documents_parse_to_their_nodes() {
        assert_eq!(
            parse(" {\"a\": [1, -2.5e1, true, null, \"x\\u00e9\\ud83d\\ude00\"], \"b\": {}} ").unwrap(),
            DynNode::Obj(vec![
                (
                    "a".into(),
                    DynNode::Arr(vec![
                        DynNode::Int(1),
                        DynNode::Float(-25.0),
                        DynNode::Bool(true),
                        DynNode::Null,
                        DynNode::Str("x\u{e9}\u{1F600}".into()),
                    ])
                ),
                ("b".into(), DynNode::Obj(vec![])),
            ])
        );
        assert_eq!(parse("-0").unwrap(), DynNode::Int(0));
        assert_eq!(parse("-9223372036854775808").unwrap(), DynNode::Int(i64::MIN));
    }

    #[test]
    fn each_refusal_names_its_text_and_char_position() {
        assert_eq!(msg(""), "unexpected end of input");
        assert_eq!(msg("[1,]"), "json: trailing comma at pos 2");
        assert_eq!(msg("[1 2]"), "json: expected ',' or ']' at pos 3");
        assert_eq!(msg("{\"a\" 1}"), "json: expected ':' at pos 5");
        assert_eq!(msg("{1:2}"), "json: unquoted key at pos 1");
        assert_eq!(msg("{\"é\":1,\"\\u00e9\":2}"), "json: duplicate key at pos 7");
        assert_eq!(msg("\"é\" x"), "json: trailing data at pos 4");
        assert_eq!(msg("\"ab"), "json: unterminated string at pos 0");
        assert_eq!(msg("\"\\x\""), "json: unknown escape at pos 1");
        assert_eq!(msg("\"\\ud800\""), "json: lone surrogate at pos 1");
        assert_eq!(msg("\"\\ufdd0\""), "json: noncharacter at pos 1");
        assert_eq!(msg("\"\\u12\""), "json: invalid unicode escape at pos 1");
        assert_eq!(msg("01"), "json: invalid number at pos 0");
        assert_eq!(msg("tru"), "expected bool");
        assert_eq!(msg("nul"), "expected null");
        assert_eq!(msg("[é, x]"), "unexpected char 'é' at pos 1");
        assert_eq!(msg("[1, x]"), "unexpected char 'x' at pos 4");
        assert_eq!(msg("99999999999999999999"), "i:99999999999999999999");
        assert_eq!(msg("1e400"), "f:1e400");
        let deep = "[".repeat(JSON_MAX_DEPTH + 1);
        assert_eq!(msg(&deep), almide_base::json_limits::json_depth_error());
        let ok = format!("{}{}", "[".repeat(JSON_MAX_DEPTH), "]".repeat(JSON_MAX_DEPTH));
        assert!(parse(&ok).is_ok());
    }
}
