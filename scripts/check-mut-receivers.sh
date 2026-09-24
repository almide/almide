#!/usr/bin/env bash
# MUT-RECEIVER MATRIX GATE (#2466).
#
# Almide has value semantics: `let c = a` copies (C-033), and a callee that
# writes into a buffer it received by value must not be able to reach the
# caller's binding. The opt-in for writing back is a `mut` parameter, and the
# checker enforces it with E032 (a `let` binding or a non-`mut` parameter
# passed where a `mut` parameter is declared).
#
# E032 can only fire where the surface SAYS `mut`. An intrinsic whose runtime
# function takes `&mut` as its first parameter writes through that parameter,
# so its surface must declare that parameter `mut` — otherwise the checker
# accepts `let b = ...; bytes.fill(b, 0)` and the targets disagree about
# whether the caller's value changed (#2466: native wrote through, wasm did
# not). The rule, in both directions:
#
#   runtime `pub fn almide_rt_X(p: &mut T, ...)`   <=>
#   surface `@intrinsic("almide_rt_X") fn f(mut p: T, ...)`
#
# A surface `mut` over a runtime that takes the receiver by value or `&` is
# the reverse drift: callers are forced into `var` for a write that never
# happens.
#
# Mechanics: every `@intrinsic("almide_rt_X")` in stdlib/*.almd is joined to
# the `pub fn almide_rt_X` definition in runtime/rs/src/*.rs. Both sides may
# span lines (a signature split one parameter per line, attributes between
# the `@intrinsic` line and the `fn` head), so each side is parsed from the
# head to its closing parenthesis, not line-wise. Intrinsics with no runtime
# definition (compiler-lowered prims) have no first-parameter fact to compare
# and are skipped.
#
# The two source roots are overridable (MUT_RECEIVERS_STDLIB /
# MUT_RECEIVERS_RUNTIME) so a negative control can feed the gate a forged
# tree; the defaults are the real ones. `--list` prints every `&mut`-first row.
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."

python3 - "$@" <<'PY'
import os, re, sys

# ── Explicit exemptions ────────────────────────────────────────────────────
# Each entry: runtime fn name -> why the surface and runtime may disagree.
# Keep this list short and every entry justified by what the CHECKER needs.
ALLOW = {
    # `bytes.as_mut_ptr` hands out a raw pointer for FFI. The runtime needs
    # `&mut Vec<u8>` only because `Vec::as_mut_ptr` is a `&mut self` method;
    # the call itself does not write the buffer — the foreign code that later
    # receives the pointer may. Declaring the surface `mut` would force every
    # FFI caller into `var` for a write the call does not perform, and it
    # would not make the foreign write visible to the checker either (the
    # pointer escapes the call). The surface keeps `@mutating` instead, which
    # is what the codegen passes read to keep the buffer's storage unique.
    "almide_rt_bytes_as_mut_ptr": "raw-pointer escape: the call does not write; @mutating carries the uniqueness requirement",
}

def strip_comments(src, style):
    if style == "rs":
        src = re.sub(r"/\*.*?\*/", "", src, flags=re.S)
    return re.sub(r"//[^\n]*", "", src)

def balanced_params(src, open_idx):
    """Return the text between the '(' at open_idx and its matching ')'."""
    depth = 0
    for i in range(open_idx, len(src)):
        c = src[i]
        if c == "(":
            depth += 1
        elif c == ")":
            depth -= 1
            if depth == 0:
                return src[open_idx + 1:i]
    return None

def first_param(params):
    """First top-level comma-separated parameter (generics/tuples aware)."""
    depth = 0
    for i, c in enumerate(params):
        if c in "([<{":
            depth += 1
        elif c in ")]>}":
            depth -= 1
        elif c == "," and depth == 0:
            return params[:i].strip()
    return params.strip()

# 1. Runtime side: almide_rt_X -> does the first parameter have type `&mut`?
runtime = {}   # name -> (first_is_mut_ref, "file:line")
rt_head = re.compile(r"\bpub\s+fn\s+(almide_rt_[A-Za-z0-9_]+)\s*(<[^()]*?>)?\s*\(")
rt_dir = os.environ.get("MUT_RECEIVERS_RUNTIME", "runtime/rs/src")
for fname in sorted(os.listdir(rt_dir)):
    if not fname.endswith(".rs"):
        continue
    raw = open(os.path.join(rt_dir, fname)).read()
    src = strip_comments(raw, "rs")
    for m in rt_head.finditer(src):
        params = balanced_params(src, m.end() - 1)
        if params is None:
            continue
        fp = first_param(params)
        is_mut = bool(re.match(r"^(mut\s+)?[A-Za-z_][A-Za-z0-9_]*\s*:\s*&\s*(?:'[A-Za-z_]+\s+)?mut\b", fp))
        line = src.count("\n", 0, m.start()) + 1
        runtime.setdefault(m.group(1), (is_mut, f"{rt_dir}/{fname}:{line}"))

# 2. Surface side: @intrinsic("almide_rt_X") ... fn f[..](FIRST, ...) -> is FIRST `mut`?
surface = []   # (rt_name, surface_fn, first_is_mut, "file:line")
intr = re.compile(r'@intrinsic\(\s*"(almide_rt_[A-Za-z0-9_]+)"\s*\)')
fn_head = re.compile(r"\b(?:effect\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)\s*(\[[^\]]*\])?\s*\(")
stdlib_dir = os.environ.get("MUT_RECEIVERS_STDLIB", "stdlib")
for fname in sorted(os.listdir(stdlib_dir)):
    if not fname.endswith(".almd"):
        continue
    src = strip_comments(open(os.path.join(stdlib_dir, fname)).read(), "almd")
    for m in intr.finditer(src):
        h = fn_head.search(src, m.end())
        if h is None:
            continue
        # The fn head must follow the attribute block directly (only further
        # `@attr(...)` lines between them), not a later, unrelated fn.
        between = src[m.end():h.start()]
        if re.sub(r"@[A-Za-z_]+(\([^)]*\))?", "", between).strip():
            continue
        params = balanced_params(src, h.end() - 1)
        if params is None:
            continue
        fp = first_param(params)
        is_mut = bool(re.match(r"^mut\s", fp))
        line = src.count("\n", 0, h.start()) + 1
        surface.append((m.group(1), f"{fname[:-5]}.{h.group(1)}", is_mut, f"{stdlib_dir}/{fname}:{line}"))

# 3. Join and compare.
missing_mut, extra_mut, joined, allowed_hit = [], [], 0, set()
for rt, sfn, s_mut, where in surface:
    if rt not in runtime:
        continue
    joined += 1
    r_mut, rt_where = runtime[rt]
    if r_mut == s_mut:
        continue
    if rt in ALLOW:
        allowed_hit.add(rt)
        continue
    (missing_mut if r_mut else extra_mut).append((sfn, rt, where, rt_where))

stale = sorted(set(ALLOW) - allowed_hit)
mut_rows = sum(1 for rt, _, s, _ in surface if rt in runtime and runtime[rt][0])

if "--list" in sys.argv[1:]:
    for rt, sfn, s_mut, where in surface:
        if rt in runtime and runtime[rt][0]:
            print(f"{'mut ' if s_mut else '    '} {sfn:32} {rt:40} {where}")

fail = False
if missing_mut:
    fail = True
    print("MUT-RECEIVER FAIL — runtime takes `&mut` first, surface parameter is not `mut` "
          "(the checker cannot raise E032 for a `let` or non-`mut` caller):")
    for sfn, rt, where, rt_where in missing_mut:
        print(f"  + {sfn}  ({where})  ->  {rt}  ({rt_where})")
if extra_mut:
    fail = True
    print("MUT-RECEIVER FAIL — surface declares `mut`, runtime does not take `&mut` "
          "(callers are forced into `var` for a write that never happens):")
    for sfn, rt, where, rt_where in extra_mut:
        print(f"  + {sfn}  ({where})  ->  {rt}  ({rt_where})")
if stale:
    fail = True
    print("MUT-RECEIVER FAIL — allowlist entries that no longer disagree (drop them):")
    for rt in stale:
        print(f"  + {rt}")
if fail:
    sys.exit(1)
print(f"mut-receivers OK: {joined} intrinsic(s) joined to runtime/rs; "
      f"{mut_rows} take `&mut` first — {mut_rows - len(allowed_hit)} declare it `mut` on the "
      f"surface, {len(allowed_hit)} allowlisted; every other surface receiver is plain")
PY
