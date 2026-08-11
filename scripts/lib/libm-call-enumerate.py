#!/usr/bin/env python3
"""Enumerate the PLATFORM-libm call units (#1197, the determinism contract).

Almide vendors musl-libm (`runtime/rs/src/libm.rs`, the StrictMath/fdlibm
decision) so a transcendental is bit-identical native <-> wasm AND across host
platforms. A call to Rust's own `f64::exp`/`powf`/... escapes that contract:
its last ULP is the host libm's. #1197 was exactly that — `matrix.pow` printed
`…82` on native and `…84` on wasm, live on released 0.56.0, and a HAND grep for
it missed two more sites (`mha_core`, `silu_mul`). Hence: one machine-run
enumeration, feeding both the audit ledger and its gate
(scripts/check-libm-determinism.sh) — the #1176 one-instrument rule.

A unit is a FUNCTION containing at least one platform transcendental call.
`sqrt`/`abs` are NOT scanned: IEEE-754 makes them correctly rounded and
therefore identical on every platform and equal to the wasm opcodes.

Output: `file :: fn :: count`, sorted.
"""
import glob
import re
import sys

# The platform transcendental surface. `powi` is included deliberately: it is
# compiler-expanded multiplication rather than a libm call, so it is expected to
# classify as IEEE-exact — but it must be CLASSIFIED, not assumed.
CALL = re.compile(
    r"\.(exp|exp2|expm1|ln|log|log2|log10|log1p|sin|cos|tan|asin|acos|atan|"
    r"atan2|sinh|cosh|tanh|powf|powi|cbrt|hypot)\s*\("
)
FN = re.compile(
    r"\s*(?:#\[[^\]]*\]\s*)?(?:pub(?:\(crate\))?\s+)?(?:unsafe\s+)?fn\s+([A-Za-z_0-9]+)"
)


def scanned_files(root: str):
    """The shipped-behavior surface: the native runtime and the SIMD kernel.
    `libm*.rs` is the vendored reference itself — it DEFINES the algorithms, so
    scanning it would report the implementation as a violation of itself."""
    out = []
    for pat in ("runtime/rs/src/*.rs", "crates/almide-kernel/src/*.rs"):
        for p in sorted(glob.glob(f"{root}/{pat}")):
            if p.split("/")[-1].startswith("libm"):
                continue
            out.append(p)
    return out


def enumerate_units(root: str):
    units = {}
    for path in scanned_files(root):
        rel = path[len(root) + 1 :]
        fn = None
        for line in open(path, encoding="utf-8"):
            m = FN.match(line)
            if m:
                fn = m.group(1)
            if line.lstrip().startswith("//"):
                continue
            if CALL.search(line):
                key = (rel, fn or "?")
                units[key] = units.get(key, 0) + 1
    return units


if __name__ == "__main__":
    root = sys.argv[1] if len(sys.argv) > 1 else "."
    for (f, fn), n in sorted(enumerate_units(root).items()):
        print(f"{f} :: {fn} :: {n}")
