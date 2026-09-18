#!/usr/bin/env bash
# Negative controls for the ratchet-separation gate's range form. They prove
# which law judges which commit:
#
#   * a commit already on the protected line is judged by the law at its
#     parent — a rule never binds a change made before it existed;
#   * a commit of the change set must satisfy the protected tip's law and its
#     parent's — a change set cannot loosen the gate for its own commits (gut,
#     commit the mix, restore) and a stale branch meets today's rules;
#   * editing the gate, or the disposition ledger, is itself a
#     verification-artifact move;
#   * a merge is judged by the edits it made itself (remerge-diff), under the
#     law at every parent; an octopus merge, which has no remerge-diff, is
#     refused;
#   * a disposition closes only a commit already on the protected line, only
#     with exactly the flagged files, only with a real author and a different
#     real reviewer, only once per commit;
#   * a bad head or an empty range is an error, never a pass.
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
positives=0
negatives=0
pass() { if run "${@:2}"; then echo "ok   $1"; positives=$((positives + 1)); else echo "FAIL: $1 — expected green" >&2; cat "$tmp/out" >&2; exit 1; fi; }
fail() { if run "${@:2}"; then echo "FAIL: $1 — expected red" >&2; cat "$tmp/out" >&2; exit 1; else echo "ok   $1 (red)"; negatives=$((negatives + 1)); fi; }
golden=crates/almide-syntax/tests/golden/m.txt

g init -q -b main
mkdir -p "$repo/scripts"
printf '%s\n' "$OLD_LAW" > "$repo/scripts/check-ratchet-separation.sh"
edit crates/x/src/a.rs "fn a() {}"
edit "$golden" "row 0"
c0=$(commit "old law and the files it judges")

edit crates/x/src/a.rs "fn b() {}"
edit "$golden" "row 1"
c1=$(commit "implementation and a golden together, before goldens were artifacts")
cp "$root/scripts/check-ratchet-separation.sh" "$repo/scripts/check-ratchet-separation.sh"
c2=$(commit "install the new law (a gate-only change)")

# ── which law judges a commit ──────────────────────────────────────────────
RATCHET_PROTECTED="$c2" pass "a commit on the protected line made before a rule existed is judged by its parent's law" "$c0" "$c2"
RATCHET_PROTECTED="$c1" pass "installing the new law is judged by the law it replaces" "$c1" "$c2"
g checkout -q -b after "$c2"
edit crates/x/src/a.rs "fn c() {}"
edit "$golden" "row 2"
c3=$(commit "implementation and a golden together, after the rule")
RATCHET_PROTECTED="$c2" fail "a change-set commit made after the rule mixes implementation with a golden" "$c2" "$c3"
RATCHET_PROTECTED="$c3" fail "the protected line still carries that commit under the law at its parent" "$c0" "$c3"
fail "without a protected line every commit is the change set's" "$c2" "$c3"

g checkout -q -b bracket "$c2"
printf 'exit 0\n' > "$repo/scripts/check-ratchet-separation.sh"
commit "gut the gate (a gate-only change)" >/dev/null
edit crates/x/src/a.rs "fn g() {}"
edit "$golden" "row g"
commit "implementation and a golden under the gutted gate" >/dev/null
cp "$root/scripts/check-ratchet-separation.sh" "$repo/scripts/check-ratchet-separation.sh"
b3=$(commit "restore the gate")
RATCHET_PROTECTED="$c2" fail "a change set cannot gut the gate for its own commits" "$c2" "$b3"
g checkout -q --orphan nogate
g rm -rfq . >/dev/null
edit crates/x/src/a.rs "fn n() {}"
edit "$golden" "row n"
n0=$(commit "files, before any gate existed")
mkdir -p "$repo/scripts"
cp "$root/scripts/check-ratchet-separation.sh" "$repo/scripts/check-ratchet-separation.sh"
commit "install the gate" >/dev/null
printf 'exit 0\n' > "$repo/scripts/check-ratchet-separation.sh"
commit "gut the gate" >/dev/null
edit crates/x/src/a.rs "fn n2() {}"
edit "$golden" "row n2"
commit "implementation and a golden under the gutted gate" >/dev/null
cp "$root/scripts/check-ratchet-separation.sh" "$repo/scripts/check-ratchet-separation.sh"
n4=$(commit "restore the gate")
fail "without a protected line, a base older than any gate still leaves a trusted law" "$n0" "$n4"

g checkout -q -b stale "$c0"
edit crates/x/src/a.rs "fn s() {}"
edit "$golden" "row s"
s1=$(commit "a branch forked before the rule, mixing implementation with a golden")
RATCHET_PROTECTED="$c2" fail "a branch forked before a rule is judged by the rule at the protected tip" "$(g merge-base "$c2" "$s1")" "$s1"

g checkout -q -b gate-edit "$c2"
sed -i.bak 's/crates\/almide-syntax\/tests\/golden\/\*|//' "$repo/scripts/check-ratchet-separation.sh" && rm -f "$repo/scripts/check-ratchet-separation.sh.bak"
edit crates/x/src/a.rs "fn d() {}"
d1=$(commit "weaken the gate inside an implementation commit")
RATCHET_PROTECTED="$c2" fail "a gate edit cannot ride inside an implementation commit" "$c2" "$d1"

# ── merges are judged by their own edits ───────────────────────────────────
g checkout -q -b side "$c2"
edit crates/x/src/a.rs "fn side() {}"
commit "implementation on a side branch" >/dev/null
g checkout -q -b trunk "$c2"
edit docs/x.md "a note"
commit "a note on the trunk" >/dev/null
g merge -q --no-ff -m "a clean merge" side
m1=$(g rev-parse HEAD)
RATCHET_PROTECTED="$c2" pass "a clean merge carries no change of its own" "$c2" "$m1"
g checkout -q -b evil "$c2"
edit docs/y.md "another note"
commit "another note on a trunk" >/dev/null
g merge -q --no-ff --no-commit side >/dev/null || true
edit crates/x/src/a.rs "fn evil() {}"
edit "$golden" "row evil"
m2=$(commit "a merge that edits implementation and a golden itself")
RATCHET_PROTECTED="$c2" fail "a merge that mixes implementation with a golden in its own edits" "$c2" "$m2"
g checkout -q -b oct1 "$c2"; edit docs/o1.md "one"; commit "octopus arm one" >/dev/null
g checkout -q -b oct2 "$c2"; edit docs/o2.md "two"; commit "octopus arm two" >/dev/null
g checkout -q -b octo "$c2"
g merge -q --no-ff --no-commit oct1 oct2 >/dev/null 2>&1 || true
edit crates/x/src/a.rs "fn octo() {}"
edit "$golden" "row octo"
m3=$(commit "an octopus merge that edits implementation and a golden itself")
RATCHET_PROTECTED="$m3" fail "an octopus merge cannot be judged by its own edits and is refused" "$c2" "$m3"
g checkout -q --orphan pregate-root
g rm -rfq . >/dev/null
edit docs/z.md "a line with no gate"
z1=$(commit "an unrelated history with no gate")
g checkout -q -b pregate-merge "$z1"
g merge -q --no-ff --no-commit --allow-unrelated-histories "$c2" >/dev/null 2>&1 || true
edit crates/x/src/a.rs "fn z() {}"
edit "$golden" "row z"
m4=$(commit "a merge whose first parent predates the gate, mixing in its own edits")
RATCHET_PROTECTED="$m4" fail "a merge is bound by the law its other parent carries" "$c2" "$m4"

# ── a range that judges nothing is not a pass ──────────────────────────────
fail "a head that is not a commit" "$c2" no-such-ref
fail "an empty range" "$c2" "$c2"

# ── dispositions ───────────────────────────────────────────────────────────
g checkout -q -b disp "$c2"
edit crates/x/src/a.rs "fn e() {}"
edit "$golden" "row 3"
e1=$(commit "a violation that reached the protected line")
full=$(g rev-parse "$e1")
line() { printf '%s %s %s %s the move was re-measured and is not a regression' "$@"; }
disposed() { # <label> <pass|fail> <protected or empty> <ledger text>
    g checkout -q -B case "$e1"
    mkdir -p "$repo/proofs"
    printf '%s' "$4" > "$repo/proofs/ratchet-dispositions.txt"
    local e2; e2=$(commit "disposition record")
    if [ "$2" = pass ]; then RATCHET_PROTECTED="$3" pass "$1" "$c2" "$e2"; else RATCHET_PROTECTED="$3" fail "$1" "$c2" "$e2"; fi
}
nl=$'\n'
disposed "a matching record closes a commit already on the protected line" pass "$e1" "$(line "$full" "$golden" alice bob)$nl"
disposed "a last record without a trailing newline is read" pass "$e1" "$(line "$full" "$golden" alice bob)"
disposed "no protected line: no record is honoured" fail "" "$(line "$full" "$golden" alice bob)$nl"
disposed "a change set (a pull request, or a push) cannot close its own commit" fail "$c2" "$(line "$full" "$golden" alice bob)$nl"
disposed "a record naming other files does not close it" fail "$e1" "$(line "$full" crates/almide-syntax/tests/golden/other.txt alice bob)$nl"
disposed "a record whose reviewer is its author does not close it" fail "$e1" "$(line "$full" "$golden" alice alice)$nl"
disposed "the same name in another case is the same person" fail "$e1" "$(line "$full" "$golden" Alice alice)$nl"
disposed "a placeholder reviewer does not close it" fail "$e1" "$(line "$full" "$golden" alice REVIEWER)$nl"
disposed "a template token for a reviewer does not close it" fail "$e1" "$(line "$full" "$golden" claude-opus-5 @REVIEWER@)$nl"
disposed "two records for one commit do not close it" fail "$e1" "$(line "$full" "$golden" alice bob)$nl$(line "$full" "$golden" alice carol)$nl"
disposed "a record for another commit does not close it" fail "$e1" "$(line "$(g rev-parse "$c3")" "$golden" alice bob)$nl"

g checkout -q -B ledger-ride "$e1"
mkdir -p "$repo/proofs"
line "$full" "$golden" alice bob > "$repo/proofs/ratchet-dispositions.txt"
edit crates/x/src/a.rs "fn f() {}"
f1=$(commit "a disposition record inside an implementation commit")
RATCHET_PROTECTED="$e1" fail "a disposition record cannot ride inside an implementation commit" "$e1" "$f1"

echo "ratchet-separation negative controls: $positives positive + $negatives negative all behaved"
