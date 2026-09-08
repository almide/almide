#!/usr/bin/env python3
"""Python model of the Schubfach core (Giulietti) as transcribed into schubfach.almd, with the
run-time g(k) derivation, and the exact g table used by gen_table_variants.py.

    python3 schubfach_model.py [seed] [n]   — checks g_runtime == g_exact for all 617 k, then
                                              n random doubles against Python's repr

NOTE: the model follows Rust's `format!("{}")` rules (an exact tie rounds the magnitude up,
the multiple-of-10 candidate is tried from s >= 10, no C_TINY branch), so it disagrees with
Python's repr on exact ties (repr rounds to even: 1669760939663944.25 -> ...4.2, Rust ...4.3).
Those show up as "MISMATCH" lines here and are expected; the authoritative oracle is the
native leg's float.to_string via gen_harness.py's dump_native/dump_proto pair.
"""
import random, struct, sys

M63 = (1 << 63) - 1
M64 = (1 << 64) - 1

# floor-log helpers; the constants are verified exact over the f64 range against big-int logs
def flog10pow2(e): return (e * 661_971_961_083) >> 41
def flog10threeQuartersPow2(e): return (e * 661_971_961_083 - 274_743_187_321) >> 41
def flog2pow10(e): return (e * 913_124_641_741) >> 38

def g_exact(j):
    """g = floor(10^j * 2^-r) + 1 with r = flog2pow10(j) - 125; returns (g1, g0), 63 bits each."""
    r = flog2pow10(j) - 125
    if j >= 0:
        g = ((10 ** j) >> r if r >= 0 else (10 ** j) << -r) + 1
    else:
        g = ((1 << -r) // 10 ** (-j)) + 1
    assert (1 << 125) <= g < (1 << 126), (j, g)
    return g >> 63, g & M63

def g_runtime(j):
    """The same value the Almide printer derives: j >= 0: top 126 bits of 10^j (+1);
    j < 0: floor(2^N / 10^-j) with N = 125 - flog2pow10(j), by chunked small division (+1)."""
    if j >= 0:
        P = 10 ** j
        L = flog2pow10(j) + 1
        sh = 126 - L
        g = (P << sh if sh >= 0 else P >> -sh) + 1
    else:
        m = -j
        P = 1 << (125 - flog2pow10(j))
        while m >= 9:
            P //= 10 ** 9; m -= 9
        P //= 10 ** m
        g = P + 1
    return g >> 63, g & M63

def mulhi(a, b): return ((a & M64) * (b & M64)) >> 64

def rop(g1, g0, cp):
    x1 = mulhi(g0, cp)
    y0 = (g1 * cp) & M64
    y1 = mulhi(g1, cp)
    z = (y0 >> 1) + x1
    vbp = y1 + (z >> 63)
    return vbp | (((z & M63) + M63) >> 63)

C_MIN = 1 << 52
Q_MIN = -1074

def to_decimal(q, c, gfun):
    out = c & 1
    cb = c << 2
    if c != C_MIN or q == Q_MIN:
        cbl, k = cb - 2, flog10pow2(q)
    else:
        cbl, k = cb - 1, flog10threeQuartersPow2(q)
    h = q + flog2pow10(-k) + 2
    assert 2 <= h <= 5, h
    g1, g0 = gfun(-k)
    vb = rop(g1, g0, cb << h)
    vbl = rop(g1, g0, cbl << h)
    vbr = rop(g1, g0, (cb + 2) << h)
    s = vb >> 2
    def pick(lo, hi):
        return (1 if vbl + out <= lo << 2 else 0) + (2 if (hi << 2) + out <= vbr else 0)
    sp10 = (s // 10) * 10
    sel10 = pick(sp10, sp10 + 10) if s >= 10 else 0
    if sel10 == 1: return sp10, k
    if sel10 == 2: return sp10 + 10, k
    sel = pick(s, s + 1)
    if sel == 1: return s, k
    if sel == 2: return s + 1, k
    return (s if vb < (2 * s + 1) << 1 else s + 1), k

def shortest(bits, gfun=g_runtime):
    """-> (neg, f, e) with |v| = f * 10^e, f stripped of trailing zeros."""
    t = bits & ((1 << 52) - 1)
    bq = (bits >> 52) & 0x7FF
    neg = bits >> 63
    assert bq < 0x7FF
    if bq == 0 and t == 0:
        return neg, 0, 0
    f, e = to_decimal(bq - 1075, C_MIN | t, gfun) if bq else to_decimal(Q_MIN, t, gfun)
    while f % 10 == 0:
        f //= 10; e += 1
    return neg, f, e

def render(neg, f, e):
    """Rust `{}` + Almide `.0` suffix: fixed notation, never scientific."""
    ds = str(f); m = len(ds); k = m + e
    if f == 0: body = "0.0"
    elif k <= 0: body = "0." + "0" * (-k) + ds
    elif k >= m: body = ds + "0" * (k - m) + ".0"
    else: body = ds[:k] + "." + ds[k:]
    return ("-" if neg else "") + body

def oracle(bits):
    x = struct.unpack("<d", struct.pack("<Q", bits))[0]
    r = repr(x)
    if "e" in r:
        mant, ex = r.split("e"); ex = int(ex)
        neg = mant.startswith("-"); mant = mant.lstrip("-")
        ip, fp = (mant.split(".") + [""])[:2]
        f = int((ip + fp).lstrip("0") or "0"); e = ex - len(fp)
        while f % 10 == 0 and f != 0: f //= 10; e += 1
        return render(neg, f, e)
    if r in ("0.0", "-0.0"): return r
    return r if "." in r else r + ".0"

if __name__ == "__main__":
    for j in range(-292, 325):
        assert g_runtime(j) == g_exact(j), j
    print("g table: 617 entries match")
    rnd = random.Random(int(sys.argv[1]) if len(sys.argv) > 1 else 1)
    n = int(sys.argv[2]) if len(sys.argv) > 2 else 200000
    bad = 0
    for _ in range(n):
        bits = rnd.getrandbits(64)
        if (bits >> 52) & 0x7FF == 0x7FF: continue
        got, exp = render(*shortest(bits)), oracle(bits)
        if got != exp:
            bad += 1
            if bad < 10: print("MISMATCH (expected on exact ties)", hex(bits), got, exp)
    print("random:", n, "mismatches vs repr:", bad)
