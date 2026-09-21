#!/usr/bin/env bash
# PARITY GOLDENS ARE THE CLI'S OWN OUTPUT (#2183). The AST / check / run
# parity manifests (crates/almide-syntax/tests/golden, crates/almide-spine/
# tests/golden) are what the greenfield legs are judged against, and they
# claimed to come from a pinned old binary while in practice being re-recorded
# from the tree under test — with nothing able to tell a regeneration from a
# hand edit or a stale build. This gate makes the real definition enforced:
# the oracle is the CLI built from THIS commit, and the committed goldens must
# equal what it produces, byte for byte. Run the three generators against
# $ORACLE and fail on any diff.
#
#   ORACLE=target/release/almide bash scripts/check-parity-goldens.sh
#   (CI: the almide-gates job, with the release artifact and wasmtime)
#
# The `# oracle:` header line each manifest starts with names the version and
# git HEAD the rows were recorded at; it is informational (a rebase changes
# the SHA) and excluded from the diff. Everything else — a row, an exclusion,
# a reason — must match: a divergence here is either an uncommitted CLI
# behaviour change (regenerate and commit the rows, in their own commit) or a
# golden edited by hand (never do that).
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.." || exit 2

ORACLE="${ORACLE:-target/release/almide}"
case "$ORACLE" in /*) ;; *) ORACLE="$PWD/$ORACLE" ;; esac
[ -x "$ORACLE" ] || { echo "::error::ORACLE not executable: $ORACLE (cargo build --release)"; exit 2; }
export ORACLE

GOLDENS="crates/almide-syntax/tests/golden crates/almide-spine/tests/golden"
# Refuse to judge a dirty golden tree: the diff after regeneration must be
# attributable to the generators alone.
if ! git diff --quiet -- $GOLDENS; then
  echo "::error::parity goldens have uncommitted changes — commit or revert them before running this gate"
  git --no-pager diff --stat -- $GOLDENS
  exit 2
fi

# #2405: the committed header is read before anything is regenerated. The SHA
# in it stays informational (a rebase changes it), but the version and build
# kind must name the CLI built from this tree — a manifest recorded by a
# released binary or from another version's tree is refused here, exactly as
# every parity test refuses it (almide_corpus::verify_oracle_header).
. scripts/lib/oracle-header.sh
for m in crates/almide-syntax/tests/golden/spec-ast-manifest.txt \
         crates/almide-spine/tests/golden/spec-check-manifest.txt \
         crates/almide-spine/tests/golden/spec-run-manifest.txt; do
  verify_committed_header "$m" || exit 1
done

# The generators' own stale-tree refusal runs in `gate` mode here: this gate
# vouches for the binary (CI's artifact is built from this very commit, and
# its checkout is detached, so "behind upstream" has no meaning); the
# untracked-fixture check stays on. A developer wanting the full check runs
# the generators directly, or sets ALMIDE_MANIFEST_TREE_CHECK=strict.
export ALMIDE_MANIFEST_TREE_CHECK="${ALMIDE_MANIFEST_TREE_CHECK:-gate}"

rc=0
for gen in gen-ast-manifest gen-check-manifest gen-run-manifest; do
  if ! bash "scripts/$gen.sh"; then
    echo "::error::scripts/$gen.sh failed"
    rc=1
  fi
done

if ! git diff --exit-code -I '^# oracle: ' -- $GOLDENS; then
  echo "::error::parity goldens differ from what the CLI built from this tree produces (diff above; the regenerated files are left in place)."
  echo "  The greenfield legs are judged against these rows, so they must be the CLI's own output."
  echo "  If the CLI's behaviour changed on purpose: commit the regenerated rows in their own commit."
  echo "  If it did not: a golden was edited by hand, or recorded from a stale build — this regeneration is the fix."
  exit 1
fi
[ "$rc" -eq 0 ] || exit "$rc"

# Only the informational header can differ now; restore it so the gate leaves
# the tree as it found it.
git checkout -q -- $GOLDENS
echo "parity-goldens OK: ast, check and run manifests equal the CLI's output at $(git rev-parse --short=9 HEAD)"
