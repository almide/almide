#!/usr/bin/env bash
# Probe only, not a gate: does the kernel's kind pass (proofs/GuardKind.v)
# place in a header exactly the accesses the reference pass places, for every
# function of every contract program as it ships?
#
#   scripts/probe-guard-kind.sh <kept-dir> <work-dir>
#
# <kept-dir>: the output of scripts/probe-keep-corpus.sh (m/<name>.wasm with
# the anchors and the cutter's proposals beside it). The reference pass
# (scripts/probe-guard-kind/kinds_ref.py, untrusted) chooses the claims of
# each module and writes one Rocq file per module: the claims as a `ctx`, and
# per function the goal `header_ends c f wl res body = Some [...]`. The
# kernel computes each. A file that does not compile is a function where the
# two passes differ, or a body the kernel pass does not read.
#
# Writes <work-dir>/summary.txt, results.tsv (file, module, functions, header
# accesses, seconds, ok | FAILED) and keeps each failed file's messages.
# Needs coqc on PATH.
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
ROOT="$PWD"
KEPT="${1:?usage: scripts/probe-guard-kind.sh <kept-dir> <work-dir>}"
WORK="${2:?usage: scripts/probe-guard-kind.sh <kept-dir> <work-dir>}"
mkdir -p "$WORK"
WORK="$(cd "$WORK" && pwd)"
PROOFS="$ROOT/proofs"
G="$WORK/goals"

if ! (cd proofs && coqc -Q . AlmideTrust GuardWindow.v && coqc -Q . AlmideTrust GuardKind.v) > "$WORK/coqc.log" 2>&1; then
  echo "the proofs do not compile:"; tail -n 60 "$WORK/coqc.log"; exit 1
fi
echo "== the proofs compile"

t0=$(date +%s)
python3 -I scripts/probe-guard-kind/kinds_ref.py goals "$KEPT/m" "$G" ${ONLY_FIRST:+--limit "$ONLY_FIRST"} || exit 1
echo "== goals written in $(( $(date +%s) - t0 )) s: $(find "$G" -name 'K_*.v' | wc -l | tr -d ' ') file(s), $(du -sm "$G" | cut -f1) MB"

one() {
  local v="$1" c t1
  c="${v%.v}"
  t1=$(date +%s)
  if coqc -Q "$PROOFS" AlmideTrust "$v" > "$c.out" 2> "$c.err"; then echo 0 > "$c.rc"; else echo 1 > "$c.rc"; fi
  echo $(( $(date +%s) - t1 )) > "$c.sec"
}
export -f one
export PROOFS
t0=$(date +%s)
find "$G" -maxdepth 1 -name 'K_*.v' | sort | xargs -P "${JOBS:-4}" -I{} bash -c 'one "$1"' _ {}
echo "== the kernel answered in $(( $(date +%s) - t0 )) s"

python3 -I - "$G" "$WORK" <<'PY'
import os, sys
g, work = sys.argv[1], sys.argv[2]
rows, bad = [], []
tot = {"modules": 0, "functions": 0, "header": 0, "seconds": 0, "ok": 0, "unread": 0}
for line in open(os.path.join(g, "index.tsv"), encoding="utf-8"):
    f, name, nfun, nhdr, unread = line.rstrip("\n").split("\t")
    rc = open(os.path.join(g, f + ".rc")).read().strip() if os.path.exists(os.path.join(g, f + ".rc")) else "?"
    sec = open(os.path.join(g, f + ".sec")).read().strip() if os.path.exists(os.path.join(g, f + ".sec")) else "0"
    ok = rc == "0"
    rows.append((f, name, nfun, nhdr, sec, "ok" if ok else "FAILED"))
    tot["modules"] += 1
    tot["seconds"] += int(sec or 0)
    tot["unread"] += int(unread)
    if ok:
        tot["ok"] += 1
        tot["functions"] += int(nfun)
        tot["header"] += int(nhdr)
    else:
        bad.append(f)
with open(os.path.join(work, "results.tsv"), "w", encoding="utf-8") as fh:
    fh.write("file\tmodule\tfunctions\theader accesses\tseconds\tverdict\n")
    for r in rows:
        fh.write("\t".join(r) + "\n")
slow = sorted(rows, key=lambda r: -int(r[4] or 0))[:5]
with open(os.path.join(work, "summary.txt"), "w", encoding="utf-8") as fh:
    def say(s):
        print(s); fh.write(s + "\n")
    say(f"modules {tot['modules']}; the kernel agrees with the reference pass on every function of {tot['ok']}")
    say(f"in those: functions {tot['functions']}, header accesses {tot['header']}")
    say(f"functions the reference pass did not read: {tot['unread']}")
    say(f"kernel seconds, summed over files: {tot['seconds']}; slowest: " + ", ".join(f"{r[1]} {r[4]} s" for r in slow))
    say(f"files that did not compile: {len(bad)}")
    for f in bad[:20]:
        err = open(os.path.join(g, f + ".err"), encoding="utf-8", errors="replace").read().strip().splitlines()
        say(f"   {f}: " + " | ".join(l.strip()[:160] for l in err[:4]))
sys.exit(1 if bad or tot["modules"] == 0 else 0)
PY
