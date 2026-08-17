//! The head-count domain gate for the `matrix` attention/rope family (C-198).
//!
//! Wave 5 R2 found `matrix.masked_multi_head_attention(q, k, v, i32::MIN)` hanging
//! natively while wasm exited 0. The root cause was not one missing check: the family
//! had THREE answers to the same question, all point-wise —
//!
//!   * `almide_rt_matrix_rope_rotate_at`  — `n_heads.max(0) as usize` (negative → no heads)
//!   * `almide_rt_matrix_rms_norm_heads`  — `n_heads.max(1) as usize` (negative → one head)
//!   * `almide_rt_matrix_mha_core`        — no guard; `as usize` turned i32::MIN into
//!                                          18446744071562067968 and the head loop hung
//!
//! and the self-hosted wasm bodies agreed with none of them (they divided by the count,
//! so 0 trapped and a negative value silently returned a zero-filled matrix).
//!
//! The rule is now one rule: a head count below 1 is out of domain and aborts with
//! `Error: head count must be positive` + exit 1 on both targets. These tests are the
//! MATRIX GATE that keeps it total — they fail when a new family member spells its own
//! answer instead of going through the shared helper, which is the failure mode that
//! produced R2 in the first place.
//!
//! Runtime equivalence of the abort itself is checked by the cross-target harness via
//! `spec/wasm_cross/matrix_head_count_domain.almd` (the convergent side) — a fixture
//! cannot assert both an abort and a clean exit in one program, so the abort is pinned
//! here structurally and by the stdlib test.

use std::fs;

const NATIVE_MATRIX_SRCS: &[&str] = &["runtime/rs/src/matrix.rs", "runtime/rs/src/matrix_p2.rs"];
const SELF_HOSTED_SRCS: &[&str] = &["stdlib/matrix_activations.almd"];

/// The head-count parameter names the family uses.
const HEAD_COUNT_IDENTS: &[&str] = &["n_heads", "n_q_heads", "n_kv_heads"];

fn read(path: &str) -> String {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(root.join(path)).unwrap_or_else(|e| panic!("cannot read {path}: {e}"))
}

/// No native site may apply its OWN totalization to a head count. `.max(0)` and
/// `.max(1)` are exactly the two divergent conventions R2 exposed, and a bare
/// `as usize` is the unguarded one that hung.
#[test]
fn no_native_site_totalizes_a_head_count_on_its_own() {
    let mut offenders = Vec::new();
    for path in NATIVE_MATRIX_SRCS {
        for (i, line) in read(path).lines().enumerate() {
            let l = line.trim();
            if l.starts_with("//") || l.starts_with("///") {
                continue;
            }
            for ident in HEAD_COUNT_IDENTS {
                // The shared helper is the ONLY place allowed to convert.
                let converts = l.contains(&format!("{ident}.max("))
                    || l.contains(&format!("{ident} as usize"));
                if converts {
                    offenders.push(format!("{path}:{}: {l}", i + 1));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "a head count is being totalized outside `almide_rt_matrix_head_count`, which is how \
         the family grew three different answers (Wave 5 R2). Route it through the helper:\n  {}",
        offenders.join("\n  ")
    );
}

/// The helper must exist and must abort rather than clamp — a clamp would silently
/// compute with a head count the caller did not ask for, which is the class of
/// plausible-but-wrong result this language exists to avoid.
#[test]
fn the_native_helper_aborts_rather_than_clamping() {
    let src = read("runtime/rs/src/matrix.rs");
    let start = src
        .find("pub fn almide_rt_matrix_head_count")
        .expect("almide_rt_matrix_head_count is missing — the family has no shared domain rule");
    let body = &src[start..start + 400.min(src.len() - start)];
    assert!(
        body.contains("n < 1"),
        "the head-count domain must be `n < 1`, so that 0 is rejected too (0 divided by zero \
         on the self-hosted side): {body}"
    );
    assert!(
        body.contains("Error: head count must be positive") && body.contains("exit(1)"),
        "the helper must raise the unified `Error: <msg>` + exit 1 that list.chunk / \
         int.rotate_* use, so both targets can print the SAME line: {body}"
    );
}

/// The self-hosted (wasm) side must carry the same rule, spelled with the same message —
/// a different message would be a cross-target divergence in the abort itself.
#[test]
fn the_self_hosted_guard_matches_the_native_message() {
    for path in SELF_HOSTED_SRCS {
        let src = read(path);
        assert!(
            src.contains("fn __mx_head_count(n: Int) -> Int"),
            "{path} has no `__mx_head_count` — the wasm leg would fall back to dividing by \
             the head count, which traps on 0 and silently zero-fills on a negative"
        );
        assert!(
            src.contains("Error: head count must be positive"),
            "{path}'s guard must print the SAME line as the native helper, or the abort \
             itself diverges across targets"
        );
    }
}

/// Every self-hosted body that DIVIDES by a head count must divide by the guarded value,
/// never by the raw parameter. This is the exact shape that made `n_heads = 0` a wasm trap.
#[test]
fn no_self_hosted_body_divides_by_a_raw_head_count() {
    let mut offenders = Vec::new();
    for path in SELF_HOSTED_SRCS {
        for (i, line) in read(path).lines().enumerate() {
            let l = line.trim();
            if l.starts_with("//") {
                continue;
            }
            for ident in HEAD_COUNT_IDENTS {
                if l.contains(&format!("/ {ident}")) {
                    offenders.push(format!("{path}:{}: {l}", i + 1));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "a self-hosted body divides by a RAW head count — bind it through `__mx_head_count` \
         first, or a head count of 0 is a divide-by-zero trap on wasm while native returns \
         a value:\n  {}",
        offenders.join("\n  ")
    );
}

/// #1419 (C-278): the head GEOMETRY rule — the same family, one column over.
/// C-198 fixed the head COUNT domain and every fixture then used exact
/// geometry, so `n_heads * head_dim` vs `cols` stayed an unexecuted polarity:
/// exceeding it was a raw slice panic natively and an OOB memory trap on wasm
/// (fuzz seed 508666777783 index 2120). The rule lives in ONE shared helper
/// per leg, and this gate keeps every rope entry routed through it.
#[test]
fn the_native_geometry_helper_aborts_and_every_rope_entry_routes_through_it() {
    let src = read("runtime/rs/src/matrix.rs");
    let start = src
        .find("pub fn almide_rt_matrix_head_geometry")
        .expect("almide_rt_matrix_head_geometry is missing — the rope family has no shared geometry rule");
    let body = &src[start..start + 600.min(src.len() - start)];
    assert!(
        body.contains("Error: head geometry exceeds row width") && body.contains("exit(1)"),
        "the geometry helper must raise the unified `Error: <msg>` + exit 1 so both targets \
         print the SAME line: {body}"
    );
    assert!(
        body.contains("cols / head_dim_u"),
        "the geometry check must use the DIVISION form — a multiplied `n_heads * head_dim` \
         overflows on a huge head count, which is the same class of defeated guard as #1408: {body}"
    );

    // Every native rope impl (the fns that take BOTH n_heads and head_dim and
    // index `head_start + …` into the row) must call the helper.
    let p2 = read("runtime/rs/src/matrix_p2.rs");
    for entry in ["almide_rt_matrix_rope_rotate_at", "almide_rt_matrix_rope_rotate_neox_at"] {
        let s = p2
            .find(&format!("pub fn {entry}"))
            .unwrap_or_else(|| panic!("{entry} is missing from matrix_p2.rs"));
        let window = &p2[s..(s + 1200).min(p2.len())];
        assert!(
            window.contains("almide_rt_matrix_head_geometry("),
            "{entry} does not route through almide_rt_matrix_head_geometry — the geometry \
             rule grew a point-wise exception, which is how the head-count family got three \
             answers (Wave 5 R2)"
        );
    }
}

/// The self-hosted (wasm) side carries the same geometry rule with the same message.
#[test]
fn the_self_hosted_geometry_guard_matches_the_native_message() {
    for path in SELF_HOSTED_SRCS {
        let src = read(path);
        assert!(
            src.contains("fn __mx_head_geometry"),
            "{path} has no `__mx_head_geometry` — the wasm rope body would write past the \
             row block (an OOB memory trap) where native aborts"
        );
        assert!(
            src.contains("Error: head geometry exceeds row width"),
            "{path}'s geometry guard must print the SAME line as the native helper, or the \
             abort itself diverges across targets"
        );
        assert!(
            src.contains("__mx_head_geometry(__mx_head_count("),
            "{path}'s rope body must route through BOTH guards (count, then geometry) — \
             composing them is what keeps the two domains one rule each"
        );
    }
}

/// #1423 night findings (C-282): the INDEX domain — the third rule of this
/// family, and the one whose negative half was silently wrong on the wasm leg
/// (a wrapped `as usize` read outside the row block and exited 0). One shared
/// guard per leg, and every element accessor routes through it.
#[test]
fn the_native_index_guard_aborts_and_get_routes_through_it() {
    let src = read("runtime/rs/src/matrix.rs");
    let start = src
        .find("pub fn almide_rt_matrix_bounds")
        .expect("almide_rt_matrix_bounds is missing — the accessor family has no shared index rule");
    let body = &src[start..start + 400.min(src.len() - start)];
    assert!(
        body.contains("Error: matrix index out of bounds") && body.contains("exit(1)"),
        "the index guard must raise the unified `Error: <msg>` + exit 1 both targets print: {body}"
    );
    assert!(
        body.contains("idx < 0"),
        "the guard must reject a NEGATIVE index BEFORE the unsigned cast — testing only \
         `>= extent` after `as usize` is the defeated-by-overflow shape of #1408: {body}"
    );

    let s = src
        .find("pub fn almide_rt_matrix_get")
        .expect("almide_rt_matrix_get is missing");
    let get = &src[s..(s + 600).min(src.len())];
    assert_eq!(
        get.matches("almide_rt_matrix_bounds(").count(),
        2,
        "matrix.get must guard BOTH indices (row against the matrix, col against that \
         row's own width) — one call means one axis is still raw: {get}"
    );
}

/// The self-hosted (wasm) side carries the same index rule, same message.
#[test]
fn the_self_hosted_index_guard_matches_the_native_message() {
    let src = read("stdlib/matrix_core.almd");
    assert!(
        src.contains("fn __mx_bounds"),
        "stdlib/matrix_core.almd has no `__mx_bounds` — the wasm leg would raw-index, \
         which traps for a large index and reads out of the block for a negative one"
    );
    assert!(
        src.contains("Error: matrix index out of bounds"),
        "the self-hosted guard must print the SAME line as the native helper, or the abort \
         itself diverges across targets"
    );
    let s = src.find("fn matrix_get").expect("matrix_get is missing");
    let get = &src[s..(s + 500).min(src.len())];
    assert_eq!(
        get.matches("__mx_bounds(").count(),
        2,
        "the self-hosted matrix_get must guard BOTH indices, mirroring the native twin: {get}"
    );
}
