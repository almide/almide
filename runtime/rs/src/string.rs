// string extern — Rust native implementations
// TOML templates use &*{s} which dereferences String to &str

// Codepoint count, per the documented contract ("number of characters").
// The whole position API (len / index_of / last_index_of / get / slice /
// take / drop) is CODEPOINT-indexed — one unit, no byte/char mixing (#419).
pub fn almide_rt_string_len(s: &str) -> i64 { s.chars().count() as i64 }
// Case mapping never folds a non-ASCII scalar INTO ASCII (C-412, #4121): an
// ASCII scalar maps by the ASCII rules; a non-ASCII scalar takes its default
// (locale-independent, full SpecialCasing) Unicode mapping UNLESS that mapping
// contains an ASCII scalar, and then it is left unchanged — so "adm\u{131}n"
// never upper-cases to "ADMIN" nor "\u{212A}ey" lower-cases to "key". The
// scalars this holds back (Unicode as shipped with the pinned rustc): upper
// U+00DF U+0131 U+0149 U+017F U+01F0 U+1E96..U+1E9A U+FB00..U+FB06; lower
// U+0130 U+212A. The wasm tables (scripts/gen-case-tables.py) apply the same
// filter. Not for identity comparison either way.
fn almide_case_push(out: &mut String, c: char, mapped: impl Iterator<Item = char> + Clone) {
    if !c.is_ascii() && mapped.clone().any(|m| m.is_ascii()) { out.push(c) } else { out.extend(mapped) }
}
pub fn almide_rt_string_to_upper(s: &str) -> AlmideString {
    if s.is_ascii() { return s.to_ascii_uppercase().into(); }
    let mut out = String::with_capacity(s.len());
    for c in s.chars() { almide_case_push(&mut out, c, c.to_uppercase()); }
    out.into()
}
// str::to_lowercase carries Final_Sigma (Σ → ς by its context in the WHOLE
// source string); every other scalar lowers alone. Walk that result in step
// with the per-scalar lengths so each source scalar's span is known, and hold
// back a span that would fold a non-ASCII scalar into ASCII.
pub fn almide_rt_string_to_lower(s: &str) -> AlmideString {
    if s.is_ascii() { return s.to_ascii_lowercase().into(); }
    let full = s.to_lowercase();
    let mut lowered = full.chars();
    let mut out = String::with_capacity(full.len());
    for c in s.chars() {
        let span: Vec<char> = lowered.by_ref().take(c.to_lowercase().count()).collect();
        almide_case_push(&mut out, c, span.into_iter());
    }
    out.into()
}
pub fn almide_rt_string_trim(s: &str) -> AlmideString { let __almide_out: String = s.trim().to_string(); __almide_out.into() }
pub fn almide_rt_string_trim_start(s: &str) -> AlmideString { let __almide_out: String = s.trim_start().to_string(); __almide_out.into() }
pub fn almide_rt_string_trim_end(s: &str) -> AlmideString { let __almide_out: String = s.trim_end().to_string(); __almide_out.into() }
pub fn almide_rt_string_contains(s: &str, sub: &str) -> bool { s.contains(sub) }
/// Constant-time equality over the UTF-8 bytes (#4106): same algorithm as
/// `almide_rt_bytes_is_equal_constant_time` (kept self-contained so this
/// module links without `bytes`). Length mismatch is false at once.
#[inline(never)]
pub fn almide_rt_string_is_equal_constant_time(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() { return false; }
    let mut acc: u8 = 0;
    for i in 0..a.len() {
        acc |= core::hint::black_box(a[i] ^ b[i]);
    }
    core::hint::black_box(acc) == 0
}
pub fn almide_rt_string_starts_with(s: &str, prefix: &str) -> bool { s.starts_with(prefix) }
pub fn almide_rt_string_ends_with(s: &str, suffix: &str) -> bool { s.ends_with(suffix) }
// A one-byte ASCII separator is searched as a `char` (a memchr scan) rather
// than as a `&str`, whose `StrSearcher` set-up is paid on every call and
// outweighs the scan of a short line — measured 2026-10-09: `split_once` over
// a 2M-line `station;12.3` file, 0.163 s → 0.145 s. Both find the same
// matches: an ASCII byte is a whole UTF-8 character.
fn almide_string_ascii_sep(sep: &str) -> Option<char> {
    match sep.as_bytes() {
        [c] if c.is_ascii() => Some(*c as char),
        _ => None,
    }
}
// Each piece is built as the Almide String directly: collecting `String`s and
// converting the Vec would allocate, copy and free every piece twice under
// the shared buffer.
pub fn almide_rt_string_split(s: &str, sep: &str) -> Vec<AlmideString> {
    match almide_string_ascii_sep(sep) {
        Some(c) => s.split(c).map(AlmideString::from).collect(),
        None => s.split(sep).map(AlmideString::from).collect(),
    }
}
// Two allocations and no Vec — the hot-loop form when only the first
// separator matters (`station;temp` parsing). None ⇔ sep absent.
pub fn almide_rt_string_split_once(s: &str, sep: &str) -> Option<(AlmideString, AlmideString)> { let __almide_v: Option<(String, String)> = { almide_rt_string_split_once_ref(s, sep).map(|(a, b)| (a.to_string(), b.to_string())) }; __almide_v.into_almide() }
// The borrowing twin (codegen-only, `SliceBindersPass`): a `match` whose
// payload binders are only read takes the halves as slices of `s` — the same
// bytes, no copy. Not an Almide-visible function.
pub fn almide_rt_string_split_once_ref<'a>(s: &'a str, sep: &str) -> Option<(&'a str, &'a str)> {
    match almide_string_ascii_sep(sep) {
        Some(c) => s.split_once(c),
        None => s.split_once(sep),
    }
}
pub fn almide_rt_string_replace(s: &str, from: &str, to: &str) -> AlmideString { let __almide_out: String = s.replace(from, to); __almide_out.into() }
pub fn almide_rt_string_join(parts: &[AlmideString], sep: &str) -> AlmideString { almide_str_join(parts, sep) }
// Negative counts clamp to 0 (C-054 discipline; `n as usize` on a negative
// i64 reinterpreted to a huge count — a native "capacity overflow" panic
// while the wasm leg trapped on the negative alloc size).
/// Largest byte length a `repeat` may produce: 2^31, comfortably inside wasm32's
/// address space so BOTH targets accept exactly the same counts. Above it, both
/// abort in the T6 form instead of diverging into a native allocation failure
/// (exit 1) vs a wasm out-of-bounds trap (exit 134) — the C-161 rule, applied to
/// the repeat family.
pub const ALMIDE_REPEAT_MAX_BYTES: i64 = 1 << 31;

pub fn almide_rt_string_repeat(s: &str, n: i64) -> AlmideString {
    let __almide_out: String = {
        let n = n.max(0);
        if (s.len() as i64).saturating_mul(n) > ALMIDE_REPEAT_MAX_BYTES {
            almide_abort("repeat result too large");
        }
        s.repeat(n as usize)
    };
    __almide_out.into()
}
pub fn almide_rt_string_reverse(s: &str) -> AlmideString { let __almide_out: String = s.chars().rev().collect(); __almide_out.into() }
pub fn almide_rt_string_chars(s: &str) -> Vec<AlmideString> { let __almide_v: Vec<String> = { s.chars().map(|c| c.to_string()).collect() }; __almide_v.into_almide() }
pub fn almide_rt_string_char_at(s: &str, i: i64) -> Option<AlmideString> {
    let __almide_v: Option<String> = {
        if i < 0 { return None; }
        s.chars().nth(i as usize).map(|c| c.to_string())
    };
    __almide_v.into_almide()
}
pub fn almide_rt_string_char_count(s: &str) -> i64 { s.chars().count() as i64 }
pub fn almide_rt_string_index_of(s: &str, sub: &str) -> Option<i64> { s.find(sub).map(|b| s[..b].chars().count() as i64) }
pub fn almide_rt_string_last_index_of(s: &str, sub: &str) -> Option<i64> { s.rfind(sub).map(|b| s[..b].chars().count() as i64) }
pub fn almide_rt_string_count(s: &str, sub: &str) -> i64 { s.matches(sub).count() as i64 }
pub fn almide_rt_string_lines(s: &str) -> Vec<AlmideString> { let __almide_v: Vec<String> = { s.lines().map(|l| l.to_string()).collect() }; __almide_v.into_almide() }
pub fn almide_rt_string_is_empty(s: &str) -> bool { s.is_empty() }
pub fn almide_rt_string_is_whitespace(s: &str) -> bool { s.chars().all(|c| c.is_whitespace()) }
pub fn almide_rt_string_is_alpha(s: &str) -> bool { !s.is_empty() && s.chars().all(|c| c.is_alphabetic()) }
pub fn almide_rt_string_is_digit(s: &str) -> bool { !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()) }
pub fn almide_rt_string_is_alphanumeric(s: &str) -> bool { !s.is_empty() && s.chars().all(|c| c.is_alphanumeric()) }
pub fn almide_rt_string_is_upper(s: &str) -> bool { !s.is_empty() && s.chars().any(|c| c.is_alphabetic()) && s.chars().all(|c| !c.is_alphabetic() || c.is_uppercase()) }
pub fn almide_rt_string_is_lower(s: &str) -> bool { !s.is_empty() && s.chars().any(|c| c.is_alphabetic()) && s.chars().all(|c| !c.is_alphabetic() || c.is_lowercase()) }
// The first scalar upper-cases under the to_upper rule (C-412): never into ASCII from non-ASCII.
pub fn almide_rt_string_capitalize(s: &str) -> AlmideString {
    let mut c = s.chars();
    let mut out = String::with_capacity(s.len());
    if let Some(f) = c.next() { almide_case_push(&mut out, f, f.to_uppercase()); out.push_str(c.as_str()); }
    out.into()
}
pub fn almide_rt_string_to_int(s: &str) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { s.trim().parse::<i64>().map_err(|e| e.to_string()) })(); __almide_r.map_err(Into::into) }
pub fn almide_rt_string_to_float(s: &str) -> Result<f64, AlmideString> { let __almide_r: Result<f64, String> = (|| { s.trim().parse::<f64>().map_err(|e| e.to_string()) })(); __almide_r.map_err(Into::into) }
pub fn almide_rt_string_to_bytes(s: &str) -> Vec<i64> { s.bytes().map(|b| b as i64).collect() }
// UTF-8 lossy decode — the inverse of `to_bytes` (which emits UTF-8), matching
// Almide's `bytes.to_string_lossy` and every other language's bytes→string. (Was
// a Latin-1 `b as u8 as char` map, which broke the round-trip on all non-ASCII.)
// Each i64 is truncated to a byte (`b as u8`, preserving the prior wrap), then the
// byte sequence is decoded as UTF-8 with U+FFFD for each maximal invalid subpart.
pub fn almide_rt_string_from_bytes(bytes: &[i64]) -> AlmideString {
    let __almide_out: String = {
        let v: Vec<u8> = bytes.iter().map(|&b| b as u8).collect();
        String::from_utf8_lossy(&v).into_owned()
    };
    __almide_out.into()
}
pub fn almide_rt_string_codepoint(s: &str) -> Option<i64> { s.chars().next().map(|c| c as i64) }
// `try_from` BEFORE `from_u32`: the old `cp as u32` truncated, so 2^32 — which is not a
// codepoint — became 0, which is, and this leg answered a one-byte NUL string where the
// self-host answered "". The `unwrap_or_default` was already the right intent; the cast
// in front of it was silently converting invalid input into valid input.
pub fn almide_rt_string_from_codepoint(cp: i64) -> AlmideString { let __almide_out: String = u32::try_from(cp).ok().and_then(char::from_u32).map(|c| c.to_string()).unwrap_or_default(); __almide_out.into() }

pub fn almide_rt_string_slice(s: &str, start: i64, end: i64) -> AlmideString {
    let __almide_out: String = {
        // CODEPOINT indices, clamped to [0, char_count]; the `end = i64::MAX`
        // default degrades to "to the end".
        let s_idx = start.max(0) as usize;
        let e_idx = end.max(0) as usize;
        if s_idx >= e_idx { String::new() }
        else { s.chars().skip(s_idx).take(e_idx - s_idx).collect() }
    };
    __almide_out.into()
}

// str::get checks UTF-8 boundaries without scanning or copying the source.
pub fn almide_rt_string_byte_slice(s: &str, start: i64, end: i64) -> Option<AlmideString> {
    let __almide_v: Option<String> = {
        let start = usize::try_from(start).ok()?;
        let end = usize::try_from(end).ok()?;
        s.get(start..end).map(str::to_owned)
    };
    __almide_v.into_almide()
}

// A NEGATIVE width means "no padding needed" — the SAME answer the wasm self-host
// gives (`if scount >= width then copy`), and the C-034 signed-clamp rule the
// `repeat` family follows. The old `width as usize` turned -128 into ~1.8e19, so
// `w - len` was astronomical and native panicked with a raw-vec capacity overflow
// (exit 101) where wasm returned the string unchanged (differential-fuzz).
pub fn almide_rt_string_pad_left(s: &str, width: i64, pad: &str) -> AlmideString {
    let __almide_out: String = {
        let w = width.max(0) as usize; let len = s.chars().count();
        if len >= w { return (s.to_string()).into(); }
        let p = pad.chars().next().unwrap_or(' ');
        // No chosen ceiling (ratified A, 2026-08-17): the pad byte count is computed
        // with CHECKED arithmetic — the product is what overflows — and a size this
        // machine cannot satisfy is the C-197 abort via try_reserve, never a raw
        // panic. The wasm leg aborts identically at its own i32 floor bound.
        let need = (w - len)
            .checked_mul(p.len_utf8())
            .and_then(|x| x.checked_add(s.len()));
        let mut out = String::new();
        if !need.is_some_and(|n| out.try_reserve_exact(n).is_ok()) {
            almide_abort("out of memory");
        }
        for _ in 0..(w - len) { out.push(p); }
        out.push_str(s);
        out
    };
    __almide_out.into()
}

pub fn almide_rt_string_pad_right(s: &str, width: i64, pad: &str) -> AlmideString {
    let __almide_out: String = {
        let w = width.max(0) as usize; let len = s.chars().count();
        if len >= w { return (s.to_string()).into(); }
        let p = pad.chars().next().unwrap_or(' ');
        // No chosen ceiling (ratified A, 2026-08-17): the pad byte count is computed
        // with CHECKED arithmetic — the product is what overflows — and a size this
        // machine cannot satisfy is the C-197 abort via try_reserve, never a raw
        // panic. The wasm leg aborts identically at its own i32 floor bound.
        let need = (w - len)
            .checked_mul(p.len_utf8())
            .and_then(|x| x.checked_add(s.len()));
        let mut out = String::new();
        if !need.is_some_and(|n| out.try_reserve_exact(n).is_ok()) {
            almide_abort("out of memory");
        }
        out.push_str(s);
        for _ in 0..(w - len) { out.push(p); }
        out
    };
    __almide_out.into()
}

pub fn almide_rt_string_replace_first(s: &str, from: &str, to: &str) -> AlmideString { let __almide_out: String = if let Some(pos) = s.find(from) { format!("{}{}{}", &s[..pos], to, &s[pos + from.len()..]) } else { s.to_string() }; __almide_out.into() }

pub fn almide_rt_string_strip_prefix(s: &str, prefix: &str) -> Option<AlmideString> { let __almide_v: Option<String> = { s.strip_prefix(prefix).map(|r| r.to_string()) }; __almide_v.into_almide() }
pub fn almide_rt_string_strip_suffix(s: &str, suffix: &str) -> Option<AlmideString> { let __almide_v: Option<String> = { s.strip_suffix(suffix).map(|r| r.to_string()) }; __almide_v.into_almide() }
// Borrowing twins of the two above (codegen-only, `SliceBindersPass`).
pub fn almide_rt_string_strip_prefix_ref<'a>(s: &'a str, prefix: &str) -> Option<&'a str> { s.strip_prefix(prefix) }
pub fn almide_rt_string_strip_suffix_ref<'a>(s: &'a str, suffix: &str) -> Option<&'a str> { s.strip_suffix(suffix) }


pub fn almide_rt_string_first(s: &str) -> Option<AlmideString> { let __almide_v: Option<String> = { s.chars().next().map(|c| c.to_string()) }; __almide_v.into_almide() }
pub fn almide_rt_string_last(s: &str) -> Option<AlmideString> { let __almide_v: Option<String> = { s.chars().last().map(|c| c.to_string()) }; __almide_v.into_almide() }
pub fn almide_rt_string_take(s: &str, n: i64) -> AlmideString { let __almide_out: String = s.chars().take(n as usize).collect(); __almide_out.into() }
/// Byte offset where the last `n` codepoints start (`0` when the string has
/// no more than `n`; a negative `n` is a huge count, as `n as usize` makes
/// it). Walks back from the end, so finding it costs O(n), not O(len).
fn almide_string_tail_start(s: &str, n: i64) -> usize {
    let k = n as usize;
    if k == 0 {
        return s.len();
    }
    s.char_indices().rev().nth(k - 1).map_or(0, |(i, _)| i)
}
pub fn almide_rt_string_take_end(s: &str, n: i64) -> AlmideString { let __almide_out: String = s[almide_string_tail_start(s, n)..].to_string(); __almide_out.into() }
pub fn almide_rt_string_drop(s: &str, n: i64) -> AlmideString { let __almide_out: String = s.chars().skip(n as usize).collect(); __almide_out.into() }
pub fn almide_rt_string_drop_end(s: &str, n: i64) -> AlmideString { let __almide_out: String = s[..almide_string_tail_start(s, n)].to_string(); __almide_out.into() }

// ── Algorithmic primitives (Phase 3 stdlib expansion) ──

pub fn almide_rt_string_run_length_encode(s: &str) -> Vec<(AlmideString, i64)> {
    let __almide_v: Vec<(String, i64)> = {
        let mut result: Vec<(String, i64)> = Vec::new();
        for c in s.chars() {
            let cs: String = c.to_string();
            match result.last_mut() {
                Some((prev, count)) if *prev == cs => { *count += 1; }
                _ => { result.push((cs, 1)); }
            }
        }
        result
    };
    __almide_v.into_almide()
}

// ── Mutable string operations ──

pub fn almide_rt_string_push(s: &mut AlmideString, suffix: &str) { s.push_str(suffix); }
pub fn almide_rt_string_push_char(s: &mut AlmideString, ch: &str) { s.push_str(ch); }
pub fn almide_rt_string_clear(s: &mut AlmideString) { s.clear(); }
