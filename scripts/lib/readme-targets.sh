# readme-targets.sh — the README files the generated blocks live in.
#
# The blocks are DERIVED; a translation that carries a hand-copied number is a
# fossil the moment the number moves, which is exactly what
# check-readme-numbers.sh exists to prevent. So every generator writes all of
# them, and every gate reads all of them.
#
# Generators source this and call `readme_fanout "$0" "$@"` right after they
# have parsed their arguments: the first invocation re-execs itself once per
# target with README_GEN_TARGET set, and each re-exec falls through to the
# script's own body with README pointing at that file. The build-speed block is
# rendered by the almide-gates `bench` subcommand, which carries the same list
# (tools/almide-gates/src/main.almd); change both together.
README_TARGETS=(README.md README.ja.md README.zh-CN.md)

readme_fanout() {
  [ -n "${README_GEN_TARGET:-}" ] && return 0
  local self="$1"; shift
  local f
  for f in "${README_TARGETS[@]}"; do
    [ -f "$f" ] || continue
    README_GEN_TARGET="$f" bash "$self" "$@" || exit $?
  done
  exit 0
}
