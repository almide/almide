#!/usr/bin/env bash
# RUSTC WARNING RATCHET (#1228).
#
# WHY THIS EXISTS. A build that prints 40 warnings teaches everyone — human and
# model — to scroll past warning output, and then the ONE warning that means
# something is invisible inside the noise. Every warning in the #1228 sweep was
# a real signal that had been buried: an orphaned mono-discovery wrapper, a
# never-wired MIR probe, an LSP enum variant nothing constructs, a test that
# bound a pass result and asserted nothing about it. Those are exactly the
# findings a clean build surfaces on the day they appear and a noisy one hides
# for a year.
#
# WHAT IT CHECKS: the number of rustc warnings emitted for WORKSPACE targets
# (lib, bin, tests, examples, benches — `--all-targets`, because the test
# targets silted up faster than the libraries did) against
# `scripts/rustc-warnings-baseline.txt`. Dependency warnings are not counted:
# they are not ours to fix and a registry update must not be able to fail this.
#
# The baseline is 0 and the intent is that it stays 0. It is a file rather than
# a hard-coded `-D warnings` for one reason: a rustc upgrade can introduce a new
# lint that fires across the tree, and the repo's answer to that is the same
# shrink-only ledger discipline used by the embedded-size / coverage ratchets —
# raise it ON PURPOSE in the same change, with the reason in the commit message,
# then burn it back down. It fails in BOTH directions: a count below the
# baseline must be ratcheted down in the same change, so the number is never
# quietly generous.
#
# BLINDNESS GUARD (#976 class): a scan that compiles nothing reports zero
# warnings and reads as a pass forever. The unit floor below asserts the check
# actually saw the workspace, so a broken invocation fails loudly instead of
# going green.
#
# OUT OF SCOPE, on purpose: the five manifests `Cargo.toml` excludes
# (crates/almide-kernel, research/benchmark/stdlib/rust_wasm_compare,
# tools/wasmgen-harness, tools/wasmgen-harness-uu, tools/xtarget-fuzz). They are
# not part of the workspace build this gate protects, so their warnings never
# reach the build log this gate is about. All five were measured warning-free on
# 2026-08-13; covering them needs one cargo invocation each and can be added
# here when one of them starts costing something.
set -euo pipefail

# Byte-order collation, pinned: an unpinned sort orders differently on
# differently-configured machines (#1031).
export LC_ALL=C
cd "$(git rev-parse --show-toplevel)"

BASELINE_FILE="scripts/rustc-warnings-baseline.txt"
CARGO="${CARGO:-cargo}"
# The workspace has 15 members and many more targets; a scan that reports fewer
# compiled units than this did not see the workspace. Removing crates on purpose
# = lower this floor in the same change.
UNIT_FLOOR=40

json="$(mktemp)"
err="$(mktemp)"
trap 'rm -f "$json" "$err"' EXIT

if ! "$CARGO" check --release --workspace --all-targets --message-format=json > "$json" 2> "$err"; then
  cat "$err" >&2
  echo "::error::rustc-warnings: \`cargo check --workspace --all-targets\` FAILED."
  echo "This gate counts warnings; it cannot do that on a tree that does not compile."
  exit 1
fi

python3 - "$json" "$BASELINE_FILE" "$UNIT_FLOOR" <<'PY'
import json as jsonlib
import os
import sys

stream_path, baseline_path, unit_floor = sys.argv[1], sys.argv[2], int(sys.argv[3])
root = os.getcwd()

units = 0
# Keyed by source position + text: `--all-targets` checks a lib once as a lib
# and again as its own test harness, so one source defect arrives twice. The
# count has to be a count of DEFECTS, or a baseline raise would be sized against
# an artefact of the target list.
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
    # Ours only: a registry dependency's warning is not this repo's to fix, and
    # a `cargo update` must not be able to fail this gate.
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
    # Cargo's per-crate epilogue ("`almide-mir` (lib) generated 3 warnings") is
    # itself a level=warning message with NO spans. Counting it would double
    # every crate's total; the spans test drops it and keeps real diagnostics.
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
    print(f"::error::rustc-warnings: only {units} workspace units checked (floor {unit_floor})"
          " — the scan went blind (#976 class): the cargo invocation no longer reaches the"
          " workspace targets. Fix the invocation; do not lower the floor to make this pass.",
          file=sys.stderr)
    sys.exit(1)

count = len(warnings)
baseline = int(open(baseline_path, encoding="utf-8").read().strip())
print(f"rustc-warnings: {count} warning(s) over {units} workspace units (baseline {baseline})")

if count > baseline:
    for w in sorted(warnings.values()):
        print(f"  {w}")
    print(f"::error::rustc-warnings: {count} warnings exceeds the baseline {baseline}.",
          file=sys.stderr)
    print("Read each one before silencing it — an unused variable can be a computed value"
          " dropped on the floor, an unused `mut` a mutation that never happens, an unused"
          " import a code path deleted whose replacement was never wired, a never-used fn a"
          " dead feature (delete it, do not #[allow] it). Reach for #[allow(...)] only when"
          " the warning is genuinely wrong, and say why in a comment. If a new lint from a"
          " toolchain upgrade fires tree-wide, raise the baseline in the SAME change with the"
          " reason in the commit message — then burn it back down.", file=sys.stderr)
    sys.exit(1)

if count < baseline:
    print(f"::error::rustc-warnings: {count} warnings is BELOW the baseline {baseline} —"
          f" ratchet {baseline_path} down to {count} in the SAME change. The ledger only"
          " shrinks, and it has to say so.", file=sys.stderr)
    sys.exit(1)
PY
