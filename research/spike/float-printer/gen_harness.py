#!/usr/bin/env python3
"""Assemble the measurement programs (Almide has no include; the prototype only compiles on the
wasm leg because it is written over `prim.*`, so each leg gets its own concatenated file).

  gen_harness.py <printer.almd> <entry_fn> <seed> <n> <outdir>

writes into <outdir>:
  verify.almd       wasm: <entry_fn> vs float.to_string (Dragon4) over boundary + n random
  dump_proto.almd   wasm: prints <entry_fn>(x) per line over the same stream
  dump_native.almd  native: prints float.to_string(x) (Rust format!) per line, same stream
  probe_one.almd    wasm size probe: println(<entry_fn>(1.5))
  bench_proto.almd  wasm: n conversions of <entry_fn>, prints a checksum
  bench_std.almd    both legs: n conversions of float.to_string, prints a checksum
"""
import sys, os
printer, entry, seed, n, outdir = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4]), sys.argv[5]
here = os.path.dirname(os.path.abspath(__file__))
proto = open(printer).read()
common = open(os.path.join(here, "harness_common.almd")).read()
os.makedirs(outdir, exist_ok=True)

def w(name, body):
    open(os.path.join(outdir, name), "w").write(body)

w("verify.almd", proto + "\n" + common + f"""
fn check(b: Int) -> Int = {{
  let x = int.bits_to_float(b)
  let a = float.to_string(x)
  let s = {entry}(x)
  if a == s then 0 else {{
    println("MISMATCH bits=${{b}} dragon4=${{a}} proto=${{s}}")
    1
  }}
}}

fn main() -> Unit = {{
  let m1 = boundary_bits(check)
  println("boundary mismatches: ${{m1}}")
  let m2 = random_bits({seed}, {n}, check)
  println("random mismatches: ${{m2}} / {n}")
}}
""")

w("dump_proto.almd", proto + "\n" + common + f"""
fn dump(b: Int) -> Int = {{
  println({entry}(int.bits_to_float(b)))
  0
}}

fn main() -> Unit = {{
  let _a = boundary_bits(dump)
  let _b = random_bits({seed}, {n}, dump)
  println("end")
}}
""")

w("dump_native.almd", common + f"""
fn dump(b: Int) -> Int = {{
  println(float.to_string(int.bits_to_float(b)))
  0
}}

fn main() -> Unit = {{
  let _a = boundary_bits(dump)
  let _b = random_bits({seed}, {n}, dump)
  println("end")
}}
""")

w("probe_one.almd", proto + f"""
fn main() -> Unit = {{ println({entry}(1.5)) }}
""")

w("bench_proto.almd", proto + "\n" + common + f"""
fn conv(b: Int) -> Int = string.len({entry}(int.bits_to_float(b)))

fn main() -> Unit = {{
  println(int.to_string(random_bits({seed}, {n}, conv)))
}}
""")

w("bench_std.almd", common + f"""
fn conv(b: Int) -> Int = string.len(float.to_string(int.bits_to_float(b)))

fn main() -> Unit = {{
  println(int.to_string(random_bits({seed}, {n}, conv)))
}}
""")
print("wrote", outdir)
