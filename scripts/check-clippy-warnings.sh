#!/usr/bin/env bash
# CLIPPY WARNING RATCHET (#1462).
#
# The sibling of scripts/check-rustc-warnings.sh, one lint tier up: clippy
# sees what rustc does not (a `.clone()` on a Copy type, a manual reimplement
# of a std method, an eta-expanded closure), and until this gate existed it
# ran in ZERO workflows — the survey's finding. Same discipline as every
# ratchet in this repo: the count is a shrink-only ledger
# (scripts/clippy-warnings-baseline.txt), it fails in BOTH directions, and a
# toolchain upgrade that lights a new lint tree-wide is answered by raising
# the baseline ON PURPOSE in the same change, then burning it back down.
#
# Dependency warnings are not counted (not ours to fix); the per-crate
# epilogue lines are dropped (no spans); a defect is counted ONCE even when
# `--all-targets` reaches it twice (lib + test harness). The unit floor
# guards against the #976 blind-scan class: a clippy invocation that
# compiles nothing reports zero warnings forever.
#
# cargo-fmt gating, deliberately NOT here: the tree measures 6,168 rustfmt
# hunks today, and a flag-day reformat would conflict with every in-flight
# branch AND with the committed generated sources
# (crates/almide-codegen/src/generated/*), whose generators do not emit
# rustfmt shape — a fmt gate would fight the regen-diff gate. The exclusion
# and its adoption plan are recorded in docs/project/RUST-LINT-GATES.md.
set -euo pipefail

export LC_ALL=C
cd "$(git rev-parse --show-toplevel)"

BASELINE_FILE="scripts/clippy-warnings-baseline.txt"
UNIT_FLOOR=40

# THE COUNT IS TOOLCHAIN-COUPLED (first CI run's finding: 1,480 under the
# pinned 1.94.0, 1,494 under a local 1.96 — clippy grows lints every
# release). The NORMATIVE environment is CI's pinned RUST_TOOLCHAIN; the
# baseline is that toolchain's count. A local run on the pin gives the
# real verdict; a local run on any other toolchain prints its count for
# information and exits 0 — a number measured in the wrong environment
# must not be able to fail (or greenwash) the gate.
PINNED_TOOLCHAIN="1.94.0"
VERDICT=1
if rustup toolchain list 2>/dev/null | grep -q "^${PINNED_TOOLCHAIN}"; then
  CARGO="rustup run ${PINNED_TOOLCHAIN} cargo"
elif [ "${CLIPPY_REQUIRE_PIN:-}" = "1" ]; then
  echo "::error::clippy-warnings: pinned toolchain ${PINNED_TOOLCHAIN} not installed" >&2
  exit 1
else
  CARGO="${CARGO:-cargo}"
  echo "clippy-warnings: pinned toolchain ${PINNED_TOOLCHAIN} not installed —"
  echo "counting under $(rustc --version 2>/dev/null || echo unknown) for information only (no verdict)."
  VERDICT=0
fi

json="$(mktemp)"
err="$(mktemp)"
trap 'rm -f "$json" "$err"' EXIT

if ! $CARGO clippy --release --workspace --all-targets --message-format=json > "$json" 2> "$err"; then
  cat "$err" >&2
  echo "::error::clippy-warnings: \`cargo clippy --workspace --all-targets\` FAILED."
  echo "This gate counts warnings; it cannot do that on a tree that does not compile."
  exit 1
fi

python3 - "$json" "$BASELINE_FILE" "$UNIT_FLOOR" "$VERDICT" <<'PY'
import json as jsonlib
import os
import sys

stream_path, baseline_path, unit_floor = sys.argv[1], sys.argv[2], int(sys.argv[3])
verdict = sys.argv[4] == "1"
root = os.getcwd()

units = 0
warnings = {}
for line in open(stream_path, encoding="utf-8"):
    line = line.strip()
    if not line.startswith("{"):
        continue
    try:
        rec = jsonlib.loads(line)
    except ValueError:
        continue
    manifest = rec.get("manifest_path") or ""
    if not manifest.startswith(root + os.sep):
        continue
    reason = rec.get("reason")
    if reason == "compiler-artifact":
        units += 1
        continue
    if reason != "compiler-message":
        continue
    msg = rec.get("message") or {}
    if msg.get("level") != "warning":
        continue
    if not msg.get("primary_span_text") and not msg.get("spans"):
        continue
    spans = msg.get("spans") or []
    primary = next((s for s in spans if s.get("is_primary")), spans[0] if spans else None)
    if primary:
        where = f"{primary['file_name']}:{primary['line_start']}:{primary['column_start']}"
    else:
        where = "?"
    code = (msg.get("code") or {}).get("code") or "-"
    text = msg.get("message", "").splitlines()[0]
    warnings[(where, code, text)] = f"{where}  [{code}] {text}"

if units < unit_floor:
    print(f"::error::clippy-warnings: only {units} workspace units checked (floor {unit_floor})"
          " — the scan went blind (#976 class): the cargo invocation no longer reaches the"
          " workspace targets. Fix the invocation; do not lower the floor to make this pass.",
          file=sys.stderr)
    sys.exit(1)

count = len(warnings)
baseline = int(open(baseline_path, encoding="utf-8").read().strip())
print(f"clippy-warnings: {count} warning(s) over {units} workspace units (baseline {baseline})")

if not verdict:
    print("clippy-warnings: NON-PINNED toolchain — count printed for information; the"
          " pinned-toolchain run in CI is the verdict.")
    sys.exit(0)

if count > baseline:
    for w in sorted(warnings.values()):
        print(f"  {w}")
    print(f"::error::clippy-warnings: {count} warnings exceeds the baseline {baseline}.",
          file=sys.stderr)
    print("Read each one before silencing it — clippy findings are usually a simpler or"
          " more correct spelling of the same intent. Reach for #[allow(clippy::...)] only"
          " when the lint is genuinely wrong for the site, and say why in a comment. If a"
          " toolchain upgrade lights a new lint tree-wide, raise the baseline in the SAME"
          " change with the reason in the commit message — then burn it back down.",
          file=sys.stderr)
    sys.exit(1)

if count < baseline:
    print(f"::error::clippy-warnings: {count} warnings is BELOW the baseline {baseline} —"
          f" ratchet {baseline_path} down to {count} in the SAME change. The ledger only"
          " shrinks, and it has to say so.", file=sys.stderr)
    sys.exit(1)
PY
