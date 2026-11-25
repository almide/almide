#!/usr/bin/env bash
# A measurement, not a gate: the Rocq kernel's verdict (proofs/GuardWindow.v,
# `window_ok`) on EVERY byte range of every function body of every module the
# byte-level check's own tests hand to its decoder. Among them are the
# hand-written modules that reproduce the acceptances the check once gave
# wrongly (tests/guard_soundness.rs, memories.rs, count_word.rs,
# unclassified_control_flow.rs, fields.rs, rc.rs). No cutter chooses the
# ranges and the check's decoder refuses nothing here: a module is read for
# its code section only, and a range is any run of consecutive bytes of one
# function body, the declaration of its locals included.
#
#   scripts/probe-unsound-inputs.sh <work-dir>
#
# A module whose bodies give more than RANGE_LIMIT ranges is asked only about
# the ranges that end in a load or store opcode followed by two LEB128
# numbers (the only strings `dec` can return an access for); the table says
# which modules were asked which way. Also records what the stock cutters
# propose for each module (`--windows`, `--loops`) and the kernel's
# `loop_class` for every loop proposal.
#
# Writes <work-dir>/unsound-inputs.tsv (one row per module),
# <work-dir>/accepted.tsv (one row per accepted range) and
# <work-dir>/summary.txt.
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
ROOT="$PWD"
WORK="${1:?usage: scripts/probe-unsound-inputs.sh <work-dir>}"
mkdir -p "$WORK/modules" "$WORK/q" "$WORK/cut"
WORK="$(cd "$WORK" && pwd)"
COQC="${COQC:-$(command -v coqc)}"
CUT="${CUT:-$ROOT/target/release/almide-anchor-check}"

# 1. Every module the tests decode, named after the test that built it.
PROBE_DUMP_DIR="$WORK/modules" cargo test --release --locked -p almide-anchor-check > "$WORK/tests.log" 2>&1
echo "the crate's tests: exit $?"
grep -E '^test result' "$WORK/tests.log" | awk '{ p += $4; f += $6 } END { printf "tests passed %d, failed %d\n", p, f }'
echo "$(find "$WORK/modules" -name '*.wasm' | wc -l | tr -d ' ') module(s) dumped"
[ -n "$(find "$WORK/modules" -name '*.wasm' | head -n 1)" ] || { echo "no module was dumped: the hook did not run"; exit 1; }

# 2. What the stock cutters propose for each.
printf 'almide-anchors 1\nalloc -\ninit\nbounds\n' > "$WORK/no-anchors"
find "$WORK/modules" -name '*.wasm' | sort > "$WORK/modules.txt"
while read -r m; do
  n="$(basename "$m" .wasm)"
  "$CUT" --windows "$m" > "$WORK/cut/$n.windows" 2> /dev/null || true
  "$CUT" --loops "$m" "$WORK/no-anchors" > "$WORK/cut/$n.loops" 2> /dev/null || true
done < "$WORK/modules.txt"

# 3. The questions.
if ! (cd "$ROOT/proofs" \
  && "$COQC" -Q . AlmideTrust GuardWindow.v > "$WORK/GuardWindow.log" 2>&1 \
  && "$COQC" -Q . AlmideTrust GuardFresh.v > "$WORK/GuardFresh.log" 2>&1 \
  && "$COQC" -Q . AlmideTrust GuardLoop.v > "$WORK/GuardLoop.log" 2>&1); then
  echo "the proof files do not compile"
  head -n 20 "$WORK/GuardWindow.log" "$WORK/GuardFresh.log" "$WORK/GuardLoop.log" 2> /dev/null
  exit 1
fi
grep -m1 'Closed under the global context' "$WORK/GuardWindow.log" || { echo "window_sound is not closed"; exit 1; }

python3 -I - "$WORK" "${RANGE_LIMIT:-60000}" "${Q_CHUNK:-2000}" "${STRING_LIMIT:-3000000}" <<'PY'
import glob, os, re, sys
work, limit, chunk, cap = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]), int(sys.argv[4])

def uleb(b, p):
    v = s = 0
    while True:
        x = b[p]; p += 1
        v |= (x & 127) << s; s += 7
        if x < 128:
            return v, p

def bodies(b):
    """(start, end) of every function body of the code section, or None."""
    if b[:8] != b"\0asm\1\0\0\0":
        return None
    out, p = [], 8
    try:
        while p < len(b):
            sid = b[p]
            size, q = uleb(b, p + 1)
            if sid == 10:
                n, r = uleb(b, q)
                for _ in range(n):
                    bs, r2 = uleb(b, r)
                    out.append((r2, r2 + bs))
                    r = r2 + bs
            p = q + size
    except IndexError:
        return None
    return out

def ends_in_access(b, s, e):
    """Some opcode 0x28..0x3e in [s, e) is followed by exactly two LEB128 numbers that end at e."""
    for p in range(max(s, e - 11), e - 2):
        if 40 <= b[p] <= 62:
            q, k = p + 1, 0
            while q < e and k < 2:
                while q < e and b[q] >= 128:
                    q += 1
                if q >= e:
                    break
                q += 1; k += 1
            if k == 2 and q == e:
                return True
    return False

ids, mods = {}, []
for path in sorted(glob.glob(os.path.join(work, "modules", "*.wasm"))):
    name = os.path.basename(path)[:-5]
    exe, test, _ = name.rsplit(".", 2)
    exe = re.sub(r"-[0-9a-f]{16}$", "", exe)
    b = open(path, "rb").read()
    bd = bodies(b)
    if bd is None:
        mods.append((exe, test, name, "not a module", 0, 0, "-", []))
        continue
    total = sum((e - s) * (e - s + 1) // 2 for s, e in bd)
    mode = "every range" if total <= limit else "ranges that end in an access"
    asked = []
    for f, (s0, e0) in enumerate(bd):
        for s in range(s0, e0):
            for e in range(s + 1, e0 + 1):
                if mode != "every range" and not ends_in_access(b, s, e):
                    continue
                key = bytes(b[s:e])
                i = ids.setdefault(key, len(ids))
                asked.append((i, f, s - s0, e - s0))
    mods.append((exe, test, name, mode, len(bd), sum(e - s for s, e in bd), total, asked))
if len(ids) > cap:
    sys.exit(f"{len(ids)} distinct strings is more than the limit {cap}: nothing was asked")
HEAD = "From AlmideTrust Require Import GuardWindow.\nFrom Stdlib Require Import ZArith.\nFrom Stdlib Require Import List.\nImport ListNotations.\nOpen Scope Z_scope.\n"
order = sorted(ids.items(), key=lambda kv: kv[1])
for k in range(0, len(order), chunk):
    with open(os.path.join(work, "q", f"Q_{k // chunk:05d}.v"), "w", encoding="utf-8") as f:
        f.write(HEAD)
        for key, _ in order[k:k + chunk]:
            f.write("Eval vm_compute in (window_ok [" + ";".join(str(x) for x in key) + "]).\n")
with open(os.path.join(work, "strings.tsv"), "w", encoding="utf-8") as f:
    for key, i in order:
        f.write(f"{i}\t{' '.join(str(x) for x in key)}\n")
with open(os.path.join(work, "asked.tsv"), "w", encoding="utf-8") as f:
    for exe, test, name, mode, nf, nb, total, asked in mods:
        f.write(f"M\t{exe}\t{test}\t{name}\t{mode}\t{nf}\t{nb}\t{total}\t{len(asked)}\n")
        for i, fn, s, e in asked:
            f.write(f"A\t{i}\t{fn}\t{s}\t{e}\n")
print(f"{len(mods)} module(s); {sum(len(m[7]) for m in mods)} range(s) asked, {len(ids)} distinct string(s), in {(len(order) + chunk - 1) // chunk} file(s)")
print("asked every range in", sum(1 for m in mods if m[3] == "every range"), "module(s); only ranges that end in an access in", sum(1 for m in mods if m[3] == "ranges that end in an access"), "; not a module:", sum(1 for m in mods if m[3] == "not a module"))

# the loop proposals of the stock cutter, one question each
loops = []
for path in sorted(glob.glob(os.path.join(work, "cut", "*.loops"))):
    name = os.path.basename(path)[:-6]
    for line in open(path, encoding="utf-8", errors="replace"):
        if line.startswith("LOOP "):
            parts = [p.strip() for p in line[5:].split("|")]
            if len(parts) != 6:
                sys.exit(f"a LOOP line does not have six parts: {line[:100]}")
            func, acc, inc = parts[0].split()
            loops.append((name, func, acc, inc, parts[1:]))
LH = "From AlmideTrust Require Import GuardWindow GuardFresh GuardLoop.\nFrom Stdlib Require Import ZArith.\nFrom Stdlib Require Import List.\nImport ListNotations.\nOpen Scope Z_scope.\n"
lst = lambda s: "[" + ";".join(s.split()) + "]"
with open(os.path.join(work, "Loops.v"), "w", encoding="utf-8") as f:
    f.write(LH)
    for name, func, acc, inc, (hold, pre, mid, ht, rest) in loops:
        f.write(f"Eval vm_compute in (loop_class {inc} {lst(hold)} {lst(pre)} {lst(mid)} {lst(ht)} {lst(rest)}).\n")
with open(os.path.join(work, "loops-index.tsv"), "w", encoding="utf-8") as f:
    for name, func, acc, inc, _ in loops:
        f.write(f"{name}\t{func}\t{acc}\n")
print(f"{len(loops)} loop proposal(s) by the stock cutter")
PY
[ $? = 0 ] || exit 1

export COQC ROOT WORK
t0=$(date +%s)
find "$WORK/q" -name 'Q_*.v' | sort \
  | xargs -P "${Q_JOBS:-4}" -I{} bash -c 'c="${1%.v}"; timeout "${Q_SECONDS:-1800}" "$COQC" -Q "$ROOT/proofs" AlmideTrust "$1" > "$c.out" 2> "$c.err" || echo "FAILED $1: $(head -c 300 "$c.err" | tr "\n" " ")"' _ {}
"$COQC" -Q "$ROOT/proofs" AlmideTrust "$WORK/Loops.v" > "$WORK/Loops.out" 2> "$WORK/Loops.err" || { echo "the loop questions failed"; head -n 10 "$WORK/Loops.err"; }
echo "kernel time: $(( $(date +%s) - t0 )) s"

# 4. The answers, per module.
python3 -I - "$WORK" <<'PY' | tee "$WORK/summary.txt"
import collections, glob, os, sys
work = sys.argv[1]
strings = {}
for l in open(os.path.join(work, "strings.tsv"), encoding="utf-8"):
    i, s = l.rstrip("\n").split("\t")
    strings[int(i)] = s
vals = []
for p in sorted(glob.glob(os.path.join(work, "q", "Q_*.out"))):
    vals += [l.split("=", 1)[1].strip() for l in open(p, encoding="utf-8") if l.strip().startswith("= ")]
if len(vals) != len(strings):
    sys.exit(f"{len(vals)} verdict(s) for {len(strings)} string(s): a file gave no answer, and nothing is reported")
bad = set(vals) - {"true", "false"}
if bad:
    sys.exit(f"a verdict is neither true nor false: {sorted(bad)[:3]}")
ok = {i for i, v in enumerate(vals) if v == "true"}
def cut(name, ext, key):
    p = os.path.join(work, "cut", name + ext)
    return sum(1 for l in open(p, encoding="utf-8", errors="replace") if l.startswith(key)) if os.path.exists(p) else 0
def readable(name, ext, key):
    p = os.path.join(work, "cut", name + ext)
    return os.path.exists(p) and any(l.startswith(key) for l in open(p, encoding="utf-8", errors="replace"))
lv = [l.split("=", 1)[1].strip() for l in open(os.path.join(work, "Loops.out"), encoding="utf-8") if l.strip().startswith("= ")] if os.path.exists(os.path.join(work, "Loops.out")) else []
li = [l.rstrip("\n").split("\t") for l in open(os.path.join(work, "loops-index.tsv"), encoding="utf-8")]
if len(lv) != len(li):
    sys.exit(f"{len(lv)} loop verdict(s) for {len(li)} proposal(s)")
loopv = collections.defaultdict(list)
for (name, func, acc), v in zip(li, lv):
    loopv[name].append(v)
rows, acc_rows, cur = [], [], None
for l in open(os.path.join(work, "asked.tsv"), encoding="utf-8"):
    t = l.rstrip("\n").split("\t")
    if t[0] == "M":
        cur = {"exe": t[1], "test": t[2], "name": t[3], "mode": t[4], "functions": t[5], "body_bytes": t[6], "ranges": t[7], "asked": t[8], "acc": []}
        rows.append(cur)
    else:
        i = int(t[1])
        if i in ok:
            cur["acc"].append((t[2], t[3], t[4], i))
with open(os.path.join(work, "unsound-inputs.tsv"), "w", encoding="utf-8") as f:
    f.write("test_file\ttest\tmodule\tasked\tfunctions\tbody_bytes\tranges_of_the_bodies\tranges_asked\tranges_accepted\taccepted_accesses\tcutter_reads_module\tcutter_windows\tcutter_loop_proposals\tloop_classes\n")
    for r in rows:
        ends = sorted({(a[0], a[2]) for a in r["acc"]})
        f.write("\t".join([r["exe"], r["test"], r["name"], r["mode"], r["functions"], r["body_bytes"], r["ranges"], r["asked"], str(len(r["acc"])),
                           " ".join(f"{fn}:{e}" for fn, e in ends) or "-",
                           "yes" if readable(r["name"], ".windows", "ACCESSES") else "no",
                           str(cut(r["name"], ".windows", "WINDOW ")), str(cut(r["name"], ".loops", "LOOP ")),
                           " ".join(loopv.get(r["name"], [])) or "-"]) + "\n")
with open(os.path.join(work, "accepted.tsv"), "w", encoding="utf-8") as f:
    f.write("test_file\ttest\tmodule\tfunction\tstart\tend\tbytes\n")
    for r in rows:
        for fn, s, e, i in r["acc"]:
            f.write("\t".join([r["exe"], r["test"], r["name"], fn, s, e, strings[i]]) + "\n")
print(f"{len(rows)} module(s), {sum(int(r['asked']) for r in rows)} range(s) asked, {len(strings)} distinct; the kernel accepts {len(ok)} distinct string(s)")
by = collections.defaultdict(lambda: [0, 0, 0, 0])
for r in rows:
    b = by[r["exe"]]
    b[0] += 1; b[1] += int(r["asked"]); b[2] += 1 if r["acc"] else 0; b[3] += len({(a[0], a[2]) for a in r["acc"]})
for exe in sorted(by):
    b = by[exe]
    print(f"   {exe}: {b[0]} module(s), {b[1]} range(s) asked; an accepted range in {b[2]} module(s), {b[3]} distinct access(es)")
print("modules with an accepted range, by test (accesses as function:end offset in the body):")
seen = collections.OrderedDict()
for r in rows:
    if r["acc"]:
        k = (r["exe"], r["test"])
        seen.setdefault(k, set()).update((a[0], a[2]) for a in r["acc"])
for (exe, test), ends in seen.items():
    print(f"   {exe} {test}: {' '.join(f'{fn}:{e}' for fn, e in sorted(ends))}")
print("loop proposals by the stock cutter:", len(li), "; kernel classes:", dict(collections.Counter(lv)))
PY
