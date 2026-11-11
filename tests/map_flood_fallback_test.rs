//! #4104: keys crafted to collide under the map index's fast hash must not
//! make building a native `Map` quadratic. The index stays on the fast hash
//! by default; a map whose inserts probe past `ALMIDE_MAP_FLOOD_PROBES`
//! moves to an index ordered by each key's canonical bytes. That index is
//! deterministic, uses no hidden entropy, and costs O(log n) per operation
//! whatever the keys.
//!
//! The keys are made by inverting the runtime's `almide_rt_map_mix64` (the
//! Int key hash): every target hash shares its low 20 bits, so every key
//! starts its probe at the same slot of any table up to 2^20 slots. The
//! inverse is a copy of the runtime's constants, so the test also checks that
//! those constants are still the ones in `runtime/rs/src/map.rs`. Otherwise a
//! changed hash would quietly stop colliding and the test would pass blind.
//!
//! Negative control: with the flood check disabled, the same build probes
//! about n²/2 slots (≈10^10 at n = 150 000) and runs far past the bound.

use std::process::Command;

const MUL: u64 = 0x9E37_79B9_7F4A_7C15;
const C1: u64 = 0xBF58_476D_1CE4_E5B9;
const C2: u64 = 0x94D0_49BB_1331_11EB;

fn mix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(MUL);
    z = (z ^ (z >> 30)).wrapping_mul(C1);
    z = (z ^ (z >> 27)).wrapping_mul(C2);
    z ^ (z >> 31)
}

/// The multiplicative inverse of an odd `a` modulo 2^64 (Newton's iteration).
fn inv_mul(a: u64) -> u64 {
    let mut x = a;
    for _ in 0..6 {
        x = x.wrapping_mul(2u64.wrapping_sub(a.wrapping_mul(x)));
    }
    x
}

/// Undo `y = x ^ (x >> s)`.
fn unxorshift(y: u64, s: u32) -> u64 {
    let mut x = y;
    for _ in 0..(64 / s + 1) {
        x = y ^ (x >> s);
    }
    x
}

fn unmix64(h: u64) -> u64 {
    let mut z = unxorshift(h, 31);
    z = z.wrapping_mul(inv_mul(C2));
    z = unxorshift(z, 27);
    z = z.wrapping_mul(inv_mul(C1));
    z = unxorshift(z, 30);
    z.wrapping_sub(MUL)
}

#[test]
fn the_inverted_hash_is_the_runtimes() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/runtime/rs/src/map.rs")).unwrap();
    for c in ["0x9E37_79B9_7F4A_7C15", "0xBF58_476D_1CE4_E5B9", "0x94D0_49BB_1331_11EB"] {
        assert!(src.contains(c), "almide_rt_map_mix64 no longer uses {c}: update this test's inverse");
    }
    for x in [0u64, 1, 42, u64::MAX, 0x1234_5678_9ABC_DEF0] {
        assert_eq!(unmix64(mix64(x)), x);
    }
}

/// The program every flood case runs: build a map from `keys.txt`, time the
/// build, then remove ten keys, overwrite one and look three up.
const FLOOD_PROGRAM: &str = r#"import fs
fn key_of(s: String) -> KT = KEY
fn at(ks: List[String], n: Int) -> KT = key_of(list.get(ks, n) ?? "")
effect fn main() -> Unit = {
  let text = fs.read_text("keys.txt")!
  let ks = string.split(text, "\n") |> list.filter((s) => s != "")
  let pairs = ks |> list.map((s) => (key_of(s), 1))
  let t0 = datetime.monotonic_ns()!
  let m = map.from_list(pairs)
  let t1 = datetime.monotonic_ns()!
  println(int.to_string(map.len(m)))
  println(int.to_string((t1 - t0) / 1000000))
  let gone = ks |> list.take(10) |> list.map((s) => key_of(s))
  let m2 = gone |> list.fold(m, (acc, k) => map.remove(acc, k))
  let m3 = map.set(m2, at(ks, 20), 7)
  println(int.to_string(map.len(m3)))
  println("${map.contains(m3, at(ks, 0))} ${map.contains(m3, at(ks, 10))} ${map.get(m3, at(ks, 20)) ?? 0}")
}
"#;

/// Run the flood program over `keys` (one per line). `int_keys` parses them
/// to Int; otherwise the String itself is the key. Returns stdout lines.
fn run_flood(keys: &[String], int_keys: bool, target: &str) -> Vec<String> {
    let dir = tempfile::tempdir().unwrap();
    let text: String = keys.iter().map(|k| format!("{k}\n")).collect();
    std::fs::write(dir.path().join("keys.txt"), text).unwrap();
    let (kt, key) = if int_keys { ("Int", "int.parse(s) ?? 0") } else { ("String", "s") };
    let src = FLOOD_PROGRAM.replace("KT", kt).replace("KEY", key);
    std::fs::write(dir.path().join("flood.almd"), src).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["run", "flood.almd", "--target", target])
        .current_dir(dir.path())
        .output()
        .expect("spawn almide run");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(out.status.success(), "{target}: {stdout}\n{}", String::from_utf8_lossy(&out.stderr));
    stdout.lines().map(str::to_owned).collect()
}

/// Check the four lines: every key kept, the build under `max_ms` (when
/// given), ten removed, and remove / lookup / overwrite answered exactly.
fn check_flood(lines: &[String], n: usize, max_ms: Option<u64>, what: &str) {
    assert_eq!(lines.first().map(String::as_str), Some(n.to_string().as_str()), "{what}: every distinct key is kept");
    let ms: u64 = lines.get(1).and_then(|l| l.trim().parse().ok()).expect("elapsed ms");
    if let Some(max) = max_ms {
        assert!(ms < max, "{what}: building a map of {n} colliding keys took {ms} ms: the flood fallback did not engage");
    }
    assert_eq!(lines.get(2).map(String::as_str), Some((n - 10).to_string().as_str()), "{what}: ten removed keys are gone");
    assert_eq!(lines.get(3).map(String::as_str), Some("false true 7"), "{what}: a removed key is absent, a kept one present, an overwrite visible");
}

#[test]
fn colliding_int_keys_build_a_map_in_near_linear_time() {
    const N: u64 = 150_000;
    let keys: Vec<i64> = (1..=N).map(|i| unmix64((i << 20) | 0x5A5A5) as i64).collect();
    // Every key lands on the same starting slot of a 2^k table, k <= 20.
    assert!(keys.iter().all(|&k| mix64(k as u64) & 0xF_FFFF == 0x5A5A5));
    let keys: Vec<String> = keys.iter().map(i64::to_string).collect();
    check_flood(&run_flood(&keys, true, "rust"), N as usize, Some(3_000), "native");
}

/// The wasm index hashes an Int key by the high word of `k · FIB` and
/// masks its low bits (crates/almide-wasm/src/map_index.rs). Keys whose
/// products share the whole high word collide in every table size.
const FIB: u64 = 0x9E37_79B9_7F4A_7C15;

/// The multiplier of the wasm String hash (`HASH_STR_MUL`).
const STR_MUL: u64 = 0x9E37_79B1;

#[test]
fn the_wasm_int_hash_is_the_emitters() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/crates/almide-wasm/src/map_index.rs")).unwrap();
    assert!(src.contains("0x9E37_79B9_7F4A_7C15"), "$mapidx_hash_int no longer uses the Fibonacci constant: update this test");
    assert!(
        src.contains("HASH_STR_MUL: u64 = 0x9E37_79B1;") && src.contains("word-wise multiplicative hash"),
        "$mapidx_hash_str is no longer the word-wise hash times 0x9E37_79B1: update this test's mirror and its inverse"
    );
    // The mirror below against the hash's definition, at every tail length.
    assert_eq!(str_hash_wasm(b""), 0);
    assert_eq!(str_hash_wasm(b"a"), fold((1 ^ 0x61u64).wrapping_mul(STR_MUL)));
    let two_words = (16 ^ u64::from_le_bytes(*b"01234567")).wrapping_mul(STR_MUL);
    assert_eq!(str_hash_wasm(b"0123456789abcdef"), fold((two_words ^ u64::from_le_bytes(*b"89abcdef")).wrapping_mul(STR_MUL)));
    assert_eq!(str_hash_wasm(b"012345678"), fold(((9 ^ u64::from_le_bytes(*b"01234567")).wrapping_mul(STR_MUL) ^ 0x38).wrapping_mul(STR_MUL)));
}

#[test]
fn colliding_int_keys_build_a_wasm_map_in_near_linear_time() {
    const N: u64 = 150_000;
    let inv = inv_mul(FIB);
    let keys: Vec<i64> = (1..=N).map(|i| ((0x5A5A5u64 << 32) | i).wrapping_mul(inv) as i64).collect();
    assert!(keys.iter().all(|&k| (k as u64).wrapping_mul(FIB) >> 32 == 0x5A5A5));
    let keys: Vec<String> = keys.iter().map(i64::to_string).collect();
    check_flood(&run_flood(&keys, true, "wasm"), N as usize, Some(3_000), "wasm Int");
}

fn fold(h: u64) -> u32 {
    ((h >> 32) ^ h) as u32
}

/// `$mapidx_hash_str`: the state starts as the byte length; each eight
/// payload bytes (little-endian), then the 1..=8 bytes left, are XORed in
/// and the state multiplied by `STR_MUL`; the two halves are folded once.
fn str_hash_wasm(s: &[u8]) -> u32 {
    let word = |b: &[u8]| b.iter().rev().fold(0u64, |w, &c| (w << 8) | u64::from(c));
    let mut h = s.len() as u64;
    let mut rest = s;
    while rest.len() > 8 {
        h = (h ^ word(&rest[..8])).wrapping_mul(STR_MUL);
        rest = &rest[8..];
    }
    if !rest.is_empty() {
        h = (h ^ word(rest)).wrapping_mul(STR_MUL);
    }
    fold(h)
}

/// `n` distinct 16-byte keys whose hash state is 0 before the fold, so
/// every one hashes to 0 and starts at slot 0 of a table of any size. The
/// second word of a key is the state its first word leaves (`h1`), so the
/// XOR clears it; the first word is `h1` run backwards through the multiply.
/// A candidate is kept when all sixteen bytes are ASCII a line of text can
/// hold: about one in 300.
fn colliding_wasm_string_keys(n: usize) -> Vec<String> {
    let inv = inv_mul(STR_MUL);
    let line_safe = |w: u64| w.to_le_bytes().iter().all(|&b| (1..0x80).contains(&b) && b != b'\n' && b != b'\r');
    let mut keys = Vec::with_capacity(n);
    let mut x = 0x1234_5678_9ABC_DEF1u64;
    while keys.len() < n {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        // Distinct candidates: xorshift64 does not repeat within its period.
        let h1 = u64::from_le_bytes(x.to_le_bytes().map(|b| 0x21 + b % 94));
        let w1 = h1.wrapping_mul(inv) ^ 16;
        if line_safe(w1) {
            let bytes = [w1.to_le_bytes(), h1.to_le_bytes()].concat();
            keys.push(String::from_utf8(bytes).expect("ASCII"));
        }
    }
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), n, "the candidates repeat: two h1 mapped to one key");
    keys
}

#[test]
fn colliding_string_keys_build_a_wasm_map_in_near_linear_time() {
    // 150 000 strings with one hash: without the flood bound the build
    // probes about n²/2 slots, each a string comparison. Past the bound the
    // block moves to the ordered index, and the String comparator answers
    // removes, overwrites and lookups exactly.
    const N: usize = 150_000;
    let keys = colliding_wasm_string_keys(N);
    assert!(keys.iter().all(|k| str_hash_wasm(k.as_bytes()) == 0));
    check_flood(&run_flood(&keys, false, "wasm"), N, Some(3_000), "wasm String");
}
