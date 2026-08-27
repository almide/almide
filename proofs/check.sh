#!/usr/bin/env bash
# Re-derive every proof in the trust spine from scratch — the third-party
# "make verify". Anyone with Rocq/Coq runs this and the kernel + the INDEPENDENT
# re-checker (coqchk, the De Bruijn criterion) confirm every theorem, and the
# axiom audit (`Print Assumptions`) is ASSERTED, not just printed: any output
# that is not "Closed under the global context" fails the gate (#920 — the
# flagship invariant used to be a printf, so an added `Axiom` shipped green).
set -euo pipefail

# Byte-order collation, pinned: `sort`'s last-resort comparison follows the ambient
# locale, so an unpinned sort produces different output on differently-configured
# machines. #1031 caught docs/roadmap/README.md changing row order with no content change.
export LC_ALL=C
cd "$(dirname "$0")"

COQC="${COQC:-$(command -v coqc)}"
COQCHK="${COQCHK:-$(command -v coqchk)}"

# The spine, in dependency order. The compile loop, the coqchk module list, and
# the Print-Assumptions expectation are ALL derived from this one list, so a
# module added here is counted everywhere by construction. (Extract.v is the
# extraction driver — build-checker.sh's input, not a theorem module.)
SPINE_FILES=(
  Subset.v OwnershipChecker.v OwnershipLoop.v OwnershipFilter.v CoownLoop.v
  CoownCompose.v ALS.v Translation.v RuntimeModel.v NameTotality.v
  TypeConcretization.v CapabilityBound.v CapabilityReach.v CallModes.v
  StackBalance.v Termination.v FreeList.v FreeListRc.v WasmRcDec.v WasmEncode.v WasmExec.v
  WasmIsa.v WasmDecode.v CowSafety.v
)

echo "== kernel check (coqc) + axiom audit (Print Assumptions) =="
COQC_LOG="$(mktemp)"
trap 'rm -f "$COQC_LOG"' EXIT
for f in "${SPINE_FILES[@]}"; do
  "$COQC" -Q . AlmideTrust "$f" 2>&1 | tee -a "$COQC_LOG"
done

echo
echo "== axiom-cleanliness gate (#920): every Print Assumptions must be closed =="
# (c) No Admitted/admit anywhere in the spine sources — the Coq counterpart of
# ci.yml's Lean sorry-grep.
if grep -rn 'Admitted\.\|admit\.' ./*.v; then
  echo "FAIL: Admitted/admit in the proof spine — a hole is not a proof."
  exit 1
fi
# (a) Every `Print Assumptions` must have answered "Closed under the global
# context" — an `Axioms:` section in the compile output is a FAILURE, and the
# closed-count must equal the number of Print Assumptions statements in the
# spine (so deleting a Print Assumptions cannot shrink coverage silently).
if grep -n '^Axioms:' "$COQC_LOG"; then
  echo "FAIL: a theorem depends on an Axiom (Print Assumptions above names it)."
  exit 1
fi
want="$(cat "${SPINE_FILES[@]}" | grep -cE '^Print Assumptions')"
got="$(grep -c 'Closed under the global context' "$COQC_LOG")"
if [ "$got" -ne "$want" ]; then
  echo "FAIL: $want Print Assumptions statements but $got 'Closed under the global context' answers."
  exit 1
fi
echo "  ok   $got/$want assumptions audits answered 'Closed under the global context'"

# (d) The audited-theorem COUNT is quoted verbatim in three human-facing trust
# documents (the README's Memory Safety section is the third — the reader who
# sees the count first). It was hand-written and nothing checked it: by 2026-08-13 both said
# "37 audited theorems" while the real count was 73 — stale by 36, and nobody
# noticed until a PR happened to touch the neighbourhood. A number in a trust
# document that no gate reads is exactly the claim-drift this spine exists to
# prevent, so assert it HERE, where the authoritative count is computed.
#
# Deliberately a plain equality, not a regeneration: these two files are prose
# written by hand, and a generator would have to own their whole text. The gate
# names the file and the fix instead.
# cwd is proofs/ (the `cd` at the top), so these are siblings.
COUNT_DOCS=(TRUSTED_BASE.md receipt.sh ../README.md)
for doc in "${COUNT_DOCS[@]}"; do
  quoted="$(grep -oE '[0-9]+ audited theorems' "$doc" | grep -oE '^[0-9]+' | sort -u)"
  if [ -z "$quoted" ]; then
    echo "FAIL: $doc no longer quotes an 'N audited theorems' count — the claim was"
    echo "  deleted rather than updated, so nothing states the spine's size any more."
    exit 1
  fi
  if [ "$(printf '%s\n' "$quoted" | wc -l | tr -d ' ')" -ne 1 ] || [ "$quoted" -ne "$want" ]; then
    echo "FAIL: $doc claims '$(printf '%s' "$quoted" | tr '\n' '/')' audited theorems"
    echo "  but the spine has $want. Update the number in that file (and check for a"
    echo "  SECOND occurrence — the count appears once per document by convention)."
    exit 1
  fi
done
echo "  ok   both trust documents quote the measured count ($want audited theorems)"

# Detector drill (the gate.sh tamper-drill pattern): compile a scratch module
# whose theorem DOES rest on an Axiom and prove the grep above would have seen
# it — so the assertion can never rot into matching nothing.
DRILL_DIR="$(mktemp -d)"
cat > "$DRILL_DIR/Tamper.v" <<'EOF'
Axiom oops : False.
Theorem tampered : False.
Proof. exact oops. Qed.
Print Assumptions tampered.
EOF
DRILL_OUT="$("$COQC" "$DRILL_DIR/Tamper.v" 2>&1 || true)"
rm -rf "$DRILL_DIR"
if printf '%s\n' "$DRILL_OUT" | grep -q '^Axioms:'; then
  echo "  ok   detector drill: an injected Axiom IS caught by this gate's pattern"
else
  echo "FAIL: the axiom detector did not flag a tampered module — the gate's grep is blind."
  exit 1
fi

echo
echo "== independent re-check (coqchk — De Bruijn criterion) =="
COQCHK_MODULES=()
for f in "${SPINE_FILES[@]}"; do COQCHK_MODULES+=("AlmideTrust.${f%.v}"); done
COQCHK_OUT="$("$COQCHK" -Q . AlmideTrust "${COQCHK_MODULES[@]}" 2>&1)"
echo "$COQCHK_OUT"
# (b) coqchk's summary lists an "* Axioms:" section when any constant rests on
# one — the independent checker's edition of the same assertion.
if printf '%s\n' "$COQCHK_OUT" | grep -qE '^\s*\*?\s*Axioms:'; then
  echo "FAIL: coqchk reports an Axioms section — the spine is not axiom-clean."
  exit 1
fi

echo
echo "== claim-drift gate (#34, indicator ⑤): TRUSTED_BASE axiom ledger ⊆ kernel-checked =="
# Every theorem the axiom ledger CLAIMS is "Closed under the global context" must
# be a constant the kernel re-checker (coqchk) actually verified — so a public
# claim can never drift past what is proven. Ledger rows: | `name` | File.v | … |.
CHECKED="$(printf '%s\n' "$COQCHK_OUT" | grep -oE 'cst:AlmideTrust\.[A-Za-z0-9]+\.[A-Za-z0-9_]+' | sed 's/^.*\.//' | sort -u)"
CLAIMED="$(grep -oE '^\| `[A-Za-z0-9_]+`' TRUSTED_BASE.md | tr -d '|` ' | sort -u)"
drift=0
for t in $CLAIMED; do
  if printf '%s\n' "$CHECKED" | grep -qx "$t"; then
    echo "  ok   $t"
  else
    echo "  FAIL $t — claimed in the axiom ledger but NOT kernel-checked (claim drift)"; drift=1
  fi
done
[ "$drift" = 0 ] && echo "CLAIMS OK: every axiom-ledger theorem is kernel-checked (public claims ⊆ proven)." || exit 1

echo
echo "== A2 byte-binding grounding (wat2wasm cross-check; SKIP if wabt absent) =="
bash ./check-wasm-bytes.sh

echo
echo "== A2 byte-EXECUTION grounding (wasmtime cross-check; SKIP if wasmtime absent) =="
bash ./check-wasm-exec.sh

echo
echo "PROOF SPINE OK: kernel-checked, axiom-clean (Closed under the global"
echo "context — asserted, with the detector drilled), and independently re-verified."
