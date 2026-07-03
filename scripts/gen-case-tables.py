#!/usr/bin/env python3
"""Regenerate stdlib/string_to_upper.almd + string_to_lower.almd from Rust itself.

Usage:
  1. cargo run --release --manifest-path <casedump>/Cargo.toml        > casemap.txt
  2. cargo run --release --manifest-path <casedump>/Cargo.toml props  > caseprops.txt
     (casedump = the tiny dumper in scripts/casedump/ — every codepoint where
      to_uppercase/to_lowercase != identity, plus black-box Cased/Case_Ignorable
      sets probed through the Final_Sigma behavior of str::to_lowercase)
  3. python3 scripts/gen-case-tables.py casemap.txt caseprops.txt   (from the repo root)

Regenerate whenever the Rust toolchain bumps its Unicode version — the tables
are dumped FROM the oracle, so v0/v1 cannot drift.
"""

# parse the Rust dump
upper = {}   # cp -> [cps]
lower = {}
for line in open(sys.argv[1]):
    parts = line.split()
    kind, cp, targets = parts[0], int(parts[1], 16), [int(x, 16) for x in parts[2:]]
    (upper if kind == 'U' else lower)[cp] = targets

def build(mapping):
    """entries: (cp, val) with val = target cp, or 0xF00000+idx into specials"""
    specials = []
    entries = []
    for cp in sorted(mapping):
        t = mapping[cp]
        if len(t) == 1:
            entries.append((cp, t[0]))
        else:
            idx = len(specials)
            specials.append(t + [0] * (3 - len(t)))
            entries.append((cp, 0xF00000 + idx))
    tbl = ''.join(f'{cp:06X}{val:06X}' for cp, val in entries)
    sp = ''.join(f'{a:06X}{b:06X}{c:06X}' for a, b, c in specials)
    return tbl, sp, len(entries)

def emit(name, rust_name, mapping, ascii_lo, ascii_hi, ascii_delta, sigma=None):
    tbl, sp, n = build(mapping)
    if sigma:
        cased_n, ci_n = sigma
        sig_params = ", st: Int, ch: Int, ih: Int"
        sig_args = ", st, ch, ih"
        sigma_arm = (
            "      if cp == 931 then {\n"
            "        let fin = __to_lower_bck(st, p, ch, " + str(cased_n) + ", ih, " + str(ci_n) + ")"
            " and not __to_lower_fwd(p + l, end, ch, " + str(cased_n) + ", ih, " + str(ci_n) + ")\n"
            "        let w = __NM_w(dh, o, (if fin then 962 else 963))\n"
            "        __NM_walk(p + l, end, th, nent, sh, dh, o + w, st, ch, ih)\n"
            "      }\n"
            "      else {\n"
        ).replace("NM", name)
        sigma_close = "      }\n"
        sig_locals = ('  let ctbl = "' + cased_tbl + '"\n  let itbl = "' + ci_tbl + '"\n')
        sig_entry = ", h + 12, prim.handle(ctbl) + 12, prim.handle(itbl) + 12"
    else:
        sig_params = ""
        sig_args = ""
        sigma_arm = ""
        sigma_close = ""
        sig_locals = ""
        sig_entry = ""
    return f'''// {name} — SELF-HOSTED `string.{name}(s)` for v1: FULL Unicode case mapping,
// byte-identical to v0's `str::{rust_name}()`. The mapping table is GENERATED
// FROM RUST ITSELF (scratch casedump: every codepoint where {rust_name} != identity,
// {n} entries incl. 1:N SpecialCasing like ß→SS) so the two implementations
// cannot drift — regenerate with scripts/gen-case-tables.py when the Rust
// toolchain's Unicode version changes. Encoding: a sorted fixed-width hex
// string, 12 hex chars per entry = [cp:6][target:6]; target >= 0xF00000 is an
// index into the 18-hex-wide specials table [c1:6][c2:6][c3:6] (zero-padded).
// Lookup = binary search over the literal (static data, no per-call parse);
// ASCII takes the arithmetic fast path and never reaches the table.

fn __{name}_h1(b: Int) -> Int =
  if b <= 57 then b - 48 else b - 55

fn __{name}_rd6(a: Int) -> Int =
  __{name}_h1(prim.load8(a)) * 1048576 + __{name}_h1(prim.load8(a + 1)) * 65536
    + __{name}_h1(prim.load8(a + 2)) * 4096 + __{name}_h1(prim.load8(a + 3)) * 256
    + __{name}_h1(prim.load8(a + 4)) * 16 + __{name}_h1(prim.load8(a + 5))

fn __{name}_find(th: Int, lo: Int, hi: Int, cp: Int) -> Int =
  if lo >= hi then 0 - 1
  else {{
    let mid = (lo + hi) / 2
    let e = __{name}_rd6(th + mid * 12)
    if e == cp then mid
    else if e < cp then __{name}_find(th, mid + 1, hi, cp)
    else __{name}_find(th, lo, mid, cp)
  }}

fn __{name}_cp(addr: Int) -> (Int, Int) = {{
  let b0 = prim.load8(addr)
  if b0 < 128 then (b0, 1)
  else if b0 < 224 then ((b0 - 192) * 64 + (prim.load8(addr + 1) - 128), 2)
  else if b0 < 240 then ((b0 - 224) * 4096 + (prim.load8(addr + 1) - 128) * 64 + (prim.load8(addr + 2) - 128), 3)
  else ((b0 - 240) * 262144 + (prim.load8(addr + 1) - 128) * 4096 + (prim.load8(addr + 2) - 128) * 64 + (prim.load8(addr + 3) - 128), 4)
}}

fn __{name}_w(dh: Int, o: Int, cp: Int) -> Int =
  if cp < 128 then {{
    prim.store8(dh + o, cp)
    1
  }}
  else if cp < 2048 then {{
    prim.store8(dh + o, 192 + cp / 64)
    prim.store8(dh + o + 1, 128 + cp % 64)
    2
  }}
  else if cp < 65536 then {{
    prim.store8(dh + o, 224 + cp / 4096)
    prim.store8(dh + o + 1, 128 + (cp / 64) % 64)
    prim.store8(dh + o + 2, 128 + cp % 64)
    3
  }}
  else {{
    prim.store8(dh + o, 240 + cp / 262144)
    prim.store8(dh + o + 1, 128 + (cp / 4096) % 64)
    prim.store8(dh + o + 2, 128 + (cp / 64) % 64)
    prim.store8(dh + o + 3, 128 + cp % 64)
    4
  }}

fn __{name}_walk(p: Int, end: Int, th: Int, nent: Int, sh: Int, dh: Int, o: Int{sig_params}) -> Int =
  if p >= end then o
  else {{
    let b0 = prim.load8(p)
    if b0 < 128 then {{
      prim.store8(dh + o, (if b0 >= {ascii_lo} and b0 <= {ascii_hi} then b0 + {ascii_delta} else b0))
      __{name}_walk(p + 1, end, th, nent, sh, dh, o + 1{sig_args})
    }}
    else {{
      let (cp, l) = __{name}_cp(p)
{sigma_arm}      let idx = __{name}_find(th, 0, nent, cp)
      if idx < 0 then {{
        let w = __{name}_w(dh, o, cp)
        __{name}_walk(p + l, end, th, nent, sh, dh, o + w{sig_args})
      }}
      else {{
        let v = __{name}_rd6(th + idx * 12 + 6)
        if v < 15728640 then {{
          let w = __{name}_w(dh, o, v)
          __{name}_walk(p + l, end, th, nent, sh, dh, o + w{sig_args})
        }}
        else {{
          let sa = sh + (v - 15728640) * 18
          let c1 = __{name}_rd6(sa)
          let c2 = __{name}_rd6(sa + 6)
          let c3 = __{name}_rd6(sa + 12)
          let w1 = __{name}_w(dh, o, c1)
          let w2 = if c2 != 0 then __{name}_w(dh, o + w1, c2) else 0
          let w3 = if c3 != 0 then __{name}_w(dh, o + w1 + w2, c3) else 0
          __{name}_walk(p + l, end, th, nent, sh, dh, o + w1 + w2 + w3{sig_args})
        }}
      }}
{sigma_close}    }}
  }}

fn string_{name}(s: String) -> String = {{
  let tbl = "{tbl}"
  let sp = "{sp}"
  let h = prim.handle(s)
  let blen = prim.load32(h + 4)
  let buf = prim.alloc_str(blen * 3 + 8)
{sig_locals}  let outn = __{name}_walk(h + 12, h + 12 + blen, prim.handle(tbl) + 12, {n}, prim.handle(sp) + 12, prim.handle(buf) + 12, 0{sig_entry})
  prim.store32(prim.handle(buf) + 4, outn)
  buf
}}
'''

def ranges(cps):
    cps = sorted(cps)
    out = []
    for cp in cps:
        if out and cp == out[-1][1] + 1:
            out[-1][1] = cp
        else:
            out.append([cp, cp])
    return out

cased, ci = [], []
for line in open(sys.argv[2]):
    k, cp = line.split()
    (cased if k == 'C' else ci).append(int(cp, 16))
cased_r = ranges(cased)
ci_r = ranges(ci)
cased_tbl = ''.join(f'{a:06X}{b:06X}' for a, b in cased_r)
ci_tbl = ''.join(f'{a:06X}{b:06X}' for a, b in ci_r)

FINAL_SIGMA = f"""
// ── Final_Sigma (Unicode 3.13 rule, what str::to_lowercase does for Σ) ──
// Σ lowers to ς exactly when PRECEDED by a cased letter (skipping
// Case_Ignorable) and NOT FOLLOWED by one. The Cased / Case_Ignorable sets
// are range tables black-box-dumped from Rust itself ({len(cased_r)} + {len(ci_r)} ranges,
// 12 hex chars each = [start:6][end:6], binary-searched).

fn __to_lower_inr(th: Int, lo: Int, hi: Int, cp: Int) -> Bool =
  if lo >= hi then false
  else {{
    let mid = (lo + hi) / 2
    let a = __to_lower_rd6(th + mid * 12)
    let b = __to_lower_rd6(th + mid * 12 + 6)
    if cp < a then __to_lower_inr(th, lo, mid, cp)
    else if cp > b then __to_lower_inr(th, mid + 1, hi, cp)
    else true
  }}

// Step BACK over UTF-8 continuation bytes to the previous codepoint start.
fn __to_lower_prev(start: Int, i: Int) -> Int =
  if i > start and prim.load8(i - 1) >= 128 and prim.load8(i - 1) < 192 then __to_lower_prev(start, i - 1)
  else i - 1

// Scanning BACKWARD from just before Σ: skip Case_Ignorable; true iff the
// first non-ignorable codepoint is Cased.
fn __to_lower_bck(start: Int, i: Int, ch: Int, cn: Int, ih: Int, inn: Int) -> Bool =
  if i <= start then false
  else {{
    let ps = __to_lower_prev(start, i)
    let (cp, l) = __to_lower_cp(ps)
    if __to_lower_inr(ih, 0, inn, cp) then __to_lower_bck(start, ps, ch, cn, ih, inn)
    else __to_lower_inr(ch, 0, cn, cp)
  }}

// Scanning FORWARD from just after Σ: skip Case_Ignorable; true iff the first
// non-ignorable codepoint is Cased.
fn __to_lower_fwd(p: Int, end: Int, ch: Int, cn: Int, ih: Int, inn: Int) -> Bool =
  if p >= end then false
  else {{
    let (cp, l) = __to_lower_cp(p)
    if __to_lower_inr(ih, 0, inn, cp) then __to_lower_fwd(p + l, end, ch, cn, ih, inn)
    else __to_lower_inr(ch, 0, cn, cp)
  }}
"""
open('stdlib/string_to_upper.almd', 'w').write(emit('to_upper', 'to_uppercase', upper, 97, 122, -32))
lower_body = emit('to_lower', 'to_lowercase', lower, 65, 90, 32, sigma=(len(cased_r), len(ci_r)))
lower_body = lower_body.replace('fn string_to_lower', FINAL_SIGMA + '\nfn string_to_lower')
open('stdlib/string_to_lower.almd', 'w').write(lower_body)
print('generated')
