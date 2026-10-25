// json extern — JSON parse, stringify, and AlmideValue query/manipulation
// AlmideValue type is defined in value.rs (included before this file)

// The nesting bound (#4020 / #4209): JSON_MAX_DEPTH and json_depth_error(),
// declared once for every leg.
include!("../../../crates/almide-base/src/json_limits.rs");

// ── Parse + Stringify ──

pub fn almide_rt_json_stringify(v: &AlmideValue) -> AlmideString { almide_rt_value_stringify(v) }

pub fn almide_rt_json_parse(text: &str) -> Result<AlmideValue, AlmideString> {
    let __almide_r: Result<AlmideValue, String> = (|| {
        // Byte-offset recursive descent (#1678) over RFC 8259 plus I-JSON
        // (RFC 7493), STRICT (ALS-T3, C-087 / C-400 / C-401, #4111 / #4112):
        // the whole text is one value between JSON whitespace (space, tab, LF,
        // CR), and every departure is an err with one text per case — a lone
        // surrogate, an unknown or short escape, a raw control character, a
        // noncharacter, an unterminated string, a duplicate key, trailing data,
        // an unquoted key, a trailing comma, a missing separator, a number
        // outside the RFC grammar. Nothing is ever dropped: the parser used to
        // delete a lone surrogate and an unknown escape, so `ad\ud800min`
        // decoded to `admin`. An error reports the CHAR index of the offending
        // position, computed by one linear count at error time only.
        // stdlib/json_parse.almd is the self-hosted twin, clause for clause.
        //
        // Nesting is bounded (R-L0-47, R-json-04, #4020): an array or object
        // opened inside JSON_MAX_DEPTH (1000) others is
        // `json: nesting deeper than 1000`.
        fn at(what: &str, t: &str, pos: usize) -> String {
            format!("{} at pos {}", what, t[..pos].chars().count())
        }
        fn skip_ws(t: &str, pos: &mut usize) {
            let b = t.as_bytes();
            while *pos < b.len() && matches!(b[*pos], b' ' | b'\t' | b'\n' | b'\r') { *pos += 1; }
        }
        /// I-JSON §2.1: U+FDD0..=U+FDEF and every U+xFFFE / U+xFFFF.
        #[inline]
        fn is_nonchar(cp: u32) -> bool { (0xFDD0..=0xFDEF).contains(&cp) || (cp & 0xFFFE) == 0xFFFE }
        /// From `i`, the offset of the next `"` or `\` (or the end). A raw
        /// control character or a raw noncharacter on the way is an err.
        /// Noncharacters are three or four UTF-8 bytes led by 0xEF..=0xF4, so
        /// only those lead bytes are decoded.
        fn scan_plain(t: &str, mut i: usize) -> Result<usize, String> {
            let b = t.as_bytes();
            while i < b.len() {
                let c = b[i];
                if c == b'"' || c == b'\\' { return Ok(i); }
                if c < 0x20 { return Err(at("json: control character in string", t, i)); }
                if c >= 0xEF {
                    let ch = t[i..].chars().next().unwrap_or('\0');
                    if is_nonchar(ch as u32) { return Err(at("json: noncharacter", t, i)); }
                    i += ch.len_utf8();
                } else {
                    i += 1;
                }
            }
            Ok(i)
        }
        fn parse_value(t: &str, pos: &mut usize, depth: usize) -> Result<AlmideValue, String> {
            skip_ws(t, pos);
            let b = t.as_bytes();
            if *pos >= b.len() { return Err("unexpected end of input".into()); }
            match b[*pos] {
                b'"' => parse_string(t, pos).map(AlmideValue::Str),
                b'{' | b'[' if depth >= JSON_MAX_DEPTH => Err(json_depth_error()),
                b'{' => parse_object(t, pos, depth + 1),
                b'[' => parse_array(t, pos, depth + 1),
                b't' => parse_literal(t, pos, b"true", AlmideValue::Bool(true), "expected bool"),
                b'f' => parse_literal(t, pos, b"false", AlmideValue::Bool(false), "expected bool"),
                b'n' => parse_literal(t, pos, b"null", AlmideValue::Null, "expected null"),
                c if c == b'-' || c.is_ascii_digit() => parse_number(t, pos),
                _ => {
                    let ch = t[*pos..].chars().next().unwrap_or('\u{FFFD}');
                    Err(format!("unexpected char '{}' at pos {}", ch, t[..*pos].chars().count()))
                }
            }
        }
        fn parse_literal(t: &str, pos: &mut usize, word: &[u8], v: AlmideValue, msg: &str) -> Result<AlmideValue, String> {
            if t.as_bytes()[*pos..].starts_with(word) { *pos += word.len(); Ok(v) } else { Err(msg.into()) }
        }
        /// A string at its opening quote. The escape-free run becomes the
        /// `AlmideString` straight from the input slice (#2157); anything with
        /// an escape is decoded into a `String` first.
        fn parse_string(t: &str, pos: &mut usize) -> Result<AlmideString, String> {
            let open = *pos;
            let start = open + 1;
            let i = scan_plain(t, start)?;
            let b = t.as_bytes();
            if i >= b.len() { return Err(at("json: unterminated string", t, open)); }
            if b[i] == b'"' {
                *pos = i + 1;
                return Ok(AlmideString::from(&t[start..i]));
            }
            decode_string(t, pos, open, start, i).map(AlmideString::from)
        }
        /// An object key: the escape-free run is interned straight from the
        /// input slice; anything with an escape is decoded and then interned.
        fn parse_key(t: &str, pos: &mut usize) -> Result<AlmideKey, String> {
            let open = *pos;
            let start = open + 1;
            let i = scan_plain(t, start)?;
            let b = t.as_bytes();
            if i >= b.len() { return Err(at("json: unterminated string", t, open)); }
            if b[i] == b'"' {
                *pos = i + 1;
                return Ok(almide_rt_intern_key(&t[start..i]));
            }
            decode_string(t, pos, open, start, i).map(|s| almide_rt_intern_key(&s))
        }
        /// The general path: `[start, i)` is plain and `i` is at a backslash.
        fn decode_string(t: &str, pos: &mut usize, open: usize, start: usize, mut i: usize) -> Result<String, String> {
            let b = t.as_bytes();
            let mut s = String::with_capacity(i - start + 8);
            s.push_str(&t[start..i]);
            loop {
                if i >= b.len() { return Err(at("json: unterminated string", t, open)); }
                if b[i] == b'"' { *pos = i + 1; return Ok(s); }
                // b[i] is a backslash.
                let Some(&e) = b.get(i + 1) else { return Err(at("json: unterminated string", t, open)) };
                let simple = match e {
                    b'"' => Some('"'), b'\\' => Some('\\'), b'/' => Some('/'),
                    b'b' => Some('\u{0008}'), b'f' => Some('\u{000c}'),
                    b'n' => Some('\n'), b'r' => Some('\r'), b't' => Some('\t'),
                    _ => None,
                };
                if let Some(ch) = simple {
                    s.push(ch);
                    i += 2;
                } else if e == b'u' {
                    let cp = unicode_escape(t, i)?;
                    s.push(char::from_u32(cp).unwrap_or('\u{FFFD}'));
                    i += if cp >= 0x10000 { 12 } else { 6 };
                } else {
                    return Err(at("json: unknown escape", t, i));
                }
                let j = scan_plain(t, i)?;
                s.push_str(&t[i..j]);
                i = j;
            }
        }
        fn hex4(t: &str, p: usize) -> Option<u32> {
            let h = t.as_bytes().get(p..p + 4)?;
            if !h.iter().all(|c| c.is_ascii_hexdigit()) { return None; }
            u32::from_str_radix(std::str::from_utf8(h).ok()?, 16).ok()
        }
        /// `\uXXXX` at `i` (the backslash): the code point, a high+low
        /// surrogate pair joined (#651). A surrogate that is not such a pair
        /// is a lone surrogate; a noncharacter is refused.
        fn unicode_escape(t: &str, i: usize) -> Result<u32, String> {
            let b = t.as_bytes();
            let unit = hex4(t, i + 2).ok_or_else(|| at("json: invalid unicode escape", t, i))?;
            let cp = if (0xD800..=0xDBFF).contains(&unit) {
                let low = if b.get(i + 6) == Some(&b'\\') && b.get(i + 7) == Some(&b'u') { hex4(t, i + 8) } else { None };
                match low {
                    Some(lo) if (0xDC00..=0xDFFF).contains(&lo) => 0x10000 + ((unit - 0xD800) << 10) + (lo - 0xDC00),
                    _ => return Err(at("json: lone surrogate", t, i)),
                }
            } else if (0xDC00..=0xDFFF).contains(&unit) {
                return Err(at("json: lone surrogate", t, i));
            } else {
                unit
            };
            if is_nonchar(cp) { return Err(at("json: noncharacter", t, i)); }
            Ok(cp)
        }
        fn digits(b: &[u8], mut p: usize) -> usize {
            while p < b.len() && b[p].is_ascii_digit() { p += 1; }
            p
        }
        /// The number token is the run of `0-9 + - . e E` from `pos`; it must
        /// match `-? (0 | [1-9][0-9]*) (. [0-9]+)? ([eE] [+-]? [0-9]+)?` whole.
        fn parse_number(t: &str, pos: &mut usize) -> Result<AlmideValue, String> {
            let b = t.as_bytes();
            let start = *pos;
            let mut end = start;
            while end < b.len() && matches!(b[end], b'0'..=b'9' | b'+' | b'-' | b'.' | b'e' | b'E') { end += 1; }
            let bad = || Err(at("json: invalid number", t, start));
            let mut q = if b[start] == b'-' { start + 1 } else { start };
            let int_end = digits(b, q);
            if int_end == q || (b[q] == b'0' && int_end > q + 1) { return bad(); }
            q = int_end;
            let mut is_float = false;
            if q < end && b[q] == b'.' {
                let f = digits(b, q + 1);
                if f == q + 1 { return bad(); }
                q = f;
                is_float = true;
            }
            if q < end && (b[q] == b'e' || b[q] == b'E') {
                let s = if q + 1 < end && (b[q + 1] == b'+' || b[q + 1] == b'-') { q + 2 } else { q + 1 };
                let e = digits(b, s);
                if e == s { return bad(); }
                q = e;
                is_float = true;
            }
            if q != end { return bad(); }
            *pos = end;
            let s = &t[start..end];
            // A float token reads as `float.parse` reads it (C-413), the wasm
            // leg's json_parse.almd included: one that overflows binary64 is
            // the same err there, never an infinity (I-JSON, RFC 7493 §2.2).
            if is_float {
                match s.parse::<f64>() {
                    Ok(f) if f.is_infinite() => Err("float literal out of range".to_string()),
                    r => r.map(AlmideValue::Float).map_err(|e| e.to_string()),
                }
            }
            else { s.parse::<i64>().map(AlmideValue::Int).map_err(|e| e.to_string()) }
        }
        fn parse_array(t: &str, pos: &mut usize, depth: usize) -> Result<AlmideValue, String> {
            let b = t.as_bytes();
            *pos += 1;
            skip_ws(t, pos);
            // Sized for the common small container: a Vec that grows from 0 reallocates
            // at 4 and 8 on the way to a typical 8-field object (#1678 follow-up, measured
            // 3-8% on the parse benchmark).
            let mut items = Vec::with_capacity(8);
            if *pos < b.len() && b[*pos] == b']' { *pos += 1; return Ok(AlmideValue::Array(items)); }
            loop {
                items.push(parse_value(t, pos, depth)?);
                skip_ws(t, pos);
                if *pos >= b.len() { return Err("unexpected end of input".into()); }
                match b[*pos] {
                    b']' => { *pos += 1; return Ok(AlmideValue::Array(items)); }
                    b',' => {
                        let comma = *pos;
                        *pos += 1;
                        skip_ws(t, pos);
                        if *pos < b.len() && b[*pos] == b']' { return Err(at("json: trailing comma", t, comma)); }
                    }
                    _ => return Err(at("json: expected ',' or ']'", t, *pos)),
                }
            }
        }
        /// A key the object already holds is refused where the repeat starts
        /// (I-JSON §2.3). Small objects compare against the pairs read so far;
        /// from 16 keys on, a set built once answers instead.
        fn parse_object(t: &str, pos: &mut usize, depth: usize) -> Result<AlmideValue, String> {
            let b = t.as_bytes();
            *pos += 1;
            skip_ws(t, pos);
            let mut pairs: Vec<(AlmideKey, AlmideValue)> = Vec::with_capacity(8);
            if *pos < b.len() && b[*pos] == b'}' { *pos += 1; return Ok(AlmideValue::Object(pairs)); }
            let mut seen: Option<std::collections::HashSet<String>> = None;
            loop {
                if *pos >= b.len() { return Err("unexpected end of input".into()); }
                if b[*pos] != b'"' { return Err(at("json: unquoted key", t, *pos)); }
                let key_at = *pos;
                let key = parse_key(t, pos)?;
                let repeat = match &mut seen {
                    Some(set) => !set.insert(key.to_string()),
                    None => {
                        let hit = pairs.iter().any(|(k, _)| *k == key);
                        if !hit && pairs.len() >= 16 {
                            let mut set: std::collections::HashSet<String> = pairs.iter().map(|(k, _)| k.to_string()).collect();
                            set.insert(key.to_string());
                            seen = Some(set);
                        }
                        hit
                    }
                };
                if repeat { return Err(at("json: duplicate key", t, key_at)); }
                skip_ws(t, pos);
                if *pos >= b.len() { return Err("unexpected end of input".into()); }
                if b[*pos] != b':' { return Err(at("json: expected ':'", t, *pos)); }
                *pos += 1;
                let val = parse_value(t, pos, depth)?;
                pairs.push((key, val));
                skip_ws(t, pos);
                if *pos >= b.len() { return Err("unexpected end of input".into()); }
                match b[*pos] {
                    b'}' => { *pos += 1; return Ok(AlmideValue::Object(pairs)); }
                    b',' => {
                        let comma = *pos;
                        *pos += 1;
                        skip_ws(t, pos);
                        if *pos < b.len() && b[*pos] == b'}' { return Err(at("json: trailing comma", t, comma)); }
                    }
                    _ => return Err(at("json: expected ',' or '}'", t, *pos)),
                }
            }
        }
        let mut pos = 0usize;
        let v = parse_value(text, &mut pos, 0)?;
        skip_ws(text, &mut pos);
        if pos < text.len() { return Err(at("json: trailing data", text, pos)); }
        Ok(v)
    })();
    __almide_r.map_err(Into::into)
}

// ── Key-based getters ──

pub fn almide_json_get(j: &AlmideValue, key: &str) -> Option<AlmideValue> {
    match j {
        AlmideValue::Object(entries) => entries.iter().find(|(k, _)| k.as_ref() == key).map(|(_, v)| v.clone()),
        _ => None,
    }
}

pub fn almide_json_get_string(j: &AlmideValue, key: &str) -> Option<AlmideString> { let __almide_v: Option<AlmideString> = (|| {
    match almide_json_get(j, key)? { AlmideValue::Str(s) => Some(s), _ => None }
})(); __almide_v.into_almide() }

/// An Int field only: a Float value is `None`, even an integral one (`3.0`),
/// never truncated — the same answer as `value.as_int` and Codec Int decode, and
/// the self-hosted `json_get_int` the wasm legs link (#2476). The widening runs
/// one way only: `get_float` accepts an Int (C-085).
pub fn almide_json_get_int(j: &AlmideValue, key: &str) -> Option<i64> {
    match almide_json_get(j, key)? { AlmideValue::Int(n) => Some(n), _ => None }
}

pub fn almide_json_get_float(j: &AlmideValue, key: &str) -> Option<f64> {
    match almide_json_get(j, key)? { AlmideValue::Float(f) => Some(f), AlmideValue::Int(n) => Some(n as f64), _ => None }
}

pub fn almide_json_get_bool(j: &AlmideValue, key: &str) -> Option<bool> {
    match almide_json_get(j, key)? { AlmideValue::Bool(b) => Some(b), _ => None }
}

pub fn almide_json_get_array(j: &AlmideValue, key: &str) -> Option<Vec<AlmideValue>> {
    match almide_json_get(j, key)? { AlmideValue::Array(a) => Some(a), _ => None }
}

// ── Keyless extractors ──

pub fn almide_json_to_string(j: &AlmideValue) -> Option<AlmideString> { let __almide_v: Option<AlmideString> = (|| {
    match j { AlmideValue::Str(s) => Some(s.clone()), _ => None }
})(); __almide_v.into_almide() }

pub fn almide_json_to_int(j: &AlmideValue) -> Option<i64> {
    match j { AlmideValue::Int(n) => Some(*n), _ => None }
}

pub fn almide_json_as_float(j: &AlmideValue) -> Option<f64> {
    match j { AlmideValue::Float(f) => Some(*f), AlmideValue::Int(n) => Some(*n as f64), _ => None }
}

pub fn almide_json_as_bool(j: &AlmideValue) -> Option<bool> {
    match j { AlmideValue::Bool(b) => Some(*b), _ => None }
}

pub fn almide_json_as_array(j: &AlmideValue) -> Option<Vec<AlmideValue>> {
    match j { AlmideValue::Array(a) => Some(a.clone()), _ => None }
}

// ── Object operations ──

pub fn almide_json_keys(j: &AlmideValue) -> Vec<AlmideString> { let __almide_v: Vec<String> = (|| {
    match j { AlmideValue::Object(entries) => entries.iter().map(|(k, _)| k.to_string()).collect(), _ => vec![] }
})(); __almide_v.into_almide() }

pub fn almide_json_to_map(j: &AlmideValue) -> Option<AlmideMap<AlmideString, AlmideString>> { let __almide_v: Option<AlmideMap<String, String>> = (|| {
    match j {
        AlmideValue::Object(entries) => {
            let map: AlmideMap<String, String> = entries.iter().map(|(k, v)| {
                let s = match v {
                    AlmideValue::Str(s) => s.to_string(),
                    _ => String::from(almide_rt_value_stringify(v)),
                };
                (k.to_string(), s)
            }).collect();
            Some(map)
        }
        _ => None,
    }
})(); __almide_v.into_almide() }

pub fn almide_json_object(entries: &[(AlmideString, AlmideValue)]) -> AlmideValue {
    AlmideValue::Object(entries.iter().map(|(k, v)| (almide_rt_intern_key(k), v.clone())).collect())
}

pub fn almide_json_from_float(n: f64) -> AlmideValue { AlmideValue::Float(n) }
pub fn almide_json_from_string(s: &str) -> AlmideValue { AlmideValue::Str(AlmideString::from(s)) }
pub fn almide_json_from_int(n: i64) -> AlmideValue { AlmideValue::Int(n) }
pub fn almide_json_from_bool(b: bool) -> AlmideValue { AlmideValue::Bool(b) }
pub fn almide_json_array(items: &[AlmideValue]) -> AlmideValue { AlmideValue::Array(items.to_vec()) }

// ── Stringify pretty ──

pub fn almide_json_stringify_pretty(j: &AlmideValue) -> AlmideString { let __almide_v: String = (|| {
    stringify_value(j, 0)
})(); __almide_v.into_almide() }

/// JSON string quoting is `almide_rt_value_json_quote` (value.rs) — the one
/// RFC 8259 §7 rule every serializer shares (#2802). The previous `{:?}`
/// (Rust `escape_debug`) escaped combining marks as `\u{301}` — not valid
/// JSON escaping, and a byte divergence against the wasm leg.
fn json_quote(s: &str) -> String {
    String::from(almide_rt_value_json_quote(s))
}

fn stringify_value(v: &AlmideValue, depth: usize) -> String {
    let ind = "  ".repeat(depth);
    let ind1 = "  ".repeat(depth + 1);
    match v {
        AlmideValue::Null => "null".into(),
        AlmideValue::Bool(b) => if *b { "true" } else { "false" }.into(),
        AlmideValue::Int(n) => n.to_string(),
        AlmideValue::Float(f) => String::from(almide_rt_value_float_json(*f)),
        AlmideValue::Str(s) => json_quote(s),
        AlmideValue::Array(items) => {
            if items.is_empty() { return "[]".into(); }
            let parts: Vec<String> = items.iter().map(|v| format!("{}{}", ind1, stringify_value(v, depth + 1))).collect();
            format!("[\n{}\n{}]", parts.join(",\n"), ind)
        }
        AlmideValue::Object(entries) => {
            if entries.is_empty() { return "{}".into(); }
            let parts: Vec<String> = entries.iter().map(|(k, v)| format!("{}{}: {}", ind1, json_quote(k), stringify_value(v, depth + 1))).collect();
            format!("{{\n{}\n{}}}", parts.join(",\n"), ind)
        }
    }
}

// ── AlmideJsonPath type and operations ──


#[derive(Debug, Clone, PartialEq)]
pub enum AlmideJsonPath {
    JpRoot,
    JpField(std::boxed::Box<AlmideJsonPath>, String),
    JpIndex(std::boxed::Box<AlmideJsonPath>, i64),
}

impl std::fmt::Display for AlmideJsonPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlmideJsonPath::JpRoot => write!(f, "$"),
            AlmideJsonPath::JpField(parent, name) => write!(f, "{}.{}", parent, name),
            AlmideJsonPath::JpIndex(parent, i) => write!(f, "{}[{}]", parent, i),
        }
    }
}

// A record may hold a path, and a record's repr calls each field's (#2647).
impl AlmideRepr for AlmideJsonPath {
    fn almide_repr(&self) -> String { self.to_string() }
}

// `json.__json_path_repr` (#4204): the declared repr is this impl; the wasm
// legs' self-hosted twin prints the same text.
pub fn almide_rt_json_path_repr(path: &AlmideJsonPath) -> AlmideString { path.almide_repr().into_almide() }

// Wrapper functions for stdlib codegen (json.root(), json.field(), json.index())
pub fn almide_rt_json_root() -> AlmideJsonPath { AlmideJsonPath::JpRoot }
pub fn almide_rt_json_field(path: AlmideJsonPath, name: &str) -> AlmideJsonPath { AlmideJsonPath::JpField(std::boxed::Box::new(path), name.to_string()) }
pub fn almide_rt_json_index(path: AlmideJsonPath, i: i64) -> AlmideJsonPath { AlmideJsonPath::JpIndex(std::boxed::Box::new(path), i) }

/// Resolve a AlmideJsonPath to a list of traversal steps, root-first.
fn resolve_path(path: &AlmideJsonPath) -> Vec<AlmidePathStep> {
    let mut steps = Vec::new();
    let mut current = path;
    loop {
        match current {
            AlmideJsonPath::JpRoot => break,
            AlmideJsonPath::JpField(parent, name) => {
                steps.push(AlmidePathStep::Field(name.clone()));
                current = parent;
            }
            AlmideJsonPath::JpIndex(parent, i) => {
                steps.push(AlmidePathStep::Index(*i));
                current = parent;
            }
        }
    }
    steps.reverse();
    steps
}

enum AlmidePathStep {
    Field(String),
    Index(i64),
}

fn get_by_step(v: &AlmideValue, step: &AlmidePathStep) -> Option<AlmideValue> {
    match step {
        AlmidePathStep::Field(key) => almide_json_get(v, key),
        AlmidePathStep::Index(i) => match v {
            AlmideValue::Array(items) => {
                let idx = if *i < 0 { items.len() as i64 + *i } else { *i } as usize;
                items.get(idx).cloned()
            }
            _ => None,
        },
    }
}

pub fn almide_json_get_path(j: &AlmideValue, path: &AlmideJsonPath) -> Option<AlmideValue> {
    let steps = resolve_path(path);
    let mut current = j.clone();
    for step in &steps {
        current = get_by_step(&current, step)?;
    }
    Some(current)
}

pub fn almide_json_set_path(j: &AlmideValue, path: &AlmideJsonPath, value: AlmideValue) -> Result<AlmideValue, AlmideString> { let __almide_r: Result<AlmideValue, String> = (|| {
    let steps = resolve_path(path);
    // #4209: the value set_path builds nests its path's steps plus the new
    // value's own nesting deep; past parse's bound it is refused, so a
    // value made here is one `json.parse` could have made.
    if steps.len() > JSON_MAX_DEPTH || json_nests_past(&value, JSON_MAX_DEPTH - steps.len()) {
        return Err(json_depth_error());
    }
    Ok(set_at_steps(j, &steps, &value))
})(); __almide_r.map_err(Into::into) }

/// The `@reuse(j)` twin (#4207): a document at its last use is updated IN
/// PLACE — only the path is walked, every untouched sibling stays where it is
/// — where the borrowing form clones the whole document per call, so a fold of
/// n `set_path` calls was quadratic in copies. Same answer as `set_at_steps`.
pub fn almide_json_set_path_owned(j: AlmideValue, path: &AlmideJsonPath, value: AlmideValue) -> Result<AlmideValue, AlmideString> {
    let steps = resolve_path(path);
    // #4209: the same nesting bound as the borrowing form, so the in-place
    // twin cannot build a value json.parse would refuse.
    if steps.len() > JSON_MAX_DEPTH || json_nests_past(&value, JSON_MAX_DEPTH - steps.len()) {
        return Err(json_depth_error().into());
    }
    Ok(set_at_steps_owned(j, &steps, &value))
}

fn set_at_steps_owned(j: AlmideValue, steps: &[AlmidePathStep], value: &AlmideValue) -> AlmideValue {
    if steps.is_empty() { return value.clone(); }
    match &steps[0] {
        AlmidePathStep::Field(key) => match j {
            AlmideValue::Object(mut entries) => {
                let rest = &steps[1..];
                let mut hit = false;
                for (k, v) in entries.iter_mut() {
                    if k.as_ref() == key.as_str() {
                        hit = true;
                        let old = std::mem::replace(v, AlmideValue::Null);
                        *v = set_at_steps_owned(old, rest, value);
                    }
                }
                if !hit {
                    entries.push((almide_rt_intern_key(key), set_at_steps_owned(AlmideValue::Object(vec![]), rest, value)));
                }
                AlmideValue::Object(entries)
            }
            _ => AlmideValue::Object(vec![(almide_rt_intern_key(key), set_at_steps_owned(AlmideValue::Object(vec![]), &steps[1..], value))]),
        },
        AlmidePathStep::Index(i) => match j {
            AlmideValue::Array(mut items) => {
                let idx = if *i < 0 { items.len() as i64 + *i } else { *i } as usize;
                if idx < items.len() {
                    let old = std::mem::replace(&mut items[idx], AlmideValue::Null);
                    items[idx] = set_at_steps_owned(old, &steps[1..], value);
                }
                AlmideValue::Array(items)
            }
            other => other,
        },
    }
}

/// `v` holds containers nested more than `budget` deep. Recurses at most
/// `budget + 1` frames whatever `v` holds.
fn json_nests_past(v: &AlmideValue, budget: usize) -> bool {
    match v {
        AlmideValue::Array(items) => budget == 0 || items.iter().any(|x| json_nests_past(x, budget - 1)),
        AlmideValue::Object(entries) => budget == 0 || entries.iter().any(|(_, x)| json_nests_past(x, budget - 1)),
        _ => false,
    }
}

pub fn almide_json_remove_path(j: &AlmideValue, path: &AlmideJsonPath) -> AlmideValue {
    let steps = resolve_path(path);
    remove_at_steps(j, &steps)
}

fn set_at_steps(j: &AlmideValue, steps: &[AlmidePathStep], value: &AlmideValue) -> AlmideValue {
    if steps.is_empty() { return value.clone(); }
    match &steps[0] {
        AlmidePathStep::Field(key) => match j {
            AlmideValue::Object(entries) => {
                let rest = &steps[1..];
                let mut new_entries: Vec<(AlmideKey, AlmideValue)> = entries.iter()
                    .map(|(k, v)| if k.as_ref() == key.as_str() { (k.clone(), set_at_steps(v, rest, value)) } else { (k.clone(), v.clone()) })
                    .collect();
                if !entries.iter().any(|(k, _)| k.as_ref() == key.as_str()) {
                    new_entries.push((almide_rt_intern_key(key), set_at_steps(&AlmideValue::Object(vec![]), rest, value)));
                }
                AlmideValue::Object(new_entries)
            }
            _ => AlmideValue::Object(vec![(almide_rt_intern_key(key), set_at_steps(&AlmideValue::Object(vec![]), &steps[1..], value))]),
        },
        AlmidePathStep::Index(i) => match j {
            AlmideValue::Array(items) => {
                let idx = if *i < 0 { items.len() as i64 + *i } else { *i } as usize;
                let mut new_items = items.clone();
                if idx < new_items.len() {
                    new_items[idx] = set_at_steps(&new_items[idx], &steps[1..], value);
                }
                AlmideValue::Array(new_items)
            }
            _ => j.clone(),
        },
    }
}

fn remove_at_steps(j: &AlmideValue, steps: &[AlmidePathStep]) -> AlmideValue {
    if steps.is_empty() { return AlmideValue::Null; }
    match &steps[0] {
        AlmidePathStep::Field(key) => match j {
            AlmideValue::Object(entries) => {
                if steps.len() == 1 {
                    AlmideValue::Object(entries.iter().filter(|(k, _)| k.as_ref() != key.as_str()).cloned().collect())
                } else {
                    AlmideValue::Object(entries.iter().map(|(k, v)| {
                        if k.as_ref() == key.as_str() { (k.clone(), remove_at_steps(v, &steps[1..])) } else { (k.clone(), v.clone()) }
                    }).collect())
                }
            }
            other => other.clone(),
        },
        AlmidePathStep::Index(i) => match j {
            AlmideValue::Array(items) => {
                let idx = if *i < 0 { items.len() as i64 + *i } else { *i } as usize;
                if steps.len() == 1 {
                    AlmideValue::Array(items.iter().enumerate().filter(|(ii, _)| *ii != idx).map(|(_, v)| v.clone()).collect())
                } else {
                    let mut new_items = items.clone();
                    if idx < new_items.len() {
                        new_items[idx] = remove_at_steps(&new_items[idx], &steps[1..]);
                    }
                    AlmideValue::Array(new_items)
                }
            }
            other => other.clone(),
        },
    }
}
