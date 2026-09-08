#!/usr/bin/env python3
"""Derive the two TABLE-driven Schubfach variants from schubfach.almd by swapping the
`__sf_compute_g` section for a lookup (everything else — core, rop, mulhi, render — is shared):

  schubfach_table_full.almd     (a) the full 617-entry table: g1[j], g0[j] as two List[Int]
                                    literals (1,234 Ints)
  schubfach_table_compact.almd  (b) every 8th entry: 78 x 189-bit T(k0) = floor(10^k0 * 2^-r')
                                    as 3 Ints (63 bits each); g(j) = top 126 bits of
                                    T(k0) * 10^(j-k0), + 1 — exact for all 617 j (table_check.py)

Both keep the bignum helpers they need (compact: mul_small + getbits) and drop the rest.
"""
import os, re, sys
here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, here)
from schubfach_model import g_exact, flog2pow10

M63 = (1 << 63) - 1
src = open(os.path.join(here, "schubfach.almd")).read()

# cut the runtime-g section: from the bignum helpers header up to (not including) the mulhi header
start = src.index("// ───────────────────────── big-integer helpers")
end = src.index("// ───────────────────────── 64x64 -> high 64")
head, tail = src[:start], src[end:]
head = head.replace("schubfach_rt2 —", "schubfach (table variant) —")

def lit(vals):
    return "[" + ", ".join(str(v) for v in vals) + "]"

# (a) full table -------------------------------------------------------------------------------
g1s, g0s = zip(*[g_exact(j) for j in range(-292, 325)])
full = f"""// ───────────────────────── g(j) from the FULL 617-entry table (j in -292..324) ────────────────
fn __sf_g1_table() -> List[Int] = {lit(g1s)}

fn __sf_g0_table() -> List[Int] = {lit(g0s)}

fn __sf_compute_g(j: Int, sb: Int) -> Unit = {{
  let i = j + 292
  prim.store64(sb + 160, list.get(__sf_g1_table(), i) ?? 0)
  prim.store64(sb + 168, list.get(__sf_g0_table(), i) ?? 0)
}}

"""
open(os.path.join(here, "schubfach_table_full.almd"), "w").write(head + full + tail)

# (b) compact every-8th table ------------------------------------------------------------------
TB = 189
def T(k0):
    rp = flog2pow10(k0) - (TB - 1)
    if k0 >= 0:
        return (10 ** k0) >> rp if rp >= 0 else (10 ** k0) << -rp
    return (1 << -rp) // 10 ** (-k0)

k0s = list(range((-292 // 8) * 8, (324 // 8) * 8 + 1, 8))
words = []
for k0 in k0s:
    t = T(k0)
    words += [t & M63, (t >> 63) & M63, (t >> 126) & M63]

# the bignum helpers the compact variant keeps: __sf_limb, __sf_ms_body, __sf_mul_small,
# __sf_pow10_small, __sf_getbits (copied verbatim out of the runtime section)
def grab(name):
    m = re.search(r"(?:^//[^\n]*\n)*^fn " + re.escape(name) + r"\(.*?(?=\n\n)", src[start:end], re.S | re.M)
    assert m, name
    return m.group(0) + "\n\n"

kept = "".join(grab(n) for n in ["__sf_limb", "__sf_ms_body", "__sf_mul_small", "__sf_pow10_small", "__sf_getbits"])
compact = f"""// ───────────────────────── big-integer helpers kept for the rescale ─────────────────────────────
{kept}// ───────────────────────── g(j) from the COMPACT every-8th table ─────────────────────────────────
// {len(k0s)} entries k0 = {k0s[0]}, {k0s[0] + 8}, ..., {k0s[-1]}: T(k0) = floor(10^k0 * 2^-r'), 189 bits, as three
// 63-bit words (low first). g(j) - 1 = the top 126 bits of T(k0) * 10^(j - k0), j - k0 in 0..7:
// the truncation error of T is < 1, times 10^7 < 2^24, so it sits 2^-63 below the 126-bit
// granularity — exact for every j in -292..324 (checked offline against the exact g).
fn __sf_t_table() -> List[Int] = {lit(words)}

fn __sf_word(tbl: List[Int], i: Int) -> Int = list.get(tbl, i) ?? 0

fn __sf_compute_g(j: Int, sb: Int) -> Unit = {{
  let k0 = prim.bshl(prim.bshr(j, 3), 3)
  let d = j - k0
  let e = (prim.bshr(j, 3) + {-k0s[0] // 8}) * 3
  let tbl = __sf_t_table()
  let w0 = __sf_word(tbl, e)
  let w1 = __sf_word(tbl, e + 1)
  let w2 = __sf_word(tbl, e + 2)
  // T as six u32 limbs: bits [0,63) [63,126) [126,189) → limbs at 32-bit steps
  prim.store32(sb, 6)
  prim.store32(sb + 4, prim.band(w0, 4294967295))
  prim.store32(sb + 8, prim.bor(prim.bshr_u(w0, 32), prim.bshl(prim.band(w1, 1), 31)))
  prim.store32(sb + 12, prim.band(prim.bshr_u(w1, 1), 4294967295))
  prim.store32(sb + 16, prim.bor(prim.bshr_u(w1, 33), prim.bshl(prim.band(w2, 3), 30)))
  prim.store32(sb + 20, prim.band(prim.bshr_u(w2, 2), 4294967295))
  prim.store32(sb + 24, prim.bshr_u(w2, 34))
  __sf_mul_small(sb, __sf_pow10_small(d))
  // bit length of the exact 10^j * 2^-r' where r' = floor(log2 10^k0) - 188
  let l = __sf_flog2pow10(j) - (__sf_flog2pow10(k0) - 188) + 1
  let g0 = __sf_getbits(sb, l - 126, 63) + 1
  let carry = if g0 == prim.bshl(1, 63) then 1 else 0
  prim.store64(sb + 160, __sf_getbits(sb, l - 63, 63) + carry)
  prim.store64(sb + 168, prim.band(g0, 9223372036854775807))
}}

"""
open(os.path.join(here, "schubfach_table_compact.almd"), "w").write(head + compact + tail)
print("wrote schubfach_table_full.almd (%d Ints) and schubfach_table_compact.almd (%d Ints)" % (2 * 617, len(words)))
