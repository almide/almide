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
