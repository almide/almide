# Sourced by scripts/gen-{ast,check,run}-manifest.sh. The oracle every parity
# golden is recorded from is the CLI built from THIS tree (#2183): there is no
# pinned old binary, and scripts/check-parity-goldens.sh regenerates the
# goldens from the same commit's release binary in CI and fails on any diff.
#
# `oracle_header` prints the one `# oracle:` line each manifest starts with —
# the generator's `almide --version` and the git HEAD it ran at. Informational
# only: a rebase changes the SHA, so the gate diffs the ROWS and ignores this
# line (`git diff -I '^# oracle: '`), and every reader skips it
# (almide_corpus::manifest_rows). Requires $ORACLE to be set.
#
# HEAD is read where the script is sourced (the repo root) — the run generator
# later cd's into the judge mount, whose HEAD is a different repository's.
ORACLE_HEAD="$(git rev-parse --short=9 HEAD 2>/dev/null || printf 'unknown')"
oracle_header() {
  printf '# oracle: %s at %s — the CLI built from this tree; informational (scripts/check-parity-goldens.sh regenerates the rows and diffs them)\n' \
    "$("$ORACLE" --version | head -1)" "$ORACLE_HEAD"
}

# Prepend the header to a finished (sorted) manifest in place.
stamp_oracle_header() {
  local tmp
  tmp="$(mktemp)"
  { oracle_header; cat "$1"; } > "$tmp" && mv "$tmp" "$1"
}
