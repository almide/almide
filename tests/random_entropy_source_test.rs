// WHERE random.* GETS ITS ENTROPY (#4122)
// =======================================
//
// Both legs once drew from the wall clock. The embedded wasm host answered
// `prim.random_get` (fs op 32) from a generator seeded with the clock and
// reseeded on EVERY call, so all draws inside one clock tick were the same
// value; native seeded each thread's generator from the clock, so `fan` arms
// that started in the same tick shared one stream. Now op 32 reads the OS
// CSPRNG (the `getrandom` crate), and native seeds each thread from the keys
// std's `RandomState` draws from the OS per thread.
//
// spec/embedded_cross/random_entropy_streams.almd shows the observable side
// on both legs. These tests pin the sources themselves, plus the one native
// property a program cannot pin reliably: threads started at the same instant
// draw different streams (a fan's arms may run on fewer threads than arms).

// The shipped runtime source, included as text (see
// tests/runtime_random_float_construction_test.rs for why this file can be).
#[allow(dead_code, clippy::all, clippy::pedantic)]
mod runtime_random {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/runtime/rs/src/random.rs"
    ));
    // random.bytes reads the OS source through sys.rs's `fill_entropy`
    // (#4007: the runtime's foreign calls live in that one module).
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/runtime/rs/src/sys.rs"));

    // The emitted prelude's abort, which random.bytes calls on a negative n.
    fn almide_abort(msg: impl std::fmt::Display) -> ! {
        panic!("{msg}")
    }

    // ... and its out-of-memory abort, which random.bytes calls when the
    // buffer cannot be reserved.
    fn almide_abort_oom() -> ! {
        panic!("out of memory")
    }
}

fn source(rel: &str) -> String {
    std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)).expect(rel)
}

/// The code of `text` with `//` comments dropped, so a comment naming the old
/// source cannot fail the check and a code line cannot hide behind one.
fn code(text: &str) -> String {
    text.lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The body of `fn name` in `text`: from its signature to the first line that
/// closes it at column 0.
fn fn_body<'a>(text: &'a str, name: &str) -> &'a str {
    let start = text
        .find(&format!("fn {name}("))
        .unwrap_or_else(|| panic!("fn {name} not found"));
    let len = text[start..]
        .find("\n}\n")
        .unwrap_or_else(|| panic!("fn {name} has no end"));
    &text[start..start + len]
}

#[test]
fn the_embedded_hosts_op_32_reads_the_os_csprng() {
    let host = code(&source("crates/almide-wasm-run/src/host_fs.rs"));
    assert!(
        host.contains("32 => fs_entropy("),
        "op 32 is no longer served by fs_entropy"
    );
    let body = fn_body(&host, "fs_entropy");
    assert!(
        body.contains("getrandom::fill("),
        "fs_entropy does not read the OS CSPRNG:\n{body}"
    );
    assert!(
        !body.contains("SystemTime") && !body.contains("Instant"),
        "fs_entropy reads a clock:\n{body}"
    );
}

#[test]
fn the_native_generator_is_seeded_from_os_entropy() {
    let rt = code(&source("runtime/rs/src/random.rs"));
    assert!(
        !rt.contains("SystemTime") && !rt.contains("Instant"),
        "runtime/rs/src/random.rs reads a clock"
    );
    assert!(
        rt.contains("RandomState::new()"),
        "the native seed no longer comes from RandomState's OS keys"
    );
}

/// Every body of `fn name` in `text` (one per `#[cfg]` variant).
fn fn_bodies<'a>(text: &'a str, name: &str) -> Vec<&'a str> {
    let sig = format!("fn {name}(");
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(rel) = text[from..].find(&sig) {
        let start = from + rel;
        let len = text[start..]
            .find("\n}\n")
            .unwrap_or_else(|| panic!("fn {name} has no end"));
        out.push(&text[start..start + len]);
        from = start + len;
    }
    out
}

/// Every body of `fn name` in `text` whose `fn` sits at one indent level
/// (inside sys.rs's `mod almide_rt_sys_ffi`): from the signature to the
/// first line that closes it at that indent.
fn fn_bodies_in_mod<'a>(text: &'a str, name: &str) -> Vec<&'a str> {
    let sig = format!("fn {name}(");
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(rel) = text[from..].find(&sig) {
        let start = from + rel;
        let len = text[start..]
            .find("\n    }\n")
            .unwrap_or_else(|| panic!("fn {name} has no end"));
        out.push(&text[start..start + len]);
        from = start + len;
    }
    out
}

// Row 12 of the security table: `random.bytes` is the one CSPRNG, and its
// behavioural tests (lengths, two draws differ) pass a seeded xorshift too.
// These pin the source on every leg instead. Negative control: make
// `almide_rt_random_bytes` fill from `next_u64()` (or `almide_os_entropy`
// return a xorshift draw, or a `random_get` in random_bytes.almd read the
// clock) and the matching test fails.

/// Native: `random.bytes` fills only through `almide_os_entropy`, which hands
/// the chunk to sys.rs's `fill_entropy` on every unix and Windows (#4007: the
/// runtime's foreign calls live in that one module) and refuses (the abort,
/// never a fallback) anywhere else; `fill_entropy` is getentropy(3) on every
/// unix and BCryptGenRandom on Windows.
#[test]
fn native_random_bytes_reads_only_the_os_csprng() {
    let rt = code(&source("runtime/rs/src/random.rs"));
    let body = fn_body(&rt, "almide_rt_random_bytes");
    assert!(
        body.contains("almide_os_entropy("),
        "random.bytes no longer fills from almide_os_entropy:\n{body}"
    );
    for other in ["next_u64", "RNG_STATE", "almide_rng_seed", "RandomState"] {
        assert!(
            !body.contains(other),
            "random.bytes draws from `{other}`:\n{body}"
        );
    }
    let sources = fn_bodies(&rt, "almide_os_entropy");
    assert_eq!(
        sources.len(),
        2,
        "almide_os_entropy: one body handing off to sys.rs (unix, windows), one refusal for the rest"
    );
    for s in &sources {
        for other in ["next_u64", "RNG_STATE", "RandomState", "SystemTime", "Instant"] {
            assert!(!s.contains(other), "almide_os_entropy reads `{other}`:\n{s}");
        }
    }
    assert!(
        sources.iter().any(|s| s.contains("almide_rt_sys_ffi::fill_entropy(")),
        "no almide_os_entropy hands the chunk to sys.rs's fill_entropy"
    );
    assert!(
        sources
            .iter()
            .filter(|s| !s.contains("fill_entropy("))
            .all(|s| s.trim_end().ends_with("false")),
        "an almide_os_entropy with no OS source must refuse (false), not fall back"
    );
    let sys = code(&source("runtime/rs/src/sys.rs"));
    let fills = fn_bodies_in_mod(&sys, "fill_entropy");
    assert_eq!(fills.len(), 2, "fill_entropy: one body for unix, one for Windows");
    for s in &fills {
        for other in ["next_u64", "RNG_STATE", "RandomState", "SystemTime", "Instant"] {
            assert!(!s.contains(other), "fill_entropy reads `{other}`:\n{s}");
        }
    }
    assert!(
        fills.iter().any(|s| s.contains("getentropy(")),
        "no fill_entropy reads getentropy(3)"
    );
    assert!(
        fills.iter().any(|s| s.contains("BCryptGenRandom(")),
        "no fill_entropy reads BCryptGenRandom (Windows)"
    );
}

/// Wasm: the self-hosted `random_bytes` draws only through
/// `prim.random_get`, which lowers to fs op 32; the stock artifact serves
/// op 32 from WASI `random_get` (preview 1) or `wasi:random` (preview 2 / 3),
/// and the embedded host from `fs_entropy` (the test above).
#[test]
fn wasm_random_bytes_draws_only_through_op_32() {
    let almd = code(&source("stdlib/random_bytes.almd"));
    assert!(
        almd.matches("prim.random_get(").count() >= 3,
        "random_bytes.almd no longer draws its 256-byte, 8-byte and tail fills from prim.random_get:\n{almd}"
    );
    for other in ["random_int", "random.int", "random_float", "clock", "seed"] {
        assert!(!almd.contains(other), "random_bytes.almd reads `{other}`:\n{almd}");
    }
    let prim = code(&source("crates/almide-wasm/src/prim.rs"));
    let arm = &prim[prim.find("(\"random_get\", [p, n]) =>").expect("the random_get arm")..];
    let arm = &arm[..arm.find("\n            (\"").unwrap_or(arm.len())];
    assert!(
        arm.contains("OP_RANDOM_GET"),
        "prim.random_get no longer lowers to the entropy op:\n{arm}"
    );
    assert!(
        code(&source("crates/almide-wasm/src/fs_meta.rs")).contains("OP_RANDOM_GET: i32 = 32;"),
        "OP_RANDOM_GET is no longer op 32"
    );
    // The stock preview-1 shim: op 32 calls import 2, and import 2 is
    // wasi_snapshot_preview1 `random_get`.
    let shims = code(&source("crates/almide-wasi/src/wasi_shims.rs"));
    let p1 = &shims[shims.find("if has(32) {").expect("the stock op-32 arm")..];
    let p1 = &p1[..p1.find("\n    }\n").expect("the arm's end")];
    assert!(p1.contains(".call(2)"), "the stock op-32 arm no longer calls import 2:\n{p1}");
    let imports: Vec<&str> = source("crates/almide-wasi/src/lib.rs")
        .lines()
        .filter(|l| l.trim_start().starts_with("imports.import(W, \""))
        .map(|l| if l.contains("\"random_get\"") { "random_get" } else { "other" })
        .collect();
    assert_eq!(
        imports.get(2),
        Some(&"random_get"),
        "stock import 2 is no longer wasi random_get: {imports:?}"
    );
    // The preview-2 and preview-3 adapters: op 32 calls wasi:random.
    for rel in [
        "crates/almide-wasm-run/src/wasi_p2.rs",
        "crates/almide-wasm-run/src/wasi_p3_fs.rs",
    ] {
        let t = code(&source(rel));
        let arm = &t[t.find("i32_const(32).i32_eq()").unwrap_or_else(|| panic!("{rel}: no op-32 arm"))..];
        let arm = &arm[..arm.find(".end();").expect("the arm's end")];
        assert!(arm.contains("call(I_RANDOM)"), "{rel}: op 32 no longer calls wasi:random:\n{arm}");
    }
}

/// 32 threads released together by a barrier each take their first draw:
/// every thread's stream starts somewhere else. Under the clock seed, threads
/// released in the same clock tick drew the same first value.
#[test]
fn threads_started_together_draw_different_streams() {
    let n = 32;
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(n));
    let firsts: Vec<i64> = (0..n)
        .map(|_| {
            let b = barrier.clone();
            std::thread::spawn(move || {
                b.wait();
                // The full i64 range answers the raw 64-bit draw.
                runtime_random::almide_rt_random_int(i64::MIN, i64::MAX)
            })
        })
        .collect::<Vec<_>>()
        .into_iter()
        .map(|h| h.join().expect("thread"))
        .collect();
    let distinct: std::collections::BTreeSet<i64> = firsts.iter().copied().collect();
    assert_eq!(
        distinct.len(),
        n,
        "threads shared a stream: {} distinct first draws of {n}",
        distinct.len()
    );
}
