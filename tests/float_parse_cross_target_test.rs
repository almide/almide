//! Lock for `float.parse` (C-024, C-413): the wasm leg's self-hosted parser
//! (stdlib/float_parse.almd — the Clinger fast path and the multi-bit
//! high-precision decimal) returns the SAME f64 bits as the native leg, and
//! both equal the host's correctly-rounded parse, on a large differential
//! corpus:
//!   * random f64 bit patterns printed shortest, with 17 and with 25 digits;
//!   * random digit strings (1–40 digits, 19+-digit mantissas included) with
//!     exponents across the whole range, under/overflow included;
//!   * coordinate-shaped literals (the float-heavy JSON shape);
//!   * EXACT halfway texts between neighbouring doubles (normal and
//!     denormal; up to ~770 significant digits), each also nudged just above
//!     and just below, and padded past 800 digits with a far nonzero tail —
//!     the round-half-even, sticky-truncation and 800-digit-cap rows;
//!   * the fixed boundaries (5e-324, 2.2250738585072014e-308,
//!     1.7976931348623157e308 and their neighbours).
//! Each line is the parse's bits (`float.to_bits`) or its Err text.
//!
//! Skips cleanly when the `almide` binary or wasmtime is unavailable.

use std::path::Path;
use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

fn tools_available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
        && Command::new("wasmtime").arg("--version").output().is_ok()
}

fn run(source: &str, dir: &Path, wasm: bool) -> String {
    let src = dir.join("fparse.almd");
    std::fs::write(&src, source).unwrap();
    let mut args = vec!["run", src.to_str().unwrap()];
    if wasm {
        args.extend(["--target", "wasm"]);
    }
    let out = Command::new(almide_bin()).args(&args).output().expect("almide run");
    assert!(
        out.status.success(),
        "{} run failed:\n{}",
        if wasm { "wasm" } else { "native" },
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

fn program(cases: &[String]) -> String {
    let lits: Vec<String> = cases.iter().map(|c| format!("  \"{c}\"")).collect();
    format!(
        "fn show(s: String) -> String = match float.parse(s) {{\n  ok(v) => int.to_string(float.to_bits(v)),\n  err(e) => \"err(${{e}})\",\n}}\n\n\
         fn cases() -> List[String] = [\n{}\n]\n\n\
         effect fn main() -> Unit = {{\n  for s in cases() {{\n    println(show(s))\n  }}\n}}\n",
        lits.join(",\n")
    )
}

/// The host's correctly-rounded parse, in the program's line format.
fn oracle(s: &str) -> String {
    match s.trim_ascii().parse::<f64>() {
        Ok(v) if v.is_finite() => format!("{}", v.to_bits() as i64),
        Ok(_) => "err(float literal out of range)".to_string(),
        Err(_) => "err(invalid float literal)".to_string(),
    }
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// Exact decimal digits of (2m + 1) * 2^(e - 1) — the midpoint between the
/// double m * 2^e and its upper neighbour — as "d.ddd…eX" text.
fn midpoint_text(m: u64, e: i32) -> String {
    // value = n * 2^p with n = 2m + 1, p = e - 1. Digits little-endian base 10.
    let n = 2 * u128::from(m) + 1;
    let p = e - 1;
    let mut d: Vec<u8> = n.to_string().bytes().rev().map(|b| b - b'0').collect();
    let mut scale: i32 = 0; // value = digits * 10^scale
    if p >= 0 {
        for _ in 0..p {
            let mut c = 0u8;
            for x in d.iter_mut() {
                let v = *x * 2 + c;
                *x = v % 10;
                c = v / 10;
            }
            if c > 0 {
                d.push(c);
            }
        }
    } else {
        // divide by 2^-p: multiply by 5^-p and shift the decimal point.
        for _ in 0..(-p) {
            let mut c = 0u8;
            for x in d.iter_mut() {
                let v = *x * 5 + c;
                *x = v % 10;
                c = v / 10;
            }
            while c > 0 {
                d.push(c % 10);
                c /= 10;
            }
            scale -= 1;
        }
    }
    while d.len() > 1 && d[0] == 0 {
        d.remove(0);
        scale += 1;
    }
    let digits: String = d.iter().rev().map(|x| (b'0' + x) as char).collect();
    let exp10 = scale + digits.len() as i32 - 1;
    let frac = &digits[1..];
    format!("{}.{}e{}", &digits[..1], if frac.is_empty() { "0" } else { frac }, exp10)
}

fn decompose(x: f64) -> (u64, i32) {
    let b = x.to_bits();
    let be = ((b >> 52) & 0x7ff) as i32;
    let f = b & ((1u64 << 52) - 1);
    if be == 0 { (f, -1074) } else { (f | (1u64 << 52), be - 1075) }
}

fn fixed_cases() -> Vec<String> {
    [
        "5e-324", "2.4e-324", "2.5e-324", "2.4703282292062327e-324", "2.4703282292062328e-324",
        "4.9406564584124654e-324", "1e-323", "2.2250738585072014e-308", "2.2250738585072011e-308",
        "2.2250738585072012e-308", "4.4501477170144023e-308", "1.7976931348623157e308",
        "1.7976931348623158e308", "1.7976931348623159e308", "1e309", "-1e309", "1e-400", "0.0",
        "-0.0", "9007199254740993", "9007199254740992.5",
        // exact ties with 16-18 digits over 10^0..10^-2 (round half to even both ways)
        "9007199254740995", "4503599627370496.5", "4503599627370497.5", "2251799813685248.25",
        "2251799813685248.75", "-2251799813685248.25", "123456789012345678901234567890",
        "0.1", "0.3", "1e23", "8.98846567431158e307", "-65.613616999999977", "43.420273000000009",
        "0.000000000000000000000000000000000000000000001", "1e-2147483649", "1e2147483648",
        "  +3.14  ", "-.5", "5.", "1E3", "00000000000000000000123.456e-2",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn random_cases(rng: &mut Rng, n: usize) -> Vec<String> {
    let mut out = Vec::with_capacity(n * 6);
    while out.len() < n * 6 {
        let x = f64::from_bits(rng.next());
        if !x.is_finite() {
            continue;
        }
        out.push(format!("{x:?}"));
        out.push(format!("{x:.16e}"));
        out.push(format!("{x:.24e}"));
        let lat = (rng.below(360_000_000) as f64) / 1e6 - 180.0;
        out.push(format!("{lat:.15}"));
        let nd = 1 + rng.below(40) as usize;
        let digits: String = (0..nd).map(|_| (b'0' + rng.below(10) as u8) as char).collect();
        let ex = rng.below(660) as i64 - 345;
        out.push(format!("{digits}e{ex}"));
        let nd = 19 + rng.below(8) as usize;
        let digits: String = (0..nd).map(|_| (b'0' + rng.below(10) as u8) as char).collect();
        out.push(format!("{}.{}", &digits[..3], &digits[3..]));
    }
    out
}

fn halfway_cases(rng: &mut Rng, n: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut k = 0;
    while k < n {
        // Alternate a random normal, a random denormal and a near-limit value.
        let bits = match k % 3 {
            0 => rng.next() & 0x7fef_ffff_ffff_ffff,
            1 => rng.next() & 0x000f_ffff_ffff_ffff,
            _ => 0x7fe0_0000_0000_0000 | (rng.next() & 0x000f_ffff_ffff_ffff),
        };
        let x = f64::from_bits(bits);
        if !x.is_finite() || x == 0.0 {
            continue;
        }
        let (m, e) = decompose(x);
        let mid = midpoint_text(m, e);
        let (mant, ex) = mid.split_once('e').unwrap();
        out.push(mid.clone());
        // just above: a far nonzero digit appended
        out.push(format!("{mant}000000001e{ex}"));
        // just below: the last (nonzero — trailing zeros are stripped) digit lowered
        let mut below = mant.to_string();
        let last = below.pop().unwrap();
        if last != '0' {
            below.push((last as u8 - 1) as char);
            out.push(format!("{below}e{ex}"));
        }
        // past 800 digits: a zero run then a far nonzero tail (sticky) …
        let pad = 820usize.saturating_sub(mant.len());
        out.push(format!("{mant}{}1e{ex}", "0".repeat(pad)));
        // … and an all-zero tail (an exact tie however long)
        out.push(format!("{mant}{}e{ex}", "0".repeat(pad)));
        k += 1;
    }
    out
}

fn check_batch(cases: &[String], what: &str) {
    let dir = tempfile::tempdir().unwrap();
    let src = program(cases);
    let native = run(&src, dir.path(), false);
    let wasm = run(&src, dir.path(), true);
    let want: Vec<String> = cases.iter().map(|c| oracle(c)).collect();
    let nl: Vec<&str> = native.lines().collect();
    let wl: Vec<&str> = wasm.lines().collect();
    assert_eq!(nl.len(), cases.len(), "{what}: native printed {} lines for {} cases", nl.len(), cases.len());
    assert_eq!(wl.len(), cases.len(), "{what}: wasm printed {} lines for {} cases", wl.len(), cases.len());
    for i in 0..cases.len() {
        assert!(
            nl[i] == wl[i] && nl[i] == want[i],
            "{what}: float.parse({:?}): native={} wasm={} oracle={}",
            cases[i],
            nl[i],
            wl[i],
            want[i]
        );
    }
}

#[test]
fn float_parse_cross_target_fixed_and_random() {
    if !tools_available() {
        eprintln!("skipping: almide or wasmtime unavailable");
        return;
    }
    check_batch(&fixed_cases(), "fixed");
    let mut rng = Rng(0x5EED_F10A_7BA5_E001);
    for b in 0..4 {
        check_batch(&random_cases(&mut rng, 400), &format!("random batch {b}"));
    }
}

#[test]
fn float_parse_cross_target_halfway() {
    if !tools_available() {
        eprintln!("skipping: almide or wasmtime unavailable");
        return;
    }
    let mut rng = Rng(0x4A1F_3A7E_0DD5_E7E1);
    for b in 0..3 {
        check_batch(&halfway_cases(&mut rng, 60), &format!("halfway batch {b}"));
    }
}
