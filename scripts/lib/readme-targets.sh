# readme-targets.sh — the README files the generated blocks live in.
#
# The blocks are DERIVED; a translation that carries a hand-copied number is a
# fossil the moment the number moves, which is exactly what
# check-readme-numbers.sh exists to prevent. So every generator writes all of
# them, and every gate reads all of them.
#
# Generators source this and call `readme_fanout "$0" "$@"` right after they
# have parsed their arguments: the first invocation re-execs itself once per
# target with ALMIDE_README_TARGET set, and each re-exec falls through to the
# script's own body with README pointing at that file.
README_TARGETS=(README.md README.ja.md README.zh-CN.md)

readme_fanout() {
  [ -n "${ALMIDE_README_TARGET:-}" ] && return 0
  local self="$1"; shift
  local f
  for f in "${README_TARGETS[@]}"; do
    [ -f "$f" ] || continue
    ALMIDE_README_TARGET="$f" bash "$self" "$@" || exit $?
  done
  exit 0
}
