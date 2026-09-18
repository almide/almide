#!/usr/bin/env bash
# Negative controls for the ratchet-separation gate's range form: prove that
# each commit is judged by the gate AT ITS PARENT (a rule never binds a commit
# made before it existed, and binds every commit after), that editing the gate
# is itself a verification-artifact move, and that a disposition closes only
# the commit it names, with exactly the files the law flagged, with a reviewer
# other than its author, and only for a commit already on the protected line.
#
# The gate reads its laws from GIT HISTORY, so every case is a commit in a
# throwaway repository. The OLD law is the real July version of the gate
# (71fc60536, before the parity goldens were verification artifacts); the NEW
# law is the working tree's gate. The real tree is never touched.
#
# RATCHET_GATE points the controls at another copy of the driver: the
# pre-replay driver (every commit judged by the newest law) must FAIL the
# non-retroactivity control.
set -euo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
root="$PWD"
DRIVER="${RATCHET_GATE:-$root/scripts/check-ratchet-separation.sh}"
case "$DRIVER" in /*) ;; *) DRIVER="$root/$DRIVER" ;; esac
OLD_LAW="$(git show 71fc60536:scripts/check-ratchet-separation.sh)"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
repo="$tmp/repo"
mkdir -p "$repo"
g() { git -C "$repo" -c user.name=negative -c user.email=negative@localhost -c commit.gpgsign=false "$@"; }
commit() { g add -A >/dev/null; g commit -qm "$1"; g rev-parse HEAD; }
edit() { mkdir -p "$(dirname "$repo/$1")"; printf '%s\n' "$2" >> "$repo/$1"; }
run() { (cd "$repo" && bash "$DRIVER" "$@") >"$tmp/out" 2>&1; }
pass() { if run "${@:2}"; then echo "ok   $1"; else echo "FAIL: $1 — expected green" >&2; cat "$tmp/out" >&2; exit 1; fi; }
fail() { if run "${@:2}"; then echo "FAIL: $1 — expected red" >&2; cat "$tmp/out" >&2; exit 1; else echo "ok   $1 (red)"; fi; }

g init -q -b main
mkdir -p "$repo/scripts"
printf '%s\n' "$OLD_LAW" > "$repo/scripts/check-ratchet-separation.sh"
edit crates/x/src/a.rs "fn a() {}"
edit crates/almide-syntax/tests/golden/m.txt "row 0"
c0=$(commit "old law and the files it judges")

# ── non-retroactivity ──────────────────────────────────────────────────────
edit crates/x/src/a.rs "fn b() {}"
edit crates/almide-syntax/tests/golden/m.txt "row 1"
c1=$(commit "implementation and a golden together, before goldens were artifacts")
pass "a commit made before a rule existed is judged by the law at its parent" "$c0" "$c1"

cp "$root/scripts/check-ratchet-separation.sh" "$repo/scripts/check-ratchet-separation.sh"
c2=$(commit "install the new law (a gate-only change)")
pass "installing the new law is judged by the old one, which does not classify the gate" "$c1" "$c2"

# ── the law binds every commit after it exists ─────────────────────────────
edit crates/x/src/a.rs "fn c() {}"
edit crates/almide-syntax/tests/golden/m.txt "row 2"
c3=$(commit "implementation and a golden together, after the rule")
fail "a commit made after the rule mixes implementation with a golden" "$c2" "$c3"
fail "the whole history still carries that commit" "$c0" "$c3"

# ── editing the gate is a verification-artifact move ───────────────────────
g checkout -q -b gate-edit "$c2"
sed -i.bak 's/crates\/almide-syntax\/tests\/golden\/\*|//' "$repo/scripts/check-ratchet-separation.sh" && rm -f "$repo/scripts/check-ratchet-separation.sh.bak"
edit crates/x/src/a.rs "fn d() {}"
d1=$(commit "weaken the gate inside an implementation commit")
fail "a gate edit cannot ride inside an implementation commit" "$c2" "$d1"

# ── dispositions ───────────────────────────────────────────────────────────
g checkout -q -b disp "$c2"
edit crates/x/src/a.rs "fn e() {}"
edit crates/almide-syntax/tests/golden/m.txt "row 3"
e1=$(commit "a violation that reached the protected line")
g branch -f protected "$e1"
record() { # <sha> <files> <author> <reviewer>
    mkdir -p "$repo/proofs"
    printf '%s %s %s %s the move was re-measured and is not a regression\n' "$1" "$2" "$3" "$4" > "$repo/proofs/ratchet-dispositions.txt"
}
disposed() { # <label> <expect pass|fail> <protected-ref or empty> <sha> <files> <author> <reviewer>
    g checkout -q -B "case" "$e1"
    record "$4" "$5" "$6" "$7"
    local e2; e2=$(commit "disposition record")
    if [ "$2" = pass ]; then RATCHET_PROTECTED="$3" pass "$1" "$c2" "$e2"; else RATCHET_PROTECTED="$3" fail "$1" "$c2" "$e2"; fi
}
full=$(g rev-parse "$e1")
golden=crates/almide-syntax/tests/golden/m.txt
disposed "a matching record closes a commit already on the protected line" pass protected "$full" "$golden" author-a reviewer-b
disposed "no protected line: no record is honoured" fail "" "$full" "$golden" author-a reviewer-b
disposed "a change set cannot close its own commit" fail "$c2" "$full" "$golden" author-a reviewer-b
disposed "a record naming other files does not close it" fail protected "$full" "crates/almide-syntax/tests/golden/other.txt" author-a reviewer-b
disposed "a record whose reviewer is its author does not close it" fail protected "$full" "$golden" author-a author-a
disposed "a record for another commit does not close it" fail protected "$(g rev-parse "$c3")" "$golden" author-a reviewer-b

g checkout -q -B "ledger-ride" "$e1"
record "$full" "$golden" author-a reviewer-b
edit crates/x/src/a.rs "fn f() {}"
f1=$(commit "a disposition record inside an implementation commit")
RATCHET_PROTECTED=protected fail "a disposition record cannot ride inside an implementation commit" "$e1" "$f1"

echo "ratchet-separation negative controls: 3 positive + 9 negative all behaved"
