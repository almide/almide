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
