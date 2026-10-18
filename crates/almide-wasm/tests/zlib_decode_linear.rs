//! The self-hosted zlib decoders are linear in bytes on the wasm leg.
//!
//! The GCE counted judge measured the bytes `zlib.gunzip` / `zlib.inflate` /
//! `zlib.decompress` move per call at n = 20000..320000 output bytes and no
//! order class fit: the series jumped where the input (the wasm encoder's
//! stored blocks, 65535 bytes each) gained a block. Each block went through a
//! `mut out: Bytes` helper that answered a TUPLE (`(status, pos, buf, cnt)`),
//! and the write-back of such a call leaves the returned tuple holding a
//! credit on the buffer, so the next block's first push met the buffer shared
//! and copied all of it: Σ over blocks of the output so far, quadratic in the
//! block count.
//!
//! Measured here as the bytes requested per call under the canonical
//! allocator (the meter's: no free-list reuse), as the marginal cost of two
//! more calls so one-off setup cancels. A doubling output buffer requests at
//! most its caps 16, 32, …, < 4n, plus the decoder's fixed tables — so five
//! bytes per output byte at every size, and 4x the output must not take the
//! bytes past 6x.

mod harness;
use harness::run_wasm;

const SIZES: [u64; 5] = [20000, 40000, 80000, 160000, 320000];

fn requested(func: &str, enc: &str, n: u64, k: u64) -> u64 {
    let src = format!(
        "import zlib\n\neffect fn main() -> Unit = {{\n  let data = zlib.{enc}(bytes.from_list(list.range(0, {n}) |> list.map((i) => i * 7919 % 251)))!\n  var t = 0\n  for _ in 0..<{k} {{\n    t = t + bytes.len(zlib.{func}(data)!)\n  }}\n  println(\"${{t}}\")\n}}\n"
    );
    let ir = almide_spine::s5::lower_to_ir("zlib_decode_linear.almd", &src).expect("front");
    let bytes = {
        let _count = almide_wasm::alloc_count::CountGuard::set();
        let _canonical = almide_wasm::canonical_alloc::CanonicalGuard::set();
        almide_wasm::emit_program(&ir).expect("emit")
    };
    let r = run_wasm(&bytes).expect("run");
    assert_eq!(r.stdout, format!("{}\n", n * k), "{func} n={n}");
    r.alloc_count.expect("the armed module exports the counters").bytes
}

fn assert_linear(func: &str, enc: &str) {
    let per_call: Vec<u64> = SIZES.iter().map(|&n| (requested(func, enc, n, 4) - requested(func, enc, n, 2)) / 2).collect();
    let mut bad = Vec::new();
    for (&n, &b) in SIZES.iter().zip(&per_call) {
        if b > 5 * n {
            bad.push(format!("n={n}: {b} bytes per call ({:.2} per output byte)", b as f64 / n as f64));
        }
    }
    for (i, j) in [(0, 2), (1, 3), (2, 4)] {
        if per_call[j] >= 6 * per_call[i] {
            bad.push(format!("4x the output ({} -> {}) took {:.1}x the bytes", SIZES[i], SIZES[j], per_call[j] as f64 / per_call[i] as f64));
        }
    }
    assert!(bad.is_empty(), "zlib.{func} is not linear in bytes on wasm (series {per_call:?}):\n{}", bad.join("\n"));
}

#[test]
fn inflate_is_linear() {
    assert_linear("inflate", "deflate");
}

#[test]
fn gunzip_is_linear() {
    assert_linear("gunzip", "gzip");
}

#[test]
fn decompress_is_linear() {
    assert_linear("decompress", "compress");
}
