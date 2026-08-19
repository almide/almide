#!/usr/bin/env bash
# CAPABILITY / EFFECT-DECLARATION CONSISTENCY GATE (#1515).
#
# The effect axis (E006: a pure fn cannot call an effect fn) and the
# capability axis (cap witnesses: which OS floors a body reaches) must AGREE
# on the NONDETERMINISM floor: any public stdlib fn whose implementation
# transitively reaches `prim.clock_time_get` (Capability::Clock) or
# `prim.random_get` (Capability::Entropy) must be DECLARED `effect fn` in its
# public surface. Without this, a plain fn reading the wall clock makes the
# purity claim ("pure = deterministic") silently false — the hole #1515
# closed by effect-declaring `datetime.now` / `datetime.monotonic_ns`
# (`env.millis` and the whole `random.*` surface already declared).
#
# The cohort holds this without exception (koka `now() : <ndet,utc>`, lean4
# `BaseIO`, roc `!`); this gate keeps it held here. Stdout is deliberately
# OUT of scope: printing is not nondeterminism, and the plain-`fn main` +
# `println` corpus convention is a design decision, not a leak.
#
# Mechanics: parse the bundled stdlib (fn defs + call edges, regex-grade —
# names are unique enough at this granularity), compute transitive reach of
# the two nondet prims, map self-host impl fns to their PUBLIC dotted names
# through the self-host registry rows, and require `effect fn` on the public
# declaration of every reaching name.
set -uo pipefail
export LC_ALL=C
cd "$(git rev-parse --show-toplevel)"

python3 - <<'PY'
import os, re, sys

NONDET_PRIMS = {"prim.clock_time_get", "prim.random_get"}

# 1. Parse every bundled stdlib source: fn defs (name, effect?) and call edges.
defs = {}      # fn name -> (is_effect, file)
calls = {}     # fn name -> set of called names (bare and dotted)
fn_head = re.compile(r"^(effect\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)\s*[\(\[]", re.M)
call_rx = re.compile(r"\b([a-z_][A-Za-z0-9_]*(?:\.[a-z_][A-Za-z0-9_]*)?)\s*\(")

for fname in sorted(os.listdir("stdlib")):
    if not fname.endswith(".almd"):
        continue
    src = open(os.path.join("stdlib", fname)).read()
    src = re.sub(r"//[^\n]*", "", src)
    heads = list(fn_head.finditer(src))
    for i, m in enumerate(heads):
        name = m.group(2)
        body = src[m.end():heads[i + 1].start() if i + 1 < len(heads) else len(src)]
        defs[name] = (bool(m.group(1)), fname)
        edges = set(call_rx.findall(body))
        calls.setdefault(name, set()).update(edges)

# 2. Transitive reach of the nondet prims across bundled fns.
memo = {}
def reaches(fn, stack=()):
    if fn in memo:
        return memo[fn]
    if fn in stack:
        return False
    hit = False
    for c in calls.get(fn, ()):
        if c in NONDET_PRIMS:
            hit = True
            break
        base = c.split(".")[-1]
        if base in defs and reaches(base, stack + (fn,)):
            hit = True
            break
    memo[fn] = hit
    return hit

# 3. Self-host registry: impl fn -> public dotted name.
reg_src = open("crates/almide-types/src/self_host_registry.rs").read()
pairs = re.findall(r'\("([a-z_][A-Za-z0-9_]*)",\s*"([a-z_]+\.[A-Za-z0-9_.]+)"\)', reg_src)

# 4. Public effect-ness: the module surface files declare `[effect ]fn name(`.
public_effect = {}   # "module.name" -> bool
for fname in sorted(os.listdir("stdlib")):
    if not fname.endswith(".almd"):
        continue
    module = fname[:-5]
    src = re.sub(r"//[^\n]*", "", open(os.path.join("stdlib", fname)).read())
    for m in fn_head.finditer(src):
        public_effect[f"{module}.{m.group(2)}"] = bool(m.group(1))

# The PUBLIC surface = module names the compiler registers (stdlib_info.rs's
# two arrays). Impl files (clock_now.almd, random_int.almd, ...) are internal:
# their fns are reachable only through registry dispatch, and a twin name that
# has no surface declaration is not source-nameable at all (`random.choice_str`
# is E002 — measured), so it cannot bypass E006; the surface name that
# dispatches to it carries the effect gate.
info = open("crates/almide-types/src/stdlib_info.rs").read()
arrays = re.findall(r"pub const (?:STDLIB_MODULES|BUNDLED_MODULES): &\[&str\] = &\[(.*?)\];", info, re.S)
public_modules = set(re.findall(r'"([a-z0-9_]+)"', " ".join(arrays)))

offenders = []
seen = set()
def check(public):
    if public in seen:
        return
    seen.add(public)
    if not public_effect.get(public, False):
        offenders.append(public)

# Registry-mapped self-host impls: gate the PUBLIC name when it is a real
# surface declaration; an internal twin (no surface decl) is unreachable from
# source and needs no row of its own.
for impl, public in pairs:
    if impl in defs and reaches(impl) and public in public_effect:
        check(public)
# Directly-declared surface fns whose own bundled body reaches a nondet prim.
for name, (is_eff, fname) in defs.items():
    module = fname[:-5]
    if module in public_modules and f"{module}.{name}" in public_effect and reaches(name):
        check(f"{module}.{name}")

reached = sorted(x for x in seen)
if offenders:
    print("CAP/EFFECT CONSISTENCY FAIL — nondeterminism (Clock/Entropy) reachable "
          "from a PLAIN public fn; declare it `effect fn` (the #1515 rule):")
    for o in sorted(offenders):
        print(f"  + {o}")
    sys.exit(1)
print(f"cap-effect-consistency OK: {len(reached)} public fn(s) reach the nondet floor "
      f"(clock_time_get / random_get), every one effect-declared")
PY
