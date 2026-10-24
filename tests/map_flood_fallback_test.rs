//! #4104: keys crafted to collide under the map index's fast hash must not
//! make building a native `Map` quadratic. The index stays on the fast hash
//! by default; a map whose inserts probe past `ALMIDE_MAP_FLOOD_PROBES`
//! re-indexes itself under a per-process keyed hash.
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

#[test]
fn colliding_int_keys_build_a_map_in_near_linear_time() {
    const N: u64 = 150_000;
    let dir = tempfile::tempdir().unwrap();
    let keys: Vec<i64> = (1..=N).map(|i| unmix64((i << 20) | 0x5A5A5) as i64).collect();
    // Every key lands on the same starting slot of a 2^k table, k <= 20.
    assert!(keys.iter().all(|&k| mix64(k as u64) & 0xF_FFFF == 0x5A5A5));
    let text: String = keys.iter().map(|k| format!("{k}\n")).collect();
    std::fs::write(dir.path().join("keys.txt"), text).unwrap();
    std::fs::write(
        dir.path().join("flood.almd"),
        r#"import fs
effect fn main() -> Unit = {
  let text = fs.read_text("keys.txt")!
  let ks = string.split(text, "\n") |> list.filter_map((s) => int.parse(s)?)
  let pairs = ks |> list.map((k) => (k, 1))
  let t0 = datetime.monotonic_ns()!
  let m = map.from_list(pairs)
  let t1 = datetime.monotonic_ns()!
  println(int.to_string(map.len(m)))
  println(int.to_string((t1 - t0) / 1000000))
}
"#,
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["run", "flood.almd"])
        .current_dir(dir.path())
        .output()
        .expect("spawn almide run");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", String::from_utf8_lossy(&out.stderr));
    let mut lines = stdout.lines();
    assert_eq!(lines.next(), Some(N.to_string().as_str()), "every distinct key is kept");
    let ms: u64 = lines.next().and_then(|l| l.trim().parse().ok()).expect("elapsed ms");
    assert!(ms < 3_000, "building a map of {N} colliding keys took {ms} ms: the flood fallback did not engage");
}
