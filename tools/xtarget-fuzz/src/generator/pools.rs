//! Literal value pools, weighted toward the known native↔WASM
//! divergence surface.
//!
//! Every pool is a *named table with a comment* (no magic numbers): the
//! values are picked to stress the exact boundaries where the two
//! targets have historically disagreed — multibyte strings (case-fold
//! and byte-vs-char offsets), float formatting edge cases, and integer
//! width extremes. When the differential oracle finds a divergence, the
//! minimized repro almost always bottoms out at one of these literals.

/// Multibyte / Unicode string literals.
///
/// `to_upper`/`to_lower`/`capitalize` are ASCII-only on WASM but
/// full-Unicode natively (tracked divergence — see MEMORY), so the pool
/// is deliberately heavy on cased non-ASCII, length-changing folds
/// (`ß`→`SS`), combining sequences, and astral-plane scalars. These are
/// the inputs most likely to surface both the *known* case-fold gap and
/// any *new* string op that forgets multibyte handling.
pub const STRING_POOL: &[&str] = &[
    "",               // empty: off-by-one and is_empty boundary
    "hello",          // pure ASCII baseline
    "Hello, World!",  // ASCII with punctuation/space
    "日本語",          // CJK: 3 scalars, 9 UTF-8 bytes (byte-vs-char)
    "café",           // precomposed é (2-byte scalar)
    "cafe\u{0301}",   // e + combining acute (NFD form of "café")
    "ß",              // length-changing uppercase: ß → SS
    "ﬀ",              // ligature: uppercase → FF
    "Ⅷ",              // roman numeral eight (cased non-letter)
    "İ",              // dotted capital I (Turkish-ish fold hazard)
    "ΑΒΓ",            // Greek capitals (full-Unicode case path)
    "αβγ",            // Greek smalls
    "Привет",         // Cyrillic
    "😀",             // emoji: astral plane, 4 UTF-8 bytes
    "👨‍👩‍👧",          // ZWJ family: grapheme vs scalar vs byte
    "a\tb\nc",        // control chars (escape round-trip)
    "  spaced  ",     // leading/trailing ASCII whitespace (trim)
    "\u{00A0}nbsp\u{2003}", // non-ASCII whitespace (trim Unicode gap)
    "Ǆ",              // titlecase digraph
    "ﬁle",            // fi ligature prefix
];

/// Float literals stressing formatting and rounding.
///
/// `float.to_string` round-tripping and correctly-rounded `float.parse`
/// are historic divergence clusters (Dragon4 / dec2flt). The pool spans
/// signed zero, subnormals, the largest finite magnitude, and the
/// canonical `0.1 + 0.2` shape whose shortest-decimal printing differs
/// if either target's formatter is wrong.
pub const FLOAT_POOL: &[f64] = &[
    0.0,
    -0.0,                  // signed zero: -0.0 prints "-0" or "0"?
    1.0,
    -1.0,
    0.1,
    0.2,
    0.3,                   // 0.1 + 0.2 != 0.3 shape
    3.141_592_653_589_793, // π: many significant digits
    2.718_281_828_459_045, // e
    1e300,                 // large finite
    1e-300,                // small normal
    5e-324,                // smallest positive subnormal (f64 MIN_POSITIVE/2^52)
    1.797_693_134_862_315_7e308, // near f64::MAX
    123_456_789.123_456_79,
    0.000_001,             // scientific-notation threshold
    1_000_000.0,
    100.0,
    -42.5,
];

/// Integer literals at and around 64-bit boundaries.
///
/// Int width conversions (`to_u32`, `to_int8`, wrapping ops) and
/// overflow behaviour are codegen-sensitive. The pool pins both
/// extremes, the sign boundary, and small values used as indices/counts.
pub const INT_POOL: &[i64] = &[
    0,
    1,
    -1,
    2,
    7,
    10,
    42,
    255,                   // u8 boundary
    256,
    -128,                  // i8 min
    127,                   // i8 max
    65535,                 // u16 boundary
    2_147_483_647,         // i32 max
    -2_147_483_648,        // i32 min
    4_294_967_295,         // u32 max
    9_223_372_036_854_775_807,  // i64::MAX
    -9_223_372_036_854_775_808, // i64::MIN
    1_000_000,
];

/// Small non-negative integers for loop counts, indices, and list
/// sizes — kept tiny so generated programs terminate fast and produce
/// compact output. Separate from `INT_POOL` (which probes boundaries)
/// because using `i64::MAX` as a `repeat` count would be catastrophic.
pub const SMALL_COUNT_POOL: &[i64] = &[0, 1, 2, 3, 4, 5];

/// Boolean literals (trivial, but kept as a table for symmetry so the
/// term generator never special-cases a type).
pub const BOOL_POOL: &[bool] = &[true, false];

/// Identifier stems for generated bindings. Short, collision-resistant
/// within a single program because the generator suffixes a fresh
/// counter (`v0`, `v1`, …) — the stem only adds readability to repros.
pub const VAR_STEMS: &[&str] = &["v", "x", "y", "acc", "tmp", "n", "s"];
