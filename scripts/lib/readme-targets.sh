# readme-targets.sh — the pages the generated blocks live in, and the pages
# whose hand-written numbers are gated.
#
# The blocks are DERIVED; a page that carries a hand-copied number is a fossil
# the moment the number moves, which is exactly what check-readme-numbers.sh
# exists to prevent. The front-page READMEs (README.md and its translations)
# carry no generated block since the README was slimmed: every measured table
# and derived count lives on the measurements page, docs/benchmarks/README.md,
# once, in English. A translation links to it instead of copying it.
#
# Generators source this and call `readme_fanout "$0" "$@"` right after they
# have parsed their arguments: the first invocation re-execs itself once per
# target with README_GEN_TARGET set, and each re-exec falls through to the
# script's own body with README pointing at that file. The build-speed block is
# rendered by the almide-gates `bench` subcommand, which carries the same list
# (tools/almide-gates/src/main.almd); change both together.
README_TARGETS=(docs/benchmarks/README.md)

# Every page check-readme-numbers.sh reads for an undated hand-written count:
# the front pages, and the docs pages the README's sections moved to. A page
# that leaves this list stops being gated, so a section moved to a new page
# adds that page here in the same change.
README_NUMBER_PAGES=(
  README.md README.ja.md README.zh-CN.md
  docs/benchmarks/README.md docs/status.md docs/how-it-works.md docs/written-in-almide.md
)

# The page that holds the LLM-writability scorecard (the MSR row) and the
# measured sections scripts/lib/readme-freshness.sh ages.
README_MEASURED_PAGE="docs/benchmarks/README.md"

readme_fanout() {
  [ -n "${README_GEN_TARGET:-}" ] && return 0
  local self="$1"; shift
  local f
  for f in "${README_TARGETS[@]}"; do
    [ -f "$f" ] || { echo "::error::generated-block page $f is missing (scripts/lib/readme-targets.sh)"; exit 2; }
    README_GEN_TARGET="$f" bash "$self" "$@" || exit $?
  done
  exit 0
}

# The repository root as a relative link prefix from a page: "." for a file at
# the root, "../.." for docs/benchmarks/README.md. A generated block that links
# into the tree renders its links through this, so the same block is correct
# on whichever page holds it.
readme_root_prefix() {
  local d
  d="$(dirname "$1")"
  if [ "$d" = "." ]; then printf '.'; else printf '%s' "$d" | sed -E 's#[^/]+#..#g'; fi
}
