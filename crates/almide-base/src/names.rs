// Symbol spelling shared by the Rust emitter and the wasm test-runner synthesis.
//
// `rust_safe_fn_name` lives here — not in almide-codegen where it is emitted —
// because the two test legs must agree on ONE spelling. The native leg compiles
// `test "…"` to a Rust fn whose name is this function's output and hands
// `--run <pattern>` to the Rust test harness, which substring-matches THAT name;
// the wasm leg synthesizes its runner in almide-mir and must select exactly the
// same tests. When the two spellings drifted apart the filter was simply dropped
// on the wasm side and nothing noticed (#2085), so the rule now has one home and
// `test_name_matches_filter` is the only admitted way to ask the question.

/// Sanitize an IR function name into a Rust-emittable identifier.
///
/// Spaces and punctuation fold to `_`; parentheses vanish; operator characters
/// spell out. A name containing any non-ASCII character also gets an FNV-1a
/// suffix, because the fold above maps every non-ASCII byte to `_` and two
/// distinct names would otherwise collide on one skeleton.
pub fn rust_safe_fn_name(raw: &str) -> String {
    let s = raw
        .replace([' ', '-', '.', ',', ':', '[', ']'], "_")
        .replace(['(', ')'], "")
        .replace('+', "_plus_").replace('/', "_div_").replace('*', "_mul_")
        .replace('=', "_eq_").replace('!', "_bang_").replace('?', "_q_")
        .replace('<', "_lt_").replace('>', "_gt_")
        .replace('|', "_pipe_").replace('&', "_amp_").replace('%', "_mod_");
    let mut safe: String = s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
    if raw.chars().any(|c| !c.is_ascii()) {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in raw.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        safe.push_str(&format!("_{:08x}", (h >> 32) as u32 ^ h as u32));
    }
    safe
}

/// Does the test whose IR name is `ir_name` run under `--run <pattern>`?
///
/// The native leg's answer is the Rust test harness's: a case-sensitive
/// substring test against the EMITTED fn name, so the pattern is compared
/// against `rust_safe_fn_name(ir_name)` and not against the `test "…"` label.
/// A pattern spelled with the label's spaces therefore selects nothing on both
/// legs — surprising, but identically surprising, which is the property the
/// wasm leg has to reproduce.
pub fn test_name_matches_filter(ir_name: &str, pattern: &str) -> bool {
    rust_safe_fn_name(ir_name).contains(pattern)
}

#[cfg(test)]
mod tests {
    use super::{rust_safe_fn_name, test_name_matches_filter};

    #[test]
    fn same_skeleton_nonascii_names_stay_distinct() {
        let a = rust_safe_fn_name("__test_almd_a: 値を取る旗に値が無ければ断る");
        let b = rust_safe_fn_name("__test_almd_a: 旗の値は位置引数に混ざらない");
        assert_ne!(a, b);
    }

    #[test]
    fn ascii_names_are_untouched_by_the_hash_rule() {
        assert_eq!(rust_safe_fn_name("__test_almd_a + b (fast)"), "__test_almd_a__plus__b_fast");
        assert_eq!(rust_safe_fn_name("string mismatch"), "string_mismatch");
    }

    // The filter contract both legs implement. `beta fails` selecting nothing is
    // not a bug to fix here: it is what the native harness already does, and
    // #2085 is about the two legs agreeing, not about redefining the pattern.
    #[test]
    fn the_pattern_is_matched_against_the_emitted_name() {
        let n = "__test_almd_beta fails";
        assert!(test_name_matches_filter(n, "beta"));
        assert!(test_name_matches_filter(n, "beta_fails"));
        assert!(test_name_matches_filter(n, "__test_almd_"));
        assert!(!test_name_matches_filter(n, "beta fails"));
        assert!(!test_name_matches_filter(n, "BETA"));
    }
}

/// The identifier a module PATH contributes to every generated item name —
/// a module function `almide_rt_<ident>_<fn>`, a module type
/// `almide_rt_<ident>_<Type>`, a module global's static, and the
/// `module_origin` the IR carries (#3338).
///
/// INJECTIVE, and so is `<ident>_<name>` for any identifier `name`: a `_`
/// inside a segment is `_0`, a segment boundary (`.`) is `_1`, and the `_`
/// before `name` is a bare `_` — never followed by a digit, because neither a
/// segment nor a name starts with one. Reading left to right, `_0` / `_1` /
/// `_` decode without lookahead past one character, so `a.b` (`a_1b`), `a_b`
/// (`a_0b`) and module `a` + name `b_c` (`a_b_c`) are three spellings. The
/// historic `.` → `_` fold made all three `a_b`, which collided on both legs.
///
/// The identity on a single segment without `_` — every stdlib module, and so
/// every `almide_rt_<module>_<fn>` runtime symbol, keeps its spelling.
pub fn module_ident(path: &str) -> String {
    let mut out = String::with_capacity(path.len() + 4);
    for (i, seg) in path.split('.').enumerate() {
        if i > 0 {
            out.push_str("_1");
        }
        for ch in seg.chars() {
            if ch == '_' {
                out.push_str("_0");
            } else {
                out.push(ch);
            }
        }
    }
    out
}

/// A module-qualified declaration name (`a.b.Tok`) as one identifier:
/// [`module_ident`] of the module path, `_`, the bare name (`a_1b_Tok`). A
/// name with no module part is returned as is.
pub fn qualified_ident(qualified: &str) -> String {
    match qualified.rsplit_once('.') {
        Some((module, name)) => format!("{}_{}", module_ident(module), name),
        None => qualified.to_string(),
    }
}

/// The rest of a dotted IR name after the module path whose
/// [`module_ident`] is `origin` (`varlib.Pigment.encode` under origin
/// `varlib` → `Pigment.encode`), or `None` when the name does not start with
/// that module. A qualified-method fn's definition and every call site strip
/// the module this one way before prefixing `almide_rt_<origin>_`, so the
/// module is never spelled twice (#433 × #411-B).
pub fn strip_module_path<'a>(name: &'a str, origin: &str) -> Option<&'a str> {
    name.match_indices('.')
        .map(|(i, _)| i)
        .find(|&i| module_ident(&name[..i]) == origin)
        .map(|i| &name[i + 1..])
}

#[cfg(test)]
mod module_ident_tests {
    use super::{module_ident, qualified_ident};

    #[test]
    fn stdlib_and_plain_modules_keep_their_spelling() {
        for m in ["list", "string", "int8", "m", "parser"] {
            assert_eq!(module_ident(m), m);
        }
    }

    #[test]
    fn dotted_underscored_and_name_boundaries_never_collide() {
        let spellings = [
            format!("{}_{}", module_ident("a.b"), "c"),
            format!("{}_{}", module_ident("a_b"), "c"),
            format!("{}_{}", module_ident("a"), "b_c"),
            format!("{}_{}", module_ident("a._b"), "c"),
            format!("{}_{}", module_ident("a_.b"), "c"),
            format!("{}_{}", module_ident("a"), "_b_c"),
            qualified_ident("a.b.c"),
        ];
        let mut seen = std::collections::HashSet::new();
        for s in &spellings[..6] {
            assert!(seen.insert(s.clone()), "collision on {s}");
        }
        assert_eq!(spellings[6], spellings[0]);
        assert_eq!(qualified_ident("Tok"), "Tok");
    }

    #[test]
    fn strip_module_path_matches_the_encoded_origin() {
        use super::strip_module_path;
        assert_eq!(strip_module_path("varlib.Pigment.encode", "varlib"), Some("Pigment.encode"));
        assert_eq!(strip_module_path("a.b.T.m", "a_1b"), Some("T.m"));
        assert_eq!(strip_module_path("a_b.T.m", "a_0b"), Some("T.m"));
        assert_eq!(strip_module_path("a.b.T.m", "a_0b"), None);
        assert_eq!(strip_module_path("Pigment.encode", "varlib"), None);
    }
}
