# The CI slice, bash twin of crates/almide-corpus/src/ci_slice.rs — ONE switch
# for every corpus-iterating gate (owner decision 2026-10-25: heavy jobs run
# thoroughly only when going into main; other PRs run them light).
#
# ALMIDE_CI_SLICE=k (k > 1): a gate judges a commit-rotated 1-in-k slice of its
# fixtures plus every fixture the change touches. Unset or 1: the full run,
# unchanged. A fixture is in the slice when (hash(path) + offset) % k == 0,
# hash = polynomial 31 mod 1e9+7 over the repo-relative path's bytes, offset =
# the first 7 hex digits of ALMIDE_CI_SEED mod k — the same number the Rust
# twin computes (pinned by almide-corpus's bash_twin_selects_what_rust_selects).
#
# The change's own files: ALMIDE_CI_TOUCHED (a file, written by
# scripts/ci-slice-prepare.sh), else `git diff --name-only $ALMIDE_CI_BASE HEAD`.
# Under CI a slice with neither is refused.
#
# Functions:
#   ci_slice_k               k (1 on a full run)
#   ci_slice_active          true under a slice
#   ci_slice_offset          the rotation
#   ci_slice_touched <out>   write the change's paths to <out>
#   ci_slice_filter <gate>   stdin: repo-relative keys, one per line, in order;
#                            stdout: the selected keys, in order; stderr: the
#                            `keeps N of M` line. Never empty for a non-empty
#                            input. On a full run it copies stdin.
#   ci_slice_count <full> <floor> <gate>   a seed / mutant count for this run
#   ci_slice_refuse_regen <gate>           exit 2 under a slice

ci_slice_k() {
  local k="${ALMIDE_CI_SLICE:-1}"
  case "$k" in
    ''|*[!0-9]*) echo "ALMIDE_CI_SLICE=$k: expected a whole number k >= 1 (1 = the full run)" >&2; return 2 ;;
  esac
  [ "$k" -ge 1 ] || k=1
  echo "$k"
}

ci_slice_active() {
  local k; k="$(ci_slice_k)" || return 2
  [ "$k" -gt 1 ]
}

ci_slice_offset() {
  local k hex; k="$(ci_slice_k)" || return 2
  hex="$(printf '%s' "${ALMIDE_CI_SEED:-}" | tr -cd '0-9a-fA-F' | cut -c1-7)"
  echo $(( 16#${hex:-0} % k ))
}

ci_slice_touched() {
  local out="$1"
  if [ -n "${ALMIDE_CI_TOUCHED:-}" ]; then
    cat "$ALMIDE_CI_TOUCHED" > "$out" || { echo "ALMIDE_CI_TOUCHED=$ALMIDE_CI_TOUCHED: unreadable" >&2; return 2; }
  elif [ -n "${ALMIDE_CI_BASE:-}" ]; then
    if ! git cat-file -e "${ALMIDE_CI_BASE}^{commit}" 2>/dev/null; then
      git fetch -q --no-tags --depth=1 origin "$ALMIDE_CI_BASE" 2>/dev/null || true
    fi
    git diff --name-only "$ALMIDE_CI_BASE" HEAD > "$out" || {
      echo "ALMIDE_CI_BASE=$ALMIDE_CI_BASE: the base commit is not in this clone — a slice without the change's own fixtures is refused" >&2
      return 2
    }
  elif [ "${CI:-}" = "true" ]; then
    echo "ALMIDE_CI_SLICE is set under CI with neither ALMIDE_CI_TOUCHED nor ALMIDE_CI_BASE — the slice would leave out the change's own fixtures" >&2
    return 2
  else
    : > "$out"
  fi
}

ci_slice_filter() {
  local gate="${1:-gate}" k off tmp
  k="$(ci_slice_k)" || return 2
  if [ "$k" -le 1 ]; then
    tmp="$(mktemp)"; cat > "$tmp"
    echo "$gate: full run, $(grep -c . "$tmp" | tr -d ' ') of $(grep -c . "$tmp" | tr -d ' ') fixture(s)" >&2
    cat "$tmp"; rm -f "$tmp"
    return 0
  fi
  off="$(ci_slice_offset)"
  tmp="$(mktemp)"
  ci_slice_touched "$tmp" || { rm -f "$tmp"; return 2; }
  LC_ALL=C awk -v k="$k" -v off="$off" -v touchedf="$tmp" -v gate="$gate" '
    BEGIN {
      for (i = 1; i < 256; i++) ord[sprintf("%c", i)] = i
      while ((getline t < touchedf) > 0) { sub(/^als\//, "", t); if (t != "") touched[++nt] = t }
    }
    function hash(s,   h, i, n) { h = 0; n = length(s); for (i = 1; i <= n; i++) h = (h * 31 + ord[substr(s, i, 1)]) % 1000000007; return h }
    function touches(p, key,   stem, r) {
      if (p == key) return 1
      if (substr(p, 1, length(key) + 1) == key "/") return 1
      if (match(key, /\.[^.\/]*$/)) {
        stem = substr(key, 1, RSTART - 1)
        if (stem != "" && substr(stem, length(stem)) != "/" && substr(p, 1, length(stem) + 1) == stem ".") return 1
      }
      return 0
    }
    {
      key = $0; sub(/^\.\//, "", key)
      if (key == "") next
      n++; line[n] = $0
      rank[n] = (hash(key) + off) % k
      tch[n] = 0
      for (j = 1; j <= nt; j++) if (touches(touched[j], key)) { tch[n] = 1; break }
      keep[n] = (rank[n] == 0 || tch[n])
      if (keep[n]) kept++
    }
    END {
      if (n > 0 && kept == 0) {
        best = 1; for (i = 2; i <= n; i++) if (rank[i] < rank[best]) best = i
        keep[best] = 1; kept = 1
      }
      for (i = 1; i <= n; i++) if (keep[i]) { print line[i]; if (tch[i]) tt++ }
      printf "%s: CI slice 1/%d (offset %d) keeps %d of %d fixture(s), %d of them touched by the change; the full run (develop, main, PRs into main) judges all %d\n", gate, k, off, kept, n, tt + 0, n > "/dev/stderr"
    }'
  local rc=$?
  rm -f "$tmp"
  return $rc
}

ci_slice_count() {
  local full="$1" floor="$2" gate="$3" k n
  k="$(ci_slice_k)" || return 2
  if [ "$k" -le 1 ]; then echo "$full"; return 0; fi
  n=$(( (full + k - 1) / k ))
  [ "$n" -ge "$floor" ] || n="$floor"
  [ "$n" -le "$full" ] || n="$full"
  echo "$gate: CI slice 1/$k runs $n of $full (the full run runs all $full)" >&2
  echo "$n"
}

ci_slice_refuse_regen() {
  if ci_slice_active; then
    echo "${1:-gate}: refusing to regenerate a committed ledger under ALMIDE_CI_SLICE=$(ci_slice_k) — unset it to regenerate" >&2
    exit 2
  fi
}
