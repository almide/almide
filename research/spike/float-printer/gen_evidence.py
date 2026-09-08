#!/usr/bin/env python3
"""The targeted exactness sets behind docs/benchmarks/float-printer.md (beyond uniform bit
patterns). Each set is ONE program that prints one line per value; the same source runs on
both legs (native `float.to_string` is Rust's `format!`, wasm is the stdlib printer) and the
two outputs are compared with `cmp`. A line carries the input, the parsed bits where a parser
is involved (so a parser divergence is told apart from a printer one) and the printed string.

  gen_evidence.py <outdir>      writes set1..set4 .almd into <outdir>

  set1  1,000,000 seeded values, exponent field 1000..1060 (~1e-7 .. 1e10), random sign/mantissa
  set2  500,000 decimal strings (1..17 significant digits; d.dddd / dd.dd / -d.ddd / d.ddde±N /
        ddde±N) parsed with float.parse, then printed
  set3  every integer 0..100000 as a float; 10^k for k in -320..308 (parsed); 2^k for k in
        -1074..1023 (by bits); the 201 doubles around 1.0, 1e15, 1e16, 1e17, 1e22, 1e23 and the
        halfway decimals between doubles there (parsed); the subnormal minimum, the max finite,
        0.1/0.2/0.3/0.7/1.1/2.675/5e-324/9007199254740993
  set4  100,000 seeded values through float.to_fixed with precision 0..10 (the retained Dragon4
        fixed path; compared with the 0.62.0 binary as well as with native)
"""
import os, sys
here = os.path.dirname(os.path.abspath(__file__))
out = sys.argv[1]
os.makedirs(out, exist_ok=True)
common = open(os.path.join(here, "harness_common.almd")).read()

def w(name, body):
    open(os.path.join(out, name), "w").write(common + "\n" + body)

w("set1.almd", """
// a non-negative 0..m-1 from the top bits of a pattern (the low bits of xorshift are weaker)
fn __e_mod(x: Int, m: Int) -> Int = __h_shr_u(x, 20) % m

fn main() -> Unit = {
  var s = 12345
  var i = 0
  while i < 1000000 {
    s = __h_next(s)
    let e = 1000 + __e_mod(s, 61)
    s = __h_next(s)
    let mant = int.band(s, 4503599627370495)
    s = __h_next(s)
    let sign = if int.band(s, 1) == 1 then int.bshl(1, 63) else 0
    let b = int.bor(int.bor(int.bshl(e, 52), mant), sign)
    println(float.to_string(int.bits_to_float(b)))
    i = i + 1
  }
  println("end")
}
""")

w("set2.almd", """
fn __e_mod(x: Int, m: Int) -> Int = __h_shr_u(x, 20) % m

fn show(str: String) -> Unit = match float.parse(str) {
  Ok(x) => println(str + " " + int.to_string(float.to_bits(x)) + " " + float.to_string(x)),
  Err(e) => println(str + " ERR " + e),
}

fn main() -> Unit = {
  var s = 777
  var i = 0
  while i < 500000 {
    s = __h_next(s)
    let k = 1 + __e_mod(s, 17)
    var ds = ""
    var j = 0
    while j < k {
      s = __h_next(s)
      ds = ds + int.to_string(__e_mod(s, 10))
      j = j + 1
    }
    s = __h_next(s)
    let form = __e_mod(s, 5)
    s = __h_next(s)
    let pos = 1 + __e_mod(s, k)
    s = __h_next(s)
    let ex = __e_mod(s, 640) - 330
    let exs = if ex >= 0 then "e+" + int.to_string(ex) else "e-" + int.to_string(0 - ex)
    let pointed = if pos < k then string.slice(ds, 0, pos) + "." + string.slice(ds, pos, k) else ds
    let str = if form == 0 then pointed
    else if form == 1 then "-" + pointed
    else if form == 2 then ds + exs
    else if form == 3 then string.slice(ds, 0, 1) + "." + string.slice(ds, 1, k) + exs
    else "-" + pointed + exs
    show(str)
    i = i + 1
  }
  println("end")
}
""")

w("set3.almd", """
fn show(str: String) -> Unit = match float.parse(str) {
  Ok(x) => println(str + " " + int.to_string(float.to_bits(x)) + " " + float.to_string(x)),
  Err(e) => println(str + " ERR " + e),
}

fn show_bits(b: Int) -> Unit = println(int.to_string(b) + " " + float.to_string(int.bits_to_float(b)))

fn around(anchor: String) -> Unit = {
  let c = float.to_bits(float.parse(anchor) ?? 0.0)
  var d = 0 - 100
  while d <= 100 {
    show_bits(c + d)
    d = d + 1
  }
}

fn main() -> Unit = {
  var i = 0
  while i <= 100000 {
    println(float.to_string(float.from_int(i)))
    i = i + 1
  }
  var k = 0 - 320
  while k <= 308 {
    show("1e" + int.to_string(k))
    k = k + 1
  }
  // 2^-1074 .. 2^-1023 are the subnormal single bits, 2^-1022 .. 2^1023 the zero-significand normals
  var p = 0
  while p < 52 {
    show_bits(int.bshl(1, p))
    p = p + 1
  }
  var e = 1
  while e <= 2046 {
    show_bits(int.bshl(e, 52))
    e = e + 1
  }
  around("1.0")
  around("1e15")
  around("1e16")
  around("1e17")
  around("1e22")
  around("1e23")
  // halfway decimals between consecutive doubles at the anchors (parse rounds to even)
  show("1.00000000000000011102230246251565404236316680908203125")
  show("1.0000000000000002220446049250313080847263336181640625")
  show("1000000000000000.0625")
  show("1000000000000000.1875")
  show("10000000000000001")
  show("10000000000000003")
  show("100000000000000008")
  show("100000000000000024")
  show("9999999999999999.5")
  show("9007199254740993")
  show("9007199254740995")
  show("1e22")
  show("1e23")
  show("9.999999999999999e22")
  show("1.0000000000000001e23")
  show("100000000000000008388608")
  show("99999999999999991611392")
  show("2.2250738585072011e-308")
  show("2.2250738585072012e-308")
  show("2.2250738585072014e-308")
  show("4.9406564584124654e-324")
  show("2.4703282292062327e-324")
  show("2.4703282292062328e-324")
  show("1.7976931348623157e308")
  show("1.7976931348623158e308")
  show_bits(1)
  show_bits(9218868437227405311)
  show("0.1")
  show("0.2")
  show("0.3")
  show("0.7")
  show("1.1")
  show("2.675")
  show("5e-324")
  show("9007199254740993")
  println("end")
}
""")

w("set4.almd", """
fn __e_mod(x: Int, m: Int) -> Int = __h_shr_u(x, 20) % m

fn main() -> Unit = {
  var s = 4242
  var i = 0
  while i < 100000 {
    s = __h_next(s)
    let b = s
    s = __h_next(s)
    let p = __e_mod(s, 11)
    println(int.to_string(b) + " " + int.to_string(p) + " " + float.to_fixed(int.bits_to_float(b), p))
    i = i + 1
  }
  println("end")
}
""")
print("wrote", out)
