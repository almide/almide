#!/usr/bin/env python3
"""Hand-edited variants of an emitted snorm module (#4611 probes).

usage: probe.py WASM OUTDIR
Writes OUTDIR/<variant>.wasm for each probe, each from the module's WAT text
with one or more edits applied. Every edit must match at least once.
"""
import re, subprocess, sys, os

WT = os.environ.get("WASM_TOOLS", "wasm-tools")

# The inner loop's per-iteration bounds check (label @5 is the inner loop's
# depth in mul_av / mul_atv): `local.tee X; local.get CNT; i64.ge_u; if ..
# abort .. end` becomes `local.set X`.
NOBC = (re.compile(r"local\.tee (\d+)\n\s*local\.get \d+\n\s*i64\.ge_u\n\s*if ;; label = @5\n(?:[^\n]*\n)*?\s*unreachable\n\s*end\n"),
        lambda m: "local.set %s\n" % m.group(1))
# Signed div by 2 of a value read twice from one local -> shr_u.
SHR = (re.compile(r"local\.get (\d+)\n\s*local\.get \1\n\s*i64\.const 63\n\s*i64\.shr_s\n\s*i64\.const 63\n\s*i64\.shr_u\n\s*i64\.add\n\s*i64\.const 1\n\s*i64\.shr_s\n"),
       lambda m: "local.get %s\ni64.const 1\ni64.shr_u\n" % m.group(1))
# `a + b` computed twice -> once, kept in the product's own local.
CSE = (re.compile(r"local\.get (\d+)\n\s*local\.get (\d+)\n\s*i64\.add\n\s*local\.get \1\n\s*local\.get \2\n\s*i64\.add\n\s*i64\.const 1\n\s*i64\.add\n\s*i64\.mul\n\s*local\.set (\d+)\n"),
       lambda m: "local.get %s\nlocal.get %s\ni64.add\nlocal.tee %s\nlocal.get %s\ni64.const 1\ni64.add\ni64.mul\nlocal.set %s\n" % (m.group(1), m.group(2), m.group(3), m.group(3), m.group(3)))

# Only the bias simplification: `(x >>s 63) >>u 63` is `x >>u 63`.
BIAS = (re.compile(r"i64\.const 63\n\s*i64\.shr_s\n\s*i64\.const 63\n\s*i64\.shr_u\n"),
        lambda m: "i64.const 63\ni64.shr_u\n")

EDITS = {"nobc": NOBC, "shr": SHR, "cse": CSE, "bias": BIAS}
VARIANTS = {
    "p_nobc": ["nobc"],
    "p_shr": ["shr"],
    "p_bias": ["bias"],
    "p_cse": ["cse"],
    "p_nobc_shr": ["nobc", "shr"],
    "p_all3": ["nobc", "shr", "cse"],
}


def main():
    src, out = sys.argv[1], sys.argv[2]
    wat = subprocess.run([WT, "print", src], check=True, capture_output=True, text=True).stdout
    for name, edits in VARIANTS.items():
        t = wat
        for e in edits:
            rx, rep = EDITS[e]
            t, n = rx.subn(rep, t)
            print("%s: edit %s matched %d site(s)" % (name, e, n))
            if n == 0:
                print("  (no match: variant skipped)")
                break
        else:
            p = os.path.join(out, name + ".wat")
            open(p, "w").write(t)
            subprocess.run([WT, "parse", p, "-o", os.path.join(out, name + ".wasm")], check=True)


if __name__ == "__main__":
    main()
